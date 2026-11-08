use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crate::i18n::{Locale, TextKey, Translator};
use crate::theme::ThemeStore;
use crate::tray::{Tray, TrayAction};
use eframe::egui;
use spell_core::plan::{DecisionTable, ExtractPlan, OverwritePolicy};
use spell_core::secret::SecretString;
use spell_core::task::{
    CancelToken, SharedReader, TaskEvent, TaskId, TaskOp, TaskPhase, TaskRunner,
};
use spell_core::types::{ArchiveEntry, ArchiveKind, Method};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Size,
    Packed,
    Ratio,
    Method,
    Time,
}

impl SortKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Size => "size",
            Self::Packed => "packed",
            Self::Ratio => "ratio",
            Self::Method => "method",
            Self::Time => "time",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "name" => Some(Self::Name),
            "size" => Some(Self::Size),
            "packed" => Some(Self::Packed),
            "ratio" => Some(Self::Ratio),
            "method" => Some(Self::Method),
            "time" => Some(Self::Time),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Particle {
    pub pos: egui::Pos2,
    pub vy: f32,
    pub sway: f32,
    pub phase: f32,
    pub size: f32,
    pub life: f32,
    pub max_life: f32,
}

#[derive(Default)]
pub struct ArchiveTab {
    pub name: String,
    pub kind: Option<ArchiveKind>,
    pub path: Option<PathBuf>,
    pub reader: Option<SharedReader>,
    pub entries: Vec<ArchiveEntry>,
    pub search: String,
    pub selected: std::collections::BTreeSet<usize>,
    pub last_selected: Option<usize>,
}

pub struct DirItem {
    pub name: String,
    pub is_dir: bool,
}

pub struct TaskView {
    pub id: TaskId,
    pub name: String,
    pub phase: TaskPhase,
    pub progress: f32,
    pub speed: String,
    pub written: u64,
    pub total: u64,
    pub detail: Option<String>,
    pub cancel: CancelToken,
    last_bytes: u64,
    last_sample: Instant,
}

impl TaskView {
    pub fn failed(&self) -> bool {
        matches!(self.phase, TaskPhase::Failed)
    }

    pub fn active(&self) -> bool {
        matches!(
            self.phase,
            TaskPhase::Pending
                | TaskPhase::Planning
                | TaskPhase::AwaitingDecision
                | TaskPhase::Running
        )
    }
}

pub struct PendingPassword {
    pub path: PathBuf,
    pub input: String,
    pub error: Option<String>,
}

pub struct PendingExtract {
    pub plan: ExtractPlan,
    pub target: PathBuf,
    pub policy: OverwritePolicy,
    /// 预检对应的归档；即使用户切换了标签页也不能换错。
    pub reader: SharedReader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZipPackMethod {
    #[default]
    Deflate,
    Bzip2,
    Zstd,
}

impl ZipPackMethod {
    pub fn label(self) -> &'static str {
        match self {
            Self::Deflate => "Deflate",
            Self::Bzip2 => "BZip2",
            Self::Zstd => "ZSTD",
        }
    }

    pub fn method(self) -> Method {
        match self {
            Self::Deflate => Method::Deflate,
            Self::Bzip2 => Method::Bzip2,
            Self::Zstd => Method::Zstd,
        }
    }
}

#[derive(Default)]
pub struct PackState {
    pub sources: Vec<PathBuf>,
    pub output: String,
    pub store: bool,
    pub zip_method: ZipPackMethod,
    pub append: bool,
    pub encrypt: bool,
    pub password: String,
    pub error: Option<String>,
    pub confirm_overwrite: bool,
}

pub struct SpellApp {
    pub tr: Translator,
    pub themes: ThemeStore,
    pub locale: Locale,
    pub show_settings: bool,
    pub show_first_run_hint: bool,
    pub close_to_tray: bool,

    pub dir: PathBuf,
    pub dir_items: Vec<DirItem>,

    pub archive_name: String,
    pub archive_kind: Option<ArchiveKind>,
    pub archive_path: Option<PathBuf>,
    pub reader: Option<SharedReader>,
    pub entries: Vec<ArchiveEntry>,
    pub search: String,
    pub selected: std::collections::BTreeSet<usize>,
    pub last_selected: Option<usize>,
    pub sort_key: SortKey,
    pub sort_asc: bool,
    /// 非活动标签页的快照；`tabs[active_tab]` 是空占位，活动数据在字段里。
    pub tabs: Vec<ArchiveTab>,
    pub active_tab: usize,
    pub particles: Vec<Particle>,
    pub particle_seed: u32,
    pub particle_accum: f32,

    pub runner: TaskRunner,
    pub tasks: Vec<TaskView>,
    pub pending_password: Option<PendingPassword>,
    pub pending_extract: Option<PendingExtract>,
    pub show_pack: bool,
    pub pack_state: PackState,

    pub status: String,
    pub limits: spell_core::Limits,

