use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::error::{FailReason, SinkError};
use crate::limits::Limits;
use crate::plan::{DecisionTable, ExtractPlan, ResolvedAction, resolve_all};
use crate::reader::ExtractSink;
use crate::safety::atomic_fs::{AtomicFile, is_reparse_point};
use crate::safety::bomb::{Budget, LimitHit};
use crate::safety::file_time::set_mtime;
use crate::types::{ArchiveEntry, EntryId};

/// 把条目流安全落到磁盘的唯一实现。所有写盘入口（GUI 解压、CLI、主题导入）
/// 都必须在 `ExtractSink` 这一层复用它，禁止旁路。
pub struct ExtractDirSink {
    budget: Budget,
    actions: HashMap<EntryId, ResolvedAction>,
    current: Option<CurrentEntry>,
    /// 解压根目录；逐级创建子目录时用它划定「安全边界」。
    root: PathBuf,
    pub bytes_written: u64,
    pub crc_mismatch: Vec<EntryId>,
    /// 目录条目在子项写完后统一恢复 mtime。
    dir_times: Vec<(PathBuf, SystemTime)>,
}

enum CurrentEntry {
    Dir,
    File {
        atomic: AtomicFile,
        hasher: crc32fast::Hasher,
        expected_crc: Option<u32>,
        written: u64,
        id: EntryId,
        path: PathBuf,
        mtime: Option<SystemTime>,
    },
}

impl ExtractDirSink {
    pub fn new(plan: &ExtractPlan, decisions: &DecisionTable, limits: Limits) -> Self {
        Self {
            budget: Budget::new(limits),
            actions: resolve_all(plan, decisions),
            current: None,
            root: plan.root.clone(),
            bytes_written: 0,
            crc_mismatch: Vec::new(),
            dir_times: Vec::new(),
        }
    }

    pub fn actions(&self) -> &HashMap<EntryId, ResolvedAction> {
        &self.actions
    }
}

impl ExtractSink for ExtractDirSink {
    fn on_entry_begin(&mut self, id: EntryId, entry: &ArchiveEntry) -> Result<(), SinkError> {
        let path = match self.actions.get(&id) {
            Some(ResolvedAction::Write { path, .. }) => path.clone(),
            _ => {
                return Err(SinkError::Other(FailReason::DecisionNotWrite { id }));
            }
        };

        self.budget.on_entry().map_err(limit_error)?;

        if entry.kind.is_dir() {
            ensure_dir_chain(&self.root, &path)?;
            if let Some(time) = entry.mtime {
                self.dir_times.push((path, time));
            }
            self.current = Some(CurrentEntry::Dir);
            return Ok(());
        }

        if let Some(parent) = path.parent() {
            ensure_dir_chain(&self.root, parent)?;
        }

        // 目标本身是 junction / symlink：拒绝，不顺着它写出去。
        if is_reparse_point(&path)? {
            return Err(SinkError::Other(FailReason::ReparsePointOverwrite {
                path: path.display().to_string(),
            }));
        }

        let atomic = AtomicFile::create(&path)?;
        self.current = Some(CurrentEntry::File {
            atomic,
            hasher: crc32fast::Hasher::new(),
            expected_crc: entry.crc32,
            written: 0,
            id,
            path,
            mtime: entry.mtime,
        });
        Ok(())
    }

    fn on_data(&mut self, id: EntryId, chunk: &[u8]) -> Result<(), SinkError> {
        let CurrentEntry::File {
            atomic,
            hasher,
            written,
            ..
        } = self
            .current
            .as_mut()
            .ok_or(SinkError::Other(FailReason::EntryNotBegun { id }))?
        else {
            return Err(SinkError::Other(FailReason::DirHasData { id }));
        };

        let len = chunk.len() as u64;
        self.budget.on_write(*written, len).map_err(limit_error)?;

        atomic.file_mut().write_all(chunk)?;
        hasher.update(chunk);
        *written += len;
        Ok(())
    }

    fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    fn take_crc_mismatch(&mut self) -> Vec<EntryId> {
        std::mem::take(&mut self.crc_mismatch)
    }

    fn finish(&mut self) -> Result<(), SinkError> {
        // 子项写入会不断刷新父目录 mtime；全部结束后按深度倒序恢复。
        self.dir_times
            .sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
        for (path, time) in self.dir_times.drain(..) {
            let _ = set_mtime(&path, time);
        }
        Ok(())
    }

    fn on_entry_end(&mut self, id: EntryId, ok: bool) -> Result<(), SinkError> {
        let Some(current) = self.current.take() else {
            return Err(SinkError::Other(FailReason::EntryNotStarted { id }));
        };

        let CurrentEntry::File {
            atomic,
            hasher,
            expected_crc,
            written,
            id: current_id,
            path,
            mtime,
        } = current
        else {
            return Ok(());
        };

        if !ok {
            // 中止：临时文件随 AtomicFile 的 Drop 删除，原文件保持不变。
            return Ok(());
        }

        if let Some(expected) = expected_crc
            && hasher.finalize() != expected
        {
            // CRC 不符说明内容已损坏，不能落盘。丢弃 AtomicFile 会删掉临时文件，
            // 目标位置保持原样：既不覆盖已有文件，也不留下半截内容。
            self.crc_mismatch.push(current_id);
            return Err(SinkError::Other(FailReason::EntryCrcFailed {
                detail: format!("条目 CRC 校验失败，已丢弃未落盘内容: {}", path.display()),
            }));
        }

        atomic.commit()?;
        if let Some(time) = mtime {
            // 恢复归档时间失败不阻断解压（例如文件系统不支持）。
            let _ = set_mtime(&path, time);
        }
        self.bytes_written = self.bytes_written.saturating_add(written);
        Ok(())
    }
}

