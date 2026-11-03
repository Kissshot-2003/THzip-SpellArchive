pub mod archive_view;
pub mod file_browser;
pub mod settings;
pub mod spell_progress;
pub mod task_panel;

use eframe::egui;

use crate::app::SpellApp;
use crate::i18n::TextKey;
use crate::theme::ResKey;

/// 带图标的按钮；主题没有该资源时退回纯文字。
pub fn action_button(
    ui: &mut egui::Ui,
    app: &SpellApp,
    key: ResKey,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    let text = text.into();
    if let Some(texture) = app.themes.texture(key) {
        let image = egui::Image::new(egui::load::SizedTexture::new(
            texture.id(),
            egui::vec2(16.0, 16.0),
        ));
        ui.add(egui::Button::image_and_text(image, text))
    } else {
        ui.add(egui::Button::new(text))
    }
}

pub fn hint_banner(app: &mut SpellApp, root: &mut egui::Ui) {
    if !app.show_first_run_hint {
        return;
    }
    let text = app.tr.t(TextKey::FirstRunHint);
    let dismiss = app.tr.t(TextKey::Dismiss);
    let palette = app.themes.current().palette;

    egui::Panel::top("first_run_hint").show(root, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(text).color(palette.text_sub));
            if ui.small_button(dismiss).clicked() {
                app.show_first_run_hint = false;
            }
        });
    });
}

pub fn top_bar(app: &mut SpellApp, root: &mut egui::Ui) {
    let palette = app.themes.current().palette;
    let ctx = root.ctx().clone();

    egui::Panel::top("toolbar").show(root, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if let Some(texture) = app.themes.texture(ResKey::AvatarMain) {
                ui.add(
                    egui::Image::new(egui::load::SizedTexture::new(
                        texture.id(),
                        egui::vec2(22.0, 22.0),
                    ))
                    .fit_to_exact_size(egui::vec2(22.0, 22.0)),
                );
            } else {
                let (logo, _) =
                    ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(logo, egui::CornerRadius::same(4), palette.accent);
            }
            ui.label(egui::RichText::new("SpellArchive").size(16.0).strong());
            ui.separator();

            let open = app.tr.t(TextKey::Open);
            if action_button(ui, app, ResKey::IconArchive7z, open).clicked() {
                app.status = "骨架阶段：文件对话框与 zip/7z 后端待接入".to_owned();
            }

            let extract = app.tr.t(TextKey::Extract);
            let extract_standard = app.tr.standard(TextKey::Extract);
            if action_button(ui, app, ResKey::BtnExtract, extract)
                .on_hover_text(extract_standard)
                .clicked()
            {
                app.status = format!("骨架阶段：{} 需要格式后端", extract_standard);
            }

            let pack = app.tr.t(TextKey::Pack);
            if action_button(ui, app, ResKey::BtnPack, pack).clicked() {
                app.status = "骨架阶段：压缩写入后端待接入".to_owned();
            }

            let test = app.tr.t(TextKey::TestArchive);
            if action_button(ui, app, ResKey::StateOk, test).clicked() {
                app.status = "骨架阶段：完整性测试待接入".to_owned();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let settings = app.tr.t(TextKey::Settings);
                if action_button(ui, app, ResKey::BtnSettings, settings).clicked() {
                    app.show_settings = true;
                }

                let language = app.locale.label();
                if ui.button(language).clicked() {
                    app.locale = app.locale.toggled();
                }

                let names = app.themes.names();
                let mut selected = app.themes.current_index();
                egui::ComboBox::from_id_salt("theme_combo")
                    .selected_text(app.themes.current().name.clone())
                    .show_ui(ui, |ui| {
                        for (index, name) in names.iter().enumerate() {
                            ui.selectable_value(&mut selected, index, name);
                        }
                    });
                if selected != app.themes.current_index() {
                    app.set_theme(selected, &ctx);
                }
            });
        });
        ui.add_space(4.0);
    });
}

pub fn status_bar(app: &mut SpellApp, root: &mut egui::Ui) {
    let palette = app.themes.current().palette;
    let status = app.status.clone();
    let theme_name = app.themes.current().name.clone();
    let locale = app.locale.label();
    let total = app.entries.len();
    let selected = app
        .selected
        .and_then(|index| app.entries.get(index))
        .map(|entry| entry.display.to_string())
        .unwrap_or_default();

    egui::Panel::bottom("status_bar")
        .exact_size(26.0)
        .show(root, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(
                    egui::RichText::new(status)
                        .size(12.0)
                        .color(palette.text_sub),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{theme_name} · {locale} · {total} 条"))
                            .size(12.0)
                            .color(palette.text_sub),
                    );
                    if !selected.is_empty() {
                        ui.label(
                            egui::RichText::new(format!("已选：{selected}"))
                                .size(12.0)
                                .color(palette.text_sub),
                        );
                    }
                });
            });
        });
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}
