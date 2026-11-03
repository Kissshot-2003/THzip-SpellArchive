use std::path::{Path, PathBuf};

use crate::reader::ArchiveReader;
use crate::types::{ArchiveEntry, ArchiveKind};

/// 7z 后端占位。接入计划：
/// - `sevenz-rust2`（纯 Rust），AES-256 / 固实 / LZMA2
/// - 头加密（`-mhe=on`）时 `open` 返回 `PasswordRequired`
/// - 固实包单条提取需要顺序解码，进度按「已处理压缩字节」上报
pub struct SevenZReader {
    path: PathBuf,
    entries: Vec<ArchiveEntry>,
}

impl SevenZReader {
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

impl ArchiveReader for SevenZReader {
    fn kind(&self) -> ArchiveKind {
        ArchiveKind::SevenZ
    }

    fn entries(&self) -> &[ArchiveEntry] {
        &self.entries
    }

    fn is_solid(&self) -> bool {
        false
    }
}
