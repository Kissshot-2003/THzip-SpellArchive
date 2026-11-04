use std::path::PathBuf;

use eframe::egui;
use spell_core::plan::OverwritePolicy;
use spell_core::secret::SecretString;

use crate::app::SpellApp;
use crate::i18n::TextKey;
use crate::ui::format_bytes;

pub fn show(app: &mut SpellApp, ctx: &egui::Context) {
    password_prompt(app, ctx);
    precheck(app, ctx);
    pack_dialog(app, ctx);
}

fn pack_dialog(app: &mut SpellApp, ctx: &egui::Context) {
    if !app.show_pack {
        return;
    }

    let palette = app.themes.current().palette;
    let mut open = true;
    let mut pick_files = false;
    let mut pick_folder = false;
    let mut browse = false;
    let mut start = false;
    let mut cancel = false;

    egui::Window::new("创建 zip")
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("添加文件…").clicked() {
                    pick_files = true;
                }
                if ui.button("添加文件夹…").clicked() {
                    pick_folder = true;
                }
            });

            ui.add_space(4.0);
            let mut remove_index = None;
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if app.pack_state.sources.is_empty() {
                        ui.label(
                            egui::RichText::new("尚未添加任何文件或文件夹")
                                .size(12.0)
                                .color(palette.text_sub),
                        );
                    }
                    for (index, path) in app.pack_state.sources.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.small_button("×").clicked() {
                                remove_index = Some(index);
                            }
                            let text = path.display().to_string();
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&text)
                                        .size(12.0)
                                        .color(palette.text_normal),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&text);
                        });
                    }
                });
            if let Some(index) = remove_index {
                app.pack_state.sources.remove(index);
            }

            ui.separator();
            ui.horizontal(|ui| {
                ui.label("输出");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut app.pack_state.output).desired_width(380.0),
                );
                if response.changed() {
                    app.pack_state.confirm_overwrite = false;
                }
                response.on_hover_text(app.pack_state.output.clone());
                if ui.button("浏览…").clicked() {
                    browse = true;
                }
            });
            ui.checkbox(&mut app.pack_state.store, "仅存储（不压缩）");
            if ui
                .checkbox(&mut app.pack_state.append, "追加到现有 zip（保留原有条目）")
                .changed()
            {
                app.pack_state.confirm_overwrite = false;
                app.pack_state.error = None;
                if app.pack_state.append {
                    app.pack_state.encrypt = false;
                    app.pack_state.password.clear();
                }
            }

            ui.add_enabled_ui(!app.pack_state.append, |ui| {
                ui.checkbox(&mut app.pack_state.encrypt, "AES-256 加密");
            });
            if app.pack_state.encrypt && !app.pack_state.append {
                ui.add(
                    egui::TextEdit::singleline(&mut app.pack_state.password)
                        .password(true)
                        .hint_text("密码")
                        .desired_width(240.0),
                );
            }

            let output_path = PathBuf::from(app.pack_state.output.trim());
            if output_path.is_file() {
                let text = if app.pack_state.append {
                    "将追加到现有 zip；完成后原子替换原文件。"
                } else {
                    "目标文件已存在：确认后会原子替换。"
                };
                ui.label(egui::RichText::new(text).size(12.0).color(palette.warning));
            } else if app.pack_state.append {
                ui.label(
                    egui::RichText::new("追加模式需要目标 zip 已存在。")
                        .size(12.0)
                        .color(palette.error),
                );
            }
            if let Some(error) = &app.pack_state.error {
                ui.label(egui::RichText::new(error).size(12.0).color(palette.error));
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let label = if app.pack_state.append {
                    "开始追加"
                } else if !app.pack_state.sources.is_empty()
                    && output_path.is_file()
                    && app.pack_state.confirm_overwrite
                {
                    "覆盖并打包"
                } else {
                    "开始打包"
                };
                if ui.button(label).clicked() {
                    start = true;
                }
                if ui.button("取消").clicked() {
                    cancel = true;
                }
            });
        });

    if pick_files {
        app.pack_add_files();
    } else if pick_folder {
        app.pack_add_folder();
    } else if browse {
        app.pack_browse_output();
    } else if start {
        app.pack_start();
    } else if cancel || !open {
        app.show_pack = false;
        app.pack_state.password.clear();
    }
}

