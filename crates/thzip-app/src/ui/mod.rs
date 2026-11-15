pub mod archive_view;
pub mod commands;
pub mod dialogs;
pub mod file_browser;
pub mod nine_patch;
pub mod particles;
pub mod settings;
#[cfg(test)]
mod smoke;
pub mod task_panel;
pub mod thzip_progress;

use eframe::egui;

use crate::app::ThzipApp;
use crate::i18n::TextKey;
use crate::theme::{Palette, ResKey};

pub fn card_frame(palette: &Palette) -> egui::Frame {
    egui::Frame::default()
        .fill(palette.bg_panel)
        .stroke(egui::Stroke::new(1.0, palette.border.gamma_multiply(0.72)))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(12))
}

pub fn primary_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    ui.add(
        egui::Button::new(text.into().color(palette.on_accent()))
            .fill(palette.accent)
            .stroke(egui::Stroke::new(1.0, palette.accent))
            .corner_radius(egui::CornerRadius::same(6)),
    )
}

pub fn danger_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    ui.add(
        egui::Button::new(text)
            .fill(palette.error.gamma_multiply(0.2))
            .stroke(egui::Stroke::new(1.0, palette.error.gamma_multiply(0.75)))
            .corner_radius(egui::CornerRadius::same(6)),
    )
}

/// 带图标的按钮；主题没有该资源时退回纯文字。
pub fn action_button(
    ui: &mut egui::Ui,
    app: &ThzipApp,
    key: ResKey,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    styled_action_button(ui, app, key, text, false)
}

pub fn primary_action_button(
    ui: &mut egui::Ui,
    app: &ThzipApp,
    key: ResKey,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    styled_action_button(ui, app, key, text, true)
}

fn styled_action_button(
    ui: &mut egui::Ui,
    app: &ThzipApp,
    key: ResKey,
    text: impl Into<egui::WidgetText>,
    primary: bool,
) -> egui::Response {
    let palette = app.themes.palette();
    let mut text = text.into();
    if primary {
        text = text.color(palette.on_accent());
    }
    let button = if let Some(texture) = app.themes.texture(key) {
        let image = egui::Image::new(egui::load::SizedTexture::new(
            texture.id(),
            egui::vec2(16.0, 16.0),
        ));
        egui::Button::image_and_text(image, text)
    } else {
        egui::Button::new(text)
    };
    let button = if primary {
        button
            .fill(palette.accent)
            .stroke(egui::Stroke::new(1.0, palette.accent))
            .corner_radius(egui::CornerRadius::same(6))
    } else {
        button
    };
    ui.add(button)
}

pub fn hint_banner(app: &mut ThzipApp, root: &mut egui::Ui) {
    if !app.show_first_run_hint {
        return;
    }
    let text = app.tr.t(TextKey::FirstRunHint);
    let dismiss = app.tr.t(TextKey::Dismiss);
    let palette = app.themes.palette();

    egui::Panel::top("first_run_hint")
        .frame(
            egui::Frame::default()
                .fill(palette.bg_window)
                .inner_margin(egui::Margin::symmetric(14, 0)),
        )
        .show(root, |ui| {
            egui::Frame::default()
                .fill(palette.accent_soft.gamma_multiply(0.7))
                .stroke(egui::Stroke::new(1.0, palette.accent.gamma_multiply(0.55)))
                .corner_radius(egui::CornerRadius::same(7))
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new("!").strong().color(palette.accent));
                        ui.label(egui::RichText::new(text).color(palette.text_sub));
                        if ui.small_button(dismiss).clicked() {
                            app.show_first_run_hint = false;
                            app.touch_settings();
                        }
                    });
                });
        });
}

pub fn tab_bar(app: &mut ThzipApp, root: &mut egui::Ui) {
    if app.tab_count() <= 1 && app.archive_name.is_empty() {
        return;
    }

    let mut switch = None;
    let mut close = None;
    let close_tab = app.tr.t(TextKey::CloseTab);
    egui::Panel::top("tab_bar").show(root, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.add_space(2.0);
            for index in 0..app.tab_count() {
                let title = app.tab_title(index);
                let active = index == app.active_tab_index();
                if ui.selectable_label(active, title).clicked() {
                    switch = Some(index);
                }
                if ui.small_button("×").on_hover_text(&close_tab).clicked() {
                    close = Some(index);
                }
                ui.separator();
            }
        });
    });

    if let Some(index) = switch {
        app.activate_tab(index);
    }
    if let Some(index) = close {
        app.close_tab(index);
    }
}

pub fn top_bar(app: &mut ThzipApp, root: &mut egui::Ui) {
    let palette = app.themes.palette();

    egui::Panel::top("toolbar")
        .frame(
            egui::Frame::default()
                .fill(palette.bg_window)
                .stroke(egui::Stroke::new(1.0, palette.border.gamma_multiply(0.55)))
                .inner_margin(egui::Margin::symmetric(14, 10)),
        )
        .show(root, |ui| {
            let compact = ui.available_width() < 1060.0;
            if compact {
                ui.horizontal(|ui| {
                    brand(ui, app);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        window_actions(ui, app);
                    });
                });
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    archive_actions(ui, app);
                    theme_picker(ui, app);
                });
            } else {
                ui.horizontal(|ui| {
                    brand(ui, app);
                    ui.separator();
                    archive_actions(ui, app);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        window_actions(ui, app);
                        theme_picker(ui, app);
                    });
                });
            }
        });
}

