use eframe::egui;

use crate::app::SpellApp;
use crate::i18n::TextKey;

pub fn show(app: &mut SpellApp, root: &mut egui::Ui) {
    let palette = app.themes.current().palette;
    let local_files = app.tr.t(TextKey::LocalFiles);
    let go_up = app.tr.t(TextKey::GoUp);
    let dir_label = app.dir.display().to_string();
    let items = app
        .dir_items
        .iter()
        .map(|item| (item.name.clone(), item.is_dir))
        .collect::<Vec<_>>();

    egui::Panel::left("file_browser")
        .resizable(true)
        .default_size(250.0)
        .min_size(180.0)
        .show(root, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.small_button("↑").on_hover_text(go_up).clicked() {
                    app.up_dir();
                }
                ui.add_sized(
                    [ui.available_width(), 20.0],
                    egui::Label::new(
                        egui::RichText::new(&dir_label)
                            .size(11.0)
                            .color(palette.text_sub),
                    ),
                )
                .on_hover_text(&dir_label);
            });
            ui.separator();
            ui.label(egui::RichText::new(local_files).strong());
            ui.add_space(2.0);

            let mut navigate_to = None;
            let mut open_to = None;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (name, is_dir) in &items {
                        let label = if *is_dir {
                            format!("[{}]", name)
                        } else {
                            name.clone()
                        };
                        let color = if *is_dir {
                            palette.text_normal
                        } else {
                            palette.text_sub
                        };
                        let response =
                            ui.selectable_label(false, egui::RichText::new(label).color(color));
                        if response.double_clicked() && !*is_dir {
                            open_to = Some(app.dir.join(name));
                        } else if response.clicked() {
                            if *is_dir {
                                navigate_to = Some(app.dir.join(name));
                            } else {
                                let path = app.dir.join(name);
                                app.status = match spell_core::backend::sniff_file(&path) {
                                    Ok(kind) if kind != spell_core::types::ArchiveKind::Unknown => {
                                        format!("{name} · {} · 双击打开", kind.label())
                                    }
                                    _ => format!("选中：{name}"),
                                };
                            }
                        }
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
