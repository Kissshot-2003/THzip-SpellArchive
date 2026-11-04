use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::backend::name_codec;
use crate::error::ArchiveError;
use crate::limits::Limits;
use crate::plan::{
    Conflict, ConflictKind, DecisionTable, ExtractPlan, PlanAction, PlanWarning, PlannedItem,
    resolve_all,
};
use crate::reader::{ArchiveReader, ExtractReport, ExtractSink, SkipReason};
use crate::safety::display_guard::guard_display;
use crate::safety::path_sanitize::{resolve_under, sanitize_rel};
use crate::safety::{available_space, is_reparse_point};
use crate::secret::SecretString;
use crate::task::CancelToken;
use crate::types::{ArchiveEntry, ArchiveKind, EntryId, EntryKind, Method};

const READ_BUF: usize = 64 * 1024;

pub struct ZipReader {
    path: PathBuf,
    archive: Option<zip::ZipArchive<BufReader<File>>>,
    entries: Vec<ArchiveEntry>,
    password: Option<SecretString>,
}

impl ZipReader {
    pub fn new(path: &Path, password: Option<&SecretString>) -> Result<Self, ArchiveError> {
        let file = File::open(path).map_err(|err| ArchiveError::Open(err.to_string()))?;
        let reader = BufReader::with_capacity(256 * 1024, file);
        let mut archive = zip::ZipArchive::new(reader).map_err(map_open_error)?;

        let total = archive.len();
        let mut entries = Vec::with_capacity(total);

        for index in 0..total {
            let file = match archive.by_index_raw(index) {
                Ok(file) => file,
                // 头加密的 zip（少见）需要密码才能读元数据。
                Err(zip::result::ZipError::UnsupportedArchive(reason))
                    if reason == zip::result::ZipError::PASSWORD_REQUIRED =>
                {
                    return Err(ArchiveError::PasswordRequired);
                }
                Err(err) => return Err(map_open_error(err)),
            };

            let raw_name = file.name_raw().to_vec();
            let decoded = match name_codec::decode_strict_utf8(&raw_name) {
                Some(text) => name_codec::DecodedName {
                    text,
                    encoding: name_codec::NameEncoding::Utf8,
                    lossy: false,
                },
                None => name_codec::decode(&raw_name),
            };

            let encrypted = file.encrypted();
            let kind = entry_kind(&file);
            let mut entry =
                ArchiveEntry::new(index as EntryId, &raw_name, decoded.text.as_str(), kind);
            entry.size = if kind.is_dir() { 0 } else { file.size() };
            entry.packed = Some(file.compressed_size());
            // WinZip AES 的 CRC 字段按规范写 0，完整性由 AE-2 认证码保证，
            // 不能再拿 0 去和实际 CRC 比较，否则所有加密条目都会误报。
            let crc = file.crc32();
            entry.crc32 = if kind.is_dir() || (encrypted && crc == 0) {
                None
            } else {
                Some(crc)
            };
            entry.mtime = file.last_modified().and_then(dos_to_system_time);
            entry.encrypted = encrypted;
            entry.method = map_method(file.compression());
            entries.push(entry);
        }

        Ok(Self {
            path: path.to_path_buf(),
            archive: Some(archive),
            entries,
            password: password.cloned(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn archive_mut(&mut self) -> Result<&mut zip::ZipArchive<BufReader<File>>, ArchiveError> {
        self.archive
            .as_mut()
            .ok_or_else(|| ArchiveError::Other("zip 归档已关闭".to_owned()))
    }
}

impl ArchiveReader for ZipReader {
    fn kind(&self) -> ArchiveKind {
        ArchiveKind::Zip
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
                    // 暂定为覆盖，最终由 DecisionTable 决定。
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
        let mut buf = vec![0u8; READ_BUF];

        for item in &plan.items {
            if cancel.is_cancelled() {
                return Err(ArchiveError::Cancelled);
            }

            let action = match actions.get(&item.id) {
                Some(action) => action.clone(),
                None => continue,
            };
            if let crate::plan::ResolvedAction::Skip = action {
                report.skipped.push((item.id, SkipReason::PolicySkip));
                continue;
            }
            if let crate::plan::ResolvedAction::Reject(reason) = action {
                report.failed.push((item.id, reason));
                continue;
            }

            let entry = self.entries[item.id as usize].clone();
            if let Err(err) = sink.on_entry_begin(item.id, &entry) {
                report.failed.push((item.id, err.to_string()));
                continue;
            }

            if entry.kind.is_dir() {
                finish_entry(sink, item.id, true, &mut report)?;
                continue;
            }

            let mut zip_file = match self.open_entry(item.id) {
                Ok(file) => file,
                Err(err) => {
                    let _ = sink.on_entry_end(item.id, false);
                    report.failed.push((item.id, err.to_string()));
                    continue;
                }
            };

            let mut entry_ok = true;
            loop {
                if cancel.is_cancelled() {
                    let _ = sink.on_entry_end(item.id, false);
                    return Err(ArchiveError::Cancelled);
                }
                match zip_file.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(err) = sink.on_data(item.id, &buf[..n]) {
                            report.failed.push((item.id, err.to_string()));
                            entry_ok = false;
                            break;
                        }
                    }
                    Err(err) => {
                        report
                            .failed
                            .push((item.id, format!("读取条目失败: {err}")));
                        entry_ok = false;
                        break;
                    }
                }
            }

            finish_entry(sink, item.id, entry_ok, &mut report)?;
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
        let mut buf = vec![0u8; READ_BUF];

        for entry in self.entries.clone() {
            if cancel.is_cancelled() {
                return Err(ArchiveError::Cancelled);
            }
            if entry.kind.is_dir() {
                continue;
            }

            if let Err(err) = sink.on_entry_begin(entry.id, &entry) {
                report.failed.push((entry.id, err.to_string()));
                continue;
            }

            let mut zip_file = match self.open_entry(entry.id) {
                Ok(file) => file,
                Err(err) => {
                    let _ = sink.on_entry_end(entry.id, false);
                    report.failed.push((entry.id, err.to_string()));
                    continue;
                }
            };

            let mut ok = true;
            loop {
                if cancel.is_cancelled() {
                    let _ = sink.on_entry_end(entry.id, false);
                    return Err(ArchiveError::Cancelled);
                }
                match zip_file.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(err) = sink.on_data(entry.id, &buf[..n]) {
                            report.failed.push((entry.id, err.to_string()));
                            ok = false;
                            break;
                        }
                    }
                    Err(err) => {
                        report
                            .failed
                            .push((entry.id, format!("CRC/数据校验失败: {err}")));
                        ok = false;
                        break;
                    }
                }
            }

            finish_entry(sink, entry.id, ok, &mut report)?;
        }

        report.bytes_written = sink.bytes_written();
        report.crc_mismatch = sink.take_crc_mismatch();
        report.elapsed = start.elapsed();
        Ok(report)
    }
}

impl ZipReader {
    fn open_entry(
        &mut self,
        id: EntryId,
    ) -> Result<zip::read::ZipFile<'_, BufReader<File>>, ArchiveError> {
        let index = id as usize;
        let encrypted = self.entries[index].encrypted;