    settings_dirty: bool,
    last_settings_save: Instant,
    next_handoff_check: Instant,
    theme_signature: Option<(u64, SystemTime)>,
    next_theme_check: Instant,
    tray: Option<Tray>,
    quitting: bool,
}

impl SpellApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_cjk_fonts(&cc.egui_ctx);

        let prefs = crate::prefs::Prefs::load();
        let mut themes = ThemeStore::load();
        if let Some(theme) = prefs.theme.as_deref() {
            themes.select_by_id(theme);
        }
        themes.ensure_loaded(&cc.egui_ctx);
        let locale = Locale::from_code(&prefs.locale).unwrap_or(Locale::ZhCn);
        let mut tr = Translator::new();
        tr.locale = locale;
        tr.prefer_standard = prefs.prefer_standard;
        tr.set_theme(themes.current());

        let dir = prefs
            .last_dir
            .as_ref()
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()));
        let theme_signature = crate::theme::loader::signature();
        let tray = Tray::new(&tr);

        let mut app = Self {
            tr,
            themes,
            locale,
            show_settings: false,
            show_first_run_hint: prefs.show_hint,
            close_to_tray: prefs.close_to_tray,
            dir,
            dir_items: Vec::new(),
            archive_name: String::new(),
            archive_kind: None,
            archive_path: None,
            reader: None,
            entries: Vec::new(),
            search: String::new(),
            selected: std::collections::BTreeSet::new(),
            last_selected: None,
            sort_key: prefs
                .sort_key
                .as_deref()
                .and_then(SortKey::from_str)
                .unwrap_or(SortKey::Name),
            sort_asc: prefs.sort_asc,
            tabs: vec![ArchiveTab::default()],
            active_tab: 0,
            particles: Vec::new(),
            particle_seed: SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.subsec_nanos() ^ duration.as_secs() as u32)
                .unwrap_or(0x9E37_79B9),
            particle_accum: 0.0,
            runner: TaskRunner::new(2),
            tasks: Vec::new(),
            pending_password: None,
            pending_extract: None,
            show_pack: false,
            pack_state: PackState::default(),
            status: "就绪".to_owned(),
            limits: spell_core::Limits::default(),
            settings_dirty: false,
            last_settings_save: Instant::now(),
            next_handoff_check: Instant::now(),
            theme_signature,
            next_theme_check: Instant::now() + Duration::from_secs(1),
            tray,
            quitting: false,
        };

        app.themes.apply(&cc.egui_ctx);
        app.refresh_dir();
        // exe 移动或升级后，右键菜单里的旧路径自动修正。
        if let Err(err) = crate::shell_menu::refresh_if_installed() {
            crate::logging::warn("shell_menu", &format!("refresh failed: {err}"));
        }
        app
    }

    pub fn set_theme(&mut self, index: usize, ctx: &egui::Context) {
        if self.themes.set(index) {
            self.themes.ensure_loaded(ctx);
            self.tr.set_theme(self.themes.current());
            self.themes.apply(ctx);
            self.status = format!("已切换到主题：{}", self.themes.current().name);
            self.touch_settings();
        }
    }

    /// 语言切换后重建托盘菜单，避免托盘一直显示旧语言。
    pub fn rebuild_tray(&mut self) {
        if let Some(tray) = Tray::new(&self.tr) {
            self.tray = Some(tray);
        }
    }

    pub fn touch_settings(&mut self) {
        self.settings_dirty = true;
    }

    fn stash_active_tab(&mut self) -> ArchiveTab {
        ArchiveTab {
            name: std::mem::take(&mut self.archive_name),
            kind: self.archive_kind.take(),
            path: self.archive_path.take(),
            reader: self.reader.take(),
            entries: std::mem::take(&mut self.entries),
            search: std::mem::take(&mut self.search),
            selected: std::mem::take(&mut self.selected),
            last_selected: self.last_selected.take(),
        }
    }

    fn restore_active_tab(&mut self, tab: ArchiveTab) {
        self.archive_name = tab.name;
        self.archive_kind = tab.kind;
        self.archive_path = tab.path;
        self.reader = tab.reader;
        self.entries = tab.entries;
        self.search = tab.search;
        self.selected = tab.selected;
        self.last_selected = tab.last_selected;
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn active_tab_index(&self) -> usize {
        self.active_tab
    }

    pub fn tab_title(&self, index: usize) -> String {
        if index == self.active_tab {
            if self.archive_name.is_empty() {
                self.tr.t(TextKey::NewTab).to_owned()
            } else {
                self.archive_name.clone()
            }
        } else {
            self.tabs
                .get(index)
                .map(|tab| tab.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| self.tr.t(TextKey::NewTab).to_owned())
        }
    }

    pub fn activate_tab(&mut self, index: usize) {
        if index == self.active_tab || index >= self.tabs.len() {
            return;
        }
        let current = self.stash_active_tab();
        let target = std::mem::take(&mut self.tabs[index]);
        self.tabs[self.active_tab] = current;
        self.active_tab = index;
        self.restore_active_tab(target);
        self.status = format!("切换到：{}", self.tab_title(index));
    }

    pub fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        if self.tabs.len() == 1 {
            self.tabs[0] = ArchiveTab::default();
            self.active_tab = 0;
            self.archive_name.clear();
            self.archive_kind = None;
            self.archive_path = None;
            self.reader = None;
            self.entries.clear();
            self.search.clear();
            self.selected.clear();
            self.last_selected = None;
            self.status = self.tr.t(TextKey::StatusReady).to_owned();
            return;
        }

        if index == self.active_tab {
            let other = if self.active_tab + 1 < self.tabs.len() {
                self.active_tab + 1
            } else {
                self.active_tab - 1
            };
            let target = std::mem::take(&mut self.tabs[other]);
            self.tabs.remove(self.active_tab);
            self.active_tab = if other > self.active_tab {
                other - 1
            } else {
                other
            };
            self.restore_active_tab(target);
        } else {
            self.tabs.remove(index);
            if index < self.active_tab {
                self.active_tab -= 1;
            }
        }
        self.status = format!("已关闭标签页，剩余 {} 个", self.tabs.len());
    }

    /// 打开归档成功时决定是替换当前标签还是新开一个。
    fn prepare_tab_for_open(&mut self, path: &PathBuf) {
        let same = self.archive_path.as_ref() == Some(path);
        if same || self.archive_name.is_empty() {
            return;
        }
        let current = self.stash_active_tab();
        self.tabs[self.active_tab] = current;
        self.tabs.push(ArchiveTab::default());
        self.active_tab = self.tabs.len() - 1;
    }

    fn show_window(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn hide_window(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    fn poll_tray(&mut self, ctx: &egui::Context) {
        let Some(tray) = self.tray.as_ref() else {
            return;
        };
        let actions = tray.poll();
        for action in actions {
            match action {
                TrayAction::Show => self.show_window(ctx),
                TrayAction::Hide => self.hide_window(ctx),
                TrayAction::Quit => {
                    self.quitting = true;
                    self.show_window(ctx);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    /// 第二实例把要打开的文件写进 handoff.txt；这里定期取走。
    fn poll_handoff(&mut self, ctx: &egui::Context) {
        if self.next_handoff_check > Instant::now() {
            return;
        }
        self.next_handoff_check = Instant::now() + Duration::from_millis(250);

        let Some(path) = crate::prefs::handoff_path() else {
            return;
        };
        let Ok(meta) = std::fs::metadata(&path) else {
            return;
        };
        if meta
            .modified()
            .ok()
            .and_then(|time| time.elapsed().ok())
            .is_some_and(|age| age > Duration::from_secs(15))
        {
            let _ = std::fs::remove_file(&path);
            return;
        }

        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let _ = std::fs::remove_file(&path);
        self.show_window(ctx);
        let mut opened = 0usize;
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let target = PathBuf::from(line);
            if target.is_dir() {
                self.navigate(target);
            } else {
                self.open_path(target, None);
                opened += 1;
            }
        }
        if opened > 1 {
            self.status = format!("正在打开 {opened} 个转交归档（各占一个标签页）");
        }
    }

    /// 每秒检查主题文件是否有变化；有变化就重载并保持当前主题 id。
    fn poll_theme(&mut self, ctx: &egui::Context) {
        if self.next_theme_check > Instant::now() {
            return;
        }
        self.next_theme_check = Instant::now() + Duration::from_secs(1);

        let Some(signature) = crate::theme::loader::signature() else {
            return;
        };
        if self.theme_signature == Some(signature) {
            return;
        }
        self.theme_signature = Some(signature);
        crate::logging::debug("theme", "signature changed, reloading");

        let current_id = self.themes.current_id().to_owned();
        let mut themes = ThemeStore::load();
        themes.select_by_id(&current_id);
        themes.ensure_loaded(ctx);
        self.themes = themes;
        self.tr.set_theme(self.themes.current());
        self.themes.apply(ctx);
        self.status = "主题文件已重新加载".to_owned();
    }

    fn maybe_save_settings(&mut self) {
        if !self.settings_dirty {
            return;
        }
        if self.last_settings_save.elapsed() < Duration::from_millis(400) {
            return;
        }

        let prefs = crate::prefs::Prefs {
            schema: crate::prefs::CURRENT_SCHEMA,
            theme: Some(self.themes.current_id().to_owned()),
            locale: self.locale.code().to_owned(),
            prefer_standard: self.tr.prefer_standard,
            show_hint: self.show_first_run_hint,
            close_to_tray: self.close_to_tray,
            last_dir: Some(self.dir.display().to_string()),
            sort_key: Some(self.sort_key.as_str().to_owned()),
            sort_asc: self.sort_asc,
        };
        if let Err(err) = prefs.save() {
            crate::logging::warn("prefs", &format!("save failed: {err}"));
        }
        self.settings_dirty = false;
        self.last_settings_save = Instant::now();
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
        items.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| natural_cmp(&a.name, &b.name))
        });
        items.truncate(300);
        self.dir_items = items;
    }

    pub fn up_dir(&mut self) {
        if let Some(parent) = self.dir.parent() {
            self.dir = parent.to_path_buf();
            self.refresh_dir();
            self.touch_settings();
        }
    }

    pub fn navigate(&mut self, path: PathBuf) {
        self.dir = path;
        self.refresh_dir();
        self.status = format!("目录：{}", self.dir.display());
        self.touch_settings();
    }

    pub fn filtered_entries(&self) -> Vec<usize> {
        let needle = self.search.trim().to_lowercase();
        let mut rows: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| search_matches(&needle, &entry.display))
            .map(|(index, _)| index)
            .collect();

        rows.sort_by(|&a, &b| {
            let left = &self.entries[a];
            let right = &self.entries[b];
            let order = match self.sort_key {
                SortKey::Name => natural_cmp(&left.display, &right.display),
                SortKey::Size => left.size.cmp(&right.size),
                SortKey::Packed => left.packed.cmp(&right.packed),
                SortKey::Ratio => left
                    .ratio()
                    .partial_cmp(&right.ratio())
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortKey::Method => left.method.label().cmp(right.method.label()),
                SortKey::Time => left.mtime.cmp(&right.mtime),
            };
            if self.sort_asc {
                order
            } else {
                order.reverse()
            }
            .then_with(|| natural_cmp(&left.display, &right.display))
        });
        rows
    }

    pub fn select_all_filtered(&mut self) {
        for index in self.filtered_entries() {
            self.selected.insert(index);
        }
    }

    /// 工具栏 / 文件列表入口：弹原生文件对话框后启动后台打开任务。
    pub fn open_archive_dialog(&mut self) {
        let start = self
            .archive_path
            .as_ref()
            .and_then(|path| path.parent())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.dir.clone());
        let picked = rfd::FileDialog::new()
            .set_directory(start)
            .add_filter(
                "归档",
                &["zip", "7z", "tar", "gz", "tgz", "bz2", "xz", "zst", "lz4"],
            )
            .add_filter("全部文件", &["*"])
            .pick_file();
        if let Some(path) = picked {
            self.open_path(path, None);
        }
    }

    /// 导入 zip 主题包：要求包内是一级目录 + theme.toml（以及 png/）。
    pub fn import_theme_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_directory(&self.dir)
            .add_filter("主题包", &["zip"])
            .pick_file()
        else {
            return;
        };
        let Some(themes_root) = crate::theme::loader::themes_root() else {
            self.status = "找不到 themes 目录".to_owned();
            return;
        };

        let name = file_name(&path);
        let handle = self.runner.submit(TaskOp::InstallTheme {
            archive: path,
            target: themes_root,
            limits: self.limits,
        });
        self.push_task(handle, format!("导入主题：{name}"));
        self.status = "正在后台导入主题包…".to_owned();
    }

    pub fn open_path(&mut self, path: PathBuf, password: Option<SecretString>) {
        let name = file_name(&path);
        self.status = format!("正在打开：{name}");
        let handle = self.runner.submit(TaskOp::Open { path, password });
        self.push_task(handle, format!("打开：{name}"));
    }

    /// 解压前预检：选目标目录、生成计划，交给确认弹窗。
    pub fn begin_extract_dialog(&mut self) {
        let Some(reader) = self.reader.clone() else {
            self.status = self.tr.t(TextKey::EmptyArchive);
            return;
        };
        let start = self
            .archive_path
            .as_ref()
            .and_then(|path| path.parent())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.dir.clone());
        let Some(target) = rfd::FileDialog::new().set_directory(start).pick_folder() else {
            return;
        };

        let ids: Vec<u32> = if self.selected.is_empty() {
            (0..self.entries.len() as u32).collect()
        } else {
            self.selected.iter().map(|&index| index as u32).collect()
        };
        let name = self.archive_name.clone();
        let handle = self.runner.submit(TaskOp::Plan {
            reader,
            ids,
            target,
            limits: self.limits,
        });
        self.push_task(handle, format!("预检：{name}"));
        self.status = "正在后台预检…".to_owned();
    }

    pub fn start_extract(&mut self) {
        let Some(pending) = self.pending_extract.take() else {
            return;
        };
        let reader = pending.reader.clone();
        let decisions = DecisionTable {
            default: pending.policy,
            per_entry: Default::default(),
        };
        let name = self.archive_name.clone();
        let target = pending.target.clone();
        let handle = self.runner.submit(TaskOp::Extract {
            reader,
            plan: Arc::new(pending.plan),
            decisions,
            limits: self.limits,
        });
        self.push_task(handle, format!("解压：{name}"));
        self.status = format!("开始解压到：{}", target.display());
    }

    pub fn open_pack_dialog(&mut self) {
        self.pack_state = PackState {
            output: self.dir.join("archive.zip").display().to_string(),
            ..Default::default()
        };
        self.show_pack = true;
    }

    pub fn pack_add_files(&mut self) {
        let Some(files) = rfd::FileDialog::new()
            .set_directory(&self.dir)
            .set_title("选择要打包的文件")
            .pick_files()
        else {
            return;
        };
        for path in files {
            if !self.pack_state.sources.contains(&path) {
                self.pack_state.sources.push(path);
            }
        }
        self.pack_state.error = None;
    }

    pub fn pack_add_folder(&mut self) {
        let Some(folder) = rfd::FileDialog::new()
            .set_directory(&self.dir)
            .set_title("选择要打包的文件夹")
            .pick_folder()
        else {
            return;
        };
        if !self.pack_state.sources.contains(&folder) {
            self.pack_state.sources.push(folder);
        }
        self.pack_state.error = None;
    }

    pub fn pack_browse_output(&mut self) {
        let current = PathBuf::from(self.pack_state.output.trim());
        let dir = current
            .parent()
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| self.dir.clone());
        let name = current
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "archive.zip".to_owned());
        let Some(mut output) = rfd::FileDialog::new()
            .set_directory(dir)
            .set_file_name(name)
            .add_filter("zip", &["zip"])
            .save_file()
        else {
            return;
        };
        if output.extension().is_none() {
            output.set_extension("zip");
        }
        self.pack_state.output = output.display().to_string();
        self.pack_state.confirm_overwrite = false;
        self.pack_state.error = None;
    }

    pub fn pack_start(&mut self) {
        if self.pack_state.sources.is_empty() {
            self.pack_state.error = Some("请先添加文件或文件夹".to_owned());
            return;
        }

        let text = self.pack_state.output.trim();
        if text.is_empty() {
            self.pack_state.error = Some("请填写输出文件路径".to_owned());
            return;
        }
        let mut output = PathBuf::from(text);
        if output.extension().is_none() {
            output.set_extension("zip");
        }
        let Some(kind) = kind_for_output(&output) else {
            self.pack_state.error = Some(
                "不支持的输出格式；可用 .zip / .7z / .tar / .tar.gz / .tar.bz2 / .tar.xz / .tar.zst / .tar.lz4"
                    .to_owned(),
            );
            return;
        };

        if output.is_dir() {
            self.pack_state.error = Some("输出路径是一个文件夹，请指定文件名".to_owned());
            return;
        }

        let append = self.pack_state.append;
        if append && kind != ArchiveKind::Zip {
            self.pack_state.error = Some("追加模式只支持 zip".to_owned());
            return;
        }

        if append {
            if !output.is_file() {
                self.pack_state.error = Some("追加模式要求目标 zip 已存在".to_owned());
                return;
            }
        } else if output.exists() && !self.pack_state.confirm_overwrite {
            self.pack_state.confirm_overwrite = true;
            self.pack_state.error = Some("目标已存在；再次点击「开始打包」将覆盖它".to_owned());
            return;
        }

        let encrypt = self.pack_state.encrypt;
        if encrypt && !matches!(kind, ArchiveKind::Zip | ArchiveKind::SevenZ) {
            self.pack_state.error = Some("只有 zip / 7z 支持加密写入".to_owned());
            return;
        }
        if append && encrypt {
            self.pack_state.error = Some("追加模式暂不支持设置新密码".to_owned());
            return;
        }
        if encrypt && self.pack_state.password.is_empty() {
            self.pack_state.error = Some("启用加密时必须填写密码".to_owned());
            return;
        }

        let method = if self.pack_state.store {
            Method::Store
        } else if kind == ArchiveKind::Zip {
            self.pack_state.zip_method.method()
        } else {
            Method::Deflate
        };
        let password = if encrypt {
            Some(SecretString::new(self.pack_state.password.clone()))
        } else {
            None
        };
        let name = file_name(&output);
        self.status = if append {
            format!("追加到：{}", output.display())
        } else {
            format!("打包到：{}", output.display())
        };
        let handle = self.runner.submit(TaskOp::Pack {
            output,
            inputs: self.pack_state.sources.clone(),
            kind,
            method,
            append,
            password,
        });
        self.push_task(
            handle,
            format!("{}：{name}", if append { "追加" } else { "打包" }),
        );
        self.show_pack = false;
        self.pack_state.password.clear();
        self.pack_state.error = None;
    }

    pub fn start_test(&mut self) {
        let Some(reader) = self.reader.clone() else {
            self.status = self.tr.t(TextKey::EmptyArchive);
            return;
        };
        let name = self.archive_name.clone();
        let handle = self.runner.submit(TaskOp::Test {
            reader,
            limits: self.limits,
        });
        self.push_task(handle, format!("测试：{name}"));
        self.status = format!("开始完整性测试：{name}");
    }

    fn push_task(&mut self, handle: spell_core::task::TaskHandle, name: String) {
        self.tasks.push(TaskView {
            id: handle.id,
            name,
            phase: TaskPhase::Pending,
            progress: 0.0,
            speed: String::new(),
            written: 0,
            total: 0,
            detail: None,
            cancel: handle.cancel,
            last_bytes: 0,
            last_sample: Instant::now(),
        });
    }

    pub fn has_active_tasks(&self) -> bool {
        self.tasks.iter().any(TaskView::active)
    }

    pub fn clear_finished_tasks(&mut self) {
        self.tasks.retain(TaskView::active);
    }

    fn task_mut(&mut self, id: TaskId) -> Option<&mut TaskView> {
        self.tasks.iter_mut().find(|task| task.id == id)
    }

    fn apply_event(&mut self, event: TaskEvent) {
        match event {
            TaskEvent::Opened {
                task,
                path,
                kind,
                entries,
                reader,
            } => {
                self.prepare_tab_for_open(&path);
                self.archive_name = file_name(&path);
                self.archive_kind = Some(kind);
                self.archive_path = Some(path.clone());
                self.reader = Some(reader);
                self.entries = entries.to_vec();
                self.search.clear();
                self.selected.clear();
                self.last_selected = None;
                self.status = format!(
                    "已打开 {}：{} 个条目",
                    self.archive_name,
                    self.entries.len()
                );
                let count = self.entries.len();
                if let Some(task) = self.task_mut(task) {
                    task.detail = Some(format!("{count} 个条目"));
                }
            }
            TaskEvent::NeedPassword { task, path } => {
                let name = file_name(&path);
                self.status = format!("{name} 需要密码");
                if let Some(task) = self.task_mut(task) {
                    task.phase = TaskPhase::Failed;
                    task.detail = Some("需要密码".to_owned());
                }
                self.pending_password = Some(PendingPassword {
                    path,
                    input: String::new(),
                    error: None,
                });
            }
            TaskEvent::PhaseChanged { task, phase } => {
                if let Some(view) = self.task_mut(task) {
                    if phase == TaskPhase::Done {
                        view.progress = 1.0;
                    }
                    view.phase = phase;
                }
            }
            TaskEvent::Started {
                task,
                total_bytes,
                total_entries,
            } => {
                if let Some(view) = self.task_mut(task) {
                    view.total = total_bytes;
                    view.written = 0;
                    view.phase = TaskPhase::Running;
                    if total_entries > 0 && view.detail.is_none() {
                        view.detail = Some(format!("{total_entries} 个条目"));
                    }
                }
            }
            TaskEvent::Progress { task, written, .. } => {
                if let Some(view) = self.task_mut(task) {
                    view.written = written;
                    view.progress = if view.total == 0 {
                        0.0
                    } else {
                        (written as f64 / view.total as f64).clamp(0.0, 1.0) as f32
                    };
                }
            }
            TaskEvent::Warning { task, text } => {
                if let Some(view) = self.task_mut(task) {
                    view.detail = Some(text);
                }
            }
            TaskEvent::NeedDecision { task, .. } => {
                if let Some(view) = self.task_mut(task) {
                    view.phase = TaskPhase::AwaitingDecision;
                }
            }
            TaskEvent::EntryFailed { task, error, .. } => {
                if let Some(view) = self.task_mut(task) {
                    view.detail = Some(error);
                }
            }
            TaskEvent::Finished { task, report } => {
                let summary = format!(
                    "成功 {} · 跳过 {} · 失败 {} · CRC {}",
                    report.succeeded.len(),
                    report.skipped.len(),
                    report.failed.len(),
                    report.crc_mismatch.len()
                );
                if let Some(view) = self.task_mut(task) {
                    view.phase = TaskPhase::Done;
                    view.progress = 1.0;
                    view.detail = Some(summary.clone());
                    if report.bytes_written > 0 {
                        view.total = report.bytes_written;
                        view.written = report.bytes_written;
                    }
                }
                self.status = format!("任务完成：{summary}");
            }
            TaskEvent::PackFinished {
                task,
                report,
                append,
            } => {
                let summary = format!(
                    "{} 个条目 · {} → {}",
                    report.entries,
                    crate::ui::format_bytes(report.bytes_in),
                    crate::ui::format_bytes(report.bytes_out)
                );
                if let Some(view) = self.task_mut(task) {
                    view.phase = TaskPhase::Done;
                    view.progress = 1.0;
                    view.detail = Some(summary.clone());
                }
                self.status = format!(
                    "{}：{summary}",
                    if append {
                        "追加完成"
                    } else {
                        "打包完成"
                    }
                );
            }
            TaskEvent::PlanReady {
                task,
                plan,
                target,
                reader,
            } => {
                self.status = format!(
                    "预检完成：{} 个条目，解压后 {}",
                    plan.items.len(),
                    crate::ui::format_bytes(plan.total_bytes)
                );
                if let Some(view) = self.task_mut(task) {
                    view.detail = Some(format!("{} 个条目", plan.items.len()));
                }
                self.pending_extract = Some(PendingExtract {
                    plan: (*plan).clone(),
                    target,
                    policy: OverwritePolicy::Skip,
                    reader,
                });
            }
            TaskEvent::ThemeInstalled {
                task,
                report,
                source,
            } => {
                let summary = format!(
                    "主题包：成功 {} · 跳过 {} · 失败 {}",
                    report.succeeded.len(),
                    report.skipped.len(),
                    report.failed.len()
                );
                if let Some(view) = self.task_mut(task) {
                    view.phase = TaskPhase::Done;
                    view.progress = 1.0;
                    view.detail = Some(summary.clone());
                }
                crate::logging::write(&format!("theme imported from {}", source.display()));
                self.status = if report.failed.is_empty() {
                    summary
                } else {
                    format!("{summary}（部分条目失败）")
                };
                // 让下一次主题热重载轮询立即生效。
                self.next_theme_check = Instant::now();
            }
            TaskEvent::Failed { task, error } => {
                crate::logging::error("task", &format!("{task} failed: {error}"));
                if let Some(view) = self.task_mut(task) {
                    view.phase = TaskPhase::Failed;
                    view.detail = Some(error.clone());
                }
                if error.contains("密码") {
                    if let Some(path) = self.archive_path.clone() {
                        self.pending_password = Some(PendingPassword {
                            path,
                            input: String::new(),
                            error: Some(error.clone()),
                        });
                    }
                }
                self.status = format!("任务失败：{error}");
            }
        }
    }

    pub fn tick(&mut self) {
        while let Some(event) = self.runner.try_recv() {
            self.apply_event(event);
        }

        let now = Instant::now();
        for task in &mut self.tasks {
            if !matches!(task.phase, TaskPhase::Running) {
                continue;
            }
            let elapsed = now.duration_since(task.last_sample).as_secs_f64();
            if elapsed >= 0.5 {
                let delta = task.written.saturating_sub(task.last_bytes) as f64;
                let bps = delta / elapsed;
                task.speed = if bps > 0.0 {
                    format!("{}/s", crate::ui::format_bytes(bps as u64))
                } else {
                    String::new()
                };
                task.last_bytes = task.written;
                task.last_sample = now;
            }
        }
    }
}

