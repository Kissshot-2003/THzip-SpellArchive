use eframe::egui;

use crate::app::ThzipApp;
use crate::i18n::{Locale, TextKey, builtin};

/// 命令面板里可执行的命令。顺序即默认展示顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Open,
    Extract,
    Test,
    Pack,
    Settings,
    ToggleTheme,
    ToggleLanguage,
}

impl Command {
    const ALL: [Command; 7] = [
        Command::Open,
        Command::Extract,
        Command::Test,
        Command::Pack,
        Command::Settings,
        Command::ToggleTheme,
        Command::ToggleLanguage,
    ];

    fn key(self) -> TextKey {
        match self {
            Command::Open => TextKey::Open,
            Command::Extract => TextKey::Extract,
            Command::Test => TextKey::TestArchive,
            Command::Pack => TextKey::Pack,
            Command::Settings => TextKey::Settings,
            Command::ToggleTheme => TextKey::CmdToggleTheme,
            Command::ToggleLanguage => TextKey::CmdToggleLanguage,
        }
    }

    /// 需要已打开归档的命令在无归档时禁用，避免回车后只得到一条错误状态。
    fn enabled(self, app: &ThzipApp) -> bool {
        match self {
            Command::Extract | Command::Test => app.reader.is_some(),
            _ => true,
        }
    }

    fn run(self, app: &mut ThzipApp, ctx: &egui::Context) {
        match self {
            Command::Open => app.open_archive_dialog(),
            Command::Extract => app.begin_extract_dialog(),
            Command::Test => app.start_test(),
            Command::Pack => app.open_pack_dialog(),
            Command::Settings => app.show_settings = true,
            Command::ToggleTheme => app.cycle_theme(ctx),
            // 语言切换要重建托盘与右键菜单，沿用统一入口。
            Command::ToggleLanguage => {
                app.locale = app.locale.toggled();
                app.sync_locale();
            }
        }
    }
}

pub fn show(app: &mut ThzipApp, ctx: &egui::Context) {
    if !app.commands.open {
        return;
    }

    let palette = app.themes.palette();
    // 中文界面下也允许用英文关键词过滤，键盘用户不必切输入法。
    let entries: Vec<(Command, String, String, bool)> = Command::ALL
        .iter()
        .map(|&command| {
            (
                command,
                app.tr.t(command.key()),
                builtin(Locale::EnUs, command.key()).to_owned(),
                command.enabled(app),
            )
        })
        .collect();

    let mut filter = std::mem::take(&mut app.commands.filter);
    let mut run = None;
    let mut close = false;
    let title = app.tr.t(TextKey::CommandPalette);
    let hint = app.tr.t(TextKey::CommandPaletteHint);
    let empty = app.tr.t(TextKey::CommandNoMatch);

    let mut open = true;
    egui::Window::new(title)
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .title_bar(false)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 96.0))
        .default_width(420.0)
        .show(ctx, |ui| {
            egui::Frame::default()
                .fill(palette.bg_panel)
                .stroke(egui::Stroke::new(1.0, palette.border))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut filter)
                            .hint_text(hint)
                            .desired_width(f32::INFINITY),
                    );
                    // 打开即聚焦；每帧请求不会打断输入，因为焦点本来就在这个框里。
                    response.request_focus();

                    let needle = filter.trim().to_lowercase();
                    let matches: Vec<(Command, String)> = entries
                        .iter()
                        .filter(|(_, zh, en, enabled)| {
                            *enabled
                                && (zh.to_lowercase().contains(&needle)
                                    || en.to_lowercase().contains(&needle))
                        })
                        .map(|(command, zh, _, _)| (*command, zh.clone()))
                        .collect();

                    ui.add_space(6.0);
                    ui.separator();
                    if matches.is_empty() {
                        ui.label(egui::RichText::new(empty).color(palette.text_sub));
                    }
                    for (command, label) in &matches {
                        if ui
                            .add_sized(
                                [ui.available_width(), 26.0],
                                egui::Button::new(label.clone()).frame(false),
                            )
                            .clicked()
                        {
                            run = Some(*command);
                        }
                    }

                    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                        close = true;
                    }
                    // 回车执行第一条匹配项：过滤词通常已经收敛到唯一命令。
                    if ui.input(|input| input.key_pressed(egui::Key::Enter))
                        && let Some((command, _)) = matches.first()
                    {
                        run = Some(*command);
                    }
                });
        });

    app.commands.filter = filter;
    if let Some(command) = run {
        app.commands.open = false;
        command.run(app, ctx);
        return;
    }
    if close || !open {
        app.commands.open = false;
    }
}