        let password = self.password.clone();
        match (encrypted, password.as_ref()) {
            (true, Some(password)) => self
                .archive_mut()?
                .by_index_decrypt(index, password.expose().as_bytes())
                .map_err(map_entry_error),
            (true, None) => Err(ArchiveError::PasswordRequired),
            (false, _) => self.archive_mut()?.by_index(index).map_err(map_entry_error),
        }
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

fn detect_conflict(
    id: EntryId,
    path: &Path,
    kind: EntryKind,
) -> Result<Option<Conflict>, ArchiveError> {
    if is_reparse_point(path)? {
        return Ok(Some(Conflict {
            id,
            path: path.to_path_buf(),
            kind: ConflictKind::ReparsePoint,
        }));
    }

    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(ArchiveError::Io(err)),
    };

    let existing_is_dir = meta.is_dir();
    let conflict_kind = if existing_is_dir && kind.is_dir() {
        return Ok(None);
    } else if existing_is_dir {
        ConflictKind::ExistingDir
    } else {
        ConflictKind::ExistingFile
    };

    Ok(Some(Conflict {
        id,
        path: path.to_path_buf(),
        kind: conflict_kind,
    }))
}

fn entry_kind(file: &zip::read::ZipFile<'_, BufReader<File>>) -> EntryKind {
    if let Some(mode) = file.unix_mode() {
        match mode & 0o170000 {
            0o120000 => return EntryKind::Symlink,
            0o040000 => return EntryKind::Dir,
            0o100000 => {}
            _ => {}
        }
    }
    if file.is_dir() {
        EntryKind::Dir
    } else {
        EntryKind::File
    }
}

#[allow(deprecated)]
fn map_method(method: zip::CompressionMethod) -> Method {
    match method {
        zip::CompressionMethod::Stored => Method::Store,
        zip::CompressionMethod::Deflated => Method::Deflate,
        zip::CompressionMethod::Aes => Method::Unknown(99),
        // 未启用的压缩方法会落到 Unsupported(id)，按 zip 规范编号还原。
        zip::CompressionMethod::Unsupported(id) => match id {
            9 => Method::Deflate64,
            12 => Method::Bzip2,
            14 => Method::Lzma,
            93 => Method::Zstd,
            95 => Method::Xz,
            98 => Method::Ppmd,
            other => Method::Unknown(other),
        },
        other => Method::Unknown(other.to_u16()),
    }
}

fn map_open_error(err: zip::result::ZipError) -> ArchiveError {
    match err {
        zip::result::ZipError::InvalidPassword => ArchiveError::BadPassword,
        zip::result::ZipError::UnsupportedArchive(reason)
            if reason == zip::result::ZipError::PASSWORD_REQUIRED =>
        {
            ArchiveError::PasswordRequired
        }
        zip::result::ZipError::UnsupportedArchive(reason) => {
            ArchiveError::Unsupported(reason.to_owned())
        }
        zip::result::ZipError::InvalidArchive(reason) => ArchiveError::Open(reason.into_owned()),
        zip::result::ZipError::FileNotFound => {
            ArchiveError::Open("中央目录中找不到条目".to_owned())
        }
        zip::result::ZipError::Io(err) => ArchiveError::Io(err),
        other => ArchiveError::Open(other.to_string()),
    }
}

fn map_entry_error(err: zip::result::ZipError) -> ArchiveError {
    match err {
        zip::result::ZipError::InvalidPassword => ArchiveError::BadPassword,
        zip::result::ZipError::UnsupportedArchive(reason)
            if reason == zip::result::ZipError::PASSWORD_REQUIRED =>
        {
            ArchiveError::PasswordRequired
        }
        other => map_open_error(other),
    }
}

/// zip 的 DOS 时间是无时区的本地墙钟时间。这里按「墙钟数字」转成
/// `SystemTime`（当作 UTC 存放），显示层用同样的语义格式化即可保持数字不变。
fn dos_to_system_time(dt: zip::DateTime) -> Option<SystemTime> {
    let (year, month, day) = (dt.year() as i64, dt.month() as i64, dt.day() as i64);
    let (hour, minute, second) = (dt.hour() as i64, dt.minute() as i64, dt.second() as i64);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hour * 3_600 + minute * 60 + second;
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
