pub mod name_codec;
pub mod sevenz;
pub mod tar;
pub mod volume;
pub mod zip;

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::error::ArchiveError;
use crate::reader::ArchiveReader;
use crate::secret::SecretString;
use crate::types::ArchiveKind;

const SNIFF_LEN: usize = 600;

pub fn sniff_file(path: &Path) -> std::io::Result<ArchiveKind> {
    let mut file = File::open(path)?;
    let mut buf = [0u8; SNIFF_LEN];
    let read = file.read(&mut buf)?;
    Ok(classify(
        &buf[..read],
        path.extension().and_then(|e| e.to_str()),
    ))
}

/// magic bytes 优先，扩展名只用于区分复合容器（tar.gz vs gz）。
pub fn classify(header: &[u8], ext: Option<&str>) -> ArchiveKind {
    let ext = ext.unwrap_or("").to_ascii_lowercase();
    let ext_is = |needle: &str| ext == needle || ext.ends_with(&format!(".{needle}"));

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
        return if ext_is("tar.gz") || ext_is("tgz") {
            ArchiveKind::TarGz
        } else {
            ArchiveKind::Gz
        };
    }
    if header.starts_with(b"BZh") {
        return if ext_is("tar.bz2") || ext_is("tbz2") || ext_is("tbz") {
            ArchiveKind::TarBz2
        } else {
            ArchiveKind::Bz2
        };
    }
    if header.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0x00]) {
        return if ext_is("tar.xz") || ext_is("txz") {
            ArchiveKind::TarXz
        } else {
            ArchiveKind::Xz
        };
    }
    if header.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        return if ext_is("tar.zst") || ext_is("tzst") {
            ArchiveKind::TarZst
        } else {
            ArchiveKind::Zstd
        };
    }
    if header.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
        return if ext_is("tar.lz4") || ext_is("tlz4") {
            ArchiveKind::TarLz4
        } else {
            ArchiveKind::Lz4
        };
    }
    if ext_is("tar") {
        return ArchiveKind::Tar;
    }
    ArchiveKind::Unknown
}

pub fn open(
    path: &Path,
    _password: Option<&SecretString>,
) -> Result<Box<dyn ArchiveReader>, ArchiveError> {
    let kind = sniff_file(path).map_err(|err| ArchiveError::Open(err.to_string()))?;
    match kind {
        ArchiveKind::Zip => Ok(Box::new(zip::ZipReader::new(path))),
        ArchiveKind::SevenZ => Ok(Box::new(sevenz::SevenZReader::new(path))),
        ArchiveKind::Tar
        | ArchiveKind::TarGz
        | ArchiveKind::TarBz2
        | ArchiveKind::TarXz
        | ArchiveKind::TarZst
        | ArchiveKind::TarLz4 => Ok(Box::new(tar::TarReader::new(path, kind))),
        ArchiveKind::Rar => Err(ArchiveError::Unsupported(
            "RAR 仅提供可选解压特性，默认构建不支持".to_owned(),
        )),
        ArchiveKind::Gz
        | ArchiveKind::Bz2
        | ArchiveKind::Xz
        | ArchiveKind::Zstd
        | ArchiveKind::Lz4 => Err(ArchiveError::Unsupported(format!(
            "单文件流容器（{}）尚未接入",
            kind.label()
        ))),
        ArchiveKind::Unknown => Err(ArchiveError::Unsupported(
            "无法识别归档格式（magic bytes 不匹配）".to_owned(),
        )),
    }
}
