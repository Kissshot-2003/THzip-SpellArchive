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

#[cfg(windows)]
#[allow(unsafe_code)]
fn available_space_of(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

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

#[cfg(not(windows))]
fn available_space_of(_path: &Path) -> Option<u64> {
    None
}
