mod common;
mod decoder;
pub mod name_codec;
pub mod rar;
pub mod rewrite;
pub mod sevenz;
pub mod sevenz_write;
pub mod stream;
pub mod tar;
pub mod tar_write;
pub mod volume;
pub mod zip;
pub mod zip_write;

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::error::{ArchiveError, FailReason};
use crate::limits::Limits;
use crate::reader::ArchiveReader;
use crate::secret::SecretString;
use crate::task::CancelToken;
use crate::types::{ArchiveKind, Method};
use crate::writer::{ArchiveWriter, SevenZOptions};

const SNIFF_LEN: usize = 600;

pub fn sniff_file(path: &Path) -> std::io::Result<ArchiveKind> {
    let mut file = File::open(path)?;
    let mut buf = [0u8; SNIFF_LEN];
    let read = file.read(&mut buf)?;
    let kind = classify(
        &buf[..read],
        path.file_name().and_then(|name| name.to_str()),
    );

    // 扩展名没写 tar 的单文件流：解压前 1 KiB 看内层是不是 tar。
    let tar_kind = match kind {
        ArchiveKind::Gz => Some(ArchiveKind::TarGz),
        ArchiveKind::Bz2 => Some(ArchiveKind::TarBz2),
        ArchiveKind::Xz => Some(ArchiveKind::TarXz),
        ArchiveKind::Zstd => Some(ArchiveKind::TarZst),
        ArchiveKind::Lz4 => Some(ArchiveKind::TarLz4),
        _ => None,
    };
    if let Some(tar_kind) = tar_kind
        && looks_like_tar(path, kind)
    {
        return Ok(tar_kind);
    }

    Ok(kind)
}

fn looks_like_tar(path: &Path, kind: ArchiveKind) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut reader: Box<dyn Read> = match kind {
        ArchiveKind::Gz => Box::new(flate2::read::GzDecoder::new(file)),
        ArchiveKind::Bz2 => Box::new(bzip2::read::BzDecoder::new(file)),
        ArchiveKind::Xz => Box::new(lzma_rust2::XzReader::new(file, false)),
        ArchiveKind::Zstd => match zstd::stream::read::Decoder::new(file) {
            Ok(decoder) => Box::new(decoder),
            Err(_) => return false,
        },
        ArchiveKind::Lz4 => Box::new(lz4_flex::frame::FrameDecoder::new(file)),
        _ => return false,
    };

    let mut header = [0u8; 1024];
    let mut filled = 0;
    while filled < header.len() {
        match reader.read(&mut header[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(_) => return false,
        }
    }
    filled > 262 && &header[257..262] == b"ustar"
}

/// magic bytes 优先，文件名只用于区分复合容器（tar.gz vs gz）。
pub fn classify(header: &[u8], name: Option<&str>) -> ArchiveKind {
    let name = name.unwrap_or("").to_ascii_lowercase();
    let name_is = |needle: &str| name == needle || name.ends_with(&format!(".{needle}"));

    if header.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return ArchiveKind::SevenZ;
    }
    if header.starts_with(b"PK\x03\x04")
        || header.starts_with(b"PK\x05\x06")
        || header.starts_with(b"PK\x07\x08")
    {
        return ArchiveKind::Zip;
    }
    if header.starts_with(b"Rar!\x1A\x07") {
        return ArchiveKind::Rar;
    }
    if header.len() > 262 && &header[257..262] == b"ustar" {
        return ArchiveKind::Tar;
    }
    if header.starts_with(&[0x1F, 0x8B]) {
        return if name_is("tar.gz") || name_is("tgz") {
            ArchiveKind::TarGz
        } else {
            ArchiveKind::Gz
        };
    }
    if header.starts_with(b"BZh") {
        return if name_is("tar.bz2") || name_is("tbz2") || name_is("tbz") {
            ArchiveKind::TarBz2
        } else {
            ArchiveKind::Bz2
        };
    }
    if header.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0x00]) {
        return if name_is("tar.xz") || name_is("txz") {
            ArchiveKind::TarXz
        } else {
            ArchiveKind::Xz
        };
    }
    if header.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        return if name_is("tar.zst") || name_is("tzst") {
            ArchiveKind::TarZst
        } else {
            ArchiveKind::Zstd
        };
    }
    if header.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
        return if name_is("tar.lz4") || name_is("tlz4") {
            ArchiveKind::TarLz4
        } else {
            ArchiveKind::Lz4
        };
    }
    if name_is("tar") {
        return ArchiveKind::Tar;
    }
    // 非首卷没有 7z 魔数，只能靠文件名识别；首卷通常已命中 magic。
    if volume::is_volume_name(&name) {
        return ArchiveKind::SevenZ;
    }
    ArchiveKind::Unknown
}

