use eframe::egui;

use crate::theme::{Palette, ResKey, Theme};

/// 符阵进度条：底图 + 按百分比裁剪的填充贴图（UV 裁剪，不拉伸贴图）。
///
/// 主题缺贴图时退回 10 段纯色块，因此 plain 主题也能正常工作。
/// `palette` 由调用方传入，高对比度模式下贴图缺失时的兜底色也一起生效。
pub fn show(
    ui: &mut egui::Ui,
    width: f32,
    ratio: f32,
    failed: bool,
    theme: &Theme,
    palette: &Palette,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 20.0), egui::Sense::hover());
    // 贴图坐标吸附到物理像素，避免像素素材被半像素拉伸。
    let ppp = ui.ctx().pixels_per_point();
    let snap = |value: f32| (value * ppp).round() / ppp;
    let rect = egui::Rect::from_min_max(
        egui::pos2(snap(rect.min.x), snap(rect.min.y)),
        egui::pos2(snap(rect.max.x), snap(rect.max.y)),
    );
    let painter = ui.painter();
    let ratio = ratio.clamp(0.0, 1.0);
    let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

    // 底图优先九宫格，避免像素边框随宽度拉伸变形。
    match crate::ui::nine_patch::NinePatch::from_theme(theme, ResKey::ProgressSpellBg) {
        Some(bg) => bg.paint(ui, rect, egui::Color32::WHITE),
        None => match theme.texture(ResKey::ProgressSpellBg) {
            Some(bg) => {
                painter.add(egui::Shape::image(
                    bg.id(),
                    rect,
                    full_uv,
                    egui::Color32::WHITE,
                ));
            }
            None => {
                painter.rect_filled(rect, egui::CornerRadius::same(3), palette.progress_bg);
            }
        },
    }

    // 逐格点亮：量化到 1/10。
    let lit = (ratio * 10.0).round() / 10.0;

    let fill_key = if failed && theme.texture(ResKey::ProgressSpellFail).is_some() {
        ResKey::ProgressSpellFail
    } else {
        ResKey::ProgressSpellFill
    };
    let tint = if failed {
        egui::Color32::from_rgb(0xFF, 0x9A, 0xA2)
    } else {
        egui::Color32::WHITE
    };

    if lit > 0.0 {
        match theme.texture(fill_key) {
            Some(fill) => {
                // 关键：uv 与目标矩形同时按比例裁剪，否则会把左侧 52% 拉伸到整条。
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(lit, 1.0));
                let fill_rect = egui::Rect::from_min_max(
                    rect.min,
                    egui::pos2(rect.left() + rect.width() * lit, rect.max.y),
                );
                painter.add(egui::Shape::image(fill.id(), fill_rect, uv, tint));
            }
            None => {
                let segments = 10usize;
                let gap = 2.0;
                let inner = rect.shrink(2.0);
                let seg_width =
                    ((inner.width() - gap * (segments as f32 - 1.0)) / segments as f32).max(1.0);
                let lit_segments = (ratio * segments as f32).round() as usize;
                let color = if failed {
                    palette.error
                } else {
                    palette.accent
                };
                for index in 0..lit_segments {
                    let x = inner.left() + index as f32 * (seg_width + gap);
                    let segment = egui::Rect::from_min_size(
                        egui::pos2(x, inner.top()),
                        egui::vec2(seg_width, inner.height()),
                    );
                    painter.rect_filled(segment, egui::CornerRadius::same(2), color);
                }
            }
        }
    }

    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{:.0}%", ratio * 100.0),
        egui::FontId::proportional(10.0),
        palette.text_normal,
    );

    response
}
