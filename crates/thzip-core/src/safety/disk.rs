use std::path::{Path, PathBuf};

/// 目标盘剩余空间。目标目录可能尚不存在，因此向上找到第一个存在的祖先。
pub fn available_space(path: &Path) -> Option<u64> {
    let mut probe: PathBuf = path.to_path_buf();
    while !probe.exists() {
        if !probe.pop() {
            return None;
        }
    }
    available_space_of(&probe)
}

/// 卷文件系统类型。仅用于给出「换 NTFS」这类建议，判定失败一律返回 `Unknown`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSystem {
    Ntfs,
    Fat32,
    ExFat,
    Other,
    Unknown,
}

impl FileSystem {
    /// FAT32 单文件上限 4 GiB − 1 字节；exFAT/NTFS 无此限制。
    pub fn max_file_size(self) -> Option<u64> {
        match self {
            FileSystem::Fat32 => Some(4 * 1024 * 1024 * 1024 - 1),
            _ => None,
        }
    }
}

pub fn filesystem_of(path: &Path) -> FileSystem {
    // 目标目录可能不存在：先向上找到存在的祖先。
    let mut probe: PathBuf = path.to_path_buf();
    while !probe.exists() {
        if !probe.pop() {
            return FileSystem::Unknown;
        }
    }
    filesystem_of_existing(&probe)
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn filesystem_of_existing(path: &Path) -> FileSystem {
    use windows_sys::Win32::Storage::FileSystem::{GetVolumeInformationW, GetVolumePathNameW};

    let wide = crate::safety::atomic_fs::wide_path(path);

    // 先取挂载点（"C://" / "D://"），再查卷信息。
    let mut volume = vec![0u16; 261];
    let ok = unsafe { GetVolumePathNameW(wide.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) };
    if ok == 0 {
        return FileSystem::Unknown;
    }

    let mut fs_name = vec![0u16; 32];
    let ok = unsafe {
        GetVolumeInformationW(
            volume.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            fs_name.as_mut_ptr(),
            fs_name.len() as u32,
        )
    };
    if ok == 0 {
        return FileSystem::Unknown;
    }

    let end = fs_name
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(fs_name.len());
    let name = String::from_utf16_lossy(&fs_name[..end]).to_ascii_uppercase();
    match name.as_str() {
        "NTFS" => FileSystem::Ntfs,
        "FAT32" => FileSystem::Fat32,
        "EXFAT" => FileSystem::ExFat,
        _ => FileSystem::Other,
    }
}

#[cfg(not(windows))]
fn filesystem_of_existing(_path: &Path) -> FileSystem {
    FileSystem::Unknown
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn available_space_of(path: &Path) -> Option<u64> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide = crate::safety::atomic_fs::wide_path(path);

    let mut free_to_caller: u64 = 0;
    let mut total: u64 = 0;
    let mut total_free: u64 = 0;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_to_caller,
            &mut total,
            &mut total_free,
        )
    };
    if ok == 0 { None } else { Some(free_to_caller) }
}

#[cfg(unix)]
fn available_space_of(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `c_path` 以 NUL 结尾且生命周期覆盖调用；statvfs 只写结构体。
    unsafe {
        let mut stats: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stats) != 0 {
            return None;
        }
        let block = u64::from(stats.f_frsize.max(1));
        Some(u64::from(stats.f_bavail).saturating_mul(block))
    }
}

#[cfg(not(any(windows, unix)))]
fn available_space_of(_path: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::{FileSystem, available_space, filesystem_of};

    #[test]
    fn reports_fat32_file_limit() {
        assert_eq!(FileSystem::Fat32.max_file_size(), Some(4_294_967_295));
        assert_eq!(FileSystem::Ntfs.max_file_size(), None);
        assert_eq!(FileSystem::ExFat.max_file_size(), None);
    }

    #[test]
    fn probes_existing_ancestor_for_space() {
        // 目标目录不存在时也必须给出答案（向上找到存在的祖先），且不得 panic。
        let ghost = std::env::temp_dir()
            .join("thzip-nonexistent-probe-dir")
            .join("a/b");
        let _ = filesystem_of(&ghost);
        let _ = available_space(&ghost);
    }
}
