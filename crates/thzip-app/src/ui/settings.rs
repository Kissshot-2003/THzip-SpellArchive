use eframe::egui;

use crate::app::ThzipApp;
use crate::i18n::{Locale, TextKey};
use crate::theme::ResKey;

pub fn show(app: &mut ThzipApp, root: &mut egui::Ui) {
    if !app.show_settings {
        return;
    }

    let ctx = root.ctx().clone();
    let prev_standard = app.tr.prefer_standard;
    let prev_close_to_tray = app.close_to_tray;
    let shell_installed = crate::shell_menu::is_installed();
    let mut shell_want = shell_installed;
    let mut import_theme = false;
    let mut export_theme = false;
    let mut open = true;
    let title = app.tr.t(TextKey::Settings);
    let theme_label = app.tr.t(TextKey::Theme);
    let language_label = app.tr.t(TextKey::Language);
    let standard_terms = app.tr.t(TextKey::StandardTerms);
    let close_to_tray_label = app.tr.t(TextKey::CloseToTray);
    let shell_menu_label = app.tr.t(TextKey::ShellMenu);
    let import_theme_label = app.tr.t(TextKey::ImportTheme);
    let export_theme_label = app.tr.t(TextKey::ExportTheme);
    let feature_summary = app.tr.t(TextKey::FeatureSummary);
    let names = app.themes.names();
    let mut selected = app.themes.current_index();
    let theme = app.themes.current();
    let avatar = theme
        .texture(ResKey::AvatarMain)
        .map(|texture| texture.id());
    let palette = theme.palette;
    let theme_name = theme.name.clone();
    let theme_author = theme.author.clone();
    let theme_version = theme.version.clone();
    let theme_license = theme.license.clone();
    let theme_description = theme.description.clone();
    let theme_resources = theme.resources.len();
    let theme_sampling_pixel = theme.sampling_pixel;
    let theme_scale = theme.scale;
    let sampling = if theme_sampling_pixel {
        app.tr.t(TextKey::SamplingNearest)
    } else {
        app.tr.t(TextKey::SamplingLinear)
    };
    let theme_stats = app.tr.tf(
        TextKey::ThemeStats,
        &[
            ("resources", &theme_resources.to_string()),
            ("sampling", &sampling),
            ("scale", &theme_scale.to_string()),
        ],
    );
    let limits_info = app.tr.tf(
        TextKey::LimitsInfo,
        &[
            (
                "total",
                &(app.limits.max_total_bytes / (1024 * 1024 * 1024)).to_string(),
            ),
            (
                "entry",
                &(app.limits.max_entry_bytes / (1024 * 1024 * 1024)).to_string(),
            ),
            ("entries", &app.limits.max_entries.to_string()),
        ],
    );
    let config_label = crate::prefs::config_path().map(|path| {
        app.tr.tf(
            TextKey::ConfigPath,
            &[("path", &path.display().to_string())],
        )
    });
    let log_label = crate::prefs::log_path().map(|path| {
        app.tr
            .tf(TextKey::LogPath, &[("path", &path.display().to_string())])
    });

    egui::Window::new(title)
        .open(&mut open)
        .resizable(true)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .default_size([520.0, 660.0])
        .scroll([false, true])
        .show(&ctx, |ui| {
            egui::Frame::default()
                .fill(palette.bg_panel)
                .stroke(egui::Stroke::new(1.0, palette.border.gamma_multiply(0.65)))
                .corner_radius(egui::CornerRadius::same(9))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(id) = avatar {
                            egui::Frame::default()
                                .fill(palette.accent_soft)
                                .corner_radius(egui::CornerRadius::same(9))
                                .inner_margin(egui::Margin::same(6))
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::Image::new(egui::load::SizedTexture::new(
                                            id,
                                            egui::vec2(56.0, 56.0),
                                        ))
                                        .fit_to_exact_size(egui::vec2(56.0, 56.0)),
                                    );
                                });
                        }
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(theme_name.clone()).size(18.0).strong());
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} · v{} · {}",
                                    theme_author, theme_version, theme_license
                                ))
                                .size(12.0)
                                .color(palette.text_sub),
                            );
                            ui.label(
                                egui::RichText::new(theme_description.clone())
                                    .size(12.0)
                                    .color(palette.text_sub),
                            );
                        });
                    });
                });

            ui.add_space(16.0);
            ui.label(
                egui::RichText::new(theme_label)
                    .size(14.0)
                    .strong()
                    .color(palette.accent),
            );
            ui.add_space(6.0);
            egui::ComboBox::from_id_salt("settings_theme")
                .selected_text(names[selected].clone())
                .show_ui(ui, |ui| {
                    for (index, name) in names.iter().enumerate() {
                        ui.selectable_value(&mut selected, index, name);
                    }
                });

            ui.add_space(14.0);
            ui.label(
                egui::RichText::new(language_label)
                    .size(14.0)
                    .strong()
                    .color(palette.accent),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut app.locale, Locale::ZhCn, "简体中文");
                ui.selectable_value(&mut app.locale, Locale::EnUs, "English");
                ui.checkbox(&mut app.tr.prefer_standard, standard_terms);
            });

            ui.add_space(14.0);
            ui.checkbox(&mut app.close_to_tray, close_to_tray_label);
            if cfg!(windows) {
                ui.add_space(4.0);
                ui.checkbox(&mut shell_want, shell_menu_label);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if crate::ui::primary_button(ui, &palette, import_theme_label).clicked() {
                    import_theme = true;
                }
                if crate::ui::primary_button(ui, &palette, export_theme_label).clicked() {
                    export_theme = true;
                }
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(6.0);
            egui::CollapsingHeader::new(egui::RichText::new(theme_stats).size(12.0))
                .default_open(false)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(limits_info)
                            .size(12.0)
                            .color(palette.text_sub),
                    );
                    ui.label(
                        egui::RichText::new(feature_summary)
                            .size(12.0)
                            .color(palette.text_sub),
                    );
                    if let Some(label) = config_label {
                        ui.label(
                            egui::RichText::new(label)
                                .size(11.0)
                                .color(palette.text_sub),
                        );
                    }
                    if let Some(label) = log_label {
                        ui.label(
                            egui::RichText::new(label)
                                .size(11.0)
                                .color(palette.text_sub),
                        );
                    }
                });
        });

    if selected != app.themes.current_index() {
        app.set_theme(selected, &ctx);
    }
    app.sync_locale();
    if app.tr.prefer_standard != prev_standard || app.close_to_tray != prev_close_to_tray {
        app.touch_settings();
    }

    if import_theme {
        app.import_theme_dialog();
    }
    if export_theme {
        app.export_theme_dialog();
    }

    if shell_want != shell_installed {
        let result = if shell_want {
            match std::env::current_exe() {
                Ok(exe) => crate::shell_menu::install(&exe, app.locale),
                Err(err) => {
                    let text = app
                        .tr
                        .tf(TextKey::ExePathFailed, &[("error", &err.to_string())]);
                    Err(std::io::Error::other(text))
                }
            }
        } else {
            crate::shell_menu::uninstall()
        };
        let status = match result {
            Ok(()) if shell_want => app.tr.t(TextKey::ShellMenuAdded),
            Ok(()) => app.tr.t(TextKey::ShellMenuRemoved),
            Err(err) => app
                .tr
                .tf(TextKey::ShellMenuFailed, &[("error", &err.to_string())]),
        };
        app.status = status;
    }

    app.show_settings = open;
}
