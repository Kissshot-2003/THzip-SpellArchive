use eframe::egui;

use crate::theme::{ResKey, Theme};

/// 九宫格贴图的运行时数据；`slices` 单位是 1x 像素。
#[derive(Clone, Copy)]
pub struct NinePatch {
    id: egui::TextureId,
    size: egui::Vec2,
    slices: [u32; 4],
    scale: u32,
}

impl NinePatch {
    pub fn from_theme(theme: &Theme, key: ResKey) -> Option<Self> {
        let texture = theme.texture(key)?;
        let slices = theme.nine_patch.get(&key).copied()?;
        Some(Self {
            id: texture.id(),
            size: texture.size_vec2(),
            slices,
            scale: theme.scale.max(1),
        })
    }

    /// 把九宫格贴到 `rect`。缺贴图/切片越界时调用方可退回拉伸或纯色。
    pub fn paint(self, ui: &egui::Ui, rect: egui::Rect, tint: egui::Color32) {
        if self.size.x <= 1.0 || self.size.y <= 1.0 {
            return;
        }
        let scale = self.scale as f32;
        let px = [
            self.slices[0] as f32 * scale,
            self.slices[1] as f32 * scale,
            self.slices[2] as f32 * scale,
            self.slices[3] as f32 * scale,
        ];
        if px[0] + px[2] >= self.size.x || px[1] + px[3] >= self.size.y {
            // 切片超过贴图，按整体拉伸处理。
            ui.painter().image(
                self.id,
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                tint,
            );
            return;
        }

        // 目标边宽按 1x 逻辑像素；纹理 UV 按 scale 后的像素换算。
        let dest = [
            self.slices[0] as f32,
            self.slices[1] as f32,
            self.slices[2] as f32,
            self.slices[3] as f32,
        ];
        if dest[0] + dest[2] >= rect.width() || dest[1] + dest[3] >= rect.height() {
            ui.painter().image(
                self.id,
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                tint,
            );
            return;
        }

        let u = [0.0, px[0] / self.size.x, 1.0 - px[2] / self.size.x, 1.0];
        let v = [0.0, px[1] / self.size.y, 1.0 - px[3] / self.size.y, 1.0];
        let x = [
            rect.left(),
            rect.left() + dest[0],
            rect.right() - dest[2],
            rect.right(),
        ];
        let y = [
            rect.top(),
            rect.top() + dest[1],
            rect.bottom() - dest[3],
            rect.bottom(),
        ];

        let mut mesh = egui::Mesh {
            texture_id: self.id,
            ..Default::default()
        };
        for row in 0..3 {
            for col in 0..3 {
                let cell = egui::Rect::from_min_max(
                    egui::pos2(x[col], y[row]),
                    egui::pos2(x[col + 1], y[row + 1]),
                );
                if cell.width() <= 0.0 || cell.height() <= 0.0 {
                    continue;
                }
                let uv = egui::Rect::from_min_max(
                    egui::pos2(u[col], v[row]),
                    egui::pos2(u[col + 1], v[row + 1]),
                );
                mesh.add_rect_with_uv(cell, uv, tint);
            }
        }
        ui.painter().add(egui::Shape::mesh(mesh));
    }
}