fn password_prompt(app: &mut SpellApp, ctx: &egui::Context) {
    let Some(prompt) = app.pending_password.as_mut() else {
        return;
    };

    let title = app.tr.t(TextKey::PasswordPrompt);
    let cancel_label = app.tr.t(TextKey::Cancel);
    let open_label = app.tr.t(TextKey::Open);
    let path = prompt.path.display().to_string();
    let mut confirmed = false;
    let mut cancelled = false;
    let mut open = true;

    egui::Window::new(title)
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(path)
                    .size(12.0)
                    .color(app.themes.current().palette.text_sub),
            );
            ui.add_space(4.0);
            let response = ui.add(
                egui::TextEdit::singleline(&mut prompt.input)
                    .password(true)
                    .hint_text("密码")
                    .desired_width(240.0),
            );
            response.request_focus();
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                confirmed = true;
            }
            if let Some(error) = &prompt.error {
                ui.label(egui::RichText::new(error).color(app.themes.current().palette.error));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(open_label).clicked() {
                    confirmed = true;
                }
                if ui.button(cancel_label).clicked() {
                    cancelled = true;
                }
            });
        });

    if confirmed {
        if let Some(prompt) = app.pending_password.take() {
            app.open_path(prompt.path, Some(SecretString::new(prompt.input)));
        }
    } else if cancelled || !open {
        app.pending_password = None;
    }
}

fn precheck(app: &mut SpellApp, ctx: &egui::Context) {
    let Some(pending) = app.pending_extract.as_mut() else {
        return;
    };

    let palette = app.themes.current().palette;
    let title = app.tr.t(TextKey::Precheck);
    let start_label = app.tr.t(TextKey::StartExtract);
    let cancel_label = app.tr.t(TextKey::Cancel);
    let target_label = app.tr.t(TextKey::ExtractTo);
    let total_label = app.tr.t(TextKey::TotalSize);
    let largest_label = app.tr.t(TextKey::LargestFile);
    let free_label = app.tr.t(TextKey::FreeSpace);
    let conflicts_label = app.tr.t(TextKey::Conflicts);
    let warnings_label = app.tr.t(TextKey::Warnings);
    let skip_label = app.tr.t(TextKey::SkipExisting);
    let overwrite_label = app.tr.t(TextKey::OverwriteExisting);

    let plan = &pending.plan;
    let entries = plan.items.len();
    let total = plan.total_bytes;
    let largest = plan.largest_entry;
    let free = plan.free_space;
    let conflicts = plan.conflicts.len();
    let warnings = plan.warnings.len();
    let needs_password = plan.needs_password;
    let target = pending.target.display().to_string();

    let mut start = false;
    let mut cancel = false;
    let mut open = true;

    egui::Window::new(title)
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .default_width(480.0)
        .show(ctx, |ui| {
            ui.label(egui::RichText::new(format!("{entries} 个条目")).strong());
            egui::Grid::new("precheck_grid")
                .num_columns(2)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    ui.label(target_label);
                    ui.label(&target);
                    ui.end_row();

                    ui.label(total_label);
                    ui.label(format_bytes(total));
                    ui.end_row();

                    ui.label(largest_label);
                    ui.label(format_bytes(largest));
                    ui.end_row();

                    ui.label(free_label);
                    ui.label(free.map(format_bytes).unwrap_or_else(|| "未知".to_owned()));
                    ui.end_row();

                    ui.label(conflicts_label);
                    let color = if conflicts > 0 {
                        palette.warning
                    } else {
                        palette.success
                    };
                    ui.label(egui::RichText::new(conflicts.to_string()).color(color));
                    ui.end_row();

                    ui.label(warnings_label);
                    ui.label(warnings.to_string());
                    ui.end_row();
                });

            if needs_password {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("包含加密条目；若尚未提供密码，解压会中止。")
                        .color(palette.warning),
                );
            }
            if let Some(free) = free {
                if total > free {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("预计写出的数据超过目标盘剩余空间。")
                            .color(palette.error),
                    );
                }
            }

            ui.add_space(8.0);
            ui.label(egui::RichText::new("冲突处理").strong());
            ui.radio_value(&mut pending.policy, OverwritePolicy::Skip, skip_label);
            ui.radio_value(
                &mut pending.policy,
                OverwritePolicy::Overwrite,
                overwrite_label,
            );

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(start_label).clicked() {
                    start = true;
                }
                if ui.button(cancel_label).clicked() {
                    cancel = true;
                }
            });
        });

    if start {
        app.start_extract();
    } else if cancel || !open {
        app.pending_extract = None;
    }
}
