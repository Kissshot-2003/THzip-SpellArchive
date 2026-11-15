/// 主窗口几何的可见性校验。
///
/// 保存的位置在多显示器环境下是全局坐标；用户拔掉副屏后，旧坐标可能落在不存在的
/// 区域，窗口虽在运行却完全不可见。这里用 Win32 的"矩形是否命中任一显示器"来判断，
/// 命中失败时返回主屏工作区内的安全位置。
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn rescue_position(pos: [f32; 2], size: [f32; 2]) -> Option<[f32; 2]> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONULL, MONITOR_DEFAULTTOPRIMARY, MONITORINFO,
        MonitorFromRect,
    };

    let rect = RECT {
        left: pos[0] as i32,
        top: pos[1] as i32,
        right: (pos[0] + size[0]) as i32,
        bottom: (pos[1] + size[1]) as i32,
    };

    // SAFETY: 只传入栈上合法指针；MONITORINFO 先按约定填好 cbSize。
    unsafe {
        if !MonitorFromRect(&rect, MONITOR_DEFAULTTONULL).is_null() {
            return None;
        }
        let monitor = MonitorFromRect(&rect, MONITOR_DEFAULTTOPRIMARY);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if monitor.is_null() || GetMonitorInfoW(monitor, &mut info) == 0 {
            return Some([60.0, 60.0]);
        }
        let work = info.rcWork;
        Some([work.left as f32 + 40.0, work.top as f32 + 40.0])
    }
}

#[cfg(not(windows))]
pub fn rescue_position(_pos: [f32; 2], _size: [f32; 2]) -> Option<[f32; 2]> {
    None
}
