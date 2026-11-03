mod app;
mod i18n;
mod theme;
mod ui;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([960.0, 600.0])
            .with_title("SpellArchive"),
        ..Default::default()
    };

    eframe::run_native(
        "SpellArchive",
        options,
        Box::new(|cc| Ok(Box::new(app::SpellApp::new(cc)))),
    )
}
