use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::error::ArchiveError;
use crate::safety::atomic_fs::AtomicFile;
use crate::secret::SecretString;
use crate::task::CancelToken;
use crate::types::Method;
use crate::writer::{ArchiveWriter, FinishReport, normalize_entry_name};

const WRITE_BUF: usize = 64 * 1024;

/// zip 创建后端：写同目录临时文件，`finish()` 成功后才原子替换目标。
pub struct ZipArchiveWriter {
    inner: Option<zip::ZipWriter<AtomicFile>>,
    default_method: Method,
    password: Option<SecretString>,
    names: HashSet<String>,
    entries: u32,
    bytes_in: u64,
    started: Instant,
}

impl ZipArchiveWriter {
    pub fn create(path: &Path, method: Method) -> Result<Self, ArchiveError> {
        Self::create_with(path, method, None)
    }

    pub fn create_with(
        path: &Path,
        method: Method,
        password: Option<SecretString>,
    ) -> Result<Self, ArchiveError> {
        let zip_method = map_method(method)?;
        let atomic = AtomicFile::create(path)?;
        let mut inner = zip::ZipWriter::new(atomic);
        inner.set_flush_on_finish_file(true);
        Ok(Self {
            inner: Some(inner),
            default_method: zip_method_to_method(zip_method),
            password,
            names: HashSet::new(),
            entries: 0,
            bytes_in: 0,
            started: Instant::now(),
        })
    }

    /// 追加模式：先复制现有 zip 到临时文件，再在其后追加条目。
    pub fn append(path: &Path, method: Method) -> Result<Self, ArchiveError> {
        Self::append_with(path, method, None)
    }

    pub fn append_with(
        path: &Path,
        method: Method,
        password: Option<SecretString>,
    ) -> Result<Self, ArchiveError> {
        let zip_method = map_method(method)?;

        // 先读出已有名字，避免追加同名条目（大小写不敏感）。
        let mut names = HashSet::new();
        {
            let file = File::open(path)?;
            let mut archive = zip::ZipArchive::new(file).map_err(map_zip_error)?;
            for index in 0..archive.len() {
                if let Ok(entry) = archive.by_index_raw(index) {
                    names.insert(entry.name().to_lowercase());
                }
            }
        }

        let atomic = AtomicFile::create_copy_of(path)?;
        let mut inner = zip::ZipWriter::new_append(atomic).map_err(map_zip_error)?;
        inner.set_flush_on_finish_file(true);
        Ok(Self {
            inner: Some(inner),
            default_method: zip_method_to_method(zip_method),
            password,
            names,
            entries: 0,
            bytes_in: 0,
            started: Instant::now(),
        })
    }

    fn writer(&mut self) -> Result<&mut zip::ZipWriter<AtomicFile>, ArchiveError> {
        self.inner
            .as_mut()
            .ok_or_else(|| ArchiveError::Other("zip 写入器已中止".to_owned()))
    }

    fn reserve_name(&mut self, name: &str) -> Result<(), ArchiveError> {
        let key = name.to_lowercase();
        if !self.names.insert(key) {
            return Err(ArchiveError::Other(format!(
                "归档内重名（Windows 不区分大小写）：{name}"
            )));
        }
        Ok(())
    }

    fn abort_on<T>(&mut self, result: Result<T, ArchiveError>) -> Result<T, ArchiveError> {
        if result.is_err() {
            // 丢弃 ZipWriter → 丢弃 AtomicFile → 临时文件被删除，原目标不变。
            self.inner = None;
        }
        result
    }
}

