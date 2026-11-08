use std::collections::HashSet;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::backend;
use crate::error::ArchiveError;
use crate::limits::Limits;
use crate::plan::{DecisionTable, OverwritePolicy};
use crate::safety::{ExtractDirSink, write_atomic};
use crate::task::CancelToken;
use crate::types::{ArchiveKind, EntryId, Method};
use crate::writer::FinishReport;

/// 重写归档：把除 `remove` 外的条目解到临时目录，再写成新包并原子替换原文件。
///
/// 目前只支持未加密的 zip / 7z / tar 家族；加密包需要密码重加密，暂不处理。
pub fn rewrite_without(
    path: &Path,
    kind: ArchiveKind,
    remove: &HashSet<EntryId>,
    limits: Limits,
    cancel: &CancelToken,
    progress: &dyn Fn(u64),
) -> Result<FinishReport, ArchiveError> {
    match kind {
        ArchiveKind::Zip
        | ArchiveKind::SevenZ
        | ArchiveKind::Tar
        | ArchiveKind::TarGz
        | ArchiveKind::TarBz2
        | ArchiveKind::TarXz
        | ArchiveKind::TarZst
        | ArchiveKind::TarLz4 => {}
        other => {
            return Err(ArchiveError::Unsupported(format!(
                "暂不支持修改 {} 归档",
                other.label()
            )));
        }
    }

    let mut reader = backend::open(path, None)?;
    if reader.entries().iter().any(|entry| entry.encrypted) {
        return Err(ArchiveError::Unsupported(
            "加密归档暂不支持删除条目".to_owned(),
        ));
    }

    let keep: Vec<EntryId> = reader
        .entries()
        .iter()
        .filter(|entry| !remove.contains(&entry.id))
        .map(|entry| entry.id)
        .collect();
    let entries = reader.entries().to_vec();

    let workdir = temp_workdir();
    let tmp_out = temp_output(path);
    let result = (|| -> Result<FinishReport, ArchiveError> {
        let plan = reader.plan(&keep, &workdir, &limits)?;
        let decisions = DecisionTable {
            default: OverwritePolicy::Overwrite,
            ..Default::default()
        };
        let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
        let report = reader.extract(&plan, &decisions, cancel, &mut sink)?;
        if !report.failed.is_empty() {
            return Err(ArchiveError::Other(format!(
                "保留条目解压失败 {} 条，已取消重写",
                report.failed.len()
            )));
        }

        // 先关掉源归档句柄，Windows 才能在最后原子替换文件。
        drop(reader);
        if cancel.is_cancelled() {
            return Err(ArchiveError::Cancelled);
        }

        let mut writer = backend::create_writer_with(&tmp_out, kind, Method::Deflate, None)?;
        let mut done = 0u64;
        for item in &plan.items {
            if cancel.is_cancelled() {
                return Err(ArchiveError::Cancelled);
            }
            let entry = entries
                .get(item.id as usize)
                .ok_or_else(|| ArchiveError::Other(format!("条目 {} 不存在", item.id)))?;
            if entry.kind.is_dir() {
                writer.add_dir(&item.rel_path, &entry.display)?;
                continue;
            }
            let base = done;
            let method = if entry.method == Method::Store {
                Method::Store
            } else {
                Method::Deflate
            };
            writer.add(
                &item.rel_path,
                &entry.display,
                Some(method),
                cancel,
                &|bytes| progress(base.saturating_add(bytes)),
            )?;
            done = done.saturating_add(entry.size);
            progress(done);
        }

        let report = writer.finish()?;
        write_atomic(path, |out| {
            let mut source = File::open(&tmp_out)?;
            io::copy(&mut source, out)?;
            Ok(())
        })?;
        Ok(report)
    })();

    let _ = std::fs::remove_dir_all(&workdir);
    let _ = std::fs::remove_file(&tmp_out);
    result
}

fn temp_workdir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "spellarchive-rewrite-{}-{nanos}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&path);
    path
}

fn temp_output(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    parent.join(format!(".spellarchive-rewrite-{nanos}.tmp"))
}

#[cfg(test)]
mod tests {
    use super::rewrite_without;
    use crate::backend;
    use crate::limits::Limits;
    use crate::task::CancelToken;
    use crate::types::{ArchiveKind, EntryId};
    use std::collections::HashSet;
    use std::io::Write;

    fn write_zip(path: &std::path::Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in [
            ("a.txt", &b"aaa"[..]),
            ("b.txt", &b"bbb"[..]),
            ("c/d.txt", &b"ddd"[..]),
        ] {
            writer.start_file(name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn removes_selected_entries_and_preserves_others() {
        let dir = std::env::temp_dir().join(format!(
            "spellarchive-rewrite-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("a.zip");
        write_zip(&archive);

        let reader = backend::open(&archive, None).unwrap();
        let remove: HashSet<EntryId> = reader
            .entries()
            .iter()
            .filter(|entry| entry.display.as_ref() == "b.txt")
            .map(|entry| entry.id)
            .collect();
        drop(reader);

        let report = rewrite_without(
            &archive,
            ArchiveKind::Zip,
            &remove,
            Limits::default(),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();
        assert_eq!(report.entries, 2);

        let reader = backend::open(&archive, None).unwrap();
        let names: Vec<String> = reader
            .entries()
            .iter()
            .map(|entry| entry.display.to_string())
            .collect();
        assert!(names.contains(&"a.txt".to_owned()));
        assert!(names.contains(&"c/d.txt".to_owned()));
        assert!(!names.contains(&"b.txt".to_owned()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
