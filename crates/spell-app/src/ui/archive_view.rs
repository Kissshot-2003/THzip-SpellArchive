use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::app::{SortKey, SpellApp};
use crate::i18n::TextKey;
use crate::theme::ResKey;
use crate::ui::format_bytes;

const ROW_H: f32 = 22.0;
const NAME_W: f32 = 280.0;
const SIZE_W: f32 = 84.0;
const PACKED_W: f32 = 84.0;
const RATIO_W: f32 = 60.0;
const METHOD_W: f32 = 72.0;
const TIME_W: f32 = 110.0;

pub fn show(app: &mut SpellApp, root: &mut egui::Ui) {
    let palette = app.themes.current().palette;
    let title = if app.archive_name.is_empty() {
        app.tr.t(TextKey::EmptyArchive)
    } else {
        app.archive_name.clone()
    };
    let kind = app.archive_kind.map(|kind| kind.label()).unwrap_or("—");
    let path_hint = app
        .archive_path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let search_hint = app.tr.t(TextKey::SearchHint);
    let empty_text = app.tr.t(TextKey::EmptyArchive);
    let rows = app.filtered_entries();

    egui::CentralPanel::default().show(root, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(title.clone()).strong())
                .on_hover_text(path_hint);
            ui.label(
                egui::RichText::new(format!("· {kind}"))
                    .size(12.0)
                    .color(palette.text_sub),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut search = app.search.clone();
                let response = ui.add(
                    egui::TextEdit::singleline(&mut search)
                        .hint_text(search_hint)
                        .desired_width(220.0),
                );
                if response.changed() {
                    app.search = search;
                }
                if !rows.is_empty() {
                    if ui.small_button("清空").clicked() {
                        app.selected.clear();
                        app.last_selected = None;
                    }
                    if ui.small_button("全选").clicked() {
                        app.select_all_filtered();
                    }
                }
            });
        });
        ui.add_space(2.0);

        if rows.is_empty() {
            ui.separator();
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(empty_text).color(palette.text_sub));
                ui.add_space(8.0);
                if app.reader.is_none() {
                    let open = app.tr.t(TextKey::Open);
                    if crate::ui::action_button(ui, app, ResKey::IconArchive7z, open).clicked() {
                        app.open_archive_dialog();
                    }
                    ui.label(
                        egui::RichText::new("也可以双击左侧列表中的 zip / 7z，或把文件拖到窗口")
                            .size(12.0)
                            .color(palette.text_sub),
                    );
                }
            });
            return;
        }

        let entries = &app.entries;
        let folder_icon = app.themes.texture(ResKey::IconFolder).map(|tex| tex.id());
        let file_icon = app.themes.texture(ResKey::IconFile).map(|tex| tex.id());
        let lock_icon = app
            .themes
            .texture(ResKey::IconArchiveEncrypted)
            .map(|tex| tex.id());
        let mut clicked = None;
        let mut header_click = None;
        let modifiers = ui.input(|input| input.modifiers);
        let additive = modifiers.command || modifiers.ctrl;
        let range = modifiers.shift;

        // TableBuilder 让表头与数据行共用同一套列宽，不会再出现列错位。
        TableBuilder::new(ui)
            .striped(true)
            .resizable(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(NAME_W).at_least(200.0).clip(true))
            .column(Column::initial(SIZE_W).clip(true))
            .column(Column::initial(PACKED_W).clip(true))
            .column(Column::initial(RATIO_W).clip(true))
            .column(Column::initial(METHOD_W).clip(true))
            .column(Column::initial(TIME_W).clip(true))
            .column(Column::remainder().clip(true))
            .header(24.0, |mut header| {
                header.col(|ui| {
                    sort_header(ui, app, SortKey::Name, "名称", &mut header_click);
                });
                header.col(|ui| {
                    sort_header(ui, app, SortKey::Size, "大小", &mut header_click);
                });
                header.col(|ui| {
                    sort_header(ui, app, SortKey::Packed, "压缩后", &mut header_click);
                });
                header.col(|ui| {
                    sort_header(ui, app, SortKey::Ratio, "比率", &mut header_click);
                });
                header.col(|ui| {
                    sort_header(ui, app, SortKey::Method, "方法", &mut header_click);
                });
                header.col(|ui| {
                    sort_header(ui, app, SortKey::Time, "修改时间", &mut header_click);
                });
                header.col(|ui| {
                    ui.strong("标记");
                });
            })
            .body(|body| {
                body.rows(ROW_H, rows.len(), |mut row| {
                    let index = rows[row.index()];
                    let entry = &entries[index];
                    let selected = app.selected.contains(&index);
                    let guard = spell_core::safety::guard_display(&entry.display);

                    row.col(|ui| {
                        let icon_id = if entry.kind.is_dir() {
                            folder_icon
                        } else if entry.encrypted {
                            lock_icon.or(file_icon)
                        } else {
                            file_icon
                        };
                        match icon_id {
                            Some(id) => {
                                ui.add(
                                    egui::Image::new(egui::load::SizedTexture::new(
                                        id,
                                        egui::vec2(14.0, 14.0),
                                    ))
                                    .fit_to_exact_size(egui::vec2(14.0, 14.0)),
                                );
                            }
                            None => {
                                let (dot, _) = ui.allocate_exact_size(
                                    egui::vec2(8.0, 8.0),
                                    egui::Sense::hover(),
                                );
                                let color = if entry.kind.is_dir() {
                                    palette.warning
                                } else {
                                    palette.accent
                                };
                                ui.painter()
                                    .rect_filled(dot, egui::CornerRadius::same(2), color);
                            }
                        }
                        if ui.selectable_label(selected, guard.text.clone()).clicked() {
                            clicked = Some(index);
                        }
                    });

                    row.col(|ui| {
                        ui.label(format_bytes(entry.size));
                    });
                    row.col(|ui| {
                        ui.label(
                            entry
                                .packed
                                .map(format_bytes)
                                .unwrap_or_else(|| "—".to_owned()),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            entry
                                .ratio()
                                .map(|ratio| format!("{ratio:.1}x"))
                                .unwrap_or_else(|| "—".to_owned()),
                        );
                    });
                    row.col(|ui| {
                        ui.label(entry.method.label());
                    });
                    row.col(|ui| {
                        ui.label(crate::ui::format_time(entry.mtime));
                    });
                    row.col(|ui| {
                        if entry.encrypted && lock_icon.is_none() {
                            ui.label(egui::RichText::new("锁").color(palette.warning));
                        }
                        if guard.suspicious {
                            ui.label(egui::RichText::new("伪装").color(palette.error))
                                .on_hover_text(
                                    "文件名包含双向控制字符，显示的扩展名可能与真实扩展名不同",
                                );
                        }
                    });
                });
            });

        if let Some(index) = clicked {
            if range {
                if let Some(anchor) = app.last_selected {
                    if !additive {
                        app.selected.clear();
                    }
                    let (low, high) = if anchor <= index {
                        (anchor, index)
                    } else {
                        (index, anchor)
                    };
                    for row in low..=high {
                        app.selected.insert(row);
                    }
                } else {
                    app.selected.clear();
                    app.selected.insert(index);
                }
            } else if additive {
                if !app.selected.remove(&index) {
                    app.selected.insert(index);
                }
            } else {
                app.selected.clear();
                app.selected.insert(index);
            }
            app.last_selected = Some(index);
        }

        if let Some(key) = header_click {
            if app.sort_key == key {
                app.sort_asc = !app.sort_asc;
            } else {
                app.sort_key = key;
                app.sort_asc = true;
            }
            app.touch_settings();
        }
    });
}

fn sort_header(
    ui: &mut egui::Ui,
    app: &SpellApp,
    key: SortKey,
    text: &str,
    clicked: &mut Option<SortKey>,
) {
    let label = if app.sort_key == key {
        format!("{text} {}", if app.sort_asc { "↑" } else { "↓" })
    } else {
        text.to_owned()
    };
    let response = ui.add(egui::Button::new(egui::RichText::new(label).strong()).frame(false));
    if response.clicked() {
        *clicked = Some(key);
    }
    if app.sort_key == key {
        response.on_hover_text("点击切换升序 / 降序");
    }
}