/// 在解压根目录之下逐级创建目录链，每一级都做 reparse point 检查。
///
/// 只检查直接父目录是不够的：直接父目录尚不存在时检查必然通过（`symlink_metadata`
/// 返回 NotFound），随后 `create_dir_all` 却会穿过祖先链上的 junction / symlink，
/// 把文件写到解压根目录之外。逐级检查 + 逐级创建才能封死这条路径。
fn ensure_dir_chain(root: &std::path::Path, target: &std::path::Path) -> Result<(), SinkError> {
    let relative = if root.as_os_str().is_empty() {
        None
    } else {
        target.strip_prefix(root).ok()
    };

    let Some(relative) = relative else {
        // 目标不在 root 之下（手工构造的计划）：只能检查直接父目录。
        return ensure_single_dir(target);
    };

    // root 由调用方指定，它本身是不是链接是用户的选择，不做拦截；
    // 但 root 之下任何一级是链接都必须拒绝。
    let mut current = root.to_path_buf();
    if !current.exists() {
        std::fs::create_dir_all(&current)?;
    }

    for component in relative.components() {
        current.push(component);
        if is_reparse_point(&current)? {
            return Err(SinkError::Other(FailReason::ReparsePointWrite {
                path: current.display().to_string(),
            }));
        }
        if !current.exists() {
            // 逐级创建，不使用 create_dir_all，避免穿过未校验的层级。
            match std::fs::create_dir(&current) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(err) => return Err(err.into()),
            }
        }
    }
    Ok(())
}

/// 退化路径：只检查直接父目录是不是 reparse point。
fn ensure_single_dir(path: &std::path::Path) -> Result<(), SinkError> {
    if is_reparse_point(path)? {
        return Err(SinkError::Other(FailReason::ReparsePointWrite {
            path: path.display().to_string(),
        }));
    }
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

fn limit_error(hit: LimitHit) -> SinkError {
    SinkError::Limit(hit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{OverwritePolicy, PlanAction, PlannedItem};
    use crate::types::EntryKind;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "thzip-spellarchive-sink-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// CRC 不符时内容不得落盘：临时文件被删除，目标位置保持原样。
    #[test]
    fn crc_mismatch_does_not_commit() {
        let dir = temp_dir("crc");
        let root = dir.join("out");
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("payload.bin");

        let mut plan = ExtractPlan {
            root: root.clone(),
            ..Default::default()
        };
        plan.items.push(PlannedItem {
            id: 0,
            rel_path: target.clone(),
            kind: EntryKind::File,
            size: 5,
            action: PlanAction::Create,
        });

        let decisions = DecisionTable {
            default: OverwritePolicy::Overwrite,
            ..Default::default()
        };
        let mut sink = ExtractDirSink::new(&plan, &decisions, Limits::default());

        let mut entry = ArchiveEntry::new(0, b"payload.bin", "payload.bin", EntryKind::File);
        entry.crc32 = Some(0xDEAD_BEEF); // 与实际内容不符
        sink.on_entry_begin(0, &entry).unwrap();
        sink.on_data(0, b"hello").unwrap();

        let result = sink.on_entry_end(0, true);
        assert!(result.is_err(), "CRC 不符必须报错");
        assert!(!target.exists(), "CRC 不符的文件不得落盘");
        assert_eq!(sink.take_crc_mismatch(), vec![0]);
        assert_eq!(sink.bytes_written(), 0, "未落盘的内容不计入写入量");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CRC 相符时正常落盘。
    #[test]
    fn matching_crc_commits() {
        let dir = temp_dir("crc-ok");
        let root = dir.join("out");
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("payload.bin");

        let mut plan = ExtractPlan {
            root: root.clone(),
            ..Default::default()
        };
        plan.items.push(PlannedItem {
            id: 0,
            rel_path: target.clone(),
            kind: EntryKind::File,
            size: 5,
            action: PlanAction::Create,
        });

        let decisions = DecisionTable {
            default: OverwritePolicy::Overwrite,
            ..Default::default()
        };
        let mut sink = ExtractDirSink::new(&plan, &decisions, Limits::default());

        let mut entry = ArchiveEntry::new(0, b"payload.bin", "payload.bin", EntryKind::File);
        entry.crc32 = Some(crc32fast::hash(b"hello"));
        sink.on_entry_begin(0, &entry).unwrap();
        sink.on_data(0, b"hello").unwrap();
        sink.on_entry_end(0, true).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"hello");
        assert_eq!(sink.bytes_written(), 5);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 正常路径：根目录之下的多级目录应当被逐级创建。
    #[test]
    fn creates_nested_dirs_under_root() {
        let dir = temp_dir("nested");
        let root = dir.join("root");
        let target = root.join("a").join("b").join("c.txt");
        ensure_dir_chain(&root, target.parent().unwrap()).unwrap();
        assert!(root.join("a").join("b").is_dir());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 祖先链上有 junction 时必须拒绝：只检查直接父目录不够。
    ///
    /// 直接父目录不存在时，旧实现的检查必然通过（NotFound 被当作「不是链接」），
    /// 随后的 `create_dir_all` 却会穿过祖先 junction 把内容写到解压根目录之外。
    #[cfg(windows)]
    #[test]
    fn rejects_junction_in_ancestor_chain() {
        let dir = temp_dir("junction");
        let root = dir.join("root");
        let outside = dir.join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let link = root.join("esc");
        let created = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(created, "创建 junction 失败");

        let deep = root.join("esc").join("sub");
        assert!(
            ensure_dir_chain(&root, &deep).is_err(),
            "祖先链上的 junction 必须被拒绝"
        );
        assert!(
            !outside.join("sub").exists(),
            "不得在解压根目录之外创建目录"
        );

        let _ = std::fs::remove_dir(&link);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
