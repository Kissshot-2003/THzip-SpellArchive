use eframe::egui;

use crate::app::SpellApp;
use crate::i18n::{Locale, TextKey};
use crate::theme::ResKey;

pub fn show(app: &mut SpellApp, root: &mut egui::Ui) {
    if !app.show_settings {
        return;
    }

    let ctx = root.ctx().clone();
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
                    "已实现：theme.toml 解析 + PNG 纹理、路径净化、显示名防护、限额计数、原子覆盖、CJK 字体、i18n 回退。\n待接入：zip/7z/tar 后端、文件对话框、任务线程池。",
                )
                .size(12.0)
                .color(theme.palette.text_sub),
            );
        });

    if selected != app.themes.current_index() {
        app.set_theme(selected, &ctx);
    }
    app.show_settings = open;
}
