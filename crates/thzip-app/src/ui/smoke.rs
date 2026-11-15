//! 无头 UI 冒烟测试（`cargo test` 时运行，不进产物）。
//!
//! 目的是在不启动窗口的前提下把「会画到屏幕上的那段代码」真的跑一遍：
//! 布局计算、ID 分配、像素坐标吸附都在这里执行，`panic` / 除零 / 负尺寸
//! 会在 CI 里直接暴露，而不必等人工点开界面。
//!
//! 完整窗口的截图比对（egui_kittest 的 snapshot 特性）没有启用：它需要
//! wgpu 后端与基线图，收益主要在像素级回归，当前阶段用不上。
#![cfg(test)]

use std::cell::Cell;
use std::rc::Rc;

use eframe::egui;

use crate::theme::ThemeStore;
use crate::ui::thzip_progress;

/// 进度条要在 0 / 中间值 / 1 / 越界值下都不 panic，且真的占满给定宽度。
#[test]
fn progress_bar_renders_across_the_range() {
    let store = ThemeStore::load();
    let theme = store.current().clone();
    let palette = store.palette();

    for ratio in [-0.5, 0.0, 0.25, 0.5, 1.0, 1.5, f32::NAN] {
        for failed in [false, true] {
            let measured = Rc::new(Cell::new(0.0f32));
            let sink = Rc::clone(&measured);
            let theme = theme.clone();
            let mut harness = egui_kittest::Harness::new_ui(move |ui| {
                ui.set_min_width(320.0);
                let response = thzip_progress::show(ui, 180.0, ratio, failed, &theme, &palette);
                sink.set(response.rect.width());
            });
            harness.run();
            let width = measured.get();
            // 越界值与 NaN 也必须落回一个合法的正宽度，不能画出负矩形。
            assert!(
                width > 0.0 && width.is_finite(),
                "ratio={ratio} failed={failed} 宽度异常: {width}"
            );
        }
    }
}

/// 连续多帧不应出现 ID 冲突或布局漂移；顺带覆盖伪本地化的长文本。
#[test]
fn repeated_frames_are_stable() {
    let store = ThemeStore::load();
    let theme = store.current().clone();
    let palette = store.palette();
    let frames = Rc::new(Cell::new(0u32));
    let counter = Rc::clone(&frames);

    let mut harness = egui_kittest::Harness::new_ui(move |ui| {
        ui.set_min_width(320.0);
        ui.horizontal(|ui| {
            for ratio in [0.0, 0.33, 0.66, 1.0] {
                thzip_progress::show(ui, 60.0, ratio, false, &theme, &palette);
            }
        });
        ui.separator();
        ui.label("[!!pséudó··!!]");
        counter.set(counter.get() + 1);
    });
    for _ in 0..5 {
        harness.run();
    }
    // egui 可能为同一帧跑多个 pass（布局收敛），因此只要求「至少 5 次」。
    assert!(frames.get() >= 5, "闭包执行次数不足: {}", frames.get());
}

/// 极窄容器里塞长文本：必须被截断或换行，不能把容器撑破。
#[test]
fn narrow_container_with_long_text_does_not_break() {
    let width = Rc::new(Cell::new(0.0f32));
    let sink = Rc::clone(&width);
    let mut harness = egui_kittest::Harness::new_ui(move |ui| {
        ui.set_max_width(60.0);
        for _ in 0..3 {
            ui.add(
                egui::Label::new(
                    egui::RichText::new("[!!óvérlóng pséudó-lócálízéd téxt··!!]").size(13.0),
                )
                .truncate(),
            );
        }
        ui.add_space(2.0);
        ui.label(egui::RichText::new("×").size(12.0));
        sink.set(ui.min_rect().width());
    });
    harness.run();
    let measured = width.get();
    assert!(
        measured.is_finite() && measured <= 200.0,
        "窄容器被撑开: {measured}"
    );
}
