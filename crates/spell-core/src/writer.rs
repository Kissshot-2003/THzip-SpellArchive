use std::path::Path;
use std::time::Duration;

use crate::error::ArchiveError;
use crate::task::CancelToken;
use crate::types::Method;

#[derive(Debug, Default)]
pub struct FinishReport {
    pub entries: u32,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub elapsed: Duration,
}

pub trait ArchiveWriter: Send {
    fn add(
        &mut self,
        _src: &Path,
        _name_in_archive: &str,
        _method: Option<Method>,
        _cancel: &CancelToken,
        _progress: &dyn Fn(u64),
    ) -> Result<(), ArchiveError> {
        Err(ArchiveError::Unsupported(
            "该格式的写入后端尚未接入（骨架阶段）".to_owned(),
        ))
    }

    fn add_dir(&mut self, _name_in_archive: &str) -> Result<(), ArchiveError> {
        Err(ArchiveError::Unsupported(
            "该格式的写入后端尚未接入（骨架阶段）".to_owned(),
        ))
    }

    fn set_comment(&mut self, _text: &str) -> Result<(), ArchiveError> {
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<FinishReport, ArchiveError> {
        Err(ArchiveError::Unsupported(
            "该格式的写入后端尚未接入（骨架阶段）".to_owned(),
        ))
    }
}
