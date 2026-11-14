use std::path::PathBuf;

use eframe::egui;
use thzip_core::plan::OverwritePolicy;
use thzip_core::secret::SecretString;
use thzip_core::types::ArchiveKind;

use crate::app::{RelocateMode, ThzipApp, ZipPackMethod};
use crate::i18n::TextKey;
use crate::ui::format_bytes;

pub fn show(app: &mut ThzipApp, ctx: &egui::Context) {
    password_prompt(app, ctx);
    precheck(app, ctx);
    pack_dialog(app, ctx);
    relocate_dialog(app, ctx);
    preview_window(app, ctx);
}

fn preview_window(app: &mut ThzipApp, ctx: &egui::Context) {
    if app.preview.is_none() {
        return;
    }
    let palette = app.themes.current().palette;
    let title_prefix = app.tr.t(TextKey::Preview);
    let loading = app.tr.t(TextKey::PreviewLoading);
    let binary = app.tr.t(TextKey::PreviewBinary);
    let truncated_note = app.tr.t(TextKey::PreviewTruncated);
    let name = app
        .preview
        .as_ref()
        .map(|preview| preview.name.clone())
        .unwrap_or_default();
    let mut open = true;
    egui::Window::new(format!("{title_prefix} - {name}"))
        .open(&mut open)
        .resizable(true)
        .default_size([640.0, 480.0])
        .show(ctx, |ui| {
            let Some(preview) = app.preview.as_mut() else {
                return;
            };
            if preview.loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(loading.clone());
                });
                return;
            }
            if let Some(reason) = &preview.failed {
                ui.label(egui::RichText::new(reason.to_string()).color(palette.error));
            }
            if preview.texture.is_none()
                && let Some((size, rgba)) = &preview.image
            {
                let image = egui::ColorImage::from_rgba_unmultiplied(*size, rgba);
                preview.texture = Some(ui.ctx().load_texture(
                    "preview-image",
                    image,
                    egui::TextureOptions::LINEAR,
                ));
            }
            if let Some(texture) = &preview.texture {
                let texture_size = texture.size_vec2();
                let available = ui.available_size();
                let scale = (available.x / texture_size.x)
                    .min(available.y / texture_size.y)
                    .min(1.0);
                ui.image((texture.id(), texture_size * scale));
            } else if let Some(text) = &preview.text {
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(text).monospace())
                                .selectable(true),
                        );
                    });
            } else {
                ui.label(binary.clone());
                ui.label(format_bytes(preview.data.len() as u64));
            }
            if preview.truncated {
                ui.label(egui::RichText::new(truncated_note.clone()).color(palette.warning));
            }
        });
    if !open {
        app.preview = None;
    }
}

