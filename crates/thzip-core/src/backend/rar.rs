use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use unrar_ng::{Archive, CursorBeforeHeader, OpenArchive, Process};

use crate::backend::common::{detect_conflict, ratio_suspicious};
use crate::error::{ArchiveError, FailReason};
use crate::limits::Limits;
use crate::plan::{
    ConflictKind, DecisionTable, ExtractPlan, PlanAction, PlanWarning, PlannedItem, resolve_all,
};
use crate::reader::{ArchiveReader, ExtractReport, ExtractSink, SkipReason};
use crate::safety::TempDir;
use crate::safety::available_space;
use crate::safety::display_guard::guard_display;
use crate::safety::path_sanitize::{resolve_under, sanitize_rel};
use crate::secret::SecretString;
use crate::task::CancelToken;
use crate::types::{ArchiveEntry, ArchiveKind, EntryId, EntryKind, Method};

const READ_BUF: usize = 64 * 1024;
/// 超过此大小的条目改用临时文件中转，避免整条读进内存。
const MAX_IN_MEMORY: u64 = 512 * 1024 * 1024;

pub struct RarReader {
    path: PathBuf,
    entries: Vec<ArchiveEntry>,
    password: Option<SecretString>,
    staging_limit: u64,
}

impl RarReader {
    pub fn new(path: &Path, password: Option<&SecretString>) -> Result<Self, ArchiveError> {
        Self::new_with_limit(path, password, MAX_IN_MEMORY)
    }

