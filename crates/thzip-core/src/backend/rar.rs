use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use unrar_rs::RarArchive;

use crate::backend::common::{detect_conflict, ratio_suspicious};
use crate::error::{ArchiveError, FailReason};
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

/// 喂给 `unrar-rs` 的 sink 适配器。
///
/// `copy_to` 只需要一个 `Write`，解码器产出的每个数据块直接写进
/// `ExtractSink`——中间不经过 `Vec<u8>`，也不落临时文件。
/// 因此条目大小只受磁盘和限额约束，不再有内存上限。
struct SinkWriter<'a> {
    sink: &'a mut dyn ExtractSink,
    id: EntryId,
    /// sink 第一次报错后的状态：出错即停止写入，但要把错误传回去。
    failed: Option<crate::error::SinkError>,
    cancel: &'a CancelToken,
    cancelled: bool,
}

impl Write for SinkWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.cancelled {
            return Err(std::io::Error::other("cancelled"));
        }
        if self.cancel.is_cancelled() {
            self.cancelled = true;
            return Err(std::io::Error::other("cancelled"));
        }
        if let Some(err) = self.failed.as_ref() {
            return Err(std::io::Error::other(err.to_string()));
        }
        match self.sink.on_data(self.id, buf) {
            Ok(()) => Ok(buf.len()),
            Err(err) => {
                self.failed = Some(err);
                Err(std::io::Error::other("sink write failed"))
            }
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

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
            let file = File::open(path).map_err(|err| {
                ArchiveError::Open(FailReason::Raw(format!("打开 RAR 失败: {err}")))
            })?;
            let mut archive = RarArchive::open(file)
                .map_err(|err| ArchiveError::Open(FailReason::Raw(err.to_string())))?;
            if let Some(password) = password {
                archive.set_password(password.expose());
            }

            // 先把元信息收齐再逐条解码：`by_index` 需要独占的可变借用，
            // 不能在持有 entries 迭代器的同时调用。
            let members: Vec<MemberInfo> = archive
                .entries()
                .map(|member| MemberInfo {
                    name: member.name.clone(),
                    unpacked_size: member.unpacked_size.unwrap_or(0),
                    packed_size: Some(member.compressed_size),
                    crc32: member.crc32,
                    mtime: member.mtime,
                    is_directory: member.is_directory,
                    is_symlink: member.is_symlink,
                    is_encrypted: member.is_encrypted,
                })
                .collect();
            for member in members {
                let display = member.name;
                let raw_name: Vec<u8> = display.as_bytes().to_vec();
                let kind = if member.is_directory {
                    EntryKind::Dir
                } else if member.is_symlink {
                    EntryKind::Symlink
                } else {
                    EntryKind::File
                };
                let mut entry = ArchiveEntry::new(index, &raw_name, display, kind);
                entry.size = if kind.is_dir() {
                    0
                } else {
                    member.unpacked_size
                };
                entry.packed = member.packed_size;
                entry.crc32 = member.crc32;
                entry.mtime = member.mtime;
                entry.encrypted = member.is_encrypted;
                entry.method = Method::Rar;
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

    /// 打开归档并带上密码。
    ///
    /// 每次解压都重新打开：`unrar-rs` 的解码状态（固实字典、卷游标）
    /// 绑在 archive 实例上，不能跨条目复用同一个 handle。
    fn open_archive(&self) -> Result<RarArchive, ArchiveError> {
        let file = File::open(&self.path)
            .map_err(|err| ArchiveError::Open(FailReason::Raw(format!("打开 RAR 失败: {err}"))))?;
        let mut archive = RarArchive::open(file)
            .map_err(|err| ArchiveError::Open(FailReason::Raw(err.to_string())))?;
        if let Some(password) = self.password.as_ref() {
            archive.set_password(password.expose());
        }
        Ok(archive)
    }
}

/// 从 `unrar-rs` 的成员信息里抽出来的字段。
///
/// `MemberInfo` 的字段随版本变动，这里一次性摊平成自己的结构，
/// 后端改动时只动这一处。
#[derive(Debug, Clone)]
struct MemberInfo {
    name: String,
    unpacked_size: u64,
    packed_size: Option<u64>,
    crc32: Option<u32>,
    mtime: Option<SystemTime>,
    is_directory: bool,
    is_symlink: bool,
    is_encrypted: bool,
}

impl From<&unrar_rs::MemberInfo> for MemberInfo {
    fn from(member: &unrar_rs::MemberInfo) -> Self {
        Self {
            name: member.name.clone(),
            unpacked_size: member.unpacked_size.unwrap_or(0),
            packed_size: Some(member.compressed_size),
            crc32: member.crc32,
            mtime: member.mtime,
            is_directory: member.is_directory,
            is_symlink: member.is_symlink,
            is_encrypted: member.is_encrypted,
        }
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
        let mut report = ExtractReport::default();

        let mut archive = self.open_archive()?;

        for id in 0..self.entries.len() as u32 {
            if cancel.is_cancelled() {
                return Err(ArchiveError::Cancelled);
            }

            let Some(entry) = self.entries.get(id as usize).cloned() else {
                continue;
            };
            let Some(action) = actions.get(&id).cloned() else {
                continue;
            };

            match action {
                crate::plan::ResolvedAction::Skip => {
                    report.skipped.push((id, SkipReason::PolicySkip));
                    continue;
                }
                crate::plan::ResolvedAction::Reject(reason) => {
                    report.failed.push((id, reason));
                    continue;
                }
                crate::plan::ResolvedAction::Write { .. } => {}
            }

            if let Err(err) = sink.on_entry_begin(id, &entry) {
                report.failed.push((id, err.reason()));
                continue;
            }

            if entry.kind.is_dir() {
                finish_entry(sink, id, true, &mut report)?;
                continue;
            }

            // 流式解码：解码器每产出一块就写进 sink，不在内存里攒。
            let outcome = self.decode_into(&mut archive, id, sink, cancel);
            if outcome.cancelled {
                let _ = sink.on_entry_end(id, false);
                return Err(ArchiveError::Cancelled);
            }
            match outcome.fatal {
                Some(err) => {
                    // 密码错误要向上抛，让上层弹密码框；其余按条目失败处理。
                    let _ = sink.on_entry_end(id, false);
                    if matches!(
                        err,
                        ArchiveError::PasswordRequired | ArchiveError::BadPassword
                    ) {
                        return Err(err);
                    }
                    report.failed.push((id, err.reason()));
                }
                None => {
                    if let Some(err) = outcome.sink_error {
                        report.failed.push((id, err.reason()));
                        finish_entry(sink, id, false, &mut report)?;
                        continue;
                    }
                    finish_entry(sink, id, true, &mut report)?;
                }
            }
        }

        let _ = sink.finish();
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

        let mut archive = self.open_archive()?;

        for id in 0..self.entries.len() as u32 {
            if cancel.is_cancelled() {
                return Err(ArchiveError::Cancelled);
            }
            let Some(entry) = self.entries.get(id as usize).cloned() else {
                continue;
            };
            if entry.kind.is_dir() {
                continue;
            }

            if let Err(err) = sink.on_entry_begin(id, &entry) {
                report.failed.push((id, err.reason()));
                continue;
            }

            // 测试同样走流式：数据进计数 sink，CRC 由 sink 校验。
            let outcome = self.decode_into(&mut archive, id, sink, cancel);
            if outcome.cancelled {
                let _ = sink.on_entry_end(id, false);
                return Err(ArchiveError::Cancelled);
            }
            match outcome.fatal {
                Some(err) => {
                    let _ = sink.on_entry_end(id, false);
                    if matches!(
                        err,
                        ArchiveError::PasswordRequired | ArchiveError::BadPassword
                    ) {
                        return Err(err);
                    }
                    report.failed.push((id, err.reason()));
                }
                None => {
                    if let Some(err) = outcome.sink_error {
                        report.failed.push((id, err.reason()));
                        finish_entry(sink, id, false, &mut report)?;
                        continue;
                    }
                    finish_entry(sink, id, true, &mut report)?;
                }
            }
        }

        let _ = sink.finish();
        report.bytes_written = sink.bytes_written();
        report.crc_mismatch = sink.take_crc_mismatch();
        report.elapsed = start.elapsed();
        Ok(report)
    }
}

/// 单条目的解码结果。
struct DecodeOutcome {
    /// 解码本身的致命错误（密码、损坏等）。`None` 表示解码本身成功。
    fatal: Option<ArchiveError>,
    /// sink 侧的错误：写盘失败、CRC 不符等。
    sink_error: Option<crate::error::SinkError>,
    /// 用户在解码途中取消。
    cancelled: bool,
}

impl RarReader {
    /// 把一个条目流式写进 sink。
    fn decode_into(
        &self,
        archive: &mut RarArchive,
        id: EntryId,
        sink: &mut dyn ExtractSink,
        cancel: &CancelToken,
    ) -> DecodeOutcome {
        let mut writer = SinkWriter {
            sink,
            id,
            failed: None,
            cancel,
            cancelled: false,
        };
        let result = match archive.by_index(id as usize) {
            Ok(member) => member
                .copy_to(&mut writer)
                .map(|_| ())
                .map_err(|err| map_error(&err)),
            Err(err) => Err(map_error(&err)),
        };
        DecodeOutcome {
            fatal: result.err(),
            sink_error: writer.failed,
            cancelled: writer.cancelled,
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
            report.failed.push((id, err.reason()));
            Ok(())
        }
    }
}

/// 把 `unrar-rs` 的错误映射到项目自己的错误模型。
///
/// 密码相关的错误要单独挑出来——上层靠 `PasswordRequired` 决定要不要弹
/// 密码框，混进通用错误就变成死路。
fn map_error(err: &unrar_rs::RarError) -> ArchiveError {
    let text = err.to_string();
    let lowered = text.to_ascii_lowercase();
    if lowered.contains("encrypted") || lowered.contains("password") {
        // 缺密码与密码错误都归到「需要密码」，由上层重试。
        return ArchiveError::PasswordRequired;
    }
    if lowered.contains("corrupt") || lowered.contains("truncated") {
        return ArchiveError::Open(FailReason::Raw(text));
    }
    ArchiveError::Other(FailReason::RarFailed { detail: text })
}
