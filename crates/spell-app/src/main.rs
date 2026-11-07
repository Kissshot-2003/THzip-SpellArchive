#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod i18n;
mod logging;
mod prefs;
mod shell_menu;
mod theme;
mod tray;
mod ui;

use eframe::egui;
use single_instance::SingleInstance;

fn main() -> eframe::Result<()> {
    logging::init();
    let instance = match SingleInstance::new("SpellArchive.v1") {
        Ok(instance) => instance,
        Err(err) => {
            logging::write(&format!("single-instance failed: {err}"));
            eprintln!("单实例检查失败: {err}");
            return Ok(());
        }
    };
    if !instance.is_single() {
        logging::write("second instance: handoff");
        handoff_to_existing();
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("SpellArchive"),
        ..Default::default()
    };

    // 支持「打开方式」/ 拖到 exe 上：第一个参数直接作为归档打开。
    let startup = std::env::args().nth(1).map(std::path::PathBuf::from);

    eframe::run_native(
        "SpellArchive",
        options,
        Box::new(move |cc| {
            let mut app = app::SpellApp::new(cc);
            if let Some(path) = startup {
                if path.is_dir() {
                    app.navigate(path);
                } else {
                    app.open_path(path, None);
                }
            }
            Ok(Box::new(app))
        }),
    )?;

    drop(instance);
    Ok(())
}

fn handoff_to_existing() {
    if let Some(path) = std::env::args().nth(1).map(std::path::PathBuf::from) {
        if path.exists() {
            if let Some(handoff) = prefs::handoff_path() {
                if let Some(parent) = handoff.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&handoff, path.display().to_string());
            }
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

    let title: Vec<u16> = "SpellArchive\0".encode_utf16().collect();
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
