use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::error::ArchiveError;
use crate::safety::atomic_fs::AtomicFile;
use crate::task::CancelToken;
use crate::types::{ArchiveKind, Method};
use crate::writer::{ArchiveWriter, FinishReport, normalize_entry_name};

enum Sink {
    Tar(tar::Builder<AtomicFile>),
    Gz(tar::Builder<flate2::write::GzEncoder<AtomicFile>>),
    Bz2(tar::Builder<bzip2::write::BzEncoder<AtomicFile>>),
    Xz(Box<tar::Builder<lzma_rust2::XzWriter<AtomicFile>>>),
    Zst(tar::Builder<zstd::stream::write::Encoder<'static, AtomicFile>>),
    Lz4(tar::Builder<lz4_flex::frame::FrameEncoder<AtomicFile>>),
}

impl Sink {
    fn append<R: Read>(
        &mut self,
        header: &mut tar::Header,
        name: &Path,
        data: R,
    ) -> io::Result<()> {
        match self {
            Sink::Tar(builder) => builder.append_data(header, name, data),
            Sink::Gz(builder) => builder.append_data(header, name, data),
            Sink::Bz2(builder) => builder.append_data(header, name, data),
            Sink::Xz(builder) => builder.as_mut().append_data(header, name, data),
            Sink::Zst(builder) => builder.append_data(header, name, data),
            Sink::Lz4(builder) => builder.append_data(header, name, data),
        }
    }

    fn finish(self) -> Result<AtomicFile, ArchiveError> {
        let atomic = match self {
            Sink::Tar(builder) => builder.into_inner()?,
            Sink::Gz(builder) => builder.into_inner()?.finish()?,
            Sink::Bz2(builder) => builder.into_inner()?.finish()?,
            Sink::Xz(builder) => (*builder).into_inner()?.finish()?,
            Sink::Zst(builder) => builder.into_inner()?.finish()?,
            Sink::Lz4(builder) => builder
                .into_inner()?
                .finish()
                .map_err(|err| ArchiveError::Other(format!("lz4 收尾失败: {err}")))?,
        };
        Ok(atomic)
    }
}

pub struct TarArchiveWriter {
    sink: Option<Sink>,
    names: HashSet<String>,
    entries: u32,
    bytes_in: u64,
    started: Instant,
}

impl TarArchiveWriter {
    pub fn create(path: &Path, kind: ArchiveKind) -> Result<Self, ArchiveError> {
        let atomic = AtomicFile::create(path)?;
        let sink = match kind {
            ArchiveKind::Tar => Sink::Tar(tar::Builder::new(atomic)),
            ArchiveKind::TarGz => Sink::Gz(tar::Builder::new(flate2::write::GzEncoder::new(
                atomic,
                flate2::Compression::default(),
            ))),
            ArchiveKind::TarBz2 => Sink::Bz2(tar::Builder::new(bzip2::write::BzEncoder::new(
                atomic,
                bzip2::Compression::default(),
            ))),
            ArchiveKind::TarXz => Sink::Xz(Box::new(tar::Builder::new(
                lzma_rust2::XzWriter::new(atomic, lzma_rust2::XzOptions::default())
                    .map_err(|err| ArchiveError::Other(format!("xz 初始化失败: {err}")))?,
            ))),
            ArchiveKind::TarZst => Sink::Zst(tar::Builder::new(zstd::stream::write::Encoder::new(
                atomic, 3,
            )?)),
            ArchiveKind::TarLz4 => Sink::Lz4(tar::Builder::new(
                lz4_flex::frame::FrameEncoder::new(atomic),
            )),
            other => {
                return Err(ArchiveError::Unsupported(format!(
                    "暂不支持创建 {}",
                    other.label()
                )));
            }
        };

        Ok(Self {
            sink: Some(sink),
            names: HashSet::new(),
            entries: 0,
            bytes_in: 0,
            started: Instant::now(),
        })
    }

