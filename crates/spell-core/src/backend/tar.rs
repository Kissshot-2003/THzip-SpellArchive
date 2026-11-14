use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant, UNIX_EPOCH};

use crate::backend::common::detect_conflict;
use crate::backend::decoder::open_reader;
use crate::error::{ArchiveError, FailReason};
use crate::limits::Limits;
use crate::plan::{
    ConflictKind, DecisionTable, ExtractPlan, PlanAction, PlanWarning, PlannedItem, ResolvedAction,
    resolve_all,
};
use crate::reader::{ArchiveReader, ExtractReport, ExtractSink, SkipReason};
use crate::safety::available_space;
use crate::safety::display_guard::guard_display;
use crate::safety::path_sanitize::{resolve_under, sanitize_rel};
use crate::task::CancelToken;
use crate::types::{ArchiveEntry, ArchiveKind, EntryId, EntryKind, Method};

const READ_BUF: usize = 64 * 1024;

pub struct TarReader {
    path: std::path::PathBuf,
    kind: ArchiveKind,
    entries: Vec<ArchiveEntry>,
}

impl TarReader {
    pub fn new(path: &Path, container: ArchiveKind) -> Result<Self, ArchiveError> {
        let mut archive = tar::Archive::new(open_reader(path, container)?);
        let mut entries = Vec::new();
        let mut id = 0 as EntryId;

        for item in archive.entries().map_err(map_io)? {
            let entry = item.map_err(map_entry_error)?;
            let Some(display) = entry_display(&entry) else {
                continue;
            };
            let header = entry.header();
            let kind = entry_kind(header.entry_type());
            let mut archive_entry =
                ArchiveEntry::new(id, display.as_bytes(), display.as_str(), kind);
            archive_entry.size = if kind.is_dir() {
                0
            } else {
                header.size().unwrap_or(0)
            };
            archive_entry.packed = None;
            archive_entry.crc32 = None;
            archive_entry.encrypted = false;
            archive_entry.method = method_for(container, kind);
            archive_entry.mtime = header
                .mtime()
                .ok()
                .map(|secs| UNIX_EPOCH + Duration::from_secs(secs));
            entries.push(archive_entry);
            id = id.saturating_add(1);
        }

        Ok(Self {
            path: path.to_path_buf(),
            kind: container,
            entries,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ArchiveReader for TarReader {
    fn kind(&self) -> ArchiveKind {
        self.kind
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
            ..Default::default()
        };

        for &id in ids {
            let entry = self
                .entries
                .get(id as usize)
                .ok_or(ArchiveError::Other(FailReason::EntryMissing { id }))?;

            let mut item = PlannedItem {
                id,
                rel_path: std::path::PathBuf::new(),
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

            if matches!(
                entry.kind,
                EntryKind::Symlink | EntryKind::Hardlink | EntryKind::Other
            ) {
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
        let mut archive = tar::Archive::new(open_reader(&self.path, self.kind)?);
        let mut buf = vec![0u8; READ_BUF];
        let mut id = 0 as EntryId;

        let entries = archive.entries().map_err(map_io)?;
        for item in entries {
            let mut entry = item.map_err(map_entry_error)?;
            if entry_display(&entry).is_none() {
                continue;
            }
            if id as usize >= self.entries.len() {
                break;
            }
            let ours = self.entries[id as usize].clone();
            id = id.saturating_add(1);

            if cancel.is_cancelled() {
                return Err(ArchiveError::Cancelled);
            }

            let Some(action) = actions.get(&ours.id).cloned() else {
                drain(&mut entry);
                continue;
            };

            match action {
                ResolvedAction::Skip => {
                    drain(&mut entry);
                    report.skipped.push((ours.id, SkipReason::PolicySkip));
                    continue;
                }
                ResolvedAction::Reject(reason) => {
                    drain(&mut entry);
                    report.failed.push((ours.id, reason));
                    continue;
                }
                ResolvedAction::Write { .. } => {}
            }

            if let Err(err) = sink.on_entry_begin(ours.id, &ours) {
                report.failed.push((ours.id, err.reason()));
                drain(&mut entry);
                continue;
            }

            if ours.kind.is_dir() {
                record_entry(sink, ours.id, true, &mut report);
                continue;
            }

            let mut ok = true;
            loop {
                if cancel.is_cancelled() {
                    let _ = sink.on_entry_end(ours.id, false);
                    return Err(ArchiveError::Cancelled);
                }
                match entry.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(err) = sink.on_data(ours.id, &buf[..n]) {
                            report.failed.push((ours.id, err.reason()));
                            ok = false;
                            break;
                        }
                    }
                    Err(err) => {
                        report
                            .failed
                            .push((ours.id, FailReason::EntryReadFailed { detail: err.to_string() }));
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                drain(&mut entry);
            }
            record_entry(sink, ours.id, ok, &mut report);
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
        let mut archive = tar::Archive::new(open_reader(&self.path, self.kind)?);
        let mut buf = vec![0u8; READ_BUF];
        let mut id = 0 as EntryId;

        for item in archive.entries().map_err(map_io)? {
            let mut entry = item.map_err(map_entry_error)?;
            if entry_display(&entry).is_none() {
                continue;
            }
            let Some(ours) = self.entries.get(id as usize).cloned() else {
                break;
            };
            id = id.saturating_add(1);

            if cancel.is_cancelled() {
                return Err(ArchiveError::Cancelled);
            }
            if ours.kind.is_dir() {
                drain(&mut entry);
                continue;
            }
            if let Err(err) = sink.on_entry_begin(ours.id, &ours) {
                report.failed.push((ours.id, err.reason()));
                drain(&mut entry);
                continue;
            }

            let mut ok = true;
            loop {
                if cancel.is_cancelled() {
                    let _ = sink.on_entry_end(ours.id, false);
                    return Err(ArchiveError::Cancelled);
                }
                match entry.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(err) = sink.on_data(ours.id, &buf[..n]) {
                            report.failed.push((ours.id, err.reason()));
                            ok = false;
                            break;
                        }
                    }
                    Err(err) => {
                        report
                            .failed
                            .push((ours.id, FailReason::EntryReadFailed { detail: err.to_string() }));
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                drain(&mut entry);
            }
            record_entry(sink, ours.id, ok, &mut report);
        }

        report.bytes_written = sink.bytes_written();
        report.crc_mismatch = sink.take_crc_mismatch();
        report.elapsed = start.elapsed();
        Ok(report)
    }
}

fn record_entry(sink: &mut dyn ExtractSink, id: EntryId, ok: bool, report: &mut ExtractReport) {
    match sink.on_entry_end(id, ok) {
        Ok(()) if ok => report.succeeded.push(id),
        Ok(()) => {}
        Err(err) => report.failed.push((id, err.reason())),
    }
}

fn entry_display(entry: &tar::Entry<'_, Box<dyn Read>>) -> Option<String> {
    let path = entry.path().ok()?;
    let text = path.to_string_lossy();
    let text = text
        .as_ref()
        .strip_prefix("./")
        .unwrap_or(text.as_ref())
        .trim_end_matches('/');
    if text.is_empty() {
        None
    } else {
        Some(text.to_owned())
    }
}

fn entry_kind(kind: tar::EntryType) -> EntryKind {
    if kind.is_dir() {
        EntryKind::Dir
    } else if kind.is_symlink() {
        EntryKind::Symlink
    } else if kind.is_hard_link() {
        EntryKind::Hardlink
    } else if kind.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

fn method_for(container: ArchiveKind, entry: EntryKind) -> Method {
    if entry.is_dir() {
        return Method::Store;
    }
    match container {
        ArchiveKind::Tar => Method::Store,
        ArchiveKind::TarGz => Method::Gzip,
        ArchiveKind::TarBz2 => Method::Bzip2,
        ArchiveKind::TarXz => Method::Xz,
        ArchiveKind::TarZst => Method::Zstd,
        ArchiveKind::TarLz4 => Method::Lz4,
        _ => Method::Unknown(0),
    }
}

fn drain(reader: &mut dyn Read) {
    let _ = std::io::copy(reader, &mut std::io::sink());
}

fn map_io(err: std::io::Error) -> ArchiveError {
    ArchiveError::Io(err)
}

fn map_entry_error(err: std::io::Error) -> ArchiveError {
    ArchiveError::CorruptEntry {
        entry: None,
        cause: FailReason::EntryReadFailed {
            detail: err.to_string(),
        },
    }
}