fn pack_dialog(app: &mut ThzipApp, ctx: &egui::Context) {
    if !app.show_pack {
        return;
    }

    let palette = app.themes.current().palette;
    let title = app.tr.t(TextKey::PackTitle);
    let add_files = app.tr.t(TextKey::AddFiles);
    let add_folder = app.tr.t(TextKey::AddFolder);
    let pack_empty = app.tr.t(TextKey::PackEmpty);
    let output_label = app.tr.t(TextKey::Output);
    let browse_label = app.tr.t(TextKey::Browse);
    let store_only = app.tr.t(TextKey::StoreOnly);
    let zip_method_label = app.tr.t(TextKey::ZipMethod);
    let sevenz_options_label = app.tr.t(TextKey::SevenZOptionsLabel);
    let sevenz_level_label = app.tr.t(TextKey::SevenZLevel);
    let sevenz_dict_label = app.tr.t(TextKey::SevenZDict);
    let sevenz_auto = app.tr.t(TextKey::SevenZAuto);
    let append_mode = app.tr.t(TextKey::AppendMode);
    let aes_encrypt = app.tr.t(TextKey::AesEncrypt);
    let password_hint = app.tr.t(TextKey::Password);
    let comment_label = app.tr.t(TextKey::Comment);
    let comment_hint = app.tr.t(TextKey::CommentHint);
    let formats_hint = app.tr.t(TextKey::PackFormatsHint);
    let append_hint = app.tr.t(TextKey::PackAppendHint);
    let overwrite_hint = app.tr.t(TextKey::PackOverwriteHint);
    let append_missing = app.tr.t(TextKey::PackAppendMissing);
    let start_append = app.tr.t(TextKey::StartAppend);
    let overwrite_and_pack = app.tr.t(TextKey::OverwriteAndPack);
    let start_pack = app.tr.t(TextKey::StartPack);
    let cancel_label = app.tr.t(TextKey::Cancel);
    let mut open = true;
    let mut pick_files = false;
    let mut pick_folder = false;
    let mut browse = false;
    let mut start = false;
    let mut cancel = false;

    egui::Window::new(title)
        .open(&mut open)
        .resizable(true)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .default_size([580.0, 620.0])
        .scroll([false, true])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button(add_files).clicked() {
                    pick_files = true;
                }
                if ui.button(add_folder).clicked() {
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
                            egui::RichText::new(&pack_empty)
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
                ui.label(output_label);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut app.pack_state.output)
                        .desired_width((ui.available_width() - 90.0).max(180.0)),
                );
                if response.changed() {
                    app.pack_state.confirm_overwrite = false;
                }
                response.on_hover_text(app.pack_state.output.clone());
                if ui.button(browse_label).clicked() {
                    browse = true;
                }
            });
            let output_path = PathBuf::from(app.pack_state.output.trim());
            let output_kind = crate::app::kind_for_output(&output_path);
            let is_zip = matches!(output_kind, Some(ArchiveKind::Zip));
            let is_7z = matches!(output_kind, Some(ArchiveKind::SevenZ));
            let supports_append = output_kind.is_some_and(thzip_core::backend::supports_append);
            let supports_encrypt =
                matches!(output_kind, Some(ArchiveKind::Zip | ArchiveKind::SevenZ));
            if !supports_append && app.pack_state.append {
                app.pack_state.append = false;
                app.pack_state.encrypt = false;
                app.pack_state.password.clear();
            }
            if !supports_encrypt {
                app.pack_state.encrypt = false;
                app.pack_state.password.clear();
            }

            ui.add_enabled_ui(supports_encrypt, |ui| {
                ui.checkbox(&mut app.pack_state.store, store_only);
            });
            if is_zip && !app.pack_state.store {
                ui.horizontal(|ui| {
                    ui.label(&zip_method_label);
                    egui::ComboBox::from_id_salt("pack_zip_method")
                        .selected_text(app.pack_state.zip_method.label())
                        .show_ui(ui, |ui| {
                            for method in [
                                ZipPackMethod::Deflate,
                                ZipPackMethod::Bzip2,
                                ZipPackMethod::Zstd,
                            ] {
                                ui.selectable_value(
                                    &mut app.pack_state.zip_method,
                                    method,
                                    method.label(),
                                );
                            }
                        });
                });
            }
            if is_7z && !app.pack_state.store {
                ui.horizontal(|ui| {
                    ui.label(&sevenz_options_label);
                    ui.label(&sevenz_level_label);
                    egui::ComboBox::from_id_salt("pack_7z_level")
                        .selected_text(app.pack_state.sevenz_level.min(9).to_string())
                        .show_ui(ui, |ui| {
                            for level in 0..=9 {
                                ui.selectable_value(
                                    &mut app.pack_state.sevenz_level,
                                    level,
                                    level.to_string(),
                                );
                            }
                        });
                    ui.label(&sevenz_dict_label);
                    let dict_text = if app.pack_state.sevenz_dict_mib == 0 {
                        sevenz_auto.clone()
                    } else {
                        format!("{} MiB", app.pack_state.sevenz_dict_mib)
                    };
                    egui::ComboBox::from_id_salt("pack_7z_dict")
                        .selected_text(dict_text)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut app.pack_state.sevenz_dict_mib,
                                0,
                                &sevenz_auto,
                            );
                            for mib in [1u32, 4, 16, 64, 256] {
                                ui.selectable_value(
                                    &mut app.pack_state.sevenz_dict_mib,
                                    mib,
                                    format!("{mib} MiB"),
                                );
                            }
                        });
                });
            }
            ui.add_enabled_ui(supports_append, |ui| {
                if ui
                    .checkbox(&mut app.pack_state.append, append_mode)
                    .changed()
                {
                    app.pack_state.confirm_overwrite = false;
                    app.pack_state.error = None;
                    if app.pack_state.append {
                        app.pack_state.encrypt = false;
                        app.pack_state.password.clear();
                    }
                }
            });

            ui.add_enabled_ui(supports_encrypt && !app.pack_state.append, |ui| {
                ui.checkbox(&mut app.pack_state.encrypt, aes_encrypt);
            });
            if supports_encrypt && app.pack_state.encrypt && !app.pack_state.append {
                ui.add(
                    egui::TextEdit::singleline(&mut app.pack_state.password)
                        .password(true)
                        .hint_text(password_hint)
                        .desired_width(240.0),
                );
            }
            if is_zip && !app.pack_state.append {
                ui.horizontal(|ui| {
                    ui.label(&comment_label);
                    ui.add(
                        egui::TextEdit::singleline(&mut app.pack_state.comment)
                            .hint_text(&comment_hint)
                            .desired_width((ui.available_width() - 8.0).max(180.0)),
                    );
                });
            }

            ui.label(
                egui::RichText::new(&formats_hint)
                    .size(11.0)
                    .color(palette.text_sub),
            );

            if output_path.is_file() {
                let text = if app.pack_state.append {
                    &append_hint
                } else {
                    &overwrite_hint
                };
                ui.label(egui::RichText::new(text).size(12.0).color(palette.warning));
            } else if app.pack_state.append {
                ui.label(
                    egui::RichText::new(&append_missing)
                        .size(12.0)
                        .color(palette.error),
                );
            }
            if let Some(error) = &app.pack_state.error {
                ui.label(egui::RichText::new(error).size(12.0).color(palette.error));
            }

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let label = if app.pack_state.append {
                    start_append
                } else if !app.pack_state.sources.is_empty()
                    && output_path.is_file()
                    && app.pack_state.confirm_overwrite
                {
                    overwrite_and_pack
                } else {
                    start_pack
                };
                if crate::ui::primary_button(ui, &palette, label).clicked() {
                    start = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(cancel_label).clicked() {
                        cancel = true;
                    }
                });
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