    fn new_with_limit(
        path: &Path,
        password: Option<&SecretString>,
        staging_limit: u64,
    ) -> Result<Self, ArchiveError> {
        let mut entries = Vec::new();
        let mut index = 0u32;

        {
            let archive = Self::archive_for(path, password)
                .open_for_listing()
                .map_err(map_error)?;

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
            staging_limit,
        })
    }

    /// 多卷 RAR 统一从第一卷开始处理。
    fn archive_for<'a>(path: &'a Path, password: Option<&'a SecretString>) -> Archive<'a> {
        let archive = match password {
            Some(password) => Archive::with_password(path, password.expose().as_bytes()),
            None => Archive::new(path),
        };
        if archive.is_multipart() {
            archive.as_first_part()
        } else {
            archive
        }
    }

    fn open_processing(&self) -> Result<OpenArchive<Process, CursorBeforeHeader>, ArchiveError> {
        Self::archive_for(&self.path, self.password.as_ref())
            .open_for_processing()
            .map_err(map_error)
    }

    /// 出错后重新打开并跳过前 `count` 条，继续处理后续条目。
    fn skip_to(
        &self,
        count: u32,
    ) -> Result<OpenArchive<Process, CursorBeforeHeader>, ArchiveError> {
        let mut archive = self.open_processing()?;
        for _ in 0..count {
            let Some(header) = archive.read_header().map_err(map_error)? else {
                return Err(ArchiveError::Other(FailReason::EntryCountMismatch {
                    format: "RAR".to_owned(),
                }));
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
            return Err(ArchiveError::LimitExceeded(
                crate::safety::bomb::LimitHit::EntryCount,
            ));
        }

        let mut plan = ExtractPlan {
            free_space: available_space(target),
            limits: *limits,
            ..Default::default()
        };

        for &id in ids {
            let entry = self
                .entries
                .get(id as usize)
                .ok_or(ArchiveError::Other(FailReason::EntryMissing { id }))?;

            let mut item = PlannedItem {
                id,
                rel_path: PathBuf::new(),
                kind: entry.kind,
                size: entry.size,
                action: PlanAction::Create,
            };

            if ratio_suspicious(entry, limits) {
                plan.warnings.push(PlanWarning::BombSuspected {
                    hit: crate::safety::bomb::LimitHit::Ratio,
                });
                item.action =
                    PlanAction::Reject(FailReason::Limit(crate::safety::bomb::LimitHit::Ratio));
                plan.items.push(item);
                continue;
            }

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
                Err(issue) => {
                    plan.warnings.push(PlanWarning::UnsafePath { id, issue });
                    item.action = PlanAction::Reject(FailReason::UnsafeName {
                        name: entry.display.to_string(),
                        issue,
                    });
                    plan.items.push(item);
                    continue;
                }
            };

            let abs = match resolve_under(target, &rel, limits) {
                Ok(path) => path,
                Err(issue) => {
                    plan.warnings.push(PlanWarning::UnsafePath { id, issue });
                    item.action = PlanAction::Reject(FailReason::UnsafeTarget { issue });
                    plan.items.push(item);
                    continue;
                }
            };
            item.rel_path = abs.clone();

            if let Some(conflict) = detect_conflict(id, &abs, entry.kind)? {
                if matches!(conflict.kind, ConflictKind::ReparsePoint) {
                    item.action = PlanAction::Reject(FailReason::TargetIsReparsePoint);
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
                report.failed.push((id, err.reason()));
                archive = header.skip().map_err(map_error)?;
                continue;
            }

            if entry.kind.is_dir() {
                archive = header.skip().map_err(map_error)?;
                finish_entry(sink, id, true, &mut report)?;
                continue;
            }

            let mut entry_ok = true;
            if entry.size > self.staging_limit {
                // 大条目先解到临时文件，再分块喂给 sink，避免整条读进内存。
                let staging = TempDir::create("thzip-spellarchive-rar")?;
                let temp = staging.path().join(format!("{id}.tmp"));
                let next = match header.extract_to(&temp) {
                    Ok(next) => next,
                    Err(err) => {
                        let mapped = map_error(err);
                        let _ = sink.on_entry_end(id, false);
                        let _ = std::fs::remove_file(&temp);
                        if matches!(
                            mapped,
                            ArchiveError::PasswordRequired | ArchiveError::BadPassword
                        ) {
                            return Err(mapped);
                        }
                        report.failed.push((id, mapped.reason()));
                        archive = self.skip_to(index)?;
                        continue;
                    }
                };

                let mut file = match File::open(&temp) {
                    Ok(file) => file,
                    Err(err) => {
                        let _ = sink.on_entry_end(id, false);
                        let _ = std::fs::remove_file(&temp);
                        report.failed.push((
                            id,
                            FailReason::StagedReadFailed {
                                detail: err.to_string(),
                            },
                        ));
                        archive = next;
                        continue;
                    }
                };
                let mut chunk = vec![0u8; READ_BUF];
                loop {
                    if cancel.is_cancelled() {
                        drop(file);
                        let _ = std::fs::remove_file(&temp);
                        let _ = sink.on_entry_end(id, false);
                        return Err(ArchiveError::Cancelled);
                    }
                    match file.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Err(err) = sink.on_data(id, &chunk[..n]) {
                                report.failed.push((id, err.reason()));
                                entry_ok = false;
                                break;
                            }
                        }
                        Err(err) => {
                            report.failed.push((
                                id,
                                FailReason::StagedReadFailed {
                                    detail: err.to_string(),
                                },
                            ));
                            entry_ok = false;
                            break;
                        }
                    }
                }
                drop(file);
                let _ = std::fs::remove_file(&temp);
                archive = next;
            } else {
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
                        report.failed.push((id, mapped.reason()));
                        archive = self.skip_to(index)?;
                        continue;
                    }
                };

                for chunk in data.chunks(READ_BUF) {
                    if cancel.is_cancelled() {
                        let _ = sink.on_entry_end(id, false);
                        return Err(ArchiveError::Cancelled);
                    }
                    if let Err(err) = sink.on_data(id, chunk) {
                        report.failed.push((id, err.reason()));
                        entry_ok = false;
                        break;
                    }
                }
                archive = next;
            }
            finish_entry(sink, id, entry_ok, &mut report)?;
        }

        report.bytes_written = sink.bytes_written();
        report.crc_mismatch = sink.take_crc_mismatch();
        report.elapsed = start.elapsed();
        Ok(report)
    }

    fn test(
        &mut self,
        _limits: &Limits,
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

            if let Err(err) = sink.on_entry_begin(id, &entry) {
                report.failed.push((id, err.reason()));
                archive = header.skip().map_err(map_error)?;
                continue;
            }

            if entry.size > self.staging_limit {
                let staging = TempDir::create("thzip-spellarchive-rar")?;
                let temp = staging.path().join(format!("{id}.tmp"));
                let next = match header.extract_to(&temp) {
                    Ok(next) => next,
                    Err(err) => {
                        let mapped = map_error(err);
                        let _ = sink.on_entry_end(id, false);
                        let _ = std::fs::remove_file(&temp);
                        if matches!(
                            mapped,
                            ArchiveError::PasswordRequired | ArchiveError::BadPassword
                        ) {
                            return Err(mapped);
                        }
                        report.failed.push((id, mapped.reason()));
                        archive = self.skip_to(index)?;
                        continue;
                    }
                };
                let mut file = match File::open(&temp) {
                    Ok(file) => file,
                    Err(err) => {
                        let _ = sink.on_entry_end(id, false);
                        let _ = std::fs::remove_file(&temp);
                        report.failed.push((
                            id,
                            FailReason::StagedReadFailed {
                                detail: err.to_string(),
                            },
                        ));
                        archive = next;
                        continue;
                    }
                };
                let mut chunk = vec![0u8; READ_BUF];
                let mut ok = true;
                loop {
                    if cancel.is_cancelled() {
                        drop(file);
                        let _ = std::fs::remove_file(&temp);
                        let _ = sink.on_entry_end(id, false);
                        return Err(ArchiveError::Cancelled);
                    }
                    match file.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Err(err) = sink.on_data(id, &chunk[..n]) {
                                report.failed.push((id, err.reason()));
                                ok = false;
                                break;
                            }
                        }
                        Err(err) => {
                            report.failed.push((
                                id,
                                FailReason::StagedReadFailed {
                                    detail: err.to_string(),
                                },
                            ));
                            ok = false;
                            break;
                        }
                    }
                }
                drop(file);
                let _ = std::fs::remove_file(&temp);
                archive = next;
                finish_entry(sink, id, ok, &mut report)?;
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
                            report.failed.push((id, err.reason()));
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
                    report.failed.push((id, mapped.reason()));
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
            report.failed.push((id, err.reason()));
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
        Code::BadArchive | Code::UnknownFormat | Code::EOpen => {
            ArchiveError::Open(FailReason::Raw(err.to_string()))
        }
        other => ArchiveError::Other(FailReason::RarFailed {
            detail: format!("{other:?}"),
        }),
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
    use super::{RarReader, days_from_civil, dos_to_system_time, is_symlink};
    use crate::limits::Limits;
    use crate::plan::{DecisionTable, OverwritePolicy};
    use crate::reader::ArchiveReader;
    use crate::safety::ExtractDirSink;
    use crate::task::CancelToken;

    #[test]
    fn stages_entries_to_temp_file() {
        let archive =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/version.rar");
        // 阈值 0 强制走临时文件中转。
        let mut reader = RarReader::new_with_limit(&archive, None, 0).unwrap();
        let target = std::env::temp_dir().join(format!(
            "thzip-spellarchive-rar-stage-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let ids: Vec<u32> = (0..reader.entries().len() as u32).collect();
        let plan = reader.plan(&ids, &target, &Limits::default()).unwrap();
        let decisions = DecisionTable {
            default: OverwritePolicy::Overwrite,
            ..Default::default()
        };
        let mut sink = ExtractDirSink::new(&plan, &decisions, Limits::default());
        let report = reader
            .extract(&plan, &decisions, &CancelToken::new(), &mut sink)
            .unwrap();
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(
            std::fs::read_to_string(target.join("VERSION")).unwrap(),
            "unrar-0.4.0"
        );
        let _ = std::fs::remove_dir_all(&target);
    }

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
