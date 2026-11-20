use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use zip::extra_fields::ExtraField;

use crate::backend::NameOptions;
use crate::backend::common::{detect_conflict, ratio_suspicious};
use crate::backend::name_codec::{self, NameEncoding, NameOverride};
use crate::error::{ArchiveError, FailReason};
use crate::limits::Limits;
use crate::plan::{
    ConflictKind, DecisionTable, ExtractPlan, PlanAction, PlanWarning, PlannedItem, resolve_all,
};
use crate::reader::{ArchiveReader, ExtractReport, ExtractSink, NameReport, SkipReason};
use crate::safety::available_space;
use crate::safety::display_guard::guard_display;
use crate::safety::path_sanitize::{resolve_under, sanitize_rel};
use crate::secret::SecretString;
use crate::task::CancelToken;
use crate::types::{ArchiveEntry, ArchiveKind, EntryId, EntryKind, Method};

const READ_BUF: usize = 64 * 1024;

pub struct ZipReader {
    path: PathBuf,
    archive: Option<zip::ZipArchive<ZipSource>>,
    entries: Vec<ArchiveEntry>,
    password: Option<SecretString>,
}

/// zip 条目读取器。LZMA（方法 14）由 lzma-rust2 自行解码：
/// zip crate 的 LZMA 解压器不认识 ZIP 内部头，会把合法条目误判为格式错误。
trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

/// 小包整体读入内存，避免逐条目 seek 触发系统调用与杀软扫描。
type ZipSource = Box<dyn ReadSeek + Send>;

const SMALL_ARCHIVE_LIMIT: u64 = 8 * 1024 * 1024;

enum ZipEntryReader<'a> {
    Plain(Box<zip::read::ZipFile<'a, ZipSource>>),
    Lzma(Box<lzma_rust2::LzmaReader<zip::read::ZipFile<'a, ZipSource>>>),
    Xz(Box<liblzma::bufread::XzDecoder<BufReader<zip::read::ZipFile<'a, ZipSource>>>>),
}

impl std::io::Read for ZipEntryReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(reader) => reader.read(buf),
            Self::Lzma(reader) => reader.read(buf),
            Self::Xz(reader) => reader.read(buf),
        }
    }
}

impl ZipReader {
    pub fn new(
        path: &Path,
        password: Option<&SecretString>,
        options: NameOptions,
    ) -> Result<Self, ArchiveError> {
        let mut file =
            File::open(path).map_err(|err| ArchiveError::Open(FailReason::Raw(err.to_string())))?;
        let size = file.metadata().map(|meta| meta.len()).unwrap_or(u64::MAX);
        let source: ZipSource = if size <= SMALL_ARCHIVE_LIMIT {
            let mut bytes = Vec::with_capacity(size as usize);
            file.read_to_end(&mut bytes)?;
            Box::new(Cursor::new(bytes))
        } else {
            Box::new(BufReader::with_capacity(256 * 1024, file))
        };
        let mut archive = zip::ZipArchive::new(source).map_err(map_open_error)?;

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
            // zip crate 9.x 的 name() 已处理 bit 11 与 Info-ZIP Unicode Path（0x7075，
            // 含严格一致性校验），name_raw() 则保留扩展应用前的原始字节。
            // 这两路是权威名字，优先于打分判定；只有 raw 强制才绕过。
            let has_unicode_path = file
                .extra_data_fields()
                .any(|field| matches!(field, ExtraField::UnicodePath(_)));
            let authoritative = file.name().ok().map(|name| name.into_owned());
            let raw_is_utf8 = std::str::from_utf8(&raw_name).is_ok();
            let (display, encoding, confidence) = if options.encoding == NameOverride::Raw {
                let guess = name_codec::decode_with(&raw_name, NameOverride::Raw);
                (guess.text, guess.encoding, guess.confidence)
            } else if let Some(name) = authoritative
                && (has_unicode_path || raw_is_utf8)
            {
                (name, NameEncoding::Utf8, 100)
            } else {
                let guess = name_codec::decode_with(&raw_name, options.encoding);
                (guess.text, guess.encoding, guess.confidence)
            };

            let encrypted = file.encrypted();
            let kind = entry_kind(&file);
            let mut entry = ArchiveEntry::new(index as EntryId, &raw_name, display.as_str(), kind);
            entry.name_encoding = encoding;
            entry.name_confidence = confidence;
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

    fn archive_mut(&mut self) -> Result<&mut zip::ZipArchive<ZipSource>, ArchiveError> {
        self.archive.as_mut().ok_or_else(|| {
            ArchiveError::Other(FailReason::ArchiveClosed {
                format: "zip".to_owned(),
            })
        })
    }
}

impl ArchiveReader for ZipReader {
    fn kind(&self) -> ArchiveKind {
        ArchiveKind::Zip
    }