fn relocate_dialog(app: &mut ThzipApp, ctx: &egui::Context) {
    let palette = app.themes.current().palette;
    let Some(dialog) = app.relocate.as_mut() else {
        return;
    };

    let (title, label) = match dialog.mode {
        RelocateMode::Rename => (
            app.tr.t(TextKey::RelocateRenameTitle),
            app.tr.t(TextKey::RelocatePathLabel),
        ),
        RelocateMode::Move => (
            app.tr.t(TextKey::RelocateMoveTitle),
            app.tr.t(TextKey::RelocateFolderLabel),
        ),
        RelocateMode::BatchRename => (
            app.tr.t(TextKey::RelocateBatchTitle),
            app.tr.t(TextKey::RelocateBatchLabel),
        ),
        RelocateMode::RegexRename => (
            app.tr.t(TextKey::RelocateRegexTitle),
            app.tr.t(TextKey::RelocateRegexPatternLabel),
        ),
    };
    let regex_mode = dialog.mode == RelocateMode::RegexRename;
    let replace_label = app.tr.t(TextKey::RelocateRegexReplaceLabel);
    let regex_hint = app.tr.t(TextKey::RelocateRegexHint);
    let case_label = app.tr.t(TextKey::RelocateCaseInsensitive);
    let apply_label = app.tr.t(TextKey::RelocateApply);
    let cancel_label = app.tr.t(TextKey::Cancel);
    let mut confirmed = false;
    let mut cancelled = false;
    let mut open = true;

    egui::Window::new(title)
        .open(&mut open)
        .resizable(true)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .default_size([440.0, 260.0])
        .show(ctx, |ui| {
            ui.label(label);
            if regex_mode {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut dialog.regex_pattern)
                        .hint_text("ep(\\d+)")
                        .desired_width(ui.available_width()),
                );
                response.request_focus();
                if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    confirmed = true;
                }
                ui.add_space(6.0);
                ui.label(&replace_label);
                let replacement = ui.add(
                    egui::TextEdit::singleline(&mut dialog.regex_replacement)
                        .hint_text("episode$1")
                        .desired_width(ui.available_width()),
                );
                if replacement.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
                {
                    confirmed = true;
                }
                ui.checkbox(&mut dialog.regex_case_insensitive, case_label);
                ui.label(
                    egui::RichText::new(regex_hint)
                        .size(12.0)
                        .color(palette.text_sub),
                );
            } else {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut dialog.input)
                        .desired_width(ui.available_width()),
                );
                response.request_focus();
                if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    confirmed = true;
                }
            }
            if let Some(error) = &dialog.error {
                ui.label(egui::RichText::new(error).color(palette.error));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if crate::ui::primary_button(ui, &palette, apply_label).clicked() {
                    confirmed = true;
                }
                if ui.button(cancel_label).clicked() {
                    cancelled = true;
                }
            });
        });

    if confirmed {
        app.apply_relocate();
    } else if cancelled || !open {
        app.relocate = None;
    }
}