fn brand(ui: &mut egui::Ui, app: &ThzipApp) {
    let palette = app.themes.palette();
    egui::Frame::default()
        .fill(palette.bg_panel)
        .stroke(egui::Stroke::new(1.0, palette.border))
        .corner_radius(egui::CornerRadius::same(7))
        .inner_margin(egui::Margin::same(5))
        .show(ui, |ui| {
            if let Some(texture) = app.themes.texture(ResKey::AvatarMain) {
                ui.add(
                    egui::Image::new(egui::load::SizedTexture::new(
                        texture.id(),
                        egui::vec2(24.0, 24.0),
                    ))
                    .fit_to_exact_size(egui::vec2(24.0, 24.0)),
                );
            } else {
                let (logo, _) =
                    ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(logo, egui::CornerRadius::same(4), palette.accent);
                ui.painter().text(
                    logo.center(),
                    egui::Align2::CENTER_CENTER,
                    "S",
                    egui::FontId::proportional(15.0),
                    palette.on_accent(),
                );
            }
        });
    ui.vertical(|ui| {
        ui.add_space(1.0);
        ui.label(
            egui::RichText::new("THzip-SpellArchive")
                .size(17.0)
                .strong(),
        );
        ui.label(
            egui::RichText::new("ARCHIVE WORKSPACE")
                .size(9.0)
                .color(palette.text_sub),
        );
    });
}

fn archive_actions(ui: &mut egui::Ui, app: &mut ThzipApp) {
    let open = app.tr.t(TextKey::Open);
    if primary_action_button(ui, app, ResKey::IconArchive7z, open).clicked() {
        app.open_archive_dialog();
    }

    let has_archive = app.reader.is_some();
    ui.add_enabled_ui(has_archive, |ui| {
        let extract = app.tr.t(TextKey::Extract);
        let extract_standard = app.tr.standard(TextKey::Extract);
        if action_button(ui, app, ResKey::BtnExtract, extract)
            .on_hover_text(extract_standard)
            .clicked()
        {
            app.begin_extract_dialog();
        }
    });

    let pack = app.tr.t(TextKey::Pack);
    if action_button(ui, app, ResKey::BtnPack, pack).clicked() {
        app.open_pack_dialog();
    }

    ui.add_enabled_ui(has_archive, |ui| {
        let test = app.tr.t(TextKey::TestArchive);
        if action_button(ui, app, ResKey::StateOk, test).clicked() {
            app.start_test();
        }
    });
}

fn theme_picker(ui: &mut egui::Ui, app: &mut ThzipApp) {
    let ctx = ui.ctx().clone();
    let names = app.themes.names();
    let mut selected = app.themes.current_index();
    egui::ComboBox::from_id_salt("theme_combo")
        .selected_text(app.themes.current().name.clone())
        .width(170.0)
        .show_ui(ui, |ui| {
            for (index, name) in names.iter().enumerate() {
                ui.selectable_value(&mut selected, index, name);
            }
        });
    if selected != app.themes.current_index() {
        app.set_theme(selected, &ctx);
    }
}

fn window_actions(ui: &mut egui::Ui, app: &mut ThzipApp) {
    let settings = app.tr.t(TextKey::Settings);
    if action_button(ui, app, ResKey::BtnSettings, settings).clicked() {
        app.show_settings = true;
    }

    let language = app.locale.label();
    if ui.button(language).clicked() {
        app.locale = app.locale.toggled();
        app.sync_locale();
    }
}

pub fn status_bar(app: &mut ThzipApp, root: &mut egui::Ui) {
    let palette = app.themes.palette();
    let status = app.status.clone();
    let theme_name = app.themes.current().name.clone();
    let locale = app.locale.label();
    let total = app.entries.len();
    let selected = match app.selected.len() {
        0 => String::new(),
        1 => app
            .selected
            .iter()
            .next()
            .and_then(|&index| app.entries.get(index))
            .map(|entry| {
                app.tr
                    .tf(TextKey::SelectedOne, &[("name", entry.display.as_ref())])
            })
            .unwrap_or_default(),
        count => app
            .tr
            .tf(TextKey::SelectedMany, &[("count", &count.to_string())]),
    };
    let suffix = app.tr.tf(
        TextKey::StatusSuffix,
        &[
            ("theme", &theme_name),
            ("locale", locale),
            ("total", &total.to_string()),
        ],
    );

    egui::Panel::bottom("status_bar")
        .exact_size(30.0)
        .frame(
            egui::Frame::default()
                .fill(palette.bg_window)
                .inner_margin(egui::Margin::symmetric(14, 0)),
        )
        .show(root, |ui| {
            ui.horizontal_centered(|ui| {
                let (dot, _) = ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
                ui.painter()
                    .circle_filled(dot.center(), 3.5, palette.success.gamma_multiply(0.85));
                let status_width = (ui.available_width() - 380.0).max(120.0);
                ui.add_sized(
                    [status_width, 20.0],
                    egui::Label::new(
                        egui::RichText::new(&status)
                            .size(12.0)
                            .color(palette.text_sub),
                    )
                    .truncate(),
                )
                .on_hover_text(&status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.available_width() > 260.0 {
                        ui.label(
                            egui::RichText::new(suffix.clone())
                                .size(11.0)
                                .color(palette.text_sub.gamma_multiply(0.8)),
                        );
                    }
                    if !selected.is_empty() {
                        ui.label(
                            egui::RichText::new(selected)
                                .size(11.0)
                                .color(palette.accent),
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
pub fn drag_overlay(app: &ThzipApp, ctx: &egui::Context) {
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

    let palette = app.themes.palette();
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
        app.tr.t(TextKey::DropBrowseFolder)
    } else {
        app.tr.t(TextKey::DropOpenArchive)
    };
    let subtitle = if hovered.len() == 1 {
        hovered[0]
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| app.tr.t(TextKey::OneFile))
    } else {
        app.tr
            .tf(TextKey::ItemCount, &[("count", &hovered.len().to_string())])
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
