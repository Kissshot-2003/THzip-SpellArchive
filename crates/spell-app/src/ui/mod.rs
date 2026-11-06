pub mod archive_view;
pub mod dialogs;
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
                app.touch_settings();
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
                app.open_archive_dialog();
            }

            let extract = app.tr.t(TextKey::Extract);
            let extract_standard = app.tr.standard(TextKey::Extract);
            if action_button(ui, app, ResKey::BtnExtract, extract)
                .on_hover_text(extract_standard)
                .clicked()
            {
                app.begin_extract_dialog();
            }

            let pack = app.tr.t(TextKey::Pack);
            if action_button(ui, app, ResKey::BtnPack, pack).clicked() {
                app.open_pack_dialog();
            }

            let test = app.tr.t(TextKey::TestArchive);
            if action_button(ui, app, ResKey::StateOk, test).clicked() {
                app.start_test();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let settings = app.tr.t(TextKey::Settings);
                if action_button(ui, app, ResKey::BtnSettings, settings).clicked() {
                    app.show_settings = true;
                }

                let language = app.locale.label();
                if ui.button(language).clicked() {
                    app.locale = app.locale.toggled();
                    app.tr.locale = app.locale;
                    app.touch_settings();
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

/// zip 的 DOS 时间按「墙钟数字当 UTC」存放；这里按同样的语义格式化，数字不会漂移。
pub fn format_time(time: Option<std::time::SystemTime>) -> String {
    let Some(time) = time else {
        return "—".to_owned();
    };
    let Ok(duration) = time.duration_since(std::time::UNIX_EPOCH) else {
        return "—".to_owned();
    };
    let secs = duration.as_secs() as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    if !(1..=9999).contains(&year) {
        return "—".to_owned();
    }
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rem / 3_600,
        (rem % 3_600) / 60
    )
}

/// Howard Hinnant 的 civil_from_days。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// 文件拖到窗口上方时的全屏提示；不拦截鼠标事件。
pub fn drag_overlay(app: &SpellApp, ctx: &egui::Context) {
    let hovered: Vec<std::path::PathBuf> = ctx.input(|input| {
        input
            .raw
            .hovered_files
            .iter()
            .filter_map(|file| file.path.clone())
            .collect()
    });
    if hovered.is_empty() {
        return;
    }

    let palette = app.themes.current().palette;
    let rect = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::unique("drag_overlay"),
    ));

    painter.rect_filled(
        rect,
        egui::CornerRadius::ZERO,
        egui::Color32::from_black_alpha(150),
    );
    let inner = rect.shrink(28.0);
    painter.rect_stroke(
        inner,
        egui::CornerRadius::same(10),
        egui::Stroke::new(2.0, palette.accent),
        egui::StrokeKind::Inside,
    );

    let first_is_dir = hovered.iter().any(|path| path.is_dir());
    let title = if first_is_dir {
        "松开以浏览文件夹".to_owned()
    } else {
        "松开以打开归档".to_owned()
    };
    let subtitle = if hovered.len() == 1 {
        hovered[0]
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "1 个文件".to_owned())
    } else {
        format!("{} 个项目", hovered.len())
    };

    painter.text(
        rect.center() - egui::vec2(0.0, 14.0),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(22.0),
        palette.text_normal,
    );
    painter.text(
        rect.center() + egui::vec2(0.0, 18.0),
        egui::Align2::CENTER_CENTER,
        subtitle,
        egui::FontId::proportional(14.0),
        palette.text_sub,
    );
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
