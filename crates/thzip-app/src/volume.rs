use std::path::Path;

/// 目标目录所在卷是否是 FAT32。
///
/// FAT32 单文件上限 4 GiB，超出时解压一定失败；预检阶段提前提示比写到一半再报错更好。
/// 路径不存在或无法查询时返回 false，宁可漏提示也不误报。
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn is_fat32(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;

    // GetVolumeInformationW 需要卷根（"C:\\"）；只处理本地盘符，UNC 走默认不提示。
    let drive = match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => letter,
            _ => return false,
        },
        _ => return false,
    };
    let root: Vec<u16> = format!("{}:\\", drive as char)
        .encode_utf16()
        .chain(Some(0))
        .collect();

    let mut name = [0u16; 32];
    // SAFETY: root 以 NUL 结尾；其余输出缓冲要么是合法可写指针，要么允许传 null。
    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            name.as_mut_ptr(),
            name.len() as u32,
        )
    };
    if ok == 0 {
        return false;
    }
    let end = name
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(name.len());
    String::from_utf16_lossy(&name[..end]).eq_ignore_ascii_case("FAT32")
}

#[cfg(not(windows))]
pub fn is_fat32(_path: &Path) -> bool {
    false
}
