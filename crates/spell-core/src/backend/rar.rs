use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use unrar_ng::{Archive, CursorBeforeHeader, OpenArchive, Process};

use crate::backend::common::detect_conflict;
use crate::error::ArchiveError;
use crate::limits::Limits;
use crate::plan::{
    ConflictKind, DecisionTable, ExtractPlan, PlanAction, PlanWarning, PlannedItem, resolve_all,
};
use crate::reader::{ArchiveReader, ExtractReport, ExtractSink, SkipReason};
use crate::safety::available_space;
use crate::safety::display_guard::guard_display;
use crate::safety::path_sanitize::{resolve_under, sanitize_rel};
use crate::secret::SecretString;
use crate::task::CancelToken;
use crate::types::{ArchiveEntry, ArchiveKind, EntryId, EntryKind, Method};

const READ_BUF: usize = 64 * 1024;
/// unrar 的 `read()` 会把整条数据读进内存；超过此阈值直接拒绝。
const MAX_IN_MEMORY: u64 = 512 * 1024 * 1024;

pub struct RarReader {
    path: PathBuf,
    entries: Vec<ArchiveEntry>,
    password: Option<SecretString>,
}

impl RarReader {
    pub fn new(path: &Path, password: Option<&SecretString>) -> Result<Self, ArchiveError> {
        let mut entries = Vec::new();
        let mut index = 0u32;

        {
            let archive = match password {
                Some(password) => Archive::with_password(path, password.expose().as_bytes())
                    .open_for_listing()
                    .map_err(map_error)?,
                None => Archive::new(path).open_for_listing().map_err(map_error)?,
            };

            for item in archive {
                let header = item.map_err(map_error)?;
                let display = header.filename.to_string_lossy().into_owned();
                let raw_name: Vec<u8> = display.as_bytes().to_vec();
                let kind = if header.is_directory() {
                    EntryKind::Dir
                } else if is_symlink(header.file_attr) {
                    EntryKind::Symlink
                } else {
                    EntryKind::File
                };
                let mut entry = ArchiveEntry::new(index, &raw_name, display, kind);
                entry.size = if kind.is_dir() {
                    0
                } else {
                    header.unpacked_size
                };
                entry.packed = None;
                entry.crc32 = if kind.is_dir() {
                    None
                } else {
                    Some(header.file_crc)
                };
                entry.mtime = dos_to_system_time(header.file_time);
                entry.encrypted = header.is_encrypted();
                entry.method = if header.method == 0x30 {
                    Method::Store
                } else {
                    Method::Rar
                };
                entries.push(entry);
                index = index.saturating_add(1);
            }
        }

        Ok(Self {
            path: path.to_path_buf(),
            entries,
            password: password.cloned(),
        })
    }

    fn open_processing(&self) -> Result<OpenArchive<Process, CursorBeforeHeader>, ArchiveError> {
        match self.password.as_ref() {
            Some(password) => Archive::with_password(&self.path, password.expose().as_bytes())
                .open_for_processing()
                .map_err(map_error),
            None => Archive::new(&self.path)
                .open_for_processing()
                .map_err(map_error),
        }
    }

    /// 出错后重新打开并跳过前 `count` 条，继续处理后续条目。
    fn skip_to(
        &self,
        count: u32,
    ) -> Result<OpenArchive<Process, CursorBeforeHeader>, ArchiveError> {
        let mut archive = self.open_processing()?;
        for _ in 0..count {
            let Some(header) = archive.read_header().map_err(map_error)? else {
                return Err(ArchiveError::Other(
                    "RAR 条目数与列表不一致，无法继续".to_owned(),
                ));
            };
            archive = header.skip().map_err(map_error)?;
        }
        Ok(archive)
    }
}

impl ArchiveReader for RarReader {
    fn kind(&self) -> ArchiveKind {
        ArchiveKind::Rar
    }

    fn entries(&self) -> &[ArchiveEntry] {
        &self.entries
    }