impl eframe::App for SpellApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if ctx.input(|input| input.viewport().close_requested())
            && self.close_to_tray
            && self.tray.is_some()
            && !self.quitting
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_window(&ctx);
        }

        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        });
        for path in &dropped {
            if path.is_dir() {
                self.navigate(path.clone());
            }
        }
        let files: Vec<&PathBuf> = dropped.iter().filter(|path| !path.is_dir()).collect();
        for file in &files {
            self.open_path((*file).clone(), None);
        }
        if files.len() > 1 {
            self.status = format!("正在打开 {} 个归档（各占一个标签页）", files.len());
        }

        self.poll_tray(&ctx);
        self.poll_handoff(&ctx);
        self.poll_theme(&ctx);
        self.tick();

        crate::ui::hint_banner(self, ui);
        crate::ui::top_bar(self, ui);
        crate::ui::tab_bar(self, ui);
        crate::ui::status_bar(self, ui);
        crate::ui::file_browser::show(self, ui);
        crate::ui::task_panel::show(self, ui);
        crate::ui::archive_view::show(self, ui);
        crate::ui::dialogs::show(self, &ctx);
        crate::ui::settings::show(self, ui);
        crate::ui::drag_overlay(self, &ctx);
        crate::ui::particles::draw(self, &ctx);

        self.maybe_save_settings();

        // 活动任务时 30fps；空闲时也保持低频唤醒，便于 250ms 轮询第二实例的 handoff。
        let delay = if self.has_active_tasks() || self.settings_dirty {
            Duration::from_millis(33)
        } else {
            Duration::from_millis(500)
        };
        ctx.request_repaint_after(delay);
    }
}

