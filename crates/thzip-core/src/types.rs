use std::sync::Arc;
use std::time::SystemTime;

use crate::backend::name_codec::NameEncoding;

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
    Gzip,
    Deflate,
    Deflate64,
    Lzma,
    Lzma2,
    Bzip2,
    Xz,
    Zstd,
    Lz4,
    Rar,
    Ppmd,
    Unknown(u16),
}

impl Method {
    pub fn label(self) -> &'static str {
        match self {
            Method::Store => "Store",
            Method::Gzip => "GZip",
            Method::Deflate => "Deflate",
            Method::Deflate64 => "Deflate64",
            Method::Lzma => "LZMA",
            Method::Lzma2 => "LZMA2",
            Method::Bzip2 => "BZip2",
            Method::Xz => "XZ",
            Method::Zstd => "ZSTD",
            Method::Lz4 => "LZ4",
            Method::Rar => "RAR",
            Method::Ppmd => "PPMd",
            // winzip AES：压缩方法字段被 99 覆盖，实际方法在 extra field 里。
            Method::Unknown(99) => "AES",
            Method::Unknown(_) => "Unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    pub id: EntryId,
    /// 归档里的原始名字字节，供追溯与 `raw` 模式还原；落盘不使用它。
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
    /// 已做编码还原的展示名，落盘路径由它经 safety 净化后得到。
    pub display: Arc<str>,
    /// 名字编码判定结果（zip 由标志位 / 0x7075 或打分得出，其余格式默认 UTF-8）。
    pub name_encoding: NameEncoding,
    /// 判定置信度 0-100；低于 60 时 UI 应提示可手动切换。
    pub name_confidence: u8,
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
            name_encoding: NameEncoding::Utf8,
            name_confidence: 100,
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
            name_encoding: NameEncoding::Utf8,
            name_confidence: 100,
        }
    }

    pub fn ratio(&self) -> Option<f64> {
        match self.packed {
            Some(p) if self.size > 0 && p > 0 => Some(self.size as f64 / p as f64),
            _ => None,
        }
    }
}