    fn entries(&self) -> &[ArchiveEntry] {
        &self.entries
    }

    fn comment(&self) -> Option<String> {
        let raw = self.archive.as_ref()?.comment();
        if raw.is_empty() {
            return None;
        }
        Some(String::from_utf8_lossy(raw).into_owned())
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
            root: target.to_path_buf(),
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

            if entry.name_confidence < 60 {
                plan.warnings.push(PlanWarning::NameEncodingAmbiguous {
                    id,
                    confidence: entry.name_confidence,
                });
            }

            if matches!(entry.kind, EntryKind::Symlink | EntryKind::Hardlink) {
                plan.warnings.push(PlanWarning::SymlinkSkipped { id });
                item.action = PlanAction::Skip;
                plan.items.push(item);
                continue;
            }

            // 明确不支持的方法在预检阶段提示并跳过，避免解压到一半才失败。
            //
            // 加密条目不再列入：WinZip AES 条目的压缩方法记作 99，真实方法在
            // 0x9901 extra field 里，zip crate 会用真实方法覆盖它并按
            // 「先解密、再按真实方法解压」走完，因此 AES + LZMA / AES + XZ
            // 这些组合现在都能正常解开。
            let unsupported = match entry.method {
                Method::Unknown(code) => code != 99,
                _ => false,
            };
            if unsupported {
                plan.warnings.push(PlanWarning::UnsupportedMethod {
                    id,
                    method: entry.method,
                });
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

        plan.warnings
            .extend(crate::backend::common::plan_safety_warnings(
                target,
                &plan.items,
                plan.total_bytes,
                std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0),
                limits,
            ));
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
        let mut report = ExtractReport {
            name: NameReport {
                encoding: self
                    .entries
                    .iter()
                    .find(|entry| entry.name_encoding != NameEncoding::Utf8)
                    .map(|entry| entry.name_encoding)
                    .unwrap_or(NameEncoding::Utf8),
                confidence: self
                    .entries
                    .iter()
                    .map(|entry| entry.name_confidence)
                    .min()
                    .unwrap_or(100),
                renamed: 0,
                undecodable: self
                    .entries
                    .iter()
                    .filter(|entry| entry.name_confidence < 60)
                    .count(),
                samples: self
                    .entries
                    .iter()
                    .take(20)
                    .map(|entry| {
                        (
                            name_codec::escape_raw(&entry.raw_name),
                            entry.display.to_string(),
                        )
                    })
                    .collect(),
            },
            ..Default::default()
        };
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
                report.failed.push((item.id, err.reason()));
                continue;
            }

            if entry.kind.is_dir() {
                finish_entry(sink, item.id, true, &mut report)?;
                continue;
            }

