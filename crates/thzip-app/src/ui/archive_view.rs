use eframe::egui;
use egui_extras::{Column, TableBuilder};
use thzip_core::types::Method;

use crate::app::{RelocateMode, SortKey, ThzipApp};
use crate::i18n::TextKey;
use crate::theme::ResKey;
use crate::ui::format_bytes;

const ROW_H: f32 = 30.0;
const NAME_W: f32 = 320.0;
const SIZE_W: f32 = 92.0;
const PACKED_W: f32 = 92.0;
const RATIO_W: f32 = 68.0;
const METHOD_W: f32 = 80.0;
const TIME_W: f32 = 126.0;

pub fn show(app: &mut ThzipApp, root: &mut egui::Ui) {
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
    let comment = app.comment.clone();
    let comment_label = app.tr.t(TextKey::Comment);
    let search_hint = app.tr.t(TextKey::SearchHint);
    let empty_text = app.tr.t(TextKey::EmptyArchive);
    let empty_hint = app.tr.t(TextKey::EmptyHint);
    let clear_selection = app.tr.t(TextKey::ClearSelection);
    let select_all = app.tr.t(TextKey::SelectAll);
    let delete_selected_text = app.tr.t(TextKey::DeleteSelected);
    let rename_selected_text = app.tr.t(TextKey::RenameSelected);
    let move_selected_text = app.tr.t(TextKey::MoveSelected);
    let batch_rename_text = app.tr.t(TextKey::BatchRenameSelected);
    let regex_rename_text = app.tr.t(TextKey::RegexRenameSelected);
    let col_name = app.tr.t(TextKey::ColName);
    let col_size = app.tr.t(TextKey::ColSize);
    let col_packed = app.tr.t(TextKey::ColPacked);
    let col_ratio = app.tr.t(TextKey::ColRatio);
    let col_method = app.tr.t(TextKey::ColMethod);
    let col_time = app.tr.t(TextKey::ColTime);
    let col_flags = app.tr.t(TextKey::ColFlags);
    let sort_hint = app.tr.t(TextKey::SortHint);
    let flag_locked = app.tr.t(TextKey::FlagLocked);
    let flag_spoofed = app.tr.t(TextKey::FlagSpoofed);
    let spoofed_hint = app.tr.t(TextKey::SpoofedHint);
    let unknown_method = app.tr.t(TextKey::Unknown);
    let rows = app.filtered_entries();
    let panel_bg =
        crate::ui::nine_patch::NinePatch::from_theme(app.themes.current(), ResKey::PanelBg);

    let mut delete_selected = false;
    let mut relocate_rename = false;
    let mut relocate_move = false;
    let mut relocate_batch = false;
    let mut relocate_regex = false;
    egui::CentralPanel::default()
        .frame(
            egui::Frame::default()
                .fill(palette.bg_window)
                .inner_margin(egui::Margin::same(14)),
        )
        .show(root, |ui| {
            if let Some(bg) = panel_bg {
                bg.paint(ui, ui.max_rect(), egui::Color32::WHITE);
            }
            if app.reader.is_some() {
                ui.horizontal_wrapped(|ui| {
                    let hover = match &comment {
                        Some(text) => format!("{path_hint}\n\n{}: {text}", comment_label),
                        None => path_hint.clone(),
                    };
                    ui.label(egui::RichText::new(title.clone()).size(20.0).strong())
                        .on_hover_text(hover);
                    egui::Frame::default()
                        .fill(palette.accent_soft.gamma_multiply(0.72))
                        .corner_radius(egui::CornerRadius::same(5))
                        .inner_margin(egui::Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(kind).size(11.0).color(palette.accent));
                        });
                    if let Some(text) = &comment {
                        egui::Frame::default()
                            .fill(palette.warning.gamma_multiply(0.18))
                            .corner_radius(egui::CornerRadius::same(5))
                            .inner_margin(egui::Margin::symmetric(8, 3))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(comment_label.clone())
                                        .size(11.0)
                                        .color(palette.warning),
                                )
                                .on_hover_text(text.clone());
                            });
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut search = app.search.clone();
                        let width = ui.available_width().clamp(150.0, 240.0);
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut search)
                                .hint_text(search_hint)
                                .desired_width(width),
                        );
                        if app.focus_search {
                            response.request_focus();
                            app.focus_search = false;
                        }
                        if response.changed() {
                            app.search = search;
                        }
                    });
                });
                if !rows.is_empty() {
                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(&select_all).clicked() {
                            app.select_all_filtered();
                        }
                        if ui.button(&clear_selection).clicked() {
                            app.selected.clear();
                            app.last_selected = None;
                        }
                        if !app.selected.is_empty() {
                            ui.separator();
                            if app.selected.len() == 1 && ui.button(&rename_selected_text).clicked() {
                                relocate_rename = true;
                            }
                            if ui.button(&move_selected_text).clicked() {
                                relocate_move = true;
                            }
                            if app.selected.len() >= 2 && ui.button(&batch_rename_text).clicked() {
                                relocate_batch = true;
                            }
                            if app.selected.len() >= 2 && ui.button(&regex_rename_text).clicked() {
                                relocate_regex = true;
                            }
                            if crate::ui::danger_button(ui, &palette, &delete_selected_text).clicked()
                            {
                                delete_selected = true;
                            }
                        }
                    });
                }
                ui.add_space(12.0);
            }

            if rows.is_empty() {
                let top_space = (ui.available_height() - 250.0).max(24.0) * 0.32;
                ui.add_space(top_space);
                ui.vertical_centered(|ui| {
                    let width = ui.available_width().min(560.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 220.0),
                        egui::Layout::top_down(egui::Align::Center),
                        |ui| {
                            crate::ui::card_frame(&palette)
                                .inner_margin(egui::Margin::symmetric(24, 22))
                                .show(ui, |ui| {
                                    ui.vertical_centered(|ui| {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(62.0, 62.0),
                                            egui::Layout::centered_and_justified(
                                                egui::Direction::TopDown,
                                            ),
                                            |ui| {
                                                egui::Frame::default()
                                                    .fill(palette.accent_soft)
                                                    .corner_radius(egui::CornerRadius::same(12))
                                                    .inner_margin(egui::Margin::same(12))
                                                    .show(ui, |ui| {
                                                        ui.set_min_size(egui::vec2(38.0, 38.0));
                                                        if let Some(texture) = app
                                                            .themes
                                                            .texture(ResKey::IconArchiveZip)
                                                        {
                                                            ui.centered_and_justified(|ui| {
                                                                ui.add(
                                                                    egui::Image::new(
                                                                        egui::load::SizedTexture::new(
                                                                            texture.id(),
                                                                            egui::vec2(34.0, 34.0),
                                                                        ),
                                                                    )
                                                                    .fit_to_exact_size(egui::vec2(
                                                                        34.0, 34.0,
                                                                    )),
                                                                );
                                                            });
                                                        } else {
                                                            ui.centered_and_justified(|ui| {
                                                                ui.label(
                                                                    egui::RichText::new("SA")
                                                                        .size(18.0)
                                                                        .strong()
                                                                        .color(palette.accent),
                                                                );
                                                            });
                                                        }
                                                    });
                                            },
                                        );
                                        ui.add_space(12.0);
                                        ui.label(
                                            egui::RichText::new(empty_text).size(19.0).strong(),
                                        );
                                        ui.add_space(4.0);
                                        ui.label(
                                            egui::RichText::new(&empty_hint)
                                                .size(13.0)
                                                .color(palette.text_sub),
                                        );
                                        if app.reader.is_none() {
                                            ui.add_space(16.0);
                                            let open = app.tr.t(TextKey::Open);
                                            if crate::ui::primary_action_button(
                                                ui,
                                                app,
                                                ResKey::IconArchive7z,
                                                open,
                                            )
                                            .clicked()
                                            {
                                                app.open_archive_dialog();
                                            }
                                        }
                                    });
                                });
                        },
                    );
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
            let compact = ui.available_width() < 800.0;
            let (name_w, size_w, packed_w, ratio_w, method_w, time_w) = if compact {
                (190.0, 72.0, 72.0, 52.0, 60.0, 96.0)
            } else {
                (NAME_W, SIZE_W, PACKED_W, RATIO_W, METHOD_W, TIME_W)
            };
            TableBuilder::new(ui)
                .id_salt(if compact {
                    "archive_table_compact"
                } else {
                    "archive_table"
                })
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(
                    Column::initial(name_w)
                        .at_least(if compact { 150.0 } else { 160.0 })
                        .clip(true),
                )
                .column(Column::initial(size_w).clip(true))
                .column(Column::initial(packed_w).clip(true))
                .column(Column::initial(ratio_w).clip(true))
                .column(Column::initial(method_w).clip(true))
                .column(Column::initial(time_w).clip(true))
                .column(Column::remainder().clip(true))
                .header(32.0, |mut header| {
                    header.col(|ui| {
                        sort_header(
                            ui,
                            app,
                            SortKey::Name,
                            &col_name,
                            &sort_hint,
                            &mut header_click,
                        );
                    });
                    header.col(|ui| {
                        sort_header(
                            ui,
                            app,
                            SortKey::Size,
                            &col_size,
                            &sort_hint,
                            &mut header_click,
                        );
                    });
                    header.col(|ui| {
                        sort_header(
                            ui,
                            app,
                            SortKey::Packed,
                            &col_packed,
                            &sort_hint,
                            &mut header_click,
                        );
                    });
                    header.col(|ui| {
                        sort_header(
                            ui,
                            app,
                            SortKey::Ratio,
                            &col_ratio,
                            &sort_hint,
                            &mut header_click,
                        );
                    });
                    header.col(|ui| {
                        sort_header(
                            ui,
                            app,
                            SortKey::Method,
                            &col_method,
                            &sort_hint,
                            &mut header_click,
                        );
                    });
                    header.col(|ui| {
                        sort_header(
                            ui,
                            app,
                            SortKey::Time,
                            &col_time,
                            &sort_hint,
                            &mut header_click,
                        );
                    });
                    header.col(|ui| {
                        ui.strong(&col_flags);
                    });
                })
                .body(|body| {
                    body.rows(ROW_H, rows.len(), |mut row| {
                        let index = rows[row.index()];
                        let entry = &entries[index];
                        let selected = app.selected.contains(&index);
                        let guard = thzip_core::safety::guard_display(&entry.display);
                        row.set_selected(selected);

                        let mut row_clicked = false;
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
                                    ui.painter().rect_filled(
                                        dot,
                                        egui::CornerRadius::same(2),
                                        color,
                                    );
                                }
                            }
                            let response = ui.selectable_label(selected, guard.text.clone());
                            if response.clicked() {
                                row_clicked = true;
                            }
                        });
                        if row_clicked {
                            clicked = Some((index, row.index()));
                        }

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
                            let method = match entry.method {
                                Method::Unknown(code) if code != 99 => unknown_method.clone(),
                                _ => entry.method.label().to_owned(),
                            };
                            ui.label(method);
                        });
                        row.col(|ui| {
                            ui.label(crate::ui::format_time(entry.mtime));
                        });
                        row.col(|ui| {
                            if entry.encrypted && lock_icon.is_none() {
                                ui.label(egui::RichText::new(&flag_locked).color(palette.warning));
                            }
                            if guard.suspicious {
                                ui.label(egui::RichText::new(&flag_spoofed).color(palette.error))
                                    .on_hover_text(&spoofed_hint);
                            }
                        });
                    });
                });

            if let Some((index, clicked_row)) = clicked {
                if range {
                    if let Some(anchor_row) = app
                        .last_selected
                        .and_then(|anchor| rows.iter().position(|&index| index == anchor))
                    {
                        if !additive {
                            app.selected.clear();
                        }
                        let (low, high) = if anchor_row <= clicked_row {
                            (anchor_row, clicked_row)
                        } else {
                            (clicked_row, anchor_row)
                        };
                        for &entry_index in &rows[low..=high] {
                            app.selected.insert(entry_index);
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

            if delete_selected {
                app.delete_selected_entries();
            } else if relocate_rename {
                app.begin_relocate(RelocateMode::Rename);
            } else if relocate_move {
                app.begin_relocate(RelocateMode::Move);
            } else if relocate_batch {
                app.begin_relocate(RelocateMode::BatchRename);
            } else if relocate_regex {
                app.begin_relocate(RelocateMode::RegexRename);
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
    app: &ThzipApp,
    key: SortKey,
    text: &str,
    hint: &str,
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
        response.on_hover_text(hint);
    }
}
