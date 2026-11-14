use crate::error::SinkError;
use crate::reader::ExtractSink;
use crate::types::{ArchiveEntry, EntryId};

/// 把单个条目读进内存，超过上限后继续 drain 但不再保存。
/// 用于预览：不写盘，也不能让压缩炸弹吃光内存。
pub struct MemorySink {
    buf: Vec<u8>,
    limit: u64,
    truncated: bool,
    ok: bool,
}

impl MemorySink {
    pub fn new(limit: u64) -> Self {
        Self {
            buf: Vec::new(),
            limit,
            truncated: false,
            ok: true,
        }
    }

    pub fn into_parts(self) -> (Vec<u8>, bool, bool) {
        (self.buf, self.truncated, self.ok)
    }
}

impl ExtractSink for MemorySink {
    fn on_entry_begin(&mut self, _id: EntryId, _entry: &ArchiveEntry) -> Result<(), SinkError> {
        self.buf.clear();
        self.truncated = false;
        self.ok = true;
        Ok(())
    }

    fn on_data(&mut self, _id: EntryId, chunk: &[u8]) -> Result<(), SinkError> {
        let remaining = self.limit.saturating_sub(self.buf.len() as u64) as usize;
        if chunk.len() > remaining {
            self.truncated = true;
        }
        let take = chunk.len().min(remaining);
        self.buf.extend_from_slice(&chunk[..take]);
        Ok(())
    }

    fn on_entry_end(&mut self, _id: EntryId, ok: bool) -> Result<(), SinkError> {
        self.ok = ok;
        Ok(())
    }

    fn bytes_written(&self) -> u64 {
        self.buf.len() as u64
    }
}