    fn plan(
        &self,
        ids: &[EntryId],
        target: &Path,
        limits: &Limits,
    ) -> Result<ExtractPlan, ArchiveError> {
        if self.entries.len() > limits.max_entries as usize {
            return Err(ArchiveError::LimitExceeded("条目数超过上限"));
        }

        let mut plan = ExtractPlan {
            free_space: available_space(target),
            ..Default::default()
        };

        for &id in ids {
            let entry = self
                .entries
                .get(id as usize)
                .ok_or_else(|| ArchiveError::Other(format!("条目 {id} 不存在")))?;

            let mut item = PlannedItem {
                id,
                rel_path: PathBuf::new(),
                kind: entry.kind,
                size: entry.size,
                action: PlanAction::Create,
            };

            let guard = guard_display(&entry.display);
            if guard.suspicious {
                plan.warnings.push(PlanWarning::SuspiciousDisplayName {
                    id,
                    detail: guard.text.clone(),
                });
            }

            if matches!(entry.kind, EntryKind::Symlink | EntryKind::Hardlink) {
                plan.warnings.push(PlanWarning::SymlinkSkipped { id });
                item.action = PlanAction::Skip;
                plan.items.push(item);
                continue;
            }

            let rel = match sanitize_rel(&entry.display, limits) {
                Ok(rel) => rel,
                Err(err) => {
                    let detail = err.to_string();
                    plan.warnings.push(PlanWarning::UnsafePath {
                        id,
                        detail: detail.clone(),
                    });
                    item.action = PlanAction::Reject(detail);
                    plan.items.push(item);
                    continue;
                }
            };

            let abs = match resolve_under(target, &rel, limits) {
                Ok(path) => path,
                Err(err) => {
                    let detail = err.to_string();
                    plan.warnings.push(PlanWarning::UnsafePath {
                        id,
                        detail: detail.clone(),
                    });
                    item.action = PlanAction::Reject(detail);
                    plan.items.push(item);
                    continue;
                }
            };
            item.rel_path = abs.clone();

            if let Some(conflict) = detect_conflict(id, &abs, entry.kind)? {
                if matches!(conflict.kind, ConflictKind::ReparsePoint) {
                    item.action =
                        PlanAction::Reject("目标已存在且是重解析点（junction/symlink）".to_owned());
                } else {
                    item.action = PlanAction::Overwrite;
                }
                plan.conflicts.push(conflict);
            }

            if !entry.kind.is_dir() {
                plan.total_bytes = plan.total_bytes.saturating_add(entry.size);
                plan.largest_entry = plan.largest_entry.max(entry.size);
            }
            plan.items.push(item);
        }

        plan.needs_password = plan
            .items
            .iter()
            .any(|item| self.entries[item.id as usize].encrypted);

        Ok(plan)
    }

    fn extract(
        &mut self,
        plan: &ExtractPlan,
        decisions: &DecisionTable,
        cancel: &CancelToken,
        sink: &mut dyn ExtractSink,
    ) -> Result<ExtractReport, ArchiveError> {
        let start = Instant::now();
        let actions = resolve_all(plan, decisions);
        let mut report = ExtractReport::default();
        let mut archive = self.open_processing()?;
        let mut index = 0u32;

        while let Some(header) = archive.read_header().map_err(map_error)? {
            let id = index;
            index = index.saturating_add(1);

            if cancel.is_cancelled() {
                return Err(ArchiveError::Cancelled);
            }

            let Some(entry) = self.entries.get(id as usize).cloned() else {
                archive = header.skip().map_err(map_error)?;
                continue;
            };
            let Some(action) = actions.get(&id).cloned() else {
                archive = header.skip().map_err(map_error)?;
                continue;
            };

            match action {
                crate::plan::ResolvedAction::Skip => {
                    report.skipped.push((id, SkipReason::PolicySkip));
                    archive = header.skip().map_err(map_error)?;
                    continue;
                }
                crate::plan::ResolvedAction::Reject(reason) => {
                    report.failed.push((id, reason));
                    archive = header.skip().map_err(map_error)?;
                    continue;
                }
                crate::plan::ResolvedAction::Write { .. } => {}
            }

            if let Err(err) = sink.on_entry_begin(id, &entry) {
                report.failed.push((id, err.to_string()));
                archive = header.skip().map_err(map_error)?;
                continue;
            }

            if entry.kind.is_dir() {
                archive = header.skip().map_err(map_error)?;
                finish_entry(sink, id, true, &mut report)?;
                continue;
            }

            if entry.size > MAX_IN_MEMORY {
                let _ = sink.on_entry_end(id, false);
                report.failed.push((
                    id,
                    format!(
                        "RAR 条目超过 {} MiB，暂不支持流式解压",
                        MAX_IN_MEMORY / (1024 * 1024)
                    ),
                ));
                archive = header.skip().map_err(map_error)?;
                continue;
            }

            let (data, next) = match header.read() {
                Ok(value) => value,
                Err(err) => {
                    let mapped = map_error(err);
                    let _ = sink.on_entry_end(id, false);
                    if matches!(
                        mapped,
                        ArchiveError::PasswordRequired | ArchiveError::BadPassword
                    ) {
                        return Err(mapped);
                    }
                    report.failed.push((id, mapped.to_string()));
                    archive = self.skip_to(index)?;
                    continue;
                }
            };

            let mut entry_ok = true;
            for chunk in data.chunks(READ_BUF) {
                if cancel.is_cancelled() {
                    let _ = sink.on_entry_end(id, false);
                    return Err(ArchiveError::Cancelled);
                }
                if let Err(err) = sink.on_data(id, chunk) {
                    report.failed.push((id, err.to_string()));
                    entry_ok = false;
                    break;
                }
            }
            archive = next;
            finish_entry(sink, id, entry_ok, &mut report)?;
        }

        report.bytes_written = sink.bytes_written();
        report.crc_mismatch = sink.take_crc_mismatch();
        report.elapsed = start.elapsed();
        Ok(report)
    }

