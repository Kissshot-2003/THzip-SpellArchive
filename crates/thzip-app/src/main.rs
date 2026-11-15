#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// 堆分配分析（`--features dhat`）。只在开发排障时开启：
/// 编译时带 `#[global_allocator]`，运行时把每个分配点的累计量写到
/// `dhat-heap.json`，用 https://nnethercote.github.io/dh_view/dh_view.html 打开。
///
/// 放在这里而不是单独的 profile 构建，是因为内存问题几乎都跟具体交互路径
/// 相关（打开大归档、翻预览、切主题），必须在真实界面里跑才复现得出来。
#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

mod app;
mod i18n;
mod limit_fields;
mod logging;
mod prefs;
mod secret_store;
mod shell_menu;
mod startup;
mod theme;
mod tray;
mod ui;
mod volume;
mod window_place;

use eframe::egui;
use single_instance::SingleInstance;

fn main() -> eframe::Result<()> {
    // 必须在任何分配之前建立 profiler，否则启动阶段的分配统计不到。
    #[cfg(feature = "dhat")]
    let _profiler = dhat::Profiler::new_heap();

    logging::init();
    let instance = match SingleInstance::new("THzip-SpellArchive.v1") {
        Ok(instance) => instance,
        Err(err) => {
            logging::error_fields(
                "startup",
                "single-instance failed",
                &[("error", &err.to_string())],
            );
            eprintln!("单实例检查失败: {err}");
            return Ok(());
        }
    };
    if !instance.is_single() {
        logging::write("second instance: handoff");
        handoff_to_existing();
        return Ok(());
    }

    // 升级过渡：旧版仍在运行时，把启动参数交给旧版，避免同时开两个窗口。
    if legacy_instance_running() {
        logging::write("legacy instance running: handoff");
        handoff_to_legacy();
        activate_window("SpellArchive");
        return Ok(());
    }

    let prefs = prefs::Prefs::load();
    sweep_stale_temp_files();
    let window_size = prefs
        .window_size
        .map(|[width, height]| [width.max(960.0), height.max(600.0)])
        .unwrap_or([1280.0, 820.0]);

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(window_size)
        .with_min_inner_size([960.0, 600.0])
        .with_title("THzip-SpellArchive");
    if let Some(pos) = prefs.window_pos {
        // 坐标是否仍落在可见屏幕上由 app 首帧用 Win32 校验后修正。
        viewport = viewport.with_position(pos);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    // 支持「打开方式」/ 拖到 exe 上 / 资源管理器右键动词。
    let request = startup::StartupRequest::parse(std::env::args().skip(1));

    eframe::run_native(
        "THzip-SpellArchive",
        options,
        Box::new(move |cc| Ok(Box::new(app::ThzipApp::new(cc, Some(request))))),
    )?;

    drop(instance);
    Ok(())
}

fn handoff_to_existing() {
    let request = startup::StartupRequest::parse(std::env::args().skip(1));
    write_handoff(prefs::handoff_path(), &request);
    activate_window("THzip-SpellArchive");
}

/// 旧版正在运行时，把启动参数写到旧 AppData 的 handoff 并激活旧窗口。
fn handoff_to_legacy() {
    let request = startup::StartupRequest::parse(std::env::args().skip(1));
    write_handoff(prefs::legacy_handoff_path(), &request);
}

fn write_handoff(path: Option<std::path::PathBuf>, request: &startup::StartupRequest) {
    let Some(path) = path else {
        return;
    };
    if request.paths.is_empty() {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, request.to_handoff());
}

fn legacy_instance_running() -> bool {
    match SingleInstance::new("SpellArchive.v1") {
        Ok(instance) => !instance.is_single(),
        Err(_) => false,
    }
}

/// 清掉上次异常退出留下的解压临时文件。24 小时门槛避免误删另一个实例正在写的文件；
/// 清理失败（目录不存在、无权限）不影响启动，因此全部忽略错误。
fn sweep_stale_temp_files() {
    let age = std::time::Duration::from_secs(24 * 3600);
    if let Some(config_dir) =
        prefs::config_path().and_then(|path| path.parent().map(|p| p.to_path_buf()))
    {
        let removed = sweep_dir(&config_dir, age);
        if removed > 0 {
            logging::debug(
                "startup",
                &format!("swept {removed} stale temp files in config dir"),
            );
        }
    }
    let removed = sweep_dir(&std::env::temp_dir(), age);
    if removed > 0 {
        logging::debug(
            "startup",
            &format!("swept {removed} stale temp files in temp dir"),
        );
    }
}

/// 只认本程序自己的临时文件名（`.thzip-spellarchive.<pid>.<n>.tmp`），
/// 避免误删其他程序写在同一目录的文件。
fn sweep_dir(dir: &std::path::Path, min_age: std::time::Duration) -> usize {
    const PREFIX: &str = ".thzip-spellarchive.";
    const SUFFIX: &str = ".tmp";
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let now = std::time::SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(PREFIX) || !name.ends_with(SUFFIX) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let age = meta
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .unwrap_or_default();
        if age < min_age {
            continue;
        }
        if std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn activate_window(window_title: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow,
    };

    let title: Vec<u16> = window_title.encode_utf16().chain(Some(0)).collect();
    let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if !hwnd.is_null() {
        unsafe {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
fn activate_window(_window_title: &str) {}