fn password_prompt(app: &mut ThzipApp, ctx: &egui::Context) {
    let Some(prompt) = app.pending_password.as_mut() else {
        return;
    };

    let title = app.tr.t(TextKey::PasswordPrompt);
    let cancel_label = app.tr.t(TextKey::Cancel);
    let open_label = app.tr.t(TextKey::Open);
    let password_hint = app.tr.t(TextKey::Password);
    let path = prompt.path.display().to_string();
    let mut confirmed = false;
    let mut cancelled = false;
    let mut open = true;

    let palette = app.themes.current().palette;
    egui::Window::new(title)
        .open(&mut open)
        .resizable(true)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .default_size([440.0, 250.0])
        .show(ctx, |ui| {
            ui.label(egui::RichText::new(path).size(12.0).color(palette.text_sub));
            ui.add_space(4.0);
            let response = ui.add(
                egui::TextEdit::singleline(&mut prompt.input)
                    .password(true)
                    .hint_text(&password_hint)
                    .desired_width(240.0),
            );
            response.request_focus();
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                confirmed = true;
            }
            if let Some(error) = &prompt.error {
                ui.label(egui::RichText::new(error).color(palette.error));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if crate::ui::primary_button(ui, &palette, open_label).clicked() {
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

fn precheck(app: &mut ThzipApp, ctx: &egui::Context) {
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
    let rename_label = app.tr.t(TextKey::RenameExisting);
    let autono_label = app.tr.t(TextKey::AutoNumberExisting);

    let plan = &pending.plan;
    let entries = plan.items.len();
    let total = plan.total_bytes;
    let largest = plan.largest_entry;
    let free = plan.free_space;
    let conflicts = plan.conflicts.len();
    let warnings = plan.warnings.len();
    let needs_password = plan.needs_password;
    let conflict_lines: Vec<String> = plan
        .conflicts
        .iter()
        .map(|item| crate::i18n::warnings::conflict(app.locale, item))
        .collect();
    let warning_lines: Vec<String> = plan
        .warnings
        .iter()
        .map(|item| crate::i18n::warnings::warning(app.locale, item))
        .collect();
    let target = pending.target.display().to_string();
    let entry_count = app
        .tr
        .tf(TextKey::EntryCount, &[("count", &entries.to_string())]);
    let unknown_label = app.tr.t(TextKey::Unknown);
    let encrypted_notice = app.tr.t(TextKey::PrecheckEncrypted);
    let low_space_notice = app.tr.t(TextKey::PrecheckLowSpace);
    let conflict_policy = app.tr.t(TextKey::ConflictPolicy);

    let mut start = false;
    let mut cancel = false;
    let mut open = true;

    egui::Window::new(title)
        .open(&mut open)
        .resizable(true)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .default_size([500.0, 560.0])
        .scroll([false, true])
        .show(ctx, |ui| {
            ui.label(egui::RichText::new(&entry_count).size(17.0).strong());
            ui.add_space(6.0);
            egui::Grid::new("precheck_grid")
                .num_columns(2)
                .spacing([18.0, 6.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(target_label).color(palette.text_sub));
                    ui.label(&target);
                    ui.end_row();

                    ui.label(egui::RichText::new(total_label).color(palette.text_sub));
                    ui.label(format_bytes(total));
                    ui.end_row();

                    ui.label(egui::RichText::new(largest_label).color(palette.text_sub));
                    ui.label(format_bytes(largest));
                    ui.end_row();

                    ui.label(egui::RichText::new(free_label).color(palette.text_sub));
                    ui.label(
                        free.map(format_bytes)
                            .unwrap_or_else(|| unknown_label.clone()),
                    );
                    ui.end_row();

                    ui.label(egui::RichText::new(&conflicts_label).color(palette.text_sub));
                    let color = if conflicts > 0 {
                        palette.warning
                    } else {
                        palette.success
                    };
                    ui.label(egui::RichText::new(conflicts.to_string()).color(color));
                    ui.end_row();

                    ui.label(egui::RichText::new(&warnings_label).color(palette.text_sub));
                    ui.label(warnings.to_string());
                    ui.end_row();
                });

            if !conflict_lines.is_empty() {
                ui.add_space(4.0);
                egui::CollapsingHeader::new(format!("{conflicts_label}: {conflicts}"))
                    .default_open(false)
                    .show(ui, |ui| {
                        for line in conflict_lines.iter().take(crate::i18n::warnings::MAX_LINES) {
                            ui.label(egui::RichText::new(line).size(12.0));
                        }
                        if conflict_lines.len() > crate::i18n::warnings::MAX_LINES {
                            ui.label("…");
                        }
                    });
            }
            if !warning_lines.is_empty() {
                egui::CollapsingHeader::new(format!("{warnings_label}: {warnings}"))
                    .default_open(false)
                    .show(ui, |ui| {
                        for line in warning_lines.iter().take(crate::i18n::warnings::MAX_LINES) {
                            ui.label(egui::RichText::new(line).size(12.0));
                        }
                        if warning_lines.len() > crate::i18n::warnings::MAX_LINES {
                            ui.label("…");
                        }
                    });
            }

            if needs_password {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(&encrypted_notice).color(palette.warning));
            }
            if let Some(free) = free
                && total > free
            {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(&low_space_notice).color(palette.error));
            }

            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(&conflict_policy)
                    .size(14.0)
                    .strong()
                    .color(palette.accent),
            );
            ui.radio_value(&mut pending.policy, OverwritePolicy::Skip, skip_label);
            ui.radio_value(
                &mut pending.policy,
                OverwritePolicy::Overwrite,
                overwrite_label,
            );
            ui.radio_value(&mut pending.policy, OverwritePolicy::Rename, rename_label);
            ui.radio_value(
                &mut pending.policy,
                OverwritePolicy::AutoNumber,
                autono_label,
            );

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if crate::ui::primary_button(ui, &palette, start_label).clicked() {
                    start = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(cancel_label).clicked() {
                        cancel = true;
                    }
                });
            });
        });

    if start {
        app.start_extract();
    } else if cancel || !open {
        app.pending_extract = None;
    }
}
