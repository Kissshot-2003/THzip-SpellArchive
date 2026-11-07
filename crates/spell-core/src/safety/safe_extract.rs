use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::error::SinkError;
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
    pub bytes_written: u64,
    pub crc_mismatch: Vec<EntryId>,
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
            bytes_written: 0,
            crc_mismatch: Vec::new(),
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
                return Err(SinkError::Other(format!(
                    "条目 {id} 的决策不是写入，后端不应调用 on_entry_begin"
                )));
            }
        };

        self.budget.on_entry().map_err(limit_error)?;

        if entry.kind.is_dir() {
            ensure_safe_dir(&path)?;
            self.current = Some(CurrentEntry::Dir);
            return Ok(());
        }

        if let Some(parent) = path.parent() {
            ensure_safe_dir(parent)?;
        }

        // 目标本身是 junction / symlink：拒绝，不顺着它写出去。
        if is_reparse_point(&path)? {
            return Err(SinkError::Other(format!(
                "{} 已存在且是重解析点，拒绝覆盖",
                path.display()
            )));
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
            .ok_or_else(|| SinkError::Other(format!("条目 {id} 未调用 on_entry_begin")))?
        else {
            return Err(SinkError::Other(format!("目录条目 {id} 不应有数据")));
        };

        let len = chunk.len() as u64;
        self.budget
            .on_write(*written, len, 0)
            .map_err(limit_error)?;

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

    fn on_entry_end(&mut self, id: EntryId, ok: bool) -> Result<(), SinkError> {
        let Some(current) = self.current.take() else {
            return Err(SinkError::Other(format!("条目 {id} 未开始")));
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

        if let Some(expected) = expected_crc {
            if hasher.finalize() != expected {
                self.crc_mismatch.push(current_id);
            }
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

fn ensure_safe_dir(path: &std::path::Path) -> Result<(), SinkError> {
    if is_reparse_point(path)? {
        return Err(SinkError::Other(format!(
            "{} 是重解析点，拒绝写入",
            path.display()
        )));
    }
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

fn limit_error(hit: LimitHit) -> SinkError {
    SinkError::Limit(hit.message())
}
