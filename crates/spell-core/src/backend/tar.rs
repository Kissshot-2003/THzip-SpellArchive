use std::path::{Path, PathBuf};

use crate::reader::ArchiveReader;
use crate::types::{ArchiveEntry, ArchiveKind};

/// tar 家族占位。接入计划：
/// - `tar` crate 负责容器；压缩层分别用 flate2 / bzip2 / lzma-rs / zstd / lz4_flex
/// - 需要显式处理：pax 扩展头、GNU longname、hardlink、base-256 长度、`./` 前缀
pub struct TarReader {
    path: PathBuf,
    kind: ArchiveKind,
    entries: Vec<ArchiveEntry>,
}

impl TarReader {
    pub fn new(path: &Path, kind: ArchiveKind) -> Self {
        Self {
            path: path.to_path_buf(),
            kind,
            entries: Vec::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ArchiveReader for TarReader {
    fn kind(&self) -> ArchiveKind {
        self.kind
    }

    fn entries(&self) -> &[ArchiveEntry] {
        &self.entries
    }
}
