use std::path::Path;
use std::time::Duration;

use crate::error::{ArchiveError, SinkError};
use crate::limits::Limits;
use crate::plan::{DecisionTable, ExtractPlan};
use crate::task::CancelToken;
use crate::types::{ArchiveEntry, ArchiveKind, EntryId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    SymlinkNotAllowed,
    HardlinkNotAllowed,
    UnsupportedMethod,
    CaseCollision,
    PolicySkip,
}

#[derive(Debug, Default)]
pub struct ExtractReport {
    pub succeeded: Vec<EntryId>,
    pub failed: Vec<(EntryId, String)>,
    pub skipped: Vec<(EntryId, SkipReason)>,
    pub bytes_written: u64,
    pub elapsed: Duration,
    pub crc_mismatch: Vec<EntryId>,
}

/// 唯一写盘与计数入口。膨胀熔断、磁盘检查、进度、CRC 都在这里。
pub trait ExtractSink {
    fn on_entry_begin(&mut self, id: EntryId, entry: &ArchiveEntry) -> Result<(), SinkError>;
    fn on_data(&mut self, id: EntryId, chunk: &[u8]) -> Result<(), SinkError>;
    fn on_entry_end(&mut self, id: EntryId, ok: bool) -> Result<(), SinkError>;
}

pub trait ArchiveReader: Send {
    fn kind(&self) -> ArchiveKind;
    fn entries(&self) -> &[ArchiveEntry];

    fn is_solid(&self) -> bool {
        false
    }

    fn comment(&self) -> Option<&str> {
        None
    }

    /// 只读预检：净化路径、检测冲突、估算总量。不写盘。
    fn plan(
        &self,
        _ids: &[EntryId],
        _target: &Path,
        _limits: &Limits,
    ) -> Result<ExtractPlan, ArchiveError> {
        Err(ArchiveError::Unsupported(
            "该格式的后端尚未接入（骨架阶段）".to_owned(),
        ))
    }

    /// 执行解压。decisions 已由 UI 预先解析，worker 不再询问用户。
    fn extract(
        &mut self,
        _plan: &ExtractPlan,
        _decisions: &DecisionTable,
        _cancel: &CancelToken,
        _sink: &mut dyn ExtractSink,
    ) -> Result<ExtractReport, ArchiveError> {
        Err(ArchiveError::Unsupported(
            "该格式的后端尚未接入（骨架阶段）".to_owned(),
        ))
    }

    fn test(
        &mut self,
        _cancel: &CancelToken,
        _sink: &mut dyn ExtractSink,
    ) -> Result<ExtractReport, ArchiveError> {
        Err(ArchiveError::Unsupported(
            "该格式的后端尚未接入（骨架阶段）".to_owned(),
        ))
    }
}
