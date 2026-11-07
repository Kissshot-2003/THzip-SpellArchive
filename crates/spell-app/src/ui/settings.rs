use eframe::egui;

use crate::app::SpellApp;
use crate::i18n::{Locale, TextKey};
use crate::theme::ResKey;

pub fn show(app: &mut SpellApp, root: &mut egui::Ui) {
    if !app.show_settings {
        return;
    }

    let ctx = root.ctx().clone();
    let prev_locale = app.locale;
    let prev_standard = app.tr.prefer_standard;
    let prev_close_to_tray = app.close_to_tray;
    let shell_installed = crate::shell_menu::is_installed();
    let mut shell_want = shell_installed;
    let mut import_theme = false;
    let mut open = true;
    let title = app.tr.t(TextKey::Settings);
    let theme_label = app.tr.t(TextKey::Theme);
    let language_label = app.tr.t(TextKey::Language);
    let standard_terms = app.tr.t(TextKey::StandardTerms);
    let names = app.themes.names();
    let mut selected = app.themes.current_index();
    let theme = app.themes.current().clone();

    egui::Window::new(title)
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .default_width(440.0)
        .show(&ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(texture) = theme.texture(ResKey::AvatarMain) {
                    ui.add(
                        egui::Image::new(egui::load::SizedTexture::new(
                            texture.id(),
                            egui::vec2(64.0, 64.0),
                        ))
                        .fit_to_exact_size(egui::vec2(64.0, 64.0)),
                    );
                }
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(theme.name.clone()).strong());
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · v{} · {}",
                            theme.author, theme.version, theme.license
                        ))
                        .size(12.0)
                        .color(theme.palette.text_sub),
                    );
                    ui.label(
                        egui::RichText::new(theme.description.clone())
                            .size(12.0)
                            .color(theme.palette.text_sub),
                    );
                });
            });

            ui.add_space(8.0);
            ui.label(theme_label);
            egui::ComboBox::from_id_salt("settings_theme")
                .selected_text(names[selected].clone())
                .show_ui(ui, |ui| {
                    for (index, name) in names.iter().enumerate() {
                        ui.selectable_value(&mut selected, index, name);
                    }
                });

            ui.add_space(6.0);
            ui.checkbox(&mut app.tr.prefer_standard, standard_terms);

            ui.add_space(6.0);
            ui.label(language_label);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut app.locale, Locale::ZhCn, "简体中文");
                ui.selectable_value(&mut app.locale, Locale::EnUs, "English");
            });

            ui.add_space(6.0);
            ui.checkbox(&mut app.close_to_tray, "关闭窗口时最小化到托盘");

            if cfg!(windows) {
                ui.add_space(6.0);
                ui.checkbox(
                    &mut shell_want,
                    "资源管理器右键菜单（Win11 在“显示更多选项”里）",
                );
            }

            ui.add_space(6.0);
            if ui.button("导入主题包…").clicked() {
                import_theme = true;
            }

            ui.separator();
            ui.label(
                egui::RichText::new(format!(
                    "资源：{} 个纹理 · 采样：{} · 基准 {}x",
                    theme.resources.len(),
                    if theme.sampling_pixel { "NEAREST（像素）" } else { "LINEAR" },
                    theme.scale
                ))
                .size(12.0)
                .color(theme.palette.text_sub),
            );
            ui.label(
                egui::RichText::new(format!(
                    "安全限额：总量 {} GiB · 单条 {} GiB · 条目 {}",
                    app.limits.max_total_bytes / (1024 * 1024 * 1024),
                    app.limits.max_entry_bytes / (1024 * 1024 * 1024),
                    app.limits.max_entries
                ))
                .size(12.0),
            );
            ui.label(
                egui::RichText::new(
                    "已实现：zip / 7z / tar 读写、单文件流读取、AES-256、预检、CRC、后台任务、\n取消、密码重试、主题与 i18n 回退、路径净化、限额熔断、原子覆盖、设置持久化、\n日志、单实例、托盘、右键菜单、主题包导入。\n待接入：RAR、拖拽多文件同时打开。",
                )
                .size(12.0)
                .color(theme.palette.text_sub),
            );
            if let Some(path) = crate::prefs::config_path() {
                ui.label(
                    egui::RichText::new(format!("设置文件：{}", path.display()))
                        .size(11.0)
                        .color(theme.palette.text_sub),
                );
            }
            if let Some(path) = crate::prefs::log_path() {
                ui.label(
                    egui::RichText::new(format!("日志文件：{}", path.display()))
                        .size(11.0)
                        .color(theme.palette.text_sub),
                );
            }
        });

    if selected != app.themes.current_index() {
        app.set_theme(selected, &ctx);
    }
    if app.locale != app.tr.locale {
        app.tr.locale = app.locale;
    }
    if app.locale != prev_locale
        || app.tr.prefer_standard != prev_standard
        || app.close_to_tray != prev_close_to_tray
    {
        app.touch_settings();
    }

    if import_theme {
        app.import_theme_dialog();
    }

    if shell_want != shell_installed {
        let result = if shell_want {
            std::env::current_exe()
                .map_err(|err| std::io::Error::other(format!("取 exe 路径失败: {err}")))
                .and_then(|exe| crate::shell_menu::install(&exe))
        } else {
            crate::shell_menu::uninstall()
        };
        app.status = match result {
            Ok(()) if shell_want => "已添加资源管理器右键菜单".to_owned(),
            Ok(()) => "已移除资源管理器右键菜单".to_owned(),
            Err(err) => format!("右键菜单更新失败: {err}"),
        };
    }

    app.show_settings = open;
}
