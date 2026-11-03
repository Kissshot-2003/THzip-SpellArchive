use std::fs::{File, OpenOptions};
use std::io;
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
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "目标路径没有父目录"))?;
    if !parent.exists() {
        std::fs::create_dir_all(parent)?;
    }

    let temp = temp_path(parent);
    let result = (|| -> io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        write(&mut file)?;
        file.sync_all()?;
        drop(file);
        replace_file(&temp, target)
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn temp_path(parent: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    parent.join(format!(
        ".spellarchive.{}.{}.tmp",
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
        let dir = std::env::temp_dir().join(format!("spellarchive-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("a.txt");

        std::fs::write(&target, b"old").unwrap();
        write_atomic(&target, |file| file.write_all(b"new")).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keeps_original_when_write_fails() {
        let dir =
            std::env::temp_dir().join(format!("spellarchive-test-fail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("b.txt");

        std::fs::write(&target, b"keep").unwrap();
        let result = write_atomic(&target, |_| Err(std::io::Error::other("boom")));
        assert!(result.is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"keep");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
