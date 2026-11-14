use crate::error::{FailReason, SinkError};
use crate::limits::Limits;
use crate::reader::ExtractSink;
use crate::safety::bomb::Budget;
use crate::types::{ArchiveEntry, EntryId};

/// 只算 CRC、不写盘的校验 sink（`test()` 用）。
#[derive(Default)]
pub struct VerifySink {
    budget: Option<Budget>,
    bytes: u64,
    current: Option<Current>,
    crc_mismatch: Vec<EntryId>,
}

struct Current {
    id: EntryId,
    expected: Option<u32>,
    hasher: crc32fast::Hasher,
    written: u64,
}

impl VerifySink {
    pub fn with_limits(limits: Limits) -> Self {
        Self {
            budget: Some(Budget::new(limits)),
            ..Default::default()
        }
    }
}

impl ExtractSink for VerifySink {
    fn on_entry_begin(&mut self, id: EntryId, entry: &ArchiveEntry) -> Result<(), SinkError> {
        if let Some(budget) = self.budget.as_mut() {
            budget.on_entry().map_err(SinkError::Limit)?;
        }
        self.current = Some(Current {
            id,
            expected: entry.crc32,
            hasher: crc32fast::Hasher::new(),
            written: 0,
        });
        Ok(())
    }

    fn on_data(&mut self, id: EntryId, chunk: &[u8]) -> Result<(), SinkError> {
        let Some(current) = self.current.as_mut() else {
            return Err(SinkError::Other(FailReason::EntryNotStarted { id }));
        };
        if current.id != id {
            return Err(SinkError::Other(FailReason::EntryOrder));
        }

        if let Some(budget) = self.budget.as_mut() {
            budget
                .on_write(current.written, chunk.len() as u64, 0)
                .map_err(SinkError::Limit)?;
        }

        current.hasher.update(chunk);
        current.written = current.written.saturating_add(chunk.len() as u64);
        self.bytes = self.bytes.saturating_add(chunk.len() as u64);
        Ok(())
    }

    fn on_entry_end(&mut self, _id: EntryId, ok: bool) -> Result<(), SinkError> {
        let Some(current) = self.current.take() else {
            return Ok(());
        };
        if !ok {
            return Ok(());
        }
        if let Some(expected) = current.expected {
            if current.hasher.finalize() != expected {
                self.crc_mismatch.push(current.id);
            }
        }
        Ok(())
    }

    fn bytes_written(&self) -> u64 {
        self.bytes
    }

    fn take_crc_mismatch(&mut self) -> Vec<EntryId> {
        std::mem::take(&mut self.crc_mismatch)
    }
}