    fn test(
        &mut self,
        cancel: &CancelToken,
        sink: &mut dyn ExtractSink,
    ) -> Result<ExtractReport, ArchiveError> {
        let start = Instant::now();
        let mut report = ExtractReport::default();
        let mut archive = self.open_processing()?;
        let mut index = 0u32;

        while let Some(header) = archive.read_header().map_err(map_error)? {
            let id = index;
            index = index.saturating_add(1);

            if cancel.is_cancelled() {
                return Err(ArchiveError::Cancelled);
            }

            let Some(entry) = self.entries.get(id as usize).cloned() else {
                archive = header.skip().map_err(map_error)?;
                continue;
            };

            if entry.kind.is_dir() {
                archive = header.skip().map_err(map_error)?;
                continue;
            }

            if entry.size > MAX_IN_MEMORY {
                report.failed.push((
                    id,
                    format!(
                        "RAR 条目超过 {} MiB，暂不支持完整性测试",
                        MAX_IN_MEMORY / (1024 * 1024)
                    ),
                ));
                archive = header.skip().map_err(map_error)?;
                continue;
            }

            if let Err(err) = sink.on_entry_begin(id, &entry) {
                report.failed.push((id, err.to_string()));
                archive = header.skip().map_err(map_error)?;
                continue;
            }

            match header.read() {
                Ok((data, next)) => {
                    let mut ok = true;
                    for chunk in data.chunks(READ_BUF) {
                        if cancel.is_cancelled() {
                            let _ = sink.on_entry_end(id, false);
                            return Err(ArchiveError::Cancelled);
                        }
                        if let Err(err) = sink.on_data(id, chunk) {
                            report.failed.push((id, err.to_string()));
                            ok = false;
                            break;
                        }
                    }
                    archive = next;
                    finish_entry(sink, id, ok, &mut report)?;
                }
                Err(err) => {
                    let mapped = map_error(err);
                    let _ = sink.on_entry_end(id, false);
                    if matches!(
                        mapped,
                        ArchiveError::PasswordRequired | ArchiveError::BadPassword
                    ) {
                        return Err(mapped);
                    }
                    report.failed.push((id, mapped.to_string()));
                    archive = self.skip_to(index)?;
                }
            }
        }

        report.bytes_written = sink.bytes_written();
        report.crc_mismatch = sink.take_crc_mismatch();
        report.elapsed = start.elapsed();
        Ok(report)
    }
}

fn finish_entry(
    sink: &mut dyn ExtractSink,
    id: EntryId,
    ok: bool,
    report: &mut ExtractReport,
) -> Result<(), ArchiveError> {
    match sink.on_entry_end(id, ok) {
        Ok(()) if ok => {
            report.succeeded.push(id);
            Ok(())
        }
        Ok(()) => Ok(()),
        Err(err) => {
            report.failed.push((id, err.to_string()));
            Ok(())
        }
    }
}

fn is_symlink(file_attr: u32) -> bool {
    file_attr & 0o170000 == 0o120000
}

fn map_error(err: unrar_ng::error::UnrarError) -> ArchiveError {
    use unrar_ng::error::Code;
    match err.code {
        Code::MissingPassword => ArchiveError::PasswordRequired,
        Code::BadPassword => ArchiveError::BadPassword,
        Code::BadArchive | Code::UnknownFormat | Code::EOpen => ArchiveError::Open(err.to_string()),
        other => ArchiveError::Other(format!("RAR 处理失败: {other:?}")),
    }
}

/// RAR4 的 `file_time` 是 MS-DOS 日期时间；按墙钟数字转成 SystemTime。
fn dos_to_system_time(dos: u32) -> Option<SystemTime> {
    let second = (dos & 0x1F) * 2;
    let minute = (dos >> 5) & 0x3F;
    let hour = (dos >> 11) & 0x1F;
    let day = (dos >> 16) & 0x1F;
    let month = (dos >> 21) & 0x0F;
    let year = 1980 + ((dos >> 25) & 0x7F) as i64;

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }

    let days = days_from_civil(year, month as i64, day as i64);
    let secs = days * 86_400 + i64::from(hour) * 3_600 + i64::from(minute) * 60 + i64::from(second);
    if secs < 0 {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_secs(secs as u64))
}

/// Howard Hinnant 的 civil_from_days 逆向算法。
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::{days_from_civil, dos_to_system_time, is_symlink};

    #[test]
    fn converts_dos_time() {
        // 2021-04-05 06:07:08
        let dos = ((2021 - 1980) << 25) | (4 << 21) | (5 << 16) | (6 << 11) | (7 << 5) | (8 / 2);
        let time = dos_to_system_time(dos).unwrap();
        let secs = time
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(secs, 1_617_602_828);
    }

    #[test]
    fn detects_unix_symlink_mode() {
        assert!(is_symlink(0o120777));
        assert!(!is_symlink(0o100644));
        assert!(!is_symlink(0x20));
    }

    #[test]
    fn civil_days_matches_epoch() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 1, 1), 10957);
    }
}