/// 自然排序：`file2` 排在 `file10` 前；数字段按数值比较。
pub(crate) fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let left: Vec<char> = a.chars().collect();
    let right: Vec<char> = b.chars().collect();
    let (mut i, mut j) = (0usize, 0usize);

    while i < left.len() && j < right.len() {
        if left[i].is_ascii_digit() && right[j].is_ascii_digit() {
            let l_start = i;
            while i < left.len() && left[i].is_ascii_digit() {
                i += 1;
            }
            let r_start = j;
            while j < right.len() && right[j].is_ascii_digit() {
                j += 1;
            }

            let l_num: Vec<char> = left[l_start..i]
                .iter()
                .copied()
                .skip_while(|c| *c == '0')
                .collect();
            let r_num: Vec<char> = right[r_start..j]
                .iter()
                .copied()
                .skip_while(|c| *c == '0')
                .collect();

            let cmp = l_num
                .len()
                .cmp(&r_num.len())
                .then_with(|| l_num.cmp(&r_num));
            if cmp != Ordering::Equal {
                return cmp;
            }
            let zero_cmp = (i - l_start).cmp(&(j - r_start));
            if zero_cmp != Ordering::Equal {
                return zero_cmp;
            }
        } else {
            let lc = left[i].to_ascii_lowercase();
            let rc = right[j].to_ascii_lowercase();
            if lc != rc {
                return lc.cmp(&rc);
            }
            i += 1;
            j += 1;
        }
    }

    left.len().cmp(&right.len()).then_with(|| a.cmp(b))
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub(crate) fn kind_for_output(path: &std::path::Path) -> Option<ArchiveKind> {
    let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    for (suffix, kind) in [
        (".tar.gz", ArchiveKind::TarGz),
        (".tgz", ArchiveKind::TarGz),
        (".tar.bz2", ArchiveKind::TarBz2),
        (".tbz2", ArchiveKind::TarBz2),
        (".tbz", ArchiveKind::TarBz2),
        (".tar.xz", ArchiveKind::TarXz),
        (".txz", ArchiveKind::TarXz),
        (".tar.zst", ArchiveKind::TarZst),
        (".tzst", ArchiveKind::TarZst),
        (".tar.lz4", ArchiveKind::TarLz4),
        (".tlz4", ArchiveKind::TarLz4),
        (".zip", ArchiveKind::Zip),
        (".7z", ArchiveKind::SevenZ),
        (".tar", ArchiveKind::Tar),
    ] {
        if name.ends_with(suffix) {
            return Some(kind);
        }
    }
    None
}

