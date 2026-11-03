use std::sync::Arc;
use std::time::SystemTime;

/// 条目在单个归档内的稳定下标。禁止用文件名定位条目（zip 允许重名）。
pub type EntryId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    Hardlink,
    Other,
}

impl EntryKind {
    pub fn is_dir(self) -> bool {
        matches!(self, EntryKind::Dir)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchiveKind {
    SevenZ,
    Zip,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
    TarZst,
    TarLz4,
    Gz,
    Bz2,
    Xz,
    Zstd,
    Lz4,
    Rar,
    Unknown,
}

impl ArchiveKind {
    pub fn label(self) -> &'static str {
        match self {
            ArchiveKind::SevenZ => "7z",
            ArchiveKind::Zip => "zip",
            ArchiveKind::Tar => "tar",
            ArchiveKind::TarGz => "tar.gz",
            ArchiveKind::TarBz2 => "tar.bz2",
            ArchiveKind::TarXz => "tar.xz",
            ArchiveKind::TarZst => "tar.zst",
            ArchiveKind::TarLz4 => "tar.lz4",
            ArchiveKind::Gz => "gz",
            ArchiveKind::Bz2 => "bz2",
            ArchiveKind::Xz => "xz",
            ArchiveKind::Zstd => "zst",
            ArchiveKind::Lz4 => "lz4",
            ArchiveKind::Rar => "rar",
            ArchiveKind::Unknown => "unknown",
        }
    }

    /// 是否为「单文件流」容器（不是归档，只有一个条目）。
    pub fn is_single_stream(self) -> bool {
        matches!(
            self,
            ArchiveKind::Gz
                | ArchiveKind::Bz2
                | ArchiveKind::Xz
                | ArchiveKind::Zstd
                | ArchiveKind::Lz4
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    Store,
    Deflate,
    Deflate64,
    Lzma,
    Lzma2,
    Bzip2,
    Xz,
    Zstd,
    Lz4,
    Ppmd,
    Unknown(u16),
}

impl Method {
    pub fn label(self) -> &'static str {
        match self {
            Method::Store => "Store",
            Method::Deflate => "Deflate",
            Method::Deflate64 => "Deflate64",
            Method::Lzma => "LZMA",
            Method::Lzma2 => "LZMA2",
            Method::Bzip2 => "BZip2",
            Method::Xz => "XZ",
            Method::Zstd => "ZSTD",
            Method::Lz4 => "LZ4",
            Method::Ppmd => "PPMd",
            Method::Unknown(_) => "未知",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    pub id: EntryId,
    /// 原始字节；解析阶段绝不做有损转换，落盘时以它为准。
    pub raw_name: Arc<[u8]>,
    pub kind: EntryKind,
    pub size: u64,
    /// 固实 7z 无法给出可靠的单条压缩大小。
    pub packed: Option<u64>,
    pub crc32: Option<u32>,
    /// 7z 存 UTC；zip 存本地 DOS 时间。
    pub mtime: Option<SystemTime>,
    pub encrypted: bool,
    pub method: Method,
    /// 已做编码还原的展示名，仅供 UI 使用。
    pub display: Arc<str>,
}

impl ArchiveEntry {
    pub fn new(
        id: EntryId,
        raw_name: &[u8],
        display: impl Into<Arc<str>>,
        kind: EntryKind,
    ) -> Self {
        Self {
            id,
            raw_name: Arc::from(raw_name),
            kind,
            size: 0,
            packed: None,
            crc32: None,
            mtime: None,
            encrypted: false,
            method: Method::Store,
            display: display.into(),
        }
    }

    /// 仅用于界面骨架阶段的演示数据。
    pub fn demo(
        id: EntryId,
        name: &str,
        kind: EntryKind,
        size: u64,
        packed: u64,
        method: Method,
    ) -> Self {
        Self {
            id,
            raw_name: Arc::from(name.as_bytes()),
            kind,
            size,
            packed: Some(packed),
            crc32: None,
            mtime: None,
            encrypted: false,
            method,
            display: Arc::from(name),
        }
    }

    pub fn ratio(&self) -> Option<f64> {
        match self.packed {
            Some(p) if self.size > 0 && p > 0 => Some(self.size as f64 / p as f64),
            _ => None,
        }
    }
}
