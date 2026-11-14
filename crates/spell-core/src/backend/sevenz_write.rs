use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::backend;
use crate::error::{ArchiveError, FailReason, path_text};
use crate::limits::Limits;
use crate::plan::{DecisionTable, OverwritePolicy};
use crate::safety::ExtractDirSink;
use crate::safety::atomic_fs::AtomicFile;
use crate::secret::SecretString;
use crate::task::CancelToken;
use crate::types::{EntryId, Method};
use crate::writer::{ArchiveWriter, FinishReport, SevenZOptions, normalize_entry_name};

pub struct SevenZArchiveWriter {
    inner: Option<sevenz_rust2::ArchiveWriter<AtomicFile>>,
    entries: u32,
    bytes_in: u64,
    started: Instant,
    /// 追加模式下的暂存目录；finish / Drop 时删除。
    workdir: Option<PathBuf>,
}

impl Drop for SevenZArchiveWriter {
    fn drop(&mut self) {
        if let Some(dir) = self.workdir.take() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

impl SevenZArchiveWriter {
    pub fn create(
        path: &Path,
        method: Method,
        password: Option<SecretString>,
        options: SevenZOptions,
    ) -> Result<Self, ArchiveError> {
        let atomic = AtomicFile::create(path)?;
        let mut writer = sevenz_rust2::ArchiveWriter::new(atomic).map_err(map_sz_error)?;

        let mut methods = Vec::new();
        if let Some(password) = password.as_ref() {
            methods.push(
                sevenz_rust2::encoder_options::AesEncoderOptions::new(
                    sevenz_rust2::Password::from(password.expose()),
                )
                .into(),
            );
        }
        methods.push(if method == Method::Store {
            sevenz_rust2::EncoderMethod::COPY.into()
        } else {
            let mut lzma2 =
                sevenz_rust2::encoder_options::Lzma2Options::from_level(options.level.min(9));
            if options.dict_size > 0 {
                lzma2.set_dictionary_size(options.dict_size);
            }
            sevenz_rust2::EncoderConfiguration::new(sevenz_rust2::EncoderMethod::LZMA2)
                .with_options(lzma2.into())
        });
        writer.set_content_methods(methods);
        writer.set_encrypt_header(password.is_some());

        Ok(Self {
            inner: Some(writer),
            entries: 0,
            bytes_in: 0,
            started: Instant::now(),
            workdir: None,
        })
    }

    /// 追加：解出全部条目到临时目录，重建 7z 并原子替换，之后可继续 `add`。
    ///
    /// 加密归档无法在不知道密码的情况下重建，直接拒绝；新条目保持未加密。
    pub fn append(
        path: &Path,
        method: Method,
        options: SevenZOptions,
    ) -> Result<Self, ArchiveError> {
        let mut reader = match backend::open(path, None) {
            Ok(reader) => reader,
            Err(ArchiveError::PasswordRequired) => {
                return Err(ArchiveError::Unsupported(FailReason::EncryptedRewrite));
            }
            Err(err) => return Err(err),
        };
        if reader.entries().iter().any(|entry| entry.encrypted) {
            return Err(ArchiveError::Unsupported(FailReason::EncryptedRewrite));
        }

        let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
        let workdir = temp_workdir();
        let limits = Limits::default();
        let plan = reader.plan(&ids, &workdir, &limits)?;
        let decisions = DecisionTable {
            default: OverwritePolicy::Overwrite,
            ..Default::default()
        };
        let cancel = CancelToken::new();
        let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
        let report = match reader.extract(&plan, &decisions, &cancel, &mut sink) {
            Ok(report) => report,
            Err(err) => {
                let _ = std::fs::remove_dir_all(&workdir);
                return Err(err);
            }
        };
        let unsupported = report.failed.len() + report.skipped.len();
        if unsupported > 0 {
            let _ = std::fs::remove_dir_all(&workdir);
            return Err(ArchiveError::Unsupported(
                FailReason::AppendUnsupportedEntries { count: unsupported },
            ));
        }
        let entries = reader.entries().to_vec();
        drop(reader);

        let mut writer = Self::create(path, method, None, options)?;
        writer.workdir = Some(workdir);
        for item in &plan.items {
            let Some(entry) = entries.get(item.id as usize) else {
                return Err(ArchiveError::Other(FailReason::EntryMissing { id: item.id }));
            };
            let name = entry.display.trim_end_matches('/');
            let result = if entry.kind.is_dir() {
                writer.add_dir(&item.rel_path, name)
            } else {
                writer.add(&item.rel_path, name, None, &cancel, &|_| {})
            };
            result?;
        }
        Ok(writer)
    }

    fn writer(&mut self) -> Result<&mut sevenz_rust2::ArchiveWriter<AtomicFile>, ArchiveError> {
        self.inner.as_mut().ok_or_else(|| {
            ArchiveError::Other(FailReason::WriterClosed {
                format: "7z".to_owned(),
            })
        })
    }

    fn abort_on<T>(&mut self, result: Result<T, ArchiveError>) -> Result<T, ArchiveError> {
        if result.is_err() {
            self.inner = None;
        }
        result
    }
}

impl ArchiveWriter for SevenZArchiveWriter {
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

            let entry = sevenz_rust2::ArchiveEntry::from_path(src, name);
            let file = File::open(src)?;
            let reader = ProgressReader {
                inner: file,
                cancel,
                progress,
                written: 0,
            };
            self.writer()?
                .push_archive_entry(entry, Some(reader))
                .map_err(map_sz_error)?;
            self.bytes_in = self.bytes_in.saturating_add(meta.len());
            self.entries = self.entries.saturating_add(1);
            Ok(())
        })();

        if result.is_err() && cancel.is_cancelled() {
            self.inner = None;
            return Err(ArchiveError::Cancelled);
        }
        self.abort_on(result)
    }

    fn add_dir(&mut self, _src: &Path, name_in_archive: &str) -> Result<(), ArchiveError> {
        let result = (|| -> Result<(), ArchiveError> {
            let name = normalize_entry_name(name_in_archive)?;
            let entry = sevenz_rust2::ArchiveEntry::new_directory(&name);
            self.writer()?
                .push_archive_entry::<&[u8]>(entry, None)
                .map_err(map_sz_error)?;
            self.entries = self.entries.saturating_add(1);
            Ok(())
        })();
        self.abort_on(result)
    }

    fn set_comment(&mut self, _text: &str) -> Result<(), ArchiveError> {
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<FinishReport, ArchiveError> {
        let writer = self.inner.take().ok_or_else(|| {
            ArchiveError::Other(FailReason::WriterClosed {
                format: "7z".to_owned(),
            })
        })?;
        let mut atomic = writer.finish()?;
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

fn temp_workdir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "spellarchive-7zappend-{}-{nanos}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn map_sz_error(err: sevenz_rust2::Error) -> ArchiveError {    match err {
        sevenz_rust2::Error::PasswordRequired => ArchiveError::PasswordRequired,
        sevenz_rust2::Error::MaybeBadPassword(_) => ArchiveError::BadPassword,
        sevenz_rust2::Error::Io(err, _) => ArchiveError::Io(err),
        sevenz_rust2::Error::FileOpen(err, path) => {
            ArchiveError::Open(FailReason::Raw(format!("{path}: {err}")))
        }
        other => ArchiveError::Other(FailReason::Raw(format!("7z write failed: {other}"))),
    }
}