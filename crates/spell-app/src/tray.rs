use std::path::PathBuf;

use image::GenericImageView;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::i18n::{TextKey, Translator};

pub enum TrayAction {
    Show,
    Hide,
    Quit,
}

pub struct Tray {
    _icon: TrayIcon,
    show: MenuId,
    hide: MenuId,
    quit: MenuId,
}

impl Tray {
    pub fn new(tr: &Translator) -> Option<Self> {
        let show = MenuItem::new(tr.t(TextKey::TrayShow), true, None);
        let hide = MenuItem::new(tr.t(TextKey::TrayHide), true, None);
        let quit = MenuItem::new(tr.t(TextKey::TrayQuit), true, None);
        let separator = PredefinedMenuItem::separator();

        let menu = Menu::new();
        menu.append_items(&[&show, &hide, &separator, &quit]).ok()?;

        let mut builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("SpellArchive")
            .with_menu_on_left_click(false);
        if let Some(icon) = load_icon() {
            builder = builder.with_icon(icon);
        }
        let icon = builder.build().ok()?;

        Some(Self {
            _icon: icon,
            show: show.id().clone(),
            hide: hide.id().clone(),
            quit: quit.id().clone(),
        })
    }

    pub fn poll(&self) -> Vec<TrayAction> {
        let mut actions = Vec::new();
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.show {
                actions.push(TrayAction::Show);
            } else if event.id == self.hide {
                actions.push(TrayAction::Hide);
            } else if event.id == self.quit {
                actions.push(TrayAction::Quit);
            }
        }
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if matches!(event, TrayIconEvent::DoubleClick { .. }) {
                actions.push(TrayAction::Show);
            }
        }
        actions
    }
}

fn load_icon() -> Option<tray_icon::Icon> {
    let mut bases = vec![PathBuf::from("themes")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            bases.push(dir.join("themes"));
            bases.push(dir.join("../../themes"));
        }
    }

    for base in bases {
        let path = base.join("reimu").join("png").join("avatar.png");
        let Ok(image) = image::open(&path) else {
            continue;
        };
        let (width, height) = image.dimensions();
        if width == 0 || height == 0 {
            continue;
        }
        if let Ok(icon) = tray_icon::Icon::from_rgba(image.to_rgba8().into_raw(), width, height) {
            return Some(icon);
        }
    }

    // 找不到主题素材时用纯色占位，托盘仍然可用。
    let mut rgba = vec![0u8; 32 * 32 * 4];
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[0xF0, 0x3D, 0x52, 0xFF]);
    }
    tray_icon::Icon::from_rgba(rgba, 32, 32).ok()
}