            let mut zip_file = match self.open_entry(item.id, plan.limits.max_dict_size) {
                Ok(file) => file,
                Err(err) => {
                    let _ = sink.on_entry_end(item.id, false);
                    report.failed.push((item.id, err.reason()));
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
                            report.failed.push((item.id, err.reason()));
                            entry_ok = false;
                            break;
                        }
                    }
                    Err(err) => {
                        report.failed.push((
                            item.id,
                            FailReason::EntryReadFailed {
                                detail: err.to_string(),
                            },
                        ));
                        entry_ok = false;
                        break;
                    }
                }
            }

            finish_entry(sink, item.id, entry_ok, &mut report)?;
        }

        let _ = sink.finish();
        report.bytes_written = sink.bytes_written();
        report.crc_mismatch = sink.take_crc_mismatch();
        report.elapsed = start.elapsed();
        Ok(report)
    }

    fn test(
        &mut self,
        limits: &Limits,
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
                report.failed.push((entry.id, err.reason()));
                continue;
            }

            let mut zip_file = match self.open_entry(entry.id, limits.max_dict_size) {
                Ok(file) => file,
                Err(err) => {
                    let _ = sink.on_entry_end(entry.id, false);
                    report.failed.push((entry.id, err.reason()));
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
                            report.failed.push((entry.id, err.reason()));
                            ok = false;
                            break;
                        }
                    }
                    Err(err) => {
                        report.failed.push((
                            entry.id,
                            FailReason::EntryCrcFailed {
                                detail: err.to_string(),
                            },
                        ));
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
        max_dict_size: u64,
    ) -> Result<ZipEntryReader<'_>, ArchiveError> {
        let index = id as usize;
        let method = self.entries[index].method;
        let size = self.entries[index].size;
        let encrypted = self.entries[index].encrypted;

        // 加密条目一律交给 zip crate 处理。
        //
        // WinZip AES 规范里，加密条目的压缩方法记作 99，真实方法藏在 0x9901
        // extra field 里；zip crate 解析时会用真实方法覆盖 99，然后按
        // 「先 AES 解密、再按真实方法解压」的顺序走完。走 crate 这条路才能
        // 支持 AES + LZMA / AES + XZ 这类组合——项目自己的解码器拿不到
        // 明文流，因为 AES 解密封装在 crate 内部。
        //
        // 未加密的 LZMA / XZ 仍然走项目自己的解码器：这样字典大小上限、
        // 进度节流这些策略由本项目掌握，不依赖 crate 的行为。
        if encrypted {
            let password = self.password.clone();
            let Some(password) = password else {
                return Err(ArchiveError::PasswordRequired);
            };
            return self
                .archive_mut()?
                .by_index_decrypt(index, password.expose().as_bytes())
                .map(|file| ZipEntryReader::Plain(Box::new(file)))
                .map_err(map_entry_error);
        }

        if matches!(method, Method::Xz) {
            let raw = self
                .archive_mut()?
                .by_index_raw(index)
                .map_err(map_entry_error)?;
            let stream =
                liblzma::stream::Stream::new_stream_decoder(max_dict_size, 0).map_err(|err| {
                    ArchiveError::Other(FailReason::Raw(format!("XZ 解码器初始化失败: {err}")))
                })?;
            return Ok(ZipEntryReader::Xz(Box::new(
                liblzma::bufread::XzDecoder::new_stream(BufReader::new(raw), stream),
            )));
        }

        if matches!(method, Method::Lzma) {
            let mut raw = self
                .archive_mut()?
                .by_index_raw(index)
                .map_err(map_entry_error)?;

            // ZIP LZMA 头：2 字节版本 + 2 字节属性长度 + 5 字节 LZMA 属性 + 数据。
            let mut header = [0u8; 4];
            raw.read_exact(&mut header).map_err(ArchiveError::Io)?;
            let props_size = usize::from(u16::from_le_bytes([header[2], header[3]]));
            if !(5..=64).contains(&props_size) {
                return Err(ArchiveError::Unsupported(FailReason::CorruptLzmaHeader));
            }
            let mut props = vec![0u8; props_size];
            raw.read_exact(&mut props).map_err(ArchiveError::Io)?;

            let dict_size = u32::from_le_bytes([props[1], props[2], props[3], props[4]]);
            if u64::from(dict_size) > max_dict_size {
                return Err(ArchiveError::LimitExceeded(
                    crate::safety::bomb::LimitHit::DictionarySize,
                ));
            }

            let reader =
                lzma_rust2::LzmaReader::new_with_props(raw, size, props[0], dict_size, None)
                    .map_err(|err| {
                        ArchiveError::Other(FailReason::Raw(format!(
                            "LZMA decoder init failed: {err}"
                        )))
                    })?;
            return Ok(ZipEntryReader::Lzma(Box::new(reader)));
        }

        self.archive_mut()?
            .by_index(index)
            .map(|file| ZipEntryReader::Plain(Box::new(file)))
            .map_err(map_entry_error)
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

fn entry_kind(file: &zip::read::ZipFile<'_, ZipSource>) -> EntryKind {
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
        zip::CompressionMethod::Deflate64 => Method::Deflate64,
        zip::CompressionMethod::Bzip2 => Method::Bzip2,
        zip::CompressionMethod::Zstd => Method::Zstd,
        zip::CompressionMethod::Ppmd => Method::Ppmd,
        // zip crate 启用了 lzma / xz feature 时，这两种方法会走到专门的
        // 枚举分支而不是 Unsupported(id)。未启用时它们是 Unsupported(14/95)，
        // 下面一并处理，两条路都归到同一个 Method。
        zip::CompressionMethod::Lzma => Method::Lzma,
        zip::CompressionMethod::Xz => Method::Xz,
        // 方法 99 表示「加密」，真实压缩方法在 0x9901 extra field 里。
        // zip crate 解析时会用真实方法覆盖掉 99；只有在 extra field 缺失
        // 或格式异常时才会看到 99 本身。
        zip::CompressionMethod::Aes => Method::Unknown(99),
        // 未启用的压缩方法会落到 Unsupported(id)，按 zip 规范编号还原。
        zip::CompressionMethod::Unsupported(id) => match id {
            14 => Method::Lzma,
            95 => Method::Xz,
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
            ArchiveError::Unsupported(FailReason::Raw(reason.to_owned()))
        }
        zip::result::ZipError::InvalidArchive(reason) => {
            ArchiveError::Open(FailReason::Raw(reason.into_owned()))
        }
        zip::result::ZipError::FileNotFound => {
            ArchiveError::Open(FailReason::CentralDirectoryMissing)
        }
        zip::result::ZipError::Io(err) => ArchiveError::Io(err),
        other => ArchiveError::Open(FailReason::Raw(other.to_string())),
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
