use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::backend;
use crate::error::{ArchiveError, FailReason};
use crate::limits::Limits;
use crate::plan::{DecisionTable, OverwritePolicy};
use crate::safety::{ExtractDirSink, TempDir, write_atomic};
use crate::task::CancelToken;
use crate::types::{ArchiveEntry, ArchiveKind, EntryId, Method};
use crate::writer::{FinishReport, normalize_entry_name};

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
    rewrite_with_names(
        path,
        kind,
        remove,
        &HashMap::new(),
        limits,
        cancel,
        progress,
    )
}

/// 与 [`rewrite_without`] 相同，但可同时把保留条目改成新名字。
///
/// `rename` 以 `EntryId` 为键；目录改名时调用方必须把其子条目也一并映射，
/// 否则子条目仍会写到旧路径。
pub fn rewrite_with_names(
    path: &Path,
    kind: ArchiveKind,
    remove: &HashSet<EntryId>,
    rename: &HashMap<EntryId, String>,
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
            return Err(ArchiveError::Unsupported(FailReason::RewriteUnsupported {
                format: other.label().to_owned(),
            }));
        }
    }

    let mut reader = backend::open(path, None)?;
    if reader.entries().iter().any(|entry| entry.encrypted) {
        return Err(ArchiveError::Unsupported(FailReason::EncryptedRewrite));
    }

    let entries = reader.entries().to_vec();
    let finals = final_names(&entries, remove, rename)?;
    let keep: Vec<EntryId> = entries
        .iter()
        .filter(|entry| !remove.contains(&entry.id))
        .map(|entry| entry.id)
        .collect();

    let workdir = TempDir::create("thzip-spellarchive-rewrite")?;
    let tmp_out = temp_output(path);
    let result = (|| -> Result<FinishReport, ArchiveError> {
        let plan = reader.plan(&keep, workdir.path(), &limits)?;
        let decisions = DecisionTable {
            default: OverwritePolicy::Overwrite,
            ..Default::default()
        };
        let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
        let report = reader.extract(&plan, &decisions, cancel, &mut sink)?;
        if !report.failed.is_empty() {
            return Err(ArchiveError::Other(FailReason::RewriteExtractFailed {
                count: report.failed.len(),
            }));
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
            let entry = entries.get(item.id as usize).ok_or(ArchiveError::Other(
                FailReason::EntryMissing { id: item.id },
            ))?;
            let final_name = finals.get(&item.id).ok_or(ArchiveError::Other(
                FailReason::RenameTargetMissing { id: item.id },
            ))?;
            if entry.kind.is_dir() {
                writer.add_dir(&item.rel_path, final_name)?;
                continue;
            }
            let base = done;
            let method = if entry.method == Method::Store {
                Method::Store
            } else {
                Method::Deflate
            };
            writer.add(&item.rel_path, final_name, Some(method), cancel, &|bytes| {
                progress(base.saturating_add(bytes))
            })?;
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

    let _ = std::fs::remove_file(&tmp_out);
    result
}

/// 计算保留条目的最终归档名，并做重名 / 目录自嵌套检查。
fn final_names(
    entries: &[ArchiveEntry],
    remove: &HashSet<EntryId>,
    rename: &HashMap<EntryId, String>,
) -> Result<HashMap<EntryId, String>, ArchiveError> {
    let mut finals = HashMap::with_capacity(entries.len());
    let mut seen: HashSet<String> = HashSet::with_capacity(entries.len());
    let check_duplicates = !rename.is_empty();

    for entry in entries {
        if remove.contains(&entry.id) {
            continue;
        }
        let final_name = match rename.get(&entry.id) {
            Some(name) => {
                let name = normalize_entry_name(name)?;
                if entry.kind.is_dir() {
                    let original = entry.display.trim_end_matches('/');
                    if name.starts_with(&format!("{original}/")) {
                        return Err(ArchiveError::Other(FailReason::MoveIntoSelf {
                            name: original.to_owned(),
                        }));
                    }
                }
                name
            }
            None => entry.display.to_string(),
        };

        if check_duplicates && !seen.insert(final_name.to_lowercase()) {
            return Err(ArchiveError::Other(FailReason::RenameDuplicate {
                name: final_name,
            }));
        }
        finals.insert(entry.id, final_name);
    }
    Ok(finals)
}

fn temp_output(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    parent.join(format!(".thzip-spellarchive-rewrite-{nanos}.tmp"))
}

#[cfg(test)]
mod tests {
    use super::{rewrite_with_names, rewrite_without};
    use crate::backend;
    use crate::limits::Limits;
    use crate::task::CancelToken;
    use crate::types::{ArchiveKind, EntryId};
    use std::collections::{HashMap, HashSet};
    use std::io::Write;
    use std::path::Path;

    fn write_zip(path: &Path) {
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

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "thzip-spellarchive-rewrite-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn id_of(archive: &Path, name: &str) -> EntryId {
        let reader = backend::open(archive, None).unwrap();
        reader
            .entries()
            .iter()
            .find(|entry| entry.display.as_ref() == name)
            .map(|entry| entry.id)
            .unwrap_or_else(|| panic!("missing entry {name}"))
    }

    fn names(archive: &Path) -> Vec<String> {
        let reader = backend::open(archive, None).unwrap();
        reader
            .entries()
            .iter()
            .map(|entry| entry.display.to_string())
            .collect()
    }

    #[test]
    fn removes_selected_entries_and_preserves_others() {
        let dir = temp_dir("remove");
        let archive = dir.join("a.zip");
        write_zip(&archive);

        let remove: HashSet<EntryId> = HashSet::from([id_of(&archive, "b.txt")]);
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

        let names = names(&archive);
        assert!(names.contains(&"a.txt".to_owned()));
        assert!(names.contains(&"c/d.txt".to_owned()));
        assert!(!names.contains(&"b.txt".to_owned()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn renames_entries_and_preserves_content() {
        let dir = temp_dir("rename");
        let archive = dir.join("a.zip");
        write_zip(&archive);

        let rename = HashMap::from([(id_of(&archive, "b.txt"), "docs/b2.txt".to_owned())]);
        rewrite_with_names(
            &archive,
            ArchiveKind::Zip,
            &HashSet::new(),
            &rename,
            Limits::default(),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();

        let names = names(&archive);
        assert!(names.contains(&"docs/b2.txt".to_owned()));
        assert!(!names.contains(&"b.txt".to_owned()));
        assert!(names.contains(&"a.txt".to_owned()));

        let extract = dir.join("out");
        let mut reader = backend::open(&archive, None).unwrap();
        let ids: Vec<EntryId> = reader.entries().iter().map(|entry| entry.id).collect();
        let plan = reader.plan(&ids, &extract, &Limits::default()).unwrap();
        let mut sink = crate::safety::ExtractDirSink::new(
            &plan,
            &crate::plan::DecisionTable {
                default: crate::plan::OverwritePolicy::Overwrite,
                ..Default::default()
            },
            Limits::default(),
        );
        reader
            .extract(
                &plan,
                &crate::plan::DecisionTable {
                    default: crate::plan::OverwritePolicy::Overwrite,
                    ..Default::default()
                },
                &CancelToken::new(),
                &mut sink,
            )
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(extract.join("docs/b2.txt")).unwrap(),
            "bbb"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn moves_directory_with_its_children() {
        let dir = temp_dir("move");
        let archive = dir.join("a.zip");
        write_zip(&archive);

        let rename = HashMap::from([(id_of(&archive, "c/d.txt"), "moved/c/d.txt".to_owned())]);
        rewrite_with_names(
            &archive,
            ArchiveKind::Zip,
            &HashSet::new(),
            &rename,
            Limits::default(),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();

        let names = names(&archive);
        assert!(names.contains(&"moved/c/d.txt".to_owned()));
        assert!(!names.contains(&"c/d.txt".to_owned()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_duplicate_and_nested_renames() {
        let dir = temp_dir("bad");
        let archive = dir.join("a.zip");
        write_zip(&archive);

        let duplicate = HashMap::from([(id_of(&archive, "a.txt"), "b.txt".to_owned())]);
        let err = rewrite_with_names(
            &archive,
            ArchiveKind::Zip,
            &HashSet::new(),
            &duplicate,
            Limits::default(),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap_err();
        assert!(err.to_string().contains("重名"));

        let nested = HashMap::from([(id_of(&archive, "c/d.txt"), "../x.txt".to_owned())]);
        let err = rewrite_with_names(
            &archive,
            ArchiveKind::Zip,
            &HashSet::new(),
            &nested,
            Limits::default(),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap_err();
        assert!(err.to_string().contains(".."));

        // 原归档保持不变。
        let names = names(&archive);
        assert!(names.contains(&"a.txt".to_owned()));
        assert!(names.contains(&"b.txt".to_owned()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
