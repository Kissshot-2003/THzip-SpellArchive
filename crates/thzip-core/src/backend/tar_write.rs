use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::backend;
use crate::error::{ArchiveError, FailReason, path_text};
use crate::limits::Limits;
use crate::plan::{DecisionTable, OverwritePolicy};
use crate::safety::ExtractDirSink;
use crate::safety::TempDir;
use crate::safety::atomic_fs::AtomicFile;
use crate::task::CancelToken;
use crate::types::{ArchiveKind, EntryId, Method};
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
            Sink::Lz4(builder) => builder.into_inner()?.finish().map_err(|err| {
                ArchiveError::Other(FailReason::Raw(format!("lz4 finish failed: {err}")))
            })?,
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
    /// 追加模式下的暂存目录；finish / Drop 时删除。
    workdir: Option<TempDir>,
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
                lzma_rust2::XzWriter::new(atomic, lzma_rust2::XzOptions::default()).map_err(
                    |err| ArchiveError::Other(FailReason::Raw(format!("xz init failed: {err}"))),
                )?,
            ))),
            ArchiveKind::TarZst => Sink::Zst(tar::Builder::new(zstd::stream::write::Encoder::new(
                atomic, 3,
            )?)),
            ArchiveKind::TarLz4 => Sink::Lz4(tar::Builder::new(
                lz4_flex::frame::FrameEncoder::new(atomic),
            )),
            other => {
                return Err(ArchiveError::Unsupported(FailReason::CreateUnsupported {
                    format: other.label().to_owned(),
                }));
            }
        };

        Ok(Self {
            sink: Some(sink),
            names: HashSet::new(),
            entries: 0,
            bytes_in: 0,
            started: Instant::now(),
            workdir: None,
        })
    }

    /// 追加：先把现有条目安全解到临时目录，再重建容器并写回，之后可继续 `add`。
    ///
    /// tar 是顺序格式，没有可靠的原位追加；重写方式与删除 / 改名一致，
    /// 成功后由 `AtomicFile` 原子替换。包含符号链接等无法保留的条目时拒绝追加。
    pub fn append(
        path: &Path,
        kind: ArchiveKind,
        limits: Limits,
        cancel: &CancelToken,
    ) -> Result<Self, ArchiveError> {
        let mut reader = backend::open(path, None)?;
        let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
        let workdir = TempDir::create("thzip-spellarchive-tarappend")?;
        let plan = reader.plan(&ids, workdir.path(), &limits)?;
        let decisions = DecisionTable {
            default: OverwritePolicy::Overwrite,
            ..Default::default()
        };
        let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
        let report = reader.extract(&plan, &decisions, cancel, &mut sink)?;
        let unsupported = report.failed.len() + report.skipped.len();
        if unsupported > 0 {
            return Err(ArchiveError::Unsupported(
                FailReason::AppendUnsupportedEntries { count: unsupported },
            ));
        }
        let entries = reader.entries().to_vec();
        drop(reader);

        // 解压子项会刷新目录 mtime；在写回前恢复归档内记录的时间。
        for item in &plan.items {
            if let Some(entry) = entries.get(item.id as usize)
                && entry.kind.is_dir()
                && let Some(time) = entry.mtime
            {
                let _ = crate::safety::set_mtime(&item.rel_path, time);
            }
        }

        let mut writer = Self::create(path, kind)?;
        writer.workdir = Some(workdir);
        for item in &plan.items {
            let Some(entry) = entries.get(item.id as usize) else {
                return Err(ArchiveError::Other(FailReason::EntryMissing {
                    id: item.id,
                }));
            };
            let name = entry.display.trim_end_matches('/');
            let result = if entry.kind.is_dir() {
                writer.add_dir(&item.rel_path, name)
            } else {
                writer.add(&item.rel_path, name, None, cancel, &|_| {})
            };
            result?;
        }
        Ok(writer)
    }

    fn sink(&mut self) -> Result<&mut Sink, ArchiveError> {
        self.sink.as_mut().ok_or_else(|| {
            ArchiveError::Other(FailReason::WriterClosed {
                format: "tar".to_owned(),
            })
        })
    }

    fn reserve_name(&mut self, name: &str) -> Result<(), ArchiveError> {
        if !self.names.insert(name.to_lowercase()) {
            return Err(ArchiveError::Other(FailReason::DuplicateArchiveName {
                name: name.to_owned(),
            }));
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
            let meta = std::fs::symlink_metadata(src).map_err(|err| {
                ArchiveError::Other(FailReason::ReadFailed {
                    path: path_text(src),
                    detail: err.to_string(),
                })
            })?;
            if meta.file_type().is_symlink() {
                return Err(ArchiveError::Unsupported(FailReason::SymlinkNotAllowed {
                    path: path_text(src),
                }));
            }
            if !meta.is_file() {
                return Err(ArchiveError::Other(FailReason::NotRegularFile {
                    path: path_text(src),
                }));
            }
            self.reserve_name(&name)?;

            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(meta.len());
            // 保留只读属性；其余统一 644（跨平台没有统一 Unix mode）。
            header.set_mode(if meta.permissions().readonly() {
                0o444
            } else {
                0o644
            });
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

    fn add_dir(&mut self, src: &Path, name_in_archive: &str) -> Result<(), ArchiveError> {
        let result = (|| -> Result<(), ArchiveError> {
            let name = normalize_entry_name(name_in_archive)?;
            let name = format!("{name}/");
            self.reserve_name(&name)?;

            let meta = std::fs::symlink_metadata(src).ok();
            let mode = meta
                .as_ref()
                .map(|meta| {
                    if meta.permissions().readonly() {
                        0o555
                    } else {
                        0o755
                    }
                })
                .unwrap_or(0o755);
            let mtime = meta.as_ref().and_then(|meta| meta.modified().ok());

            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Directory);
            header.set_size(0);
            header.set_mode(mode);
            header.set_mtime(mtime_secs(mtime));
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
        let sink = self.sink.take().ok_or_else(|| {
            ArchiveError::Other(FailReason::WriterClosed {
                format: "tar".to_owned(),
            })
        })?;
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
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
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
