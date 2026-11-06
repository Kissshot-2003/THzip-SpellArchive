use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use crate::backend::common::detect_conflict;
use crate::error::ArchiveError;
use crate::limits::Limits;
use crate::plan::{
    ConflictKind, DecisionTable, ExtractPlan, PlanAction, PlanWarning, PlannedItem, ResolvedAction,
    resolve_all,
};
use crate::reader::{ArchiveReader, ExtractReport, ExtractSink, SkipReason};
use crate::safety::available_space;
use crate::safety::display_guard::guard_display;
use crate::safety::path_sanitize::{resolve_under, sanitize_rel};
use crate::secret::SecretString;
use crate::task::CancelToken;
use crate::types::{ArchiveEntry, ArchiveKind, EntryId, EntryKind, Method};

const READ_BUF: usize = 64 * 1024;

/// 7z 后端：`sevenz-rust2` 负责解析与解码；写盘仍然只经过 `ExtractSink`。
pub struct SevenZReader {
    path: PathBuf,
    archive: sevenz_rust2::Archive,
    entries: Vec<ArchiveEntry>,
    password: Option<SecretString>,
}

impl SevenZReader {
    pub fn new(path: &Path, password: Option<&SecretString>) -> Result<Self, ArchiveError> {
        let mut file = File::open(path)?;
        let sz_password = to_sz_password(password);
        let archive = sevenz_rust2::Archive::read(&mut file, &sz_password).map_err(map_sz_error)?;

        let mut entries = Vec::with_capacity(archive.files.len());
        for (file_index, file) in archive.files.iter().enumerate() {
            // 7z 常带一个名字为空的根目录条目；跳过它，避免出现空白行。
            if file.name.is_empty() {
                continue;
            }
            let id = entries.len() as EntryId;
            entries.push(build_entry(&archive, id, file_index, file));
        }

        Ok(Self {
            path: path.to_path_buf(),
            archive,
            entries,
            password: password.cloned(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn open_decoder(&self) -> Result<sevenz_rust2::ArchiveReader<File>, ArchiveError> {
        sevenz_rust2::ArchiveReader::open(&self.path, to_sz_password(self.password.as_ref()))
            .map_err(map_sz_error)
    }
}

impl ArchiveReader for SevenZReader {
    fn kind(&self) -> ArchiveKind {
        ArchiveKind::SevenZ
    }

    fn entries(&self) -> &[ArchiveEntry] {
        &self.entries
    }

    fn is_solid(&self) -> bool {
        self.archive.is_solid
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
        if plan.needs_password && self.password.is_none() {
            return Err(ArchiveError::PasswordRequired);
        }

        let start = Instant::now();
        let actions = resolve_all(plan, decisions);
        let ids: HashMap<String, EntryId> = self
            .entries
            .iter()
            .map(|entry| (entry.display.to_string(), entry.id))
            .collect();
        let mut report = ExtractReport::default();
        let mut cancelled = false;
        let mut decoder = self.open_decoder()?;
        let mut buf = vec![0u8; READ_BUF];

        let result = decoder.for_each_entries(|sz_entry, data| {
            if cancel.is_cancelled() {
                cancelled = true;
                return Ok(false);
            }

            let Some(&id) = ids.get(&sz_entry.name) else {
                drain(data);
                return Ok(true);
            };
            let action = actions.get(&id).cloned();
            let Some(action) = action else {
                drain(data);
                return Ok(true);
            };

            match action {
                ResolvedAction::Skip => {
                    drain(data);
                    report.skipped.push((id, SkipReason::PolicySkip));
                    return Ok(true);
                }
                ResolvedAction::Reject(reason) => {
                    drain(data);
                    report.failed.push((id, reason));
                    return Ok(true);
                }
                ResolvedAction::Write { .. } => {}
            }

            let ours = self.entries[id as usize].clone();
            if let Err(err) = sink.on_entry_begin(id, &ours) {
                report.failed.push((id, err.to_string()));
                drain(data);
                return Ok(true);
            }

            let mut ok = true;
            loop {
                if cancel.is_cancelled() {
                    cancelled = true;
                    let _ = sink.on_entry_end(id, false);
                    return Ok(false);
                }
                match data.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(err) = sink.on_data(id, &buf[..n]) {
                            report.failed.push((id, err.to_string()));
                            ok = false;
                            break;
                        }
                    }
                    Err(err) => {
                        report.failed.push((id, format!("读取条目失败: {err}")));
                        ok = false;
                        break;
                    }
                }
            }

            if !ok {
                // solid 包必须把当前子流读完，否则后续条目会错位。
                drain(data);
            }

            match sink.on_entry_end(id, ok) {
                Ok(()) if ok => report.succeeded.push(id),
                Ok(()) => {}
                Err(err) => report.failed.push((id, err.to_string())),
            }
            Ok(true)
        });

        if cancelled {
            return Err(ArchiveError::Cancelled);
        }
        result.map_err(map_sz_error)?;

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
        let encrypted = self.entries.iter().any(|entry| entry.encrypted);
        if encrypted && self.password.is_none() {
            return Err(ArchiveError::PasswordRequired);
        }

        let start = Instant::now();
        let ids: HashMap<String, EntryId> = self
            .entries
            .iter()
            .map(|entry| (entry.display.to_string(), entry.id))
            .collect();
        let mut report = ExtractReport::default();
        let mut cancelled = false;
        let mut decoder = self.open_decoder()?;
        let mut buf = vec![0u8; READ_BUF];

        let result = decoder.for_each_entries(|sz_entry, data| {
            if cancel.is_cancelled() {
                cancelled = true;
                return Ok(false);
            }
            let Some(&id) = ids.get(&sz_entry.name) else {
                drain(data);
                return Ok(true);
            };
            let ours = &self.entries[id as usize];
            if ours.kind.is_dir() {
                drain(data);
                return Ok(true);
            }

            if let Err(err) = sink.on_entry_begin(id, ours) {
                report.failed.push((id, err.to_string()));
                drain(data);
                return Ok(true);
            }

            let mut ok = true;
            loop {
                if cancel.is_cancelled() {
                    cancelled = true;
                    let _ = sink.on_entry_end(id, false);
                    return Ok(false);
                }
                match data.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(err) = sink.on_data(id, &buf[..n]) {
                            report.failed.push((id, err.to_string()));
                            ok = false;
                            break;
                        }
                    }
                    Err(err) => {
                        report.failed.push((id, format!("读取条目失败: {err}")));
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                drain(data);
            }

            match sink.on_entry_end(id, ok) {
                Ok(()) if ok => report.succeeded.push(id),
                Ok(()) => {}
                Err(err) => report.failed.push((id, err.to_string())),
            }
            Ok(true)
        });

        if cancelled {
            return Err(ArchiveError::Cancelled);
        }
        result.map_err(map_sz_error)?;

        report.bytes_written = sink.bytes_written();
        report.crc_mismatch = sink.take_crc_mismatch();
        report.elapsed = start.elapsed();
        Ok(report)
    }
}

fn to_sz_password(password: Option<&SecretString>) -> sevenz_rust2::Password {
    match password {
        Some(password) => sevenz_rust2::Password::from(password.expose()),
        None => sevenz_rust2::Password::empty(),
    }
}

fn build_entry(
    archive: &sevenz_rust2::Archive,
    id: EntryId,
    file_index: usize,
    file: &sevenz_rust2::ArchiveEntry,
) -> ArchiveEntry {
    let kind = if file.is_directory {
        EntryKind::Dir
    } else {
        EntryKind::File
    };
    let mut entry = ArchiveEntry::new(id, file.name.as_bytes(), file.name.as_str(), kind);
    entry.size = if kind.is_dir() { 0 } else { file.size };
    entry.packed = if archive.is_solid {
        None
    } else {
        Some(file.compressed_size)
    };
    entry.crc32 = if file.has_crc && !kind.is_dir() {
        Some(file.crc as u32)
    } else {
        None
    };
    entry.mtime = file
        .has_last_modified_date
        .then(|| SystemTime::from(file.last_modified_date));
    entry.method = method_for(archive, file_index);
    entry.encrypted = encrypted_for(archive, file_index);
    entry
}

fn method_for(archive: &sevenz_rust2::Archive, index: usize) -> Method {
    use sevenz_rust2::EncoderMethod as Id;
    let Some(block_index) = archive.stream_map.file_block_index[index] else {
        return Method::Store;
    };
    for coder in &archive.blocks[block_index].coders {
        let id = coder.encoder_method_id();
        if id == Id::ID_COPY {
            return Method::Store;
        }
        if id == Id::ID_LZMA {
            return Method::Lzma;
        }
        if id == Id::ID_LZMA2 {
            return Method::Lzma2;
        }
        if id == Id::ID_BZIP2 {
            return Method::Bzip2;
        }
        if id == Id::ID_PPMD {
            return Method::Ppmd;
        }
        if id == Id::ID_DEFLATE {
            return Method::Deflate;
        }
        if id == Id::ID_DEFLATE64 {
            return Method::Deflate64;
        }
        if id == Id::ID_ZSTD {
            return Method::Zstd;
        }
        if id == Id::ID_LZ4 {
            return Method::Lz4;
        }
    }
    Method::Unknown(0)
}

fn encrypted_for(archive: &sevenz_rust2::Archive, index: usize) -> bool {
    let Some(block_index) = archive.stream_map.file_block_index[index] else {
        return false;
    };
    archive.blocks[block_index]
        .coders
        .iter()
        .any(|coder| coder.encoder_method_id() == sevenz_rust2::EncoderMethod::ID_AES256_SHA256)
}

fn drain(data: &mut dyn Read) {
    let _ = std::io::copy(data, &mut std::io::sink());
}

fn map_sz_error(err: sevenz_rust2::Error) -> ArchiveError {
    match err {
        sevenz_rust2::Error::PasswordRequired => ArchiveError::PasswordRequired,
        sevenz_rust2::Error::MaybeBadPassword(_) => ArchiveError::BadPassword,
        sevenz_rust2::Error::ChecksumVerificationFailed => ArchiveError::CorruptEntry {
            entry: None,
            detail: "7z 数据校验失败".to_owned(),
        },
        sevenz_rust2::Error::Io(err, _) => ArchiveError::Io(err),
        sevenz_rust2::Error::FileOpen(err, path) => ArchiveError::Open(format!("{path}: {err}")),
        sevenz_rust2::Error::BadSignature(_) => ArchiveError::Open("7z 签名无效".to_owned()),
        sevenz_rust2::Error::UnsupportedCompressionMethod(method) => {
            ArchiveError::Unsupported(format!("7z 压缩方法不支持: {method}"))
        }
        other => ArchiveError::Open(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::VerifySink;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "spellarchive-7z-{tag}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn make_archive(dir: &Path) -> PathBuf {
        let src = dir.join("src");
        std::fs::create_dir_all(src.join("子目录")).unwrap();
        std::fs::write(src.join("a.txt"), b"hello 7z").unwrap();
        std::fs::write(src.join("子目录/b.bin"), [1u8, 2, 3, 4]).unwrap();
        let archive = dir.join("sample.7z");
        sevenz_rust2::compress_to_path(&src, &archive).unwrap();
        archive
    }

    #[test]
    fn lists_and_extracts() {
        let dir = TempDir::new("read");
        let archive = make_archive(dir.path());

        let mut reader = SevenZReader::new(&archive, None).unwrap();
        assert!(!reader.entries().is_empty());
        assert!(
            reader
                .entries()
                .iter()
                .any(|entry| entry.display.contains("a.txt"))
        );

        let target = dir.path().join("out");
        let limits = Limits::default();
        let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
        let plan = reader.plan(&ids, &target, &limits).unwrap();
        let decisions = DecisionTable {
            default: crate::plan::OverwritePolicy::Overwrite,
            ..Default::default()
        };
        let mut sink = crate::safety::ExtractDirSink::new(&plan, &decisions, limits);
        let report = reader
            .extract(&plan, &decisions, &CancelToken::new(), &mut sink)
            .unwrap();
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        let mut found = false;
        for entry in walk(&target) {
            if entry.ends_with("a.txt") {
                assert_eq!(std::fs::read(&entry).unwrap(), b"hello 7z");
                found = true;
            }
        }
        assert!(found, "未找到解压出的 a.txt");
    }

    fn walk(root: &Path) -> Vec<PathBuf> {
        let mut result = Vec::new();
        let Ok(read) = std::fs::read_dir(root) else {
            return result;
        };
        for item in read.flatten() {
            let path = item.path();
            if path.is_dir() {
                result.extend(walk(&path));
            } else {
                result.push(path);
            }
        }
        result
    }

    #[test]
    fn extracts_single_entry_from_solid_archive() {
        let dir = TempDir::new("single");
        let archive = make_archive(dir.path());
        let mut reader = SevenZReader::new(&archive, None).unwrap();
        let target = dir.path().join("single-out");
        let limits = Limits::default();
        let target_id = reader
            .entries()
            .iter()
            .find(|entry| entry.display.ends_with("b.bin"))
            .unwrap()
            .id;
        let plan = reader.plan(&[target_id], &target, &limits).unwrap();
        let decisions = DecisionTable {
            default: crate::plan::OverwritePolicy::Overwrite,
            ..Default::default()
        };
        let mut sink = crate::safety::ExtractDirSink::new(&plan, &decisions, limits);
        let report = reader
            .extract(&plan, &decisions, &CancelToken::new(), &mut sink)
            .unwrap();
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        let files = walk(&target);
        assert_eq!(files.len(), 1, "{files:?}");
        assert!(files[0].ends_with("b.bin"));
        assert_eq!(std::fs::read(&files[0]).unwrap(), vec![1u8, 2, 3, 4]);
    }

    #[test]
    fn encrypted_archive_requires_password() {
        let dir = TempDir::new("encrypted");
        make_archive(dir.path());
        let encrypted = dir.path().join("enc.7z");
        sevenz_rust2::compress_to_path_encrypted(
            dir.path().join("src"),
            &encrypted,
            sevenz_rust2::Password::from("hunter2"),
        )
        .unwrap();

        let missing = SevenZReader::new(&encrypted, None);
        assert!(matches!(missing, Err(ArchiveError::PasswordRequired)));

        let password = SecretString::new("hunter2");
        let mut reader = SevenZReader::new(&encrypted, Some(&password)).unwrap();
        assert!(
            reader
                .entries()
                .iter()
                .any(|entry| entry.display.contains("a.txt"))
        );

        let target = dir.path().join("enc-out");
        let limits = Limits::default();
        let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
        let plan = reader.plan(&ids, &target, &limits).unwrap();
        let decisions = DecisionTable {
            default: crate::plan::OverwritePolicy::Overwrite,
            ..Default::default()
        };
        let mut sink = crate::safety::ExtractDirSink::new(&plan, &decisions, limits);
        let report = reader
            .extract(&plan, &decisions, &CancelToken::new(), &mut sink)
            .unwrap();
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert!(walk(&target).iter().any(|path| path.ends_with("a.txt")));
    }

    #[test]
    fn test_verifies_checksum() {
        let dir = TempDir::new("test");
        let archive = make_archive(dir.path());
        let mut reader = SevenZReader::new(&archive, None).unwrap();
        let mut sink = VerifySink::with_limits(Limits::default());
        let report = reader.test(&CancelToken::new(), &mut sink).unwrap();
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert!(report.succeeded.len() >= 2);
        assert!(report.crc_mismatch.is_empty());
    }
}
