use eframe::egui;

use crate::app::SpellApp;
use crate::theme::ResKey;

/// 主题粒子：花瓣/星屑从窗口顶部飘落，不拦截鼠标事件。
pub fn draw(app: &mut SpellApp, ctx: &egui::Context) {
    let (texture, enabled, content) = {
        let theme = app.themes.current();
        let texture = theme
            .texture(ResKey::ParticleSakura)
            .or_else(|| theme.texture(ResKey::ParticleSparkle))
            .map(|texture| texture.id());
        (texture, theme.particles, ctx.content_rect())
    };
    let Some(texture) = texture.filter(|_| enabled) else {
        app.particles.clear();
        return;
    };

    let dt = ctx.input(|input| input.stable_dt).clamp(0.0, 0.1);

    app.particles.retain_mut(|particle| {
        particle.phase += dt * 2.0;
        particle.pos.x += particle.phase.sin() * particle.sway * dt;
        particle.pos.y += particle.vy * dt;
        particle.life -= dt;
        particle.life > 0.0 && particle.pos.y < content.bottom() + 24.0
    });

    app.particle_accum += dt * 8.0;
    while app.particle_accum >= 1.0 && app.particles.len() < 80 {
        app.particle_accum -= 1.0;
        let size = 8.0 + rand(&mut app.particle_seed) * 10.0;
        let life = 6.0 + rand(&mut app.particle_seed) * 6.0;
        app.particles.push(crate::app::Particle {
            pos: egui::pos2(
                content.left() + rand(&mut app.particle_seed) * content.width(),
                content.top() - size,
            ),
            vy: 24.0 + rand(&mut app.particle_seed) * 46.0,
            sway: 10.0 + rand(&mut app.particle_seed) * 30.0,
            phase: rand(&mut app.particle_seed) * std::f32::consts::TAU,
            size,
            life,
            max_life: life,
        });
    }

    if app.particles.is_empty() {
        return;
    }

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::unique("theme_particles"),
    ));
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    for particle in &app.particles {
        let alpha = ((particle.life / particle.max_life).clamp(0.0, 1.0) * 0.85 * 255.0) as u8;
        let rect = egui::Rect::from_center_size(
            particle.pos,
            egui::vec2(particle.size, particle.size * 0.85),
        );
        painter.image(texture, rect, uv, egui::Color32::from_white_alpha(alpha));
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(33));
}

fn rand(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*seed >> 8) as f32 / 16_777_216.0
}
