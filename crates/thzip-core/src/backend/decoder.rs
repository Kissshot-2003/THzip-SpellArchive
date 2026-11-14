use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::error::{ArchiveError, FailReason};
use crate::types::ArchiveKind;

/// 打开 tar 容器或单文件流的解压读取器。调用方负责按需再包一层 tar。
pub(crate) fn open_reader(path: &Path, kind: ArchiveKind) -> Result<Box<dyn Read>, ArchiveError> {
    let file = File::open(path)?;
    let reader: Box<dyn Read> = match kind {
        ArchiveKind::Tar => Box::new(file),
        ArchiveKind::TarGz | ArchiveKind::Gz => Box::new(flate2::read::GzDecoder::new(file)),
        ArchiveKind::TarBz2 | ArchiveKind::Bz2 => Box::new(bzip2::read::BzDecoder::new(file)),
        ArchiveKind::TarXz | ArchiveKind::Xz => Box::new(lzma_rust2::XzReader::new(file, false)),
        ArchiveKind::TarZst | ArchiveKind::Zstd => {
            Box::new(zstd::stream::read::Decoder::new(file)?)
        }
        ArchiveKind::TarLz4 | ArchiveKind::Lz4 => {
            Box::new(lz4_flex::frame::FrameDecoder::new(file))
        }
        other => {
            return Err(ArchiveError::Unsupported(FailReason::NotDecompressible {
                format: other.label().to_owned(),
            }));
        }
    };
    Ok(reader)
}
