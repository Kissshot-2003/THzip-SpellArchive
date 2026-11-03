use std::sync::Arc;

use crate::i18n::{Locale, TextKey, Translator};
use crate::theme::ThemeStore;
use eframe::egui;
use spell_core::safety::guard_display;
use spell_core::types::{ArchiveEntry, EntryKind, Method};

pub struct DirItem {
    pub name: String,
    pub is_dir: bool,
}

pub struct TaskView {
    pub name: String,
    pub phase: String,
    pub progress: f32,
    pub speed: String,
    pub failed: bool,
}

pub struct SpellApp {
    pub tr: Translator,
    pub themes: ThemeStore,
    pub locale: Locale,
    pub show_settings: bool,
    pub show_first_run_hint: bool,

    pub dir: std::path::PathBuf,
    pub dir_items: Vec<DirItem>,

    pub archive_name: String,
    pub entries: Vec<ArchiveEntry>,
    pub search: String,
    pub selected: Option<usize>,
    pub tasks: Vec<TaskView>,
    pub demo_running: bool,

    pub status: String,
    pub limits: spell_core::Limits,
}

impl SpellApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_cjk_fonts(&cc.egui_ctx);

        let themes = ThemeStore::load(&cc.egui_ctx);
        let mut tr = Translator::new();
        tr.set_theme(themes.current());

        let mut app = Self {
            tr,
            themes,
            locale: Locale::ZhCn,
            show_settings: false,
            show_first_run_hint: true,
            dir: std::env::current_dir().unwrap_or_else(|_| ".".into()),
            dir_items: Vec::new(),
            archive_name: "reimu_spellcards.7z".to_owned(),
            entries: demo_entries(),
            search: String::new(),
            selected: Some(2),
            tasks: demo_tasks(),
            demo_running: true,
            status: "就绪 · 骨架阶段：格式后端未接入，列表为演示数据".to_owned(),
            limits: spell_core::Limits::default(),
        };

        app.themes.apply(&cc.egui_ctx);
        app.refresh_dir();
        app
    }

    pub fn set_theme(&mut self, index: usize, ctx: &egui::Context) {
        if self.themes.set(index) {
            self.tr.set_theme(self.themes.current());
            self.themes.apply(ctx);
            self.status = format!("主题已切换：{}", self.themes.current().name);
        }
    }

    pub fn refresh_dir(&mut self) {
        self.dir_items.clear();
        let Ok(read) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let mut items: Vec<DirItem> = read
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    return None;
                }
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                Some(DirItem { name, is_dir })
            })
            .collect();
        items.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
        items.truncate(300);
        self.dir_items = items;
    }

    pub fn up_dir(&mut self) {
        if let Some(parent) = self.dir.parent() {
            self.dir = parent.to_path_buf();
            self.refresh_dir();
        }
    }

    pub fn navigate(&mut self, path: std::path::PathBuf) {
        self.dir = path;
        self.refresh_dir();
        self.status = format!("目录：{}", self.dir.display());
    }

    pub fn filtered_entries(&self) -> Vec<usize> {
        let needle = self.search.trim().to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                needle.is_empty() || entry.display.to_lowercase().contains(&needle)
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub fn tick(&mut self, ctx: &egui::Context) {
        if !self.demo_running {
            return;
        }
        let dt = ctx.input(|input| input.stable_dt).min(0.1);
        let mut animating = false;
        for task in &mut self.tasks {
            if task.progress < 1.0 {
                task.progress = (task.progress + dt * 0.04).min(1.0);
                task.phase = if task.progress >= 1.0 {
                    self.tr.t(TextKey::TaskDone)
                } else {
                    self.tr.t(TextKey::TaskRunning)
                };
                animating = true;
            }
        }
        if animating {
            ctx.request_repaint();
        }
    }
}

impl eframe::App for SpellApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.tick(&ctx);

        crate::ui::hint_banner(self, ui);
        crate::ui::top_bar(self, ui);
        crate::ui::status_bar(self, ui);
        crate::ui::file_browser::show(self, ui);
        crate::ui::task_panel::show(self, ui);
        crate::ui::archive_view::show(self, ui);
        crate::ui::settings::show(self, ui);
    }
}

fn install_cjk_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 骨架阶段直接读系统字体；正式版改为 fontdb 查找 + 内嵌子集兜底。
    for candidate in ["C:/Windows/Fonts/simhei.ttf", "C:/Windows/Fonts/Deng.ttf"] {
        if let Ok(bytes) = std::fs::read(candidate) {
            fonts
                .font_data
                .insert("cjk".to_owned(), egui::FontData::from_owned(bytes).into());
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .push("cjk".to_owned());
            }
            break;
        }
    }

    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(18.0));
    });
}

fn demo_entry(
    id: u32,
    name: &str,
    kind: EntryKind,
    size: u64,
    packed: u64,
    method: Method,
    encrypted: bool,
) -> ArchiveEntry {
    let guard = guard_display(name);
    let mut entry = ArchiveEntry::demo(id, name, kind, size, packed, method);
    entry.display = Arc::from(guard.text);
    entry.encrypted = encrypted;
    entry
}

fn demo_entries() -> Vec<ArchiveEntry> {
    use EntryKind::{Dir, File};
    use Method::{Deflate, Lzma2, Ppmd, Store};

    vec![
        demo_entry(0, "素材/", Dir, 0, 0, Store, false),
        demo_entry(1, "文档/", Dir, 0, 0, Store, false),
        demo_entry(2, "灵梦_梦想封印.png", File, 248_320, 61_512, Lzma2, false),
        demo_entry(
            3,
            "魔理沙_极限火花.png",
            File,
            1_204_224,
            402_113,
            Lzma2,
            false,
        ),
        demo_entry(4, "早苗_青蛙符卡.svg", File, 38_912, 9_004, Deflate, false),
        demo_entry(5, "琪露诺_冰晶.png", File, 96_256, 41_002, Lzma2, false),
        demo_entry(
            6,
            "结界设计稿.psd",
            File,
            24_887_296,
            24_102_400,
            Store,
            false,
        ),
        demo_entry(7, "曲目列表.txt", File, 4_096, 802, Lzma2, false),
        demo_entry(8, "设定集.pdf", File, 8_421_376, 6_992_128, Deflate, true),
        demo_entry(9, "符卡数据.bin", File, 1_048_576, 131_072, Ppmd, false),
        demo_entry(
            10,
            "旧备份.7z",
            File,
            512_000_000,
            498_000_000,
            Store,
            false,
        ),
        demo_entry(
            11,
            "演出录像.mp4",
            File,
            1_073_741_824,
            1_068_000_000,
            Store,
            false,
        ),
        demo_entry(12, "readme.txt", File, 1_024, 412, Lzma2, false),
        demo_entry(13, "evil\u{202E}txt.exe", File, 65_536, 65_000, Store, true),
    ]
}

fn demo_tasks() -> Vec<TaskView> {
    vec![
        TaskView {
            name: "解除符卡封印：reimu_spellcards.7z".to_owned(),
            phase: "咏唱进行中…".to_owned(),
            progress: 0.52,
            speed: "86.2 MiB/s".to_owned(),
            failed: false,
        },
        TaskView {
            name: "构筑封印结界：summer_pack.tar.zst".to_owned(),
            phase: "咏唱进行中…".to_owned(),
            progress: 0.18,
            speed: "18.4 MiB/s".to_owned(),
            failed: false,
        },
    ]
}
