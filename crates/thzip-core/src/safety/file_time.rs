use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// 把文件的最后修改时间设为归档条目记录的时间。
///
/// 解压落盘由 `AtomicFile` 完成，替换后临时文件会带上“现在”的时间，
/// 因此需要在 commit 之后单独恢复归档时间。失败不影响解压结果。
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn set_mtime(path: &Path, time: SystemTime) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, OPEN_EXISTING, SetFileTime,
    };

    let Some(file_time) = to_filetime(time) else {
        return Ok(());
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();

    // SAFETY: 路径已按 Windows 宽字符并以 NUL 结尾；句柄在返回前关闭。
    unsafe {
        let handle = CreateFileW(
            wide.as_ptr(),
            FILE_WRITE_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            if path.is_dir() {
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS
            } else {
                FILE_ATTRIBUTE_NORMAL
            },
            std::ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        let result = SetFileTime(handle, std::ptr::null(), std::ptr::null(), &file_time);
        CloseHandle(handle);
        if result == 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(windows)]
fn to_filetime(time: SystemTime) -> Option<windows_sys::Win32::Foundation::FILETIME> {
    // FILETIME 起点是 1601-01-01，单位 100ns。
    const EPOCH_DIFF_SECS: u64 = 11_644_473_600;
    let duration = time.duration_since(UNIX_EPOCH).ok()?;
    let intervals = duration
        .as_secs()
        .checked_add(EPOCH_DIFF_SECS)?
        .checked_mul(10_000_000)?
        .checked_add(u64::from(duration.subsec_nanos() / 100))?;
    Some(windows_sys::Win32::Foundation::FILETIME {
        dwLowDateTime: intervals as u32,
        dwHighDateTime: (intervals >> 32) as u32,
    })
}

#[cfg(unix)]
#[allow(unsafe_code)]
pub fn set_mtime(path: &Path, time: SystemTime) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let Ok(duration) = time.duration_since(UNIX_EPOCH) else {
        // 早于 1970 的时间戳无法用 timespec 表达，静默跳过。
        return Ok(());
    };
    let Ok(text) = CString::new(path.as_os_str().as_bytes()) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path contains NUL",
        ));
    };

    let stamp = libc::timespec {
        tv_sec: duration.as_secs() as libc::time_t,
        tv_nsec: duration.subsec_nanos() as _,
    };
    // UTIME_OMIT 表示「这一项不改」；数组顺序是 [atime, mtime]，
    // 因此第一项设为 OMIT，只改 mtime。
    let times = [
        libc::timespec {
            tv_sec: 0,
            tv_nsec: libc::UTIME_OMIT,
        },
        stamp,
    ];
    // SAFETY: 路径为有效的 NUL 结尾 C 字符串，times 是长度 2 的有效数组。
    let rc = unsafe { libc::utimensat(libc::AT_FDCWD, text.as_ptr(), times.as_ptr(), 0) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(any(windows, unix)))]
pub fn set_mtime(_path: &Path, _time: SystemTime) -> std::io::Result<()> {
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::set_mtime;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn sets_file_modification_time() {
        let path = std::env::temp_dir().join(format!(
            "thzip-spellarchive-mtime-{}-{}.txt",
            std::process::id(),
            UNIX_EPOCH.elapsed().unwrap().as_nanos()
        ));
        std::fs::write(&path, b"x").unwrap();

        let target = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        set_mtime(&path, target).unwrap();
        let actual = std::fs::metadata(&path).unwrap().modified().unwrap();
        let diff = match actual.duration_since(target) {
            Ok(diff) => diff,
            Err(err) => err.duration(),
        };
        assert!(diff < Duration::from_secs(2), "偏差过大: {diff:?}");

        let _ = std::fs::remove_file(&path);
    }
}
