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

    // 支持「打开方式」/ 拖到 exe 上：第一个参数直接作为归档打开。
    let startup = std::env::args().nth(1).map(std::path::PathBuf::from);

    eframe::run_native(
        "SpellArchive",
        options,
        Box::new(move |cc| {
            let mut app = app::SpellApp::new(cc);
            if let Some(path) = startup {
                app.open_path(path, None);
            }
            Ok(Box::new(app))
        }),
    )
}
