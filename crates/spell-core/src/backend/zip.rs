use std::path::{Path, PathBuf};

use crate::reader::ArchiveReader;
use crate::types::{ArchiveEntry, ArchiveKind};

/// zip 后端占位。接入计划：
/// - `zip` crate，features = ["aes-crypto", "deflate", "bzip2", "zstd"]
/// - 先读中央目录（Zip64），再逐条流式解压
/// - `entries()` 使用 `archive.by_index_raw` 只读元数据，避免解压
/// - ZipCrypto / AE-1 / AE-2 密码校验在此实现
pub struct ZipReader {
    path: PathBuf,
    entries: Vec<ArchiveEntry>,
}

impl ZipReader {
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            entries: Vec::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ArchiveReader for ZipReader {
    fn kind(&self) -> ArchiveKind {
        ArchiveKind::Zip
    }

    fn entries(&self) -> &[ArchiveEntry] {
        &self.entries
    }
}
