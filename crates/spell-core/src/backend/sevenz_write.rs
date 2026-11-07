use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Instant;

use crate::error::ArchiveError;
use crate::safety::atomic_fs::AtomicFile;
use crate::secret::SecretString;
use crate::task::CancelToken;
use crate::types::Method;
use crate::writer::{ArchiveWriter, FinishReport, normalize_entry_name};

pub struct SevenZArchiveWriter {
    inner: Option<sevenz_rust2::ArchiveWriter<AtomicFile>>,
    entries: u32,
    bytes_in: u64,
    started: Instant,
}

impl SevenZArchiveWriter {
    pub fn create(
        path: &Path,
        method: Method,
        password: Option<SecretString>,
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
        methods.push(
            if method == Method::Store {
                sevenz_rust2::EncoderMethod::COPY
            } else {
                sevenz_rust2::EncoderMethod::LZMA2
            }
            .into(),
        );
        writer.set_content_methods(methods);
        writer.set_encrypt_header(password.is_some());

        Ok(Self {
            inner: Some(writer),
            entries: 0,
            bytes_in: 0,
            started: Instant::now(),
        })
    }

    fn writer(&mut self) -> Result<&mut sevenz_rust2::ArchiveWriter<AtomicFile>, ArchiveError> {
        self.inner
            .as_mut()
            .ok_or_else(|| ArchiveError::Other("7z 写入器已中止".to_owned()))
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
        let writer = self
            .inner
            .take()
            .ok_or_else(|| ArchiveError::Other("7z 写入器已中止".to_owned()))?;
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

fn map_sz_error(err: sevenz_rust2::Error) -> ArchiveError {
    match err {
        sevenz_rust2::Error::PasswordRequired => ArchiveError::PasswordRequired,
        sevenz_rust2::Error::MaybeBadPassword(_) => ArchiveError::BadPassword,
        sevenz_rust2::Error::Io(err, _) => ArchiveError::Io(err),
        sevenz_rust2::Error::FileOpen(err, path) => ArchiveError::Open(format!("{path}: {err}")),
        other => ArchiveError::Other(format!("7z 写入失败: {other}")),
    }
}
