use eframe::egui;

use crate::app::SpellApp;
use crate::i18n::TextKey;
use crate::theme::ResKey;

pub fn show(app: &mut SpellApp, root: &mut egui::Ui) {
    let palette = app.themes.current().palette;
    let local_files = app.tr.t(TextKey::LocalFiles);
    let go_up = app.tr.t(TextKey::GoUp);
    let dir_label = app.dir.display().to_string();
    let count_text = app.tr.tf(
        TextKey::ItemCount,
        &[("count", &app.dir_items.len().to_string())],
    );
    let items = app
        .dir_items
        .iter()
        .map(|item| (item.name.clone(), item.is_dir))
        .collect::<Vec<_>>();

    let folder_icon = app.themes.texture(ResKey::IconFolder).map(|tex| tex.id());
    let file_icon = app.themes.texture(ResKey::IconFile).map(|tex| tex.id());

    egui::Panel::left("file_browser")
        .resizable(true)
        .default_size(244.0)
        .min_size(180.0)
        .frame(
            egui::Frame::default()
                .fill(palette.bg_window)
                .inner_margin(egui::Margin::symmetric(12, 12)),
        )
        .show(root, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(local_files).size(16.0).strong());
                    ui.label(
                        egui::RichText::new(&count_text)
                            .size(11.0)
                            .color(palette.text_sub),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("↑").on_hover_text(go_up).clicked() {
                        app.up_dir();
                    }
                });
            });
            ui.add_space(10.0);
            egui::Frame::default()
                .fill(palette.bg_panel)
                .stroke(egui::Stroke::new(1.0, palette.border.gamma_multiply(0.65)))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(9, 7))
                .show(ui, |ui| {
                    ui.add_sized(
                        [ui.available_width(), 18.0],
                        egui::Label::new(
                            egui::RichText::new(&dir_label)
                                .size(11.0)
                                .color(palette.text_sub),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&dir_label);
                });
            ui.add_space(10.0);

            let mut navigate_to = None;
            let mut open_to = None;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (name, is_dir) in &items {
                        let color = if *is_dir {
                            palette.text_normal
                        } else {
                            palette.text_sub
                        };
                        let row = ui.horizontal(|ui| {
                            let icon = if *is_dir { folder_icon } else { file_icon };
                            if let Some(id) = icon {
                                ui.add(
                                    egui::Image::new(egui::load::SizedTexture::new(
                                        id,
                                        egui::vec2(15.0, 15.0),
                                    ))
                                    .fit_to_exact_size(egui::vec2(15.0, 15.0)),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new(if *is_dir { "▰" } else { "·" }).color(
                                        if *is_dir {
                                            palette.warning
                                        } else {
                                            palette.accent
                                        },
                                    ),
                                );
                            }
                            ui.add(
                                egui::Label::new(egui::RichText::new(name).size(13.0).color(color))
                                    .truncate(),
                            )
                        });
                        let response = row.inner.on_hover_text(name);
                        if response.double_clicked() && !*is_dir {
                            open_to = Some(app.dir.join(name));
                        } else if response.clicked() {
                            if *is_dir {
                                navigate_to = Some(app.dir.join(name));
                            } else {
                                let path = app.dir.join(name);
                                app.status = match spell_core::backend::sniff_file(&path) {
                                    Ok(kind) if kind != spell_core::types::ArchiveKind::Unknown => {
                                        app.tr.tf(
                                            TextKey::BrowserDoubleClick,
                                            &[("name", name), ("kind", kind.label())],
                                        )
                                    }
                                    _ => app.tr.tf(TextKey::BrowserSelected, &[("name", name)]),
                                };
                            }
                        }
                        ui.add_space(3.0);
                    }
                });
            if let Some(path) = navigate_to {
                app.navigate(path);
            }
            if let Some(path) = open_to {
                app.open_path(path, None);
            }
        });
}
