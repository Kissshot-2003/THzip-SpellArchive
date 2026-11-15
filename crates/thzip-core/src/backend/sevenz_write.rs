use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::backend;
use crate::error::{ArchiveError, FailReason, path_text};
use crate::limits::Limits;
use crate::plan::{DecisionTable, OverwritePolicy, ResolvedAction};
use crate::safety::ExtractDirSink;
use crate::safety::TempDir;
use crate::safety::atomic_fs::AtomicFile;
use crate::secret::SecretString;
use crate::task::CancelToken;
use crate::types::{EntryId, Method};
use crate::writer::{ArchiveWriter, FinishReport, SevenZOptions, normalize_entry_name};

/// 一个固态块允许累积的未压缩字节数。
///
/// 7z 的「固态」语义就是多个文件共用同一个压缩流。这不只是压缩率问题：
/// `push_archive_entry` 每调一次都会新建一整套编码器，200 个小文件就是
/// 200 次字典分配——批量提交才是把编码器摊到多个文件上的唯一办法。
const SOLID_BLOCK_BYTES: u64 = 64 * 1024 * 1024;

/// 一个固态块最多容纳的条目数。除了内存，也限制同时打开的文件句柄数。
const SOLID_BLOCK_ENTRIES: usize = 512;

pub struct SevenZArchiveWriter {
    inner: Option<sevenz_rust2::ArchiveWriter<AtomicFile>>,
    /// 已累积、尚未编码的条目。文件的实际读取推迟到提交时才做，
    /// 因此这里只占很少内存，但能显著减少编码器重建次数。
    pending: Vec<(sevenz_rust2::ArchiveEntry, PathBuf)>,
    pending_bytes: u64,
    entries: u32,
    bytes_in: u64,
    started: Instant,
    /// 追加模式下的暂存目录；finish / Drop 时删除。
    workdir: Option<TempDir>,
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
            let level = options.level.min(9);
            let threads = resolve_threads(options.threads);
            let mut lzma2 = if threads > 1 {
                // 块大小取字典大小——这正是 7-Zip 自己多线程 LZMA2 的默认做法：
                // 块边界切断跨块匹配，损失被限制在「一个字典窗口」之内，
                // 而块数与字典大小同量级，线程不会闲着。
                // 传 0 让库把它钳到字典大小，避免我们猜错预设字典。
                sevenz_rust2::encoder_options::Lzma2Options::from_level_mt(level, threads, 0)
            } else {
                sevenz_rust2::encoder_options::Lzma2Options::from_level(level)
            };
            if options.dict_size > 0 {
                lzma2.set_dictionary_size(options.dict_size);
            }
            sevenz_rust2::EncoderConfiguration::new(sevenz_rust2::EncoderMethod::LZMA2)
                .with_options(lzma2.into())
        });
        writer.set_encrypt_header(password.is_some());
        writer.set_content_methods(methods);

        Ok(Self {
            inner: Some(writer),
            pending: Vec::new(),
            pending_bytes: 0,
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
        limits: Limits,
        cancel: &CancelToken,
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
        let workdir = TempDir::create("thzip-spellarchive-7zappend")?;
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

        // 实际落盘路径可能与 `plan.items[].rel_path` 不同：包内同名条目会被 sink 改名
        // （a.txt → a~1.txt）。必须用 sink 记录的路径读回来，否则会读到别的条目的内容。
        let written: HashMap<EntryId, PathBuf> = sink
            .actions()
            .iter()
            .filter_map(|(id, action)| match action {
                ResolvedAction::Write { path, .. } => Some((*id, path.clone())),
                _ => None,
            })
            .collect();

        // 解压子项会刷新目录 mtime；在写回前恢复归档内记录的时间。
        for item in &plan.items {
            if let Some(entry) = entries.get(item.id as usize)
                && entry.kind.is_dir()
                && let Some(time) = entry.mtime
                && let Some(written_path) = written.get(&item.id)
            {
                let _ = crate::safety::set_mtime(written_path, time);
            }
        }

        let mut writer = Self::create(path, method, None, options)?;
        writer.workdir = Some(workdir);
        for item in &plan.items {
            let Some(entry) = entries.get(item.id as usize) else {
                return Err(ArchiveError::Other(FailReason::EntryMissing {
                    id: item.id,
                }));
            };
            let name = entry.display.trim_end_matches('/');
            let Some(source) = written.get(&item.id) else {
                return Err(ArchiveError::Other(FailReason::EntryMissing {
                    id: item.id,
                }));
            };
            let result = if entry.kind.is_dir() {
                writer.add_dir(source, name)
            } else {
                writer.add(source, name, None, cancel, &|_| {})
            };
            result?;
        }
        Ok(writer)
    }

    /// 把累积的条目作为一个固态块编码写出。
    ///
    /// 一个块共用一个压缩流和一套编码器，`N` 个文件只付一次字典分配与
    /// 一次线程池启动——这是 7z 创建速度的关键。
    fn flush_batch(
        &mut self,
        cancel: &CancelToken,
        progress: &dyn Fn(u64),
    ) -> Result<(), ArchiveError> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let batch = std::mem::take(&mut self.pending);
        self.pending_bytes = 0;

        let mut entries = Vec::with_capacity(batch.len());
        let mut readers = Vec::with_capacity(batch.len());
        for (entry, src) in batch {
            let file = File::open(&src).map_err(|err| {
                ArchiveError::Other(FailReason::ReadFailed {
                    path: path_text(&src),
                    detail: err.to_string(),
                })
            })?;
            entries.push(entry);
            readers.push(sevenz_rust2::SourceReader::new(ProgressReader {
                inner: file,
                cancel,
                progress,
                written: 0,
            }));
        }

        let result = self
            .writer()?
            .push_archive_entries(entries, readers)
            .map_err(map_sz_error);
        // 取消发生在批量编码中间时，读端返回的是 IO 错误；必须翻译回 Cancelled，
        // 否则界面只会看到一条「读取失败」而不知道是自己点的取消。
        if result.is_err() && cancel.is_cancelled() {
            self.inner = None;
            return Err(ArchiveError::Cancelled);
        }
        result.map(|_| ())
    }

    /// 批次达到阈值就提交；否则继续攒。
    fn maybe_flush(
        &mut self,
        cancel: &CancelToken,
        progress: &dyn Fn(u64),
    ) -> Result<(), ArchiveError> {
        if self.pending_bytes >= SOLID_BLOCK_BYTES || self.pending.len() >= SOLID_BLOCK_ENTRIES {
            return self.flush_batch(cancel, progress);
        }
        Ok(())
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

            let mut entry = sevenz_rust2::ArchiveEntry::from_path(src, name);
            entry.has_windows_attributes = true;
            entry.windows_attributes = 0x20; // FILE_ATTRIBUTE_ARCHIVE
            if meta.permissions().readonly() {
                entry.windows_attributes |= 0x01; // FILE_ATTRIBUTE_READONLY
            }
            self.pending_bytes = self.pending_bytes.saturating_add(meta.len());
            self.pending.push((entry, src.to_path_buf()));
            self.bytes_in = self.bytes_in.saturating_add(meta.len());
            self.entries = self.entries.saturating_add(1);
            self.maybe_flush(cancel, progress)?;
            Ok(())
        })();

        if result.is_err() && cancel.is_cancelled() {
            self.inner = None;
            return Err(ArchiveError::Cancelled);
        }
        self.abort_on(result)
    }

    fn add_dir(&mut self, src: &Path, name_in_archive: &str) -> Result<(), ArchiveError> {
        let result = (|| -> Result<(), ArchiveError> {
            // 目录条目没有数据流，无法并入固态块；先把手上的文件块落下去，
            // 目录在归档里的位置才不会跑到前面去。
            let noop = |_: u64| {};
            self.flush_batch(&CancelToken::new(), &noop)?;
            let name = normalize_entry_name(name_in_archive)?;
            let mut entry = sevenz_rust2::ArchiveEntry::from_path(src, name);
            entry.has_stream = false;
            entry.is_directory = true;
            entry.has_windows_attributes = true;
            entry.windows_attributes = 0x10; // FILE_ATTRIBUTE_DIRECTORY
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
        let noop = |_: u64| {};
        self.flush_batch(&CancelToken::new(), &noop)?;
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

/// 并行线程数上限。再多也压不动（IO 与内存带宽会先成为瓶颈），
/// 而且每块都要占一份字典大小的内存。
const LZMA2_MAX_THREADS: u32 = 8;

/// 把「0 = 自动」解析成实际线程数。
fn resolve_threads(requested: u32) -> u32 {
    if requested > 0 {
        return requested.clamp(1, LZMA2_MAX_THREADS);
    }
    std::thread::available_parallelism()
        .map(|count| (count.get() as u32).min(LZMA2_MAX_THREADS))
        .unwrap_or(1)
        .max(1)
}

fn map_sz_error(err: sevenz_rust2::Error) -> ArchiveError {
    match err {
        sevenz_rust2::Error::PasswordRequired => ArchiveError::PasswordRequired,
        sevenz_rust2::Error::MaybeBadPassword(_) => ArchiveError::BadPassword,
        sevenz_rust2::Error::Io(err, _) => ArchiveError::Io(err),
        sevenz_rust2::Error::FileOpen(err, path) => {
            ArchiveError::Open(FailReason::Raw(format!("{path}: {err}")))
        }
        other => ArchiveError::Other(FailReason::Raw(format!("7z write failed: {other}"))),
    }
}
