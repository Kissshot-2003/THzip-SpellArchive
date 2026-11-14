use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 原子写：先写同目录临时文件，成功后替换目标。
///
/// 直接 `File::create(target)` 会截断用户原文件；解压失败/取消时原文件就没了。
pub fn write_atomic(
    target: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let mut atomic = AtomicFile::create(target)?;
    write(atomic.file_mut())?;
    atomic.commit()
}

/// 流式版本的 [`write_atomic`]：分块写入期间目标文件保持不变。
///
/// `Drop` 时若未 [`AtomicFile::commit`]，临时文件会被删除，原文件不受影响。
pub struct AtomicFile {
    temp: PathBuf,
    target: PathBuf,
    file: Option<File>,
    done: bool,
}

impl AtomicFile {
    pub fn create(target: &Path) -> io::Result<Self> {
        let parent = target
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "target has no parent"))?;
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }

        let temp = temp_path(parent);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        Ok(Self {
            temp,
            target: target.to_path_buf(),
            file: Some(file),
            done: false,
        })
    }

    /// 复制现有目标到临时文件并打开为读写模式（供 zip 追加使用）。
    ///
    /// 之后的修改全部落在临时文件；只有 `commit` 成功才会替换原文件。
    pub fn create_copy_of(target: &Path) -> io::Result<Self> {
        let mut source = File::open(target)?;
        let mut atomic = Self::create(target)?;
        io::copy(&mut source, atomic.file_mut())?;
        atomic.reopen_rw()?;
        Ok(atomic)
    }

    /// 把临时文件重新打开为读写模式；调用方在手工复制后使用。
    pub fn reopen_rw(&mut self) -> io::Result<()> {
        let temp = self.temp.clone();
        self.file.take();
        self.file = Some(OpenOptions::new().read(true).write(true).open(&temp)?);
        Ok(())
    }

    pub fn file_mut(&mut self) -> &mut File {
        self.file.as_mut().expect("AtomicFile already committed")
    }

    /// flush + fsync 后原子替换目标。失败时原文件保持不变。
    pub fn commit(mut self) -> io::Result<()> {
        let result = (|| -> io::Result<()> {
            if let Some(file) = self.file.take() {
                file.sync_all()?;
                drop(file);
            }
            replace_file(&self.temp, &self.target)
        })();

        if result.is_ok() {
            self.done = true;
        }
        result
    }
}

impl Drop for AtomicFile {
    fn drop(&mut self) {
        if !self.done {
            self.file.take();
            let _ = std::fs::remove_file(&self.temp);
        }
    }
}

// 供 zip 等需要 Write + Seek 的写入后端直接复用原子覆盖语义。
impl Write for AtomicFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file_mut().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file_mut().flush()
    }
}

impl Read for AtomicFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file_mut().read(buf)
    }
}

impl Seek for AtomicFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.file_mut().seek(pos)
    }
}

/// 目标是否是重解析点（junction / symlink / mount point）。
///
/// 解压时必须拒绝顺着这类目录写出去（TOCTOU），否则恶意包可以先建一个
/// 指向 `C:\Windows` 的 junction，再把后续文件写进系统目录。
pub fn is_reparse_point(path: &Path) -> io::Result<bool> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        Ok(meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
    }
    #[cfg(not(windows))]
    {
        Ok(meta.file_type().is_symlink())
    }
}

fn temp_path(parent: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    parent.join(format!(
        ".thzip-spellarchive.{}.{}.tmp",
        std::process::id(),
        (nanos as u64) ^ nonce
    ))
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn replace_file(src: &Path, dst: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, ReplaceFileW,
    };

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    let src_w = wide(src);
    let dst_w = wide(dst);

    // ReplaceFileW 会保留原文件的 ACL；它要求目标已存在。
    if dst.exists() {
        let ok = unsafe {
            ReplaceFileW(
                dst_w.as_ptr(),
                src_w.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if ok != 0 {
            return Ok(());
        }
    }

    let ok = unsafe {
        MoveFileExW(
            src_w.as_ptr(),
            dst_w.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(windows))]
fn replace_file(src: &Path, dst: &Path) -> io::Result<()> {
    std::fs::rename(src, dst)
}

#[cfg(test)]
mod tests {
    use super::write_atomic;
    use std::io::Write;

    #[test]
    fn replaces_existing_file() {
        let dir =
            std::env::temp_dir().join(format!("thzip-spellarchive-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("a.txt");

        std::fs::write(&target, b"old").unwrap();
        write_atomic(&target, |file| file.write_all(b"new")).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keeps_original_when_write_fails() {
        let dir = std::env::temp_dir().join(format!(
            "thzip-spellarchive-test-fail-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("b.txt");

        std::fs::write(&target, b"keep").unwrap();
        let result = write_atomic(&target, |_| Err(std::io::Error::other("boom")));
        assert!(result.is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"keep");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