fn cjk_font_bytes() -> Option<Vec<u8>> {
    for candidate in ["C:/Windows/Fonts/simhei.ttf", "C:/Windows/Fonts/Deng.ttf"] {
        if let Ok(bytes) = std::fs::read(candidate) {
            return Some(bytes);
        }
    }

    // fontdb 只在系统字体缺失时初始化（Windows 正常情况下走不到这里）。
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    for family in [
        "Microsoft YaHei",
        "Noto Sans CJK SC",
        "Source Han Sans SC",
        "WenQuanYi Micro Hei",
        "Droid Sans Fallback",
    ] {
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            ..Default::default()
        };
        if let Some(id) = db.query(&query) {
            let bytes = db.with_face_data(id, |data, _| data.to_vec());
            if let Some(bytes) = bytes {
                return Some(bytes);
            }
        }
    }
    None
}

fn install_cjk_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 先试 Windows 系统字体；找不到再用 fontdb 查常见 CJK 字体（Linux 构建兜底）。
    if let Some(bytes) = cjk_font_bytes() {
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

/// 搜索语法：空串全匹配；`re:` 前缀按正则（不区分大小写）；含 `*`/`?` 按通配；否则子串。
pub(crate) fn search_matches(query: &str, text: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let text = text.to_lowercase();
    if let Some(pattern) = query.strip_prefix("re:") {
        return regex::RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .map(|regex| regex.is_match(&text))
            .unwrap_or_else(|_| text.contains(pattern));
    }
    if query.contains('*') || query.contains('?') {
        wildcard_match(query, &text)
    } else {
        text.contains(query)
    }
}

/// 简单的 `*` / `?` 通配匹配；调用方负责大小写归一化。
pub(crate) fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut resume = 0usize;

    while ti < text.len() {
        if pi < pattern.len() && (pattern[pi] == '?' || pattern[pi] == text[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < pattern.len() && pattern[pi] == '*' {
            star = Some(pi);
            resume = ti;
            pi += 1;
        } else if let Some(star_pos) = star {
            pi = star_pos + 1;
            resume += 1;
            ti = resume;
        } else {
            return false;
        }
    }

    while pi < pattern.len() && pattern[pi] == '*' {
        pi += 1;
    }
    pi == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::{natural_cmp, search_matches, wildcard_match};
    use std::cmp::Ordering;

    #[test]
    fn natural_sort_orders_numbers() {
        assert_eq!(natural_cmp("file2", "file10"), Ordering::Less);
        assert_eq!(natural_cmp("A2", "a10"), Ordering::Less);
        assert_eq!(natural_cmp("文件2", "文件10"), Ordering::Less);
        assert_eq!(natural_cmp("a1", "a1"), Ordering::Equal);
    }

    #[test]
    fn wildcard_matches_globs() {
        assert!(wildcard_match("*.txt", "a.txt"));
        assert!(wildcard_match("*.txt", "dir/a.txt"));
        assert!(wildcard_match("a?c", "abc"));
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("a*b*c", "aXXbYYc"));
        assert!(!wildcard_match("*.txt", "a.zip"));
        assert!(!wildcard_match("a?c", "abbc"));
        assert!(!wildcard_match("b*", "abc"));
    }

    #[test]
    fn supports_regex_search() {
        assert!(search_matches("re:^a.*\\.txt$", "A1.TXT"));
        assert!(!search_matches("re:^a.*\\.txt$", "b.txt"));
        // 非法正则退回子串匹配。
        assert!(search_matches("re:[", "["));
        assert!(search_matches("", "anything"));
    }
}