    fn sink(&mut self) -> Result<&mut Sink, ArchiveError> {
        self.sink
            .as_mut()
            .ok_or_else(|| ArchiveError::Other("tar 写入器已中止".to_owned()))
    }

    fn reserve_name(&mut self, name: &str) -> Result<(), ArchiveError> {
        if !self.names.insert(name.to_lowercase()) {
            return Err(ArchiveError::Other(format!(
                "归档内重名（Windows 不区分大小写）：{name}"
            )));
        }
        Ok(())
    }
}

impl ArchiveWriter for TarArchiveWriter {
    fn add(
        &mut self,
        src: &Path,
        name_in_archive: &str,
        _method: Option<Method>,
        cancel: &CancelToken,
        progress: &dyn Fn(u64),
    ) -> Result<(), ArchiveError> {
        let result = (|| -> Result<(), ArchiveError> {
            let name = normalize_entry_name(name_in_archive)?;
            let meta = std::fs::symlink_metadata(src)
                .map_err(|err| ArchiveError::Other(format!("无法读取 {}: {err}", src.display())))?;
            if meta.file_type().is_symlink() {
                return Err(ArchiveError::Unsupported(format!(
                    "不写入符号链接：{}",
                    src.display()
                )));
            }
            if !meta.is_file() {
                return Err(ArchiveError::Other(format!(
                    "不是普通文件：{}",
                    src.display()
                )));
            }
            self.reserve_name(&name)?;

            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(meta.len());
            header.set_mode(0o644);
            header.set_mtime(mtime_secs(meta.modified().ok()));
            header.set_cksum();

            let file = File::open(src)?;
            let reader = ProgressReader {
                inner: file,
                cancel,
                progress,
                written: 0,
            };
            self.sink()?.append(&mut header, Path::new(&name), reader)?;
            self.bytes_in = self.bytes_in.saturating_add(meta.len());
            self.entries = self.entries.saturating_add(1);
            Ok(())
        })();

        if result.is_err() {
            self.sink = None;
            if cancel.is_cancelled() {
                return Err(ArchiveError::Cancelled);
            }
        }
        result
    }

    fn add_dir(&mut self, name_in_archive: &str) -> Result<(), ArchiveError> {
        let result = (|| -> Result<(), ArchiveError> {
            let name = normalize_entry_name(name_in_archive)?;
            let name = format!("{name}/");
            self.reserve_name(&name)?;

            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Directory);
            header.set_size(0);
            header.set_mode(0o755);
            header.set_mtime(mtime_secs(Some(SystemTime::now())));
            header.set_cksum();
            self.sink()?
                .append(&mut header, Path::new(&name), io::empty())?;
            self.entries = self.entries.saturating_add(1);
            Ok(())
        })();
        if result.is_err() {
            self.sink = None;
        }
        result
    }

    fn set_comment(&mut self, _text: &str) -> Result<(), ArchiveError> {
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<FinishReport, ArchiveError> {
        let sink = self
            .sink
            .take()
            .ok_or_else(|| ArchiveError::Other("tar 写入器已中止".to_owned()))?;
        let mut atomic = sink.finish()?;
        atomic.flush()?;
        let bytes_out = atomic.file_mut().metadata()?.len();
        atomic.commit()?;

        Ok(FinishReport {
            entries: self.entries,
            bytes_in: self.bytes_in,
            bytes_out,
            elapsed: self.started.elapsed(),
        })
    }
}

struct ProgressReader<'a, R> {
    inner: R,
    cancel: &'a CancelToken,
    progress: &'a dyn Fn(u64),
    written: u64,
}

impl<R: Read> Read for ProgressReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.cancel.is_cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "已取消"));
        }
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.written = self.written.saturating_add(n as u64);
            (self.progress)(self.written);
        }
        Ok(n)
    }
}

fn mtime_secs(time: Option<SystemTime>) -> u64 {
    time.and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
