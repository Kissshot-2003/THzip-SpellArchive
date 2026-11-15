use crate::error::{FailReason, SinkError};
use crate::limits::Limits;
use crate::reader::ExtractSink;
use crate::safety::bomb::Budget;
use crate::types::{ArchiveEntry, EntryId};

/// 把单个条目读进内存，超过上限后继续 drain 但不再保存。
/// 用于预览：不写盘，也不能让压缩炸弹吃光内存。
///
/// 预览路径同样受限额与 CRC 约束：预览一个 10 GB 的炸弹条目不应该只靠
/// `max_bytes` 兜底，损坏的内容也不应该被当成正常预览展示。
pub struct MemorySink {
    buf: Vec<u8>,
    limit: u64,
    truncated: bool,
    ok: bool,
    budget: Budget,
    hasher: crc32fast::Hasher,
    expected_crc: Option<u32>,
    written: u64,
    crc_mismatch: Vec<EntryId>,
}

impl MemorySink {
    pub fn new(limit: u64) -> Self {
        Self::with_limits(limit, Limits::default())
    }

    pub fn with_limits(limit: u64, limits: Limits) -> Self {
        Self {
            buf: Vec::new(),
            limit,
            truncated: false,
            ok: true,
            budget: Budget::new(limits),
            hasher: crc32fast::Hasher::new(),
            expected_crc: None,
            written: 0,
            crc_mismatch: Vec::new(),
        }
    }

    pub fn into_parts(self) -> (Vec<u8>, bool, bool) {
        (self.buf, self.truncated, self.ok)
    }
}

impl ExtractSink for MemorySink {
    fn on_entry_begin(&mut self, _id: EntryId, entry: &ArchiveEntry) -> Result<(), SinkError> {
        self.buf.clear();
        self.truncated = false;
        self.ok = true;
        self.budget.on_entry().map_err(SinkError::Limit)?;
        self.hasher = crc32fast::Hasher::new();
        self.expected_crc = entry.crc32;
        self.written = 0;
        Ok(())
    }

    fn on_data(&mut self, _id: EntryId, chunk: &[u8]) -> Result<(), SinkError> {
        // 限额按实际解压量计，而不是按「保留了多少」计：
        // 截断只影响内存占用，炸弹依然是炸弹。
        self.budget
            .on_write(self.written, chunk.len() as u64)
            .map_err(SinkError::Limit)?;
        self.written = self.written.saturating_add(chunk.len() as u64);
        self.hasher.update(chunk);

        let remaining = self.limit.saturating_sub(self.buf.len() as u64) as usize;
        if chunk.len() > remaining {
            self.truncated = true;
        }
        let take = chunk.len().min(remaining);
        self.buf.extend_from_slice(&chunk[..take]);
        Ok(())
    }

    fn on_entry_end(&mut self, id: EntryId, ok: bool) -> Result<(), SinkError> {
        self.ok = ok;
        if !ok {
            return Ok(());
        }
        if let Some(expected) = self.expected_crc
            && self.hasher.clone().finalize() != expected
        {
            self.crc_mismatch.push(id);
            return Err(SinkError::Other(FailReason::EntryCrcFailed {
                detail: "预览内容 CRC 校验失败".to_owned(),
            }));
        }
        Ok(())
    }

    fn bytes_written(&self) -> u64 {
        self.buf.len() as u64
    }

    fn take_crc_mismatch(&mut self) -> Vec<EntryId> {
        std::mem::take(&mut self.crc_mismatch)
    }
}

#[cfg(test)]
mod tests {
    use super::MemorySink;
    use crate::error::SinkError;
    use crate::limits::{LimitProfile, Limits};
    use crate::reader::ExtractSink;
    use crate::types::{ArchiveEntry, EntryKind};

    #[test]
    fn rejects_bomb_sized_entries() {
        let limits = LimitProfile::Strict.preset();
        let mut sink = MemorySink::with_limits(1024, limits);
        let entry = ArchiveEntry::new(0, b"a.bin", "a.bin", EntryKind::File);
        sink.on_entry_begin(0, &entry).unwrap();

        let chunk = vec![0u8; 1024];
        let mut seen = None;
        for _ in 0..(3 * 1024 * 1024) {
            if let Err(err) = sink.on_data(0, &chunk) {
                seen = Some(err);
                break;
            }
        }
        assert!(matches!(seen, Some(SinkError::Limit(_))), "必须触发限额");
    }

    #[test]
    fn flags_crc_mismatch() {
        let mut sink = MemorySink::with_limits(1024, Limits::default());
        let mut entry = ArchiveEntry::new(0, b"a.bin", "a.bin", EntryKind::File);
        entry.crc32 = Some(0xDEAD_BEEF);
        sink.on_entry_begin(0, &entry).unwrap();
        sink.on_data(0, b"hello").unwrap();
        assert!(sink.on_entry_end(0, true).is_err());
        assert_eq!(sink.take_crc_mismatch(), vec![0]);
    }
}