impl ArchiveWriter for ZipArchiveWriter {
    fn add(
        &mut self,
        src: &Path,
        name_in_archive: &str,
        method: Option<Method>,
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

            let method = method.unwrap_or(self.default_method);
            let zip_method = map_method(method)?;
            let password = self.password.clone();
            let mut options = zip::write::SimpleFileOptions::default()
                .compression_method(zip_method)
                .unix_permissions(0o644)
                .large_file(meta.len() >= u32::MAX as u64);
            if let Ok(modified) = meta.modified() {
                if let Some(dt) = system_time_to_dos(modified) {
                    options = options.last_modified_time(dt);
                }
            }

            if let Some(password) = password.as_ref() {
                options = options.with_aes_encryption(zip::AesMode::Aes256, password.expose());
            }

            let mut file = File::open(src)?;
            self.writer()?
                .start_file(name, options)
                .map_err(map_zip_error)?;

            let mut buf = vec![0u8; WRITE_BUF];
            let mut written = 0u64;
            loop {
                if cancel.is_cancelled() {
                    return Err(ArchiveError::Cancelled);
                }
                let read = file.read(&mut buf)?;
                if read == 0 {
                    break;
                }
                self.writer()?.write_all(&buf[..read])?;
                written = written.saturating_add(read as u64);
                self.bytes_in = self.bytes_in.saturating_add(read as u64);
                progress(written);
            }

            self.entries = self.entries.saturating_add(1);
            Ok(())
        })();

        self.abort_on(result)
    }

    fn add_dir(&mut self, _src: &Path, name_in_archive: &str) -> Result<(), ArchiveError> {
        let result = (|| -> Result<(), ArchiveError> {
            let name = normalize_entry_name(name_in_archive)?;
            self.reserve_name(&name)?;
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
                .unix_permissions(0o755);
            self.writer()?
                .add_directory(name, options)
                .map_err(map_zip_error)?;
            self.entries = self.entries.saturating_add(1);
            Ok(())
        })();
        self.abort_on(result)
    }

    fn set_comment(&mut self, text: &str) -> Result<(), ArchiveError> {
        self.writer()?.set_comment(text);
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<FinishReport, ArchiveError> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| ArchiveError::Other("zip 写入器已中止".to_owned()))?;
        let mut atomic = inner.finish().map_err(map_zip_error)?;
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

fn map_method(method: Method) -> Result<zip::CompressionMethod, ArchiveError> {
    match method {
        Method::Store => Ok(zip::CompressionMethod::Stored),
        Method::Deflate => Ok(zip::CompressionMethod::Deflated),
        Method::Bzip2 => Ok(zip::CompressionMethod::Bzip2),
        Method::Zstd => Ok(zip::CompressionMethod::Zstd),
        other => Err(ArchiveError::Unsupported(format!(
            "zip 创建暂不支持压缩方法 {}",
            other.label()
        ))),
    }
}

fn zip_method_to_method(method: zip::CompressionMethod) -> Method {
    match method {
        zip::CompressionMethod::Stored => Method::Store,
        zip::CompressionMethod::Bzip2 => Method::Bzip2,
        zip::CompressionMethod::Zstd => Method::Zstd,
        _ => Method::Deflate,
    }
}

fn map_zip_error(err: zip::result::ZipError) -> ArchiveError {
    match err {
        zip::result::ZipError::Io(err) => ArchiveError::Io(err),
        other => ArchiveError::Other(format!("zip 写入失败: {other}")),
    }
}

/// 文件修改时间 → DOS 时间（zip 语义是本地墙钟）。失败时保留默认时间。
#[cfg(windows)]
#[allow(unsafe_code)]
fn system_time_to_dos(time: SystemTime) -> Option<zip::DateTime> {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
    use windows_sys::Win32::System::Time::FileTimeToSystemTime;

    const WINDOWS_EPOCH_OFFSET: u64 = 11_644_473_600;

    let since_epoch = time.duration_since(UNIX_EPOCH).ok()?;
    let ticks = since_epoch
        .as_secs()
        .saturating_add(WINDOWS_EPOCH_OFFSET)
        .checked_mul(10_000_000)?
        .saturating_add(since_epoch.subsec_nanos() as u64 / 100);
    let utc = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };

    let mut local = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    if unsafe { FileTimeToLocalFileTime(&utc, &mut local) } == 0 {
        return None;
    }

    let mut st = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    if unsafe { FileTimeToSystemTime(&local, &mut st) } == 0 {
        return None;
    }

    zip::DateTime::from_date_and_time(
        st.wYear,
        st.wMonth as u8,
        st.wDay as u8,
        st.wHour as u8,
        st.wMinute as u8,
        st.wSecond as u8,
    )
    .ok()
}

#[cfg(not(windows))]
fn system_time_to_dos(time: SystemTime) -> Option<zip::DateTime> {
    let since_epoch = time.duration_since(UNIX_EPOCH).ok()?;
    let secs = since_epoch.as_secs() as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    if !(1980..=2107).contains(&year) {
        return None;
    }
    zip::DateTime::from_date_and_time(
        year as u16,
        month as u8,
        day as u8,
        (rem / 3_600) as u8,
        ((rem % 3_600) / 60) as u8,
        (rem % 60) as u8,
    )
    .ok()
}

#[cfg(not(windows))]
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}
