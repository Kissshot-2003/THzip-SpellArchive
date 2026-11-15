use eframe::egui;

use crate::app::{TaskView, ThzipApp};
use crate::i18n::TextKey;
use crate::ui::thzip_progress;
use thzip_core::task::TaskPhase;

const SPEED_W: f32 = 92.0;
const STATUS_W: f32 = 116.0;
const CANCEL_W: f32 = 30.0;
const PAUSE_W: f32 = 30.0;

pub fn show(app: &mut ThzipApp, root: &mut egui::Ui) {
    // 空闲时把空间留给文件列表；出现任务后再展开队列。
    if app.tasks.is_empty() {
        return;
    }
    let theme = app.themes.current();
    let palette = app.themes.palette();
    let title = app.tr.t(TextKey::TaskQueue);
    let clear_label = app.tr.t(TextKey::ClearFinished);
    let count = app.tasks.len();
    let count_text = app
        .tr
        .tf(TextKey::TaskCount, &[("count", &count.to_string())]);
    let mut clear_finished = false;
    let mut cancel_id = None;
    let mut toggle_pause_id = None;

    egui::Panel::bottom("task_panel")
        .resizable(true)
        .default_size(168.0)
        .min_size(112.0)
        .max_size(280.0)
        .frame(
            egui::Frame::default()
                .fill(palette.bg_window)
                .inner_margin(egui::Margin::symmetric(14, 10)),
        )
        .show(root, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title.clone()).size(16.0).strong());
                egui::Frame::default()
                    .fill(palette.accent_soft.gamma_multiply(0.75))
                    .corner_radius(egui::CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(7, 2))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(count_text.clone())
                                .size(11.0)
                                .color(palette.accent),
                        );
                    });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(clear_label).clicked() {
                        clear_finished = true;
                    }
                });
            });
            ui.add_space(8.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // 所有行共用同一组固定列宽，进度条长度才一致。
                    let spacing = ui.spacing().item_spacing.x;
                    let name_width = (ui.available_width() * 0.25).clamp(140.0, 250.0);
                    let bar_width = (ui.available_width()
                        - name_width
                        - SPEED_W
                        - STATUS_W
                        - CANCEL_W
                        - PAUSE_W
                        - spacing * 5.0)
                        .max(100.0);

                    for task in &app.tasks {
                        egui::Frame::default()
                            .fill(palette.bg_panel)
                            .stroke(egui::Stroke::new(1.0, palette.border.gamma_multiply(0.55)))
                            .corner_radius(egui::CornerRadius::same(7))
                            .inner_margin(egui::Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let name = ui.allocate_ui_with_layout(
                                        egui::vec2(name_width, 22.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(task.name.clone())
                                                        .size(13.0),
                                                )
                                                .truncate(),
                                            )
                                        },
                                    );
                                    if let Some(detail) = &task.detail {
                                        name.inner.on_hover_text(detail);
                                    }

                                    thzip_progress::show(
                                        ui,
                                        bar_width,
                                        task.progress,
                                        task.failed(),
                                        theme,
                                        &palette,
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

                                    // 暂停只对「正在跑」和「已暂停」有意义：
                                    // 排队中的任务还没开始，完成的也没东西可停。
                                    let pausable = matches!(
                                        task.phase,
                                        TaskPhase::Running | TaskPhase::Paused
                                    );
                                    let paused = task.cancel.is_paused();
                                    let (glyph, hint) = if paused {
                                        ("▶", app.tr.t(TextKey::Resume))
                                    } else {
                                        ("❚❚", app.tr.t(TextKey::Pause))
                                    };
                                    ui.add_enabled_ui(pausable, |ui| {
                                        if ui
                                            .add_sized([PAUSE_W, 24.0], egui::Button::new(glyph))
                                            .on_hover_text(hint)
                                            .clicked()
                                        {
                                            toggle_pause_id = Some(task.id);
                                        }
                                    });

                                    let can_cancel = task.active();
                                    ui.add_enabled_ui(can_cancel, |ui| {
                                        if ui
                                            .add_sized([CANCEL_W, 24.0], egui::Button::new("×"))
                                            .on_hover_text(app.tr.t(TextKey::Cancel))
                                            .clicked()
                                        {
                                            cancel_id = Some(task.id);
                                        }
                                    });
                                });
                            });
                        ui.add_space(6.0);
                    }
                });
        });

    if let Some(id) = toggle_pause_id
        && let Some(task) = app.tasks.iter_mut().find(|task| task.id == id)
    {
        if task.cancel.is_paused() {
            task.cancel.resume();
            task.phase = TaskPhase::Running;
        } else {
            task.cancel.pause();
            // 界面立刻反映「已暂停」，不必等 worker 走到下一个批次边界。
            // worker 在暂停期间不会上报 PhaseChanged，所以这里不会被立刻覆盖。
            task.phase = TaskPhase::Paused;
        }
    }

    if let Some(id) = cancel_id
        && let Some(task) = app.tasks.iter().find(|task| task.id == id)
    {
        task.cancel.cancel();
        app.status = app.tr.tf(TextKey::CancellingTask, &[("name", &task.name)]);
    }
    if clear_finished {
        app.clear_finished_tasks();
    }
}

fn status_text(app: &ThzipApp, task: &TaskView) -> (String, egui::Color32) {
    let palette = app.themes.palette();
    let color = match task.phase {
        TaskPhase::Failed => palette.error,
        TaskPhase::Done => palette.success,
        _ => palette.text_sub,
    };
    (phase_label(app, task.phase), color)
}

fn phase_label(app: &ThzipApp, phase: TaskPhase) -> String {
    let key = match phase {
        TaskPhase::Pending => TextKey::PhasePending,
        TaskPhase::Planning => TextKey::PhasePlanning,
        TaskPhase::AwaitingDecision => TextKey::PhaseAwaitingDecision,
        TaskPhase::Running => TextKey::TaskRunning,
        TaskPhase::Paused => TextKey::PhasePaused,
        TaskPhase::Done => TextKey::TaskDone,
        TaskPhase::Failed => TextKey::PhaseFailed,
    };
    app.tr.t(key)
}
