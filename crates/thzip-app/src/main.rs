#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod i18n;
mod logging;
mod prefs;
mod shell_menu;
mod startup;
mod theme;
mod tray;
mod ui;

use eframe::egui;
use single_instance::SingleInstance;

fn main() -> eframe::Result<()> {
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

    let prefs = prefs::Prefs::load();
    let window_size = prefs
        .window_size
        .map(|[width, height]| [width.max(960.0), height.max(600.0)])
        .unwrap_or([1280.0, 820.0]);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(window_size)
            .with_min_inner_size([960.0, 600.0])
            .with_title("THzip-SpellArchive"),
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
    if !request.paths.is_empty() {
        if let Some(handoff) = prefs::handoff_path() {
            if let Some(parent) = handoff.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&handoff, request.to_handoff());
        }
    }
    activate_existing_window();
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn activate_existing_window() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow,
    };

    let title: Vec<u16> = "THzip-SpellArchive\0".encode_utf16().collect();
    let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if !hwnd.is_null() {
        unsafe {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
fn activate_existing_window() {}
