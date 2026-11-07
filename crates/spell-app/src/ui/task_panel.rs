use eframe::egui;

use crate::app::{SpellApp, TaskView};
use crate::i18n::TextKey;
use crate::ui::spell_progress;
use spell_core::task::TaskPhase;

const NAME_W: f32 = 240.0;
const SPEED_W: f32 = 96.0;
const STATUS_W: f32 = 120.0;
const CANCEL_W: f32 = 24.0;

pub fn show(app: &mut SpellApp, root: &mut egui::Ui) {
    let theme = app.themes.current();
    let palette = theme.palette;
    let title = app.tr.t(TextKey::TaskQueue);
    let clear_label = app.tr.t(TextKey::ClearFinished);
    let count = app.tasks.len();
    let mut clear_finished = false;
    let mut cancel_id = None;

    egui::Panel::bottom("task_panel")
        .resizable(true)
        .default_size(196.0)
        .min_size(120.0)
        .show(root, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title.clone()).strong());
                if ui.small_button(clear_label).clicked() {
                    clear_finished = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{count} 个任务"))
                            .size(12.0)
                            .color(palette.text_sub),
                    );
                });
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // 所有行共用同一组固定列宽，进度条长度才一致。
                    let spacing = ui.spacing().item_spacing.x;
                    let bar_width = (ui.available_width()
                        - NAME_W
                        - SPEED_W
                        - STATUS_W
                        - CANCEL_W
                        - spacing * 4.0)
                        .max(120.0);

                    for task in &app.tasks {
                        ui.horizontal(|ui| {
                            let name = ui.allocate_ui_with_layout(
                                egui::vec2(NAME_W, 18.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(task.name.clone()).size(13.0),
                                        )
                                        .truncate(),
                                    )
                                },
                            );
                            if let Some(detail) = &task.detail {
                                name.inner.on_hover_text(detail);
                            }

                            spell_progress::show(
                                ui,
                                bar_width,
                                task.progress,
                                task.failed(),
                                theme,
                            );

                            ui.add_sized(
                                [SPEED_W, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(task.speed.clone())
                                        .size(12.0)
                                        .color(palette.text_sub),
                                )
                                .truncate(),
                            );

                            let (status, color) = status_text(app, task);
                            let status = ui.add_sized(
                                [STATUS_W, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(status).size(12.0).color(color),
                                )
                                .truncate(),
                            );
                            if let Some(detail) = &task.detail {
                                status.on_hover_text(detail);
                            }

                            let can_cancel = task.active();
                            ui.add_enabled_ui(can_cancel, |ui| {
                                if ui
                                    .add_sized([CANCEL_W, 18.0], egui::Button::new("×").small())
                                    .clicked()
                                {
                                    cancel_id = Some(task.id);
                                }
                            });
                        });
                        ui.add_space(2.0);
                    }
                });
        });

    if let Some(id) = cancel_id {
        if let Some(task) = app.tasks.iter().find(|task| task.id == id) {
            task.cancel.cancel();
            app.status = format!("正在取消：{}", task.name);
        }
    }
    if clear_finished {
        app.clear_finished_tasks();
    }
}

fn status_text(app: &SpellApp, task: &TaskView) -> (String, egui::Color32) {
    let palette = app.themes.current().palette;
    match task.phase {
        TaskPhase::Failed => ("失败".to_owned(), palette.error),
        TaskPhase::Done => (app.tr.t(TextKey::TaskDone), palette.success),
        TaskPhase::Running => (app.tr.t(TextKey::TaskRunning), palette.text_sub),
        other => (other.label().to_owned(), palette.text_sub),
    }
}
