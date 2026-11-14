use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::backend::common::{detect_conflict, ratio_suspicious};
use crate::backend::decoder::open_reader;
use crate::error::{ArchiveError, FailReason};
use crate::limits::Limits;
use crate::plan::{
    ConflictKind, DecisionTable, ExtractPlan, PlanAction, PlanWarning, PlannedItem, ResolvedAction,
    resolve_all,
};
use crate::reader::{ArchiveReader, ExtractReport, ExtractSink};
use crate::safety::path_sanitize::{resolve_under, sanitize_rel};
use crate::task::CancelToken;
use crate::types::{ArchiveEntry, ArchiveKind, EntryId, EntryKind, Method};

const READ_BUF: usize = 64 * 1024;

/// 单文件流容器（.gz / .bz2 / .xz / .zst / .lz4，内层不是 tar）。
pub struct StreamReader {
    path: PathBuf,
    kind: ArchiveKind,
    entries: Vec<ArchiveEntry>,
}

impl StreamReader {
    pub fn new(path: &Path, kind: ArchiveKind) -> Result<Self, ArchiveError> {
        let meta = std::fs::metadata(path)?;
        let display = stream_name(path);
        let size = decompressed_size(path, kind)?;

        let mut entry = ArchiveEntry::new(0, display.as_bytes(), display.as_str(), EntryKind::File);
        entry.size = size;
        entry.packed = Some(meta.len());
        entry.crc32 = None;
        entry.mtime = meta.modified().ok();
        entry.encrypted = false;
        entry.method = method_for(kind);

        Ok(Self {
            path: path.to_path_buf(),
            kind,
            entries: vec![entry],
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ArchiveReader for StreamReader {
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
        let mut plan = ExtractPlan {
            free_space: crate::safety::available_space(target),
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

            let rel = sanitize_rel(&entry.display, limits).map_err(|issue| {
                ArchiveError::Other(FailReason::UnsafeName {
                    name: entry.display.to_string(),
                    issue,
                })
            })?;
            let abs = resolve_under(target, &rel, limits)
                .map_err(|issue| ArchiveError::Other(FailReason::UnsafeTarget { issue }))?;
            item.rel_path = abs.clone();

            if let Some(conflict) = detect_conflict(id, &abs, entry.kind)? {
                if matches!(conflict.kind, ConflictKind::ReparsePoint) {
                    item.action = PlanAction::Reject(FailReason::TargetIsReparsePoint);
                } else {
                    item.action = PlanAction::Overwrite;
                }
                plan.conflicts.push(conflict);
            }

            plan.total_bytes = plan.total_bytes.saturating_add(entry.size);
            plan.largest_entry = plan.largest_entry.max(entry.size);
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
        let entry = self.entries[0].clone();

        let Some(action) = actions.get(&entry.id).cloned() else {
            report.elapsed = start.elapsed();
            return Ok(report);
        };
        match action {
            ResolvedAction::Skip => return Ok(report),
            ResolvedAction::Reject(reason) => {
                report.failed.push((entry.id, reason));
                return Ok(report);
            }
            ResolvedAction::Write { .. } => {}
        }

        if let Err(err) = sink.on_entry_begin(entry.id, &entry) {
            report.failed.push((entry.id, err.reason()));
            report.elapsed = start.elapsed();
            return Ok(report);
        }
        let mut reader = open_reader(&self.path, self.kind)?;
        let mut buf = vec![0u8; READ_BUF];
        let mut ok = true;

        loop {
            if cancel.is_cancelled() {
                let _ = sink.on_entry_end(entry.id, false);
                return Err(ArchiveError::Cancelled);
            }
            match reader.read(&mut buf) {
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
                        FailReason::EntryExtractFailed {
                            detail: err.to_string(),
                        },
                    ));
                    ok = false;
                    break;
                }
            }
        }

        match sink.on_entry_end(entry.id, ok) {
            Ok(()) if ok => report.succeeded.push(entry.id),
            Ok(()) => {}
            Err(err) => report.failed.push((entry.id, err.reason())),
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
        let entry = self.entries[0].clone();
        let mut report = ExtractReport::default();
        let mut reader = open_reader(&self.path, self.kind)?;
        let mut buf = vec![0u8; READ_BUF];

        if let Err(err) = sink.on_entry_begin(entry.id, &entry) {
            report.failed.push((entry.id, err.reason()));
            report.elapsed = start.elapsed();
            return Ok(report);
        }
        let mut ok = true;
        loop {
            if cancel.is_cancelled() {
                let _ = sink.on_entry_end(entry.id, false);
                return Err(ArchiveError::Cancelled);
            }
            match reader.read(&mut buf) {
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
                        FailReason::EntryVerifyFailed {
                            detail: err.to_string(),
                        },
                    ));
                    ok = false;
                    break;
                }
            }
        }
        match sink.on_entry_end(entry.id, ok) {
            Ok(()) if ok => report.succeeded.push(entry.id),
            Ok(()) => {}
            Err(err) => report.failed.push((entry.id, err.reason())),
        }

        report.bytes_written = sink.bytes_written();
        report.crc_mismatch = sink.take_crc_mismatch();
        report.elapsed = start.elapsed();
        Ok(report)
    }
}

fn decompressed_size(path: &Path, kind: ArchiveKind) -> Result<u64, ArchiveError> {
    let mut reader = open_reader(path, kind)?;
    std::io::copy(&mut reader, &mut std::io::sink()).map_err(|err| ArchiveError::CorruptEntry {
        entry: None,
        cause: FailReason::StreamReadFailed {
            detail: err.to_string(),
        },
    })
}

fn stream_name(path: &Path) -> String {
    let mut name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data".to_owned());
    let lower = name.to_ascii_lowercase();
    for ext in [
        ".gzip", ".gz", ".bzip2", ".bz2", ".bz", ".xz", ".zstd", ".zst", ".lz4",
    ] {
        if lower.ends_with(ext) {
            name.truncate(name.len() - ext.len());
            break;
        }
    }
    if name.is_empty() {
        name = "data".to_owned();
    }
    name
}

fn method_for(kind: ArchiveKind) -> Method {
    match kind {
        ArchiveKind::Gz => Method::Gzip,
        ArchiveKind::Bz2 => Method::Bzip2,
        ArchiveKind::Xz => Method::Xz,
        ArchiveKind::Zstd => Method::Zstd,
        ArchiveKind::Lz4 => Method::Lz4,
        _ => Method::Unknown(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_compression_extension() {
        assert_eq!(stream_name(Path::new("a.txt.gz")), "a.txt");
        assert_eq!(stream_name(Path::new("a.tar.zst")), "a.tar");
        assert_eq!(stream_name(Path::new(".gz")), "data");
    }
}