pub fn open(
    path: &Path,
    password: Option<&SecretString>,
) -> Result<Box<dyn ArchiveReader>, ArchiveError> {
    let kind =
        sniff_file(path).map_err(|err| ArchiveError::Open(FailReason::Raw(err.to_string())))?;
    match kind {
        ArchiveKind::Zip => Ok(Box::new(zip::ZipReader::new(path, password)?)),
        ArchiveKind::SevenZ => Ok(Box::new(sevenz::SevenZReader::new(path, password)?)),
        ArchiveKind::Tar
        | ArchiveKind::TarGz
        | ArchiveKind::TarBz2
        | ArchiveKind::TarXz
        | ArchiveKind::TarZst
        | ArchiveKind::TarLz4 => Ok(Box::new(tar::TarReader::new(path, kind)?)),
        ArchiveKind::Rar => Ok(Box::new(rar::RarReader::new(path, password)?)),
        ArchiveKind::Gz
        | ArchiveKind::Bz2
        | ArchiveKind::Xz
        | ArchiveKind::Zstd
        | ArchiveKind::Lz4 => Ok(Box::new(stream::StreamReader::new(path, kind)?)),
        ArchiveKind::Unknown => Err(ArchiveError::Unsupported(FailReason::UnknownFormat)),
    }
}

/// 创建归档写入器。目前只支持 zip；7z/tar 仍是占位。
pub fn create_writer(
    path: &Path,
    kind: ArchiveKind,
    method: Method,
) -> Result<Box<dyn ArchiveWriter>, ArchiveError> {
    create_writer_with(path, kind, method, None)
}

/// 创建归档；`password` 非空时 zip 条目使用 AES-256 加密。
pub fn create_writer_with(
    path: &Path,
    kind: ArchiveKind,
    method: Method,
    password: Option<&SecretString>,
) -> Result<Box<dyn ArchiveWriter>, ArchiveError> {
    create_writer_with_options(path, kind, method, password, SevenZOptions::default())
}

/// 创建归档；可额外指定 7z 的压缩级别与字典大小。
pub fn create_writer_with_options(
    path: &Path,
    kind: ArchiveKind,
    method: Method,
    password: Option<&SecretString>,
    sevenz: SevenZOptions,
) -> Result<Box<dyn ArchiveWriter>, ArchiveError> {
    match kind {
        ArchiveKind::Zip => Ok(Box::new(zip_write::ZipArchiveWriter::create_with(
            path,
            method,
            password.cloned(),
        )?)),
        ArchiveKind::SevenZ => Ok(Box::new(sevenz_write::SevenZArchiveWriter::create(
            path,
            method,
            password.cloned(),
            sevenz,
        )?)),
        ArchiveKind::Tar
        | ArchiveKind::TarGz
        | ArchiveKind::TarBz2
        | ArchiveKind::TarXz
        | ArchiveKind::TarZst
        | ArchiveKind::TarLz4 => {
            if password.is_some() {
                return Err(ArchiveError::Unsupported(FailReason::EncryptUnsupported {
                    format: "tar".to_owned(),
                }));
            }
            Ok(Box::new(tar_write::TarArchiveWriter::create(path, kind)?))
        }
        other => Err(ArchiveError::Unsupported(FailReason::CreateUnsupported {
            format: other.label().to_owned(),
        })),
    }
}

/// 追加到现有归档。目标必须已存在；7z / tar 会先解包重建，完成后原子替换。
#[allow(clippy::too_many_arguments)]
pub fn create_writer_append(
    path: &Path,
    kind: ArchiveKind,
    method: Method,
    sevenz: SevenZOptions,
    limits: Limits,
    cancel: &CancelToken,
) -> Result<Box<dyn ArchiveWriter>, ArchiveError> {
    match kind {
        ArchiveKind::Zip => Ok(Box::new(zip_write::ZipArchiveWriter::append(
            path, method, cancel,
        )?)),
        ArchiveKind::SevenZ => Ok(Box::new(sevenz_write::SevenZArchiveWriter::append(
            path, method, sevenz, limits, cancel,
        )?)),
        ArchiveKind::Tar
        | ArchiveKind::TarGz
        | ArchiveKind::TarBz2
        | ArchiveKind::TarXz
        | ArchiveKind::TarZst
        | ArchiveKind::TarLz4 => Ok(Box::new(tar_write::TarArchiveWriter::append(
            path, kind, limits, cancel,
        )?)),
        other => Err(ArchiveError::Unsupported(FailReason::AppendUnsupported {
            format: other.label().to_owned(),
        })),
    }
}

/// 该容器是否支持追加（zip 原位追加；tar 家族先解包重建再原子替换）。
pub fn supports_append(kind: ArchiveKind) -> bool {
    matches!(
        kind,
        ArchiveKind::Zip
            | ArchiveKind::SevenZ
            | ArchiveKind::Tar
            | ArchiveKind::TarGz
            | ArchiveKind::TarBz2
            | ArchiveKind::TarXz
            | ArchiveKind::TarZst
            | ArchiveKind::TarLz4
    )
}
