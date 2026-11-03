use eframe::egui;

use crate::app::SpellApp;
use crate::i18n::TextKey;
use crate::ui::spell_progress;

const NAME_W: f32 = 240.0;
const SPEED_W: f32 = 96.0;
const STATUS_W: f32 = 120.0;

pub fn show(app: &mut SpellApp, root: &mut egui::Ui) {
    let theme = app.themes.current().clone();
    let palette = theme.palette;
    let title = app.tr.t(TextKey::TaskQueue);
    let count = app.tasks.len();

    egui::Panel::bottom("task_panel")
        .resizable(true)
        .default_size(196.0)
        .min_size(120.0)
        .show(root, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title.clone()).strong());
                if ui.small_button("重播演示").clicked() {
                    for task in &mut app.tasks {
                        task.progress = 0.0;
                        task.failed = false;
                    }
                    app.demo_running = true;
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
                    let bar_width =
                        (ui.available_width() - NAME_W - SPEED_W - STATUS_W - spacing * 3.0)
                            .max(120.0);

                    for task in &app.tasks {
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(NAME_W, 18.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(task.name.clone()).size(13.0),
                                        )
                                        .truncate(),
                                    );
                                },
                            );

                            spell_progress::show(ui, bar_width, task.progress, task.failed, &theme);

                            let status = if task.failed {
                                egui::RichText::new("失败").color(palette.error)
                            } else {
                                egui::RichText::new(task.phase.clone())
                                    .size(12.0)
                                    .color(palette.text_sub)
                            };
                            ui.add_sized(
                                [SPEED_W, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(task.speed.clone())
                                        .size(12.0)
                                        .color(palette.text_sub),
                                )
                                .truncate(),
                            );
                            ui.add_sized([STATUS_W, 18.0], egui::Label::new(status).truncate());
                        });
                        ui.add_space(2.0);
                    }
                });
        });
}
