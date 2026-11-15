use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::backend;
use crate::error::{ArchiveError, FailReason};
use crate::limits::Limits;
use crate::plan::{DecisionTable, OverwritePolicy, ResolvedAction};
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

        // 实际落盘路径可能与 `plan.items[].rel_path` 不同：包内同名条目会被 sink 改名
        // （a.txt → a~1.txt）。必须用 sink 记录的路径读回来，否则会读到别的条目的内容。
        let written: HashMap<EntryId, PathBuf> = sink
            .actions()
            .iter()
            .filter_map(|(id, action)| match action {
                ResolvedAction::Write { path, .. } => Some((*id, path.clone())),
                _ => None,
            })
            .collect();

        // 符号链接、不支持的方法、不安全路径等条目在解压阶段就被跳过，写回器也无法
        // 表达它们；明确报错，而不是静默从新归档里丢条目。
        let unsupported = plan
            .items
            .iter()
            .filter(|item| !written.contains_key(&item.id))
            .count();
        if unsupported > 0 {
            return Err(ArchiveError::Other(FailReason::RewriteUnsupportedEntries {
                count: unsupported,
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
            let source =
                written
                    .get(&item.id)
                    .ok_or(ArchiveError::Other(FailReason::EntryMissing {
                        id: item.id,
                    }))?;
            if entry.kind.is_dir() {
                writer.add_dir(source, final_name)?;
                continue;
            }
            let base = done;
            let method = if entry.method == Method::Store {
                Method::Store
            } else {
                Method::Deflate
            };
            writer.add(source, final_name, Some(method), cancel, &|bytes| {
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
///
/// 只有**改名引入**的重名才算错误：归档自带的重名条目（7z 允许）应当原样保留，
/// 否则给无关条目改名也会被牵连失败。改名产生的名字若与其他任何条目重名则报错。
fn final_names(
    entries: &[ArchiveEntry],
    remove: &HashSet<EntryId>,
    rename: &HashMap<EntryId, String>,
) -> Result<HashMap<EntryId, String>, ArchiveError> {
    let mut finals = HashMap::with_capacity(entries.len());
    let mut renamed: HashSet<EntryId> = HashSet::new();

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
                renamed.insert(entry.id);
                name
            }
            None => entry.display.to_string(),
        };
        finals.insert(entry.id, final_name);
    }

    // 统计最终名字的出现次数：改名得到的名字必须独占。
    let mut counts: HashMap<String, usize> = HashMap::with_capacity(finals.len());
    for name in finals.values() {
        *counts.entry(name.to_lowercase()).or_insert(0) += 1;
    }
    for (id, name) in &finals {
        if renamed.contains(id) && counts.get(&name.to_lowercase()).copied().unwrap_or(0) > 1 {
            return Err(ArchiveError::Other(FailReason::RenameDuplicate {
                name: name.clone(),
            }));
        }
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
    use crate::types::{ArchiveKind, EntryId, Method};
    use std::collections::{HashMap, HashSet};
    use std::io::Write;
    use std::path::Path;

    fn write_zip(path: &Path) {
        write_zip_named(
            path,
            &[
                ("a.txt", &b"aaa"[..]),
                ("b.txt", &b"bbb"[..]),
                ("c/d.txt", &b"ddd"[..]),
            ],
        );
    }

    fn write_zip_named(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
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

    /// 包内两个条目净化后落到同一路径（尾部句点会被裁剪）时，重写必须按实际落盘
    /// 路径读回内容，不能把第一个条目的内容写给第二个。
    #[test]
    fn rewrite_keeps_content_of_entries_sharing_sanitized_path() {
        let dir = temp_dir("dup");
        let archive = dir.join("a.zip");
        write_zip_named(&archive, &[("a.txt", b"FIRST"), ("a.txt.", b"SECOND")]);

        rewrite_without(
            &archive,
            ArchiveKind::Zip,
            &HashSet::new(),
            Limits::default(),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();

        let extract = dir.join("out");
        let mut reader = backend::open(&archive, None).unwrap();
        let ids: Vec<EntryId> = reader.entries().iter().map(|e| e.id).collect();
        let plan = reader.plan(&ids, &extract, &Limits::default()).unwrap();
        let decisions = crate::plan::DecisionTable {
            default: crate::plan::OverwritePolicy::Overwrite,
            ..Default::default()
        };
        let mut sink = crate::safety::ExtractDirSink::new(&plan, &decisions, Limits::default());
        reader
            .extract(&plan, &decisions, &CancelToken::new(), &mut sink)
            .unwrap();

        assert_eq!(
            std::fs::read(extract.join("a.txt")).unwrap(),
            b"FIRST",
            "第一个条目内容必须保持不变"
        );
        assert_eq!(
            std::fs::read(extract.join("a~1.txt")).unwrap(),
            b"SECOND",
            "第二个条目不得被第一个条目的内容覆盖"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 归档自带的重名条目不应当让无关的改名操作失败。
    ///
    /// 用 7z：它允许同名条目（解压按 id 区分、落盘自动改名），
    /// zip 写入器本身拒绝重名，压根到不了这一层。
    #[test]
    fn rename_survives_preexisting_duplicate_names() {
        let dir = temp_dir("predup");
        let archive = dir.join("a.7z");
        let first = dir.join("first.txt");
        let second = dir.join("second.txt");
        let third = dir.join("third.txt");
        std::fs::write(&first, b"upper").unwrap();
        std::fs::write(&second, b"lower").unwrap();
        std::fs::write(&third, b"ccc").unwrap();

        let mut writer =
            backend::create_writer(&archive, ArchiveKind::SevenZ, Method::Store).unwrap();
        writer
            .add(&first, "A.txt", None, &CancelToken::new(), &|_| {})
            .unwrap();
        writer
            .add(&second, "a.txt", None, &CancelToken::new(), &|_| {})
            .unwrap();
        writer
            .add(&third, "c.txt", None, &CancelToken::new(), &|_| {})
            .unwrap();
        writer.finish().unwrap();

        let rename = HashMap::from([(id_of(&archive, "c.txt"), "renamed.txt".to_owned())]);
        rewrite_with_names(
            &archive,
            ArchiveKind::SevenZ,
            &HashSet::new(),
            &rename,
            Limits::default(),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();

        let names = names(&archive);
        assert!(names.contains(&"renamed.txt".to_owned()));

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
