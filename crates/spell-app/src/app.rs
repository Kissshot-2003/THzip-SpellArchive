use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crate::i18n::{Locale, TextKey, Translator};
use crate::startup::{StartupIntent, StartupRequest, add_output, extract_target};
use crate::theme::ThemeStore;
use crate::tray::{Tray, TrayAction};
use eframe::egui;
use spell_core::SevenZOptions;
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

pub struct PackState {
    pub sources: Vec<PathBuf>,
    pub output: String,
    pub store: bool,
    pub zip_method: ZipPackMethod,
    pub sevenz_level: u32,
    pub sevenz_dict_mib: u32,
    pub append: bool,
    pub encrypt: bool,
    pub password: String,
    pub error: Option<String>,
    pub confirm_overwrite: bool,
}

impl Default for PackState {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            output: String::new(),
            store: false,
            zip_method: ZipPackMethod::default(),
            sevenz_level: 6,
            sevenz_dict_mib: 0,
            append: false,
            encrypt: false,
            password: String::new(),
            error: None,
            confirm_overwrite: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelocateMode {
    Rename,
    Move,
    BatchRename,
    RegexRename,
}

pub struct RelocateDialog {
    pub mode: RelocateMode,
    pub input: String,
    pub regex_pattern: String,
    pub regex_replacement: String,
    pub regex_case_insensitive: bool,
    pub error: Option<String>,
}

/// 批量改名的输入：模板文本或正则替换。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RelocateInput {
    Text(String),
    Regex {
        pattern: String,
        replacement: String,
        case_insensitive: bool,
    },
}

impl RelocateInput {
    fn text(&self) -> &str {
        match self {
            RelocateInput::Text(text) => text,
            RelocateInput::Regex { .. } => "",
        }
    }
}

#[derive(Debug)]
pub(crate) enum RelocateError {
    NoSelection,
    OneOnly,
    Invalid,
    InvalidRegex(String),
    Duplicate(String),
    Nested,
    NoChange,
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
    pub focus_search: bool,
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
    /// 打开成功后要自动执行的启动动作（解压到当前目录等）。
    startup: Option<StartupRequest>,
    /// 归档重写任务：完成后需要重新打开该路径。
    pending_rewrite: Option<(TaskId, PathBuf)>,

    pub runner: TaskRunner,
    pub tasks: Vec<TaskView>,
    pub pending_password: Option<PendingPassword>,
    pub pending_extract: Option<PendingExtract>,
    pub relocate: Option<RelocateDialog>,
    pub show_pack: bool,
    pub pack_state: PackState,

    pub status: String,
    pub limits: spell_core::Limits,

    settings_dirty: bool,
    last_settings_save: Instant,
    next_handoff_check: Instant,
    theme_signature: Option<(u64, SystemTime)>,
    next_theme_check: Instant,
    window_size: Option<[f32; 2]>,
    tray: Option<Tray>,
    quitting: bool,
}

impl SpellApp {
    pub fn new(cc: &eframe::CreationContext<'_>, startup: Option<StartupRequest>) -> Self {
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
        let status_ready = tr.t(TextKey::StatusReady);

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
            focus_search: false,
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
            startup: None,
            pending_rewrite: None,
            runner: TaskRunner::new(2),
            tasks: Vec::new(),
            pending_password: None,
            pending_extract: None,
            relocate: None,
            show_pack: false,
            pack_state: PackState::default(),
            status: status_ready,
            limits: spell_core::Limits::default(),
            settings_dirty: false,
            last_settings_save: Instant::now(),
            next_handoff_check: Instant::now(),
            theme_signature,
            next_theme_check: Instant::now() + Duration::from_secs(1),
            window_size: prefs.window_size,
            tray,
            quitting: false,
        };

        app.themes.apply(&cc.egui_ctx);
        app.refresh_dir();
        // exe 移动或升级后，右键菜单里的旧路径自动修正。
        if let Err(err) = crate::shell_menu::refresh_if_installed(locale) {
            crate::logging::warn_fields(
                "shell_menu",
                "refresh failed",
                &[("error", &err.to_string())],
            );
        }
        app.apply_startup(startup);
        app
    }

    /// 处理命令行 / 右键动词 / 第二实例转交的启动动作。
    fn apply_startup(&mut self, request: Option<StartupRequest>) {
        let Some(request) = request else {
            return;
        };
        if request.paths.is_empty() {
            return;
        }

        if request.intent == StartupIntent::Add {
            let output = add_output(&request.paths[0], request.paths.len());
            self.open_pack_with(request.paths, output);
            return;
        }

        let first = request.paths[0].clone();
        if request.intent != StartupIntent::Open {
            self.startup = Some(StartupRequest {
                intent: request.intent,
                paths: vec![first.clone()],
            });
        }
        if first.is_dir() {
            self.navigate(first);
            return;
        }
        self.open_path(first, None);

        if request.intent == StartupIntent::Open {
            for path in request.paths.iter().skip(1) {
                if path.is_dir() {
                    self.navigate(path.clone());
                } else {
                    self.open_path(path.clone(), None);
                }
            }
        }
    }

    pub fn set_theme(&mut self, index: usize, ctx: &egui::Context) {
        if self.themes.set(index) {
            self.themes.ensure_loaded(ctx);
            self.tr.set_theme(self.themes.current());
            self.themes.apply(ctx);
            self.status = self.tr.tf(
                TextKey::ThemeSwitched,
                &[("name", &self.themes.current().name)],
            );
            self.touch_settings();
        }
    }

    /// 语言切换后重建托盘菜单，避免托盘一直显示旧语言。
    pub fn rebuild_tray(&mut self) {
        if let Some(tray) = Tray::new(&self.tr) {
            self.tray = Some(tray);
        }
    }

    /// 同步界面语言：翻译器、状态栏、托盘菜单与已安装的右键菜单。
    pub fn sync_locale(&mut self) {
        if self.locale == self.tr.locale {
            return;
        }
        self.tr.locale = self.locale;
        self.status = self.tr.t(TextKey::StatusReady);
        self.rebuild_tray();
        if crate::shell_menu::is_installed() {
            if let Ok(exe) = std::env::current_exe() {
                let _ = crate::shell_menu::install(&exe, self.locale);
            }
        }
        self.touch_settings();
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
        let name = self.tab_title(index);
        self.status = self.tr.tf(TextKey::TabSwitched, &[("name", &name)]);
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
        let remaining = self.tabs.len().to_string();
        self.status = self.tr.tf(TextKey::TabClosed, &[("count", &remaining)]);
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
        let request = StartupRequest::from_handoff(&text);
        self.apply_startup(Some(request));
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
        self.status = self.tr.t(TextKey::ThemeReloaded);
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
            window_size: self.window_size,
        };
        if let Err(err) = prefs.save() {
            crate::logging::warn_fields("prefs", "save failed", &[("error", &err.to_string())]);
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
        let dir = self.dir.display().to_string();
        self.status = self.tr.tf(TextKey::DirectoryStatus, &[("path", &dir)]);
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
        let archives_filter = self.tr.t(TextKey::FilterArchives);
        let all_filter = self.tr.t(TextKey::FilterAll);
        let picked = rfd::FileDialog::new()
            .set_directory(start)
            .add_filter(
                archives_filter,
                &["zip", "7z", "tar", "gz", "tgz", "bz2", "xz", "zst", "lz4"],
            )
            .add_filter(all_filter, &["*"])
            .pick_file();
        if let Some(path) = picked {
            self.open_path(path, None);
        }
    }

    /// 导入 zip 主题包：要求包内是一级目录 + theme.toml（以及 png/）。
    pub fn import_theme_dialog(&mut self) {
        let filter = self.tr.t(TextKey::FilterThemePack);
        let Some(path) = rfd::FileDialog::new()
            .set_directory(&self.dir)
            .add_filter(filter, &["zip"])
            .pick_file()
        else {
            return;
        };
        let Some(themes_root) = crate::theme::loader::themes_root() else {
            self.status = self.tr.t(TextKey::ThemeDirMissing);
            return;
        };

        let name = file_name(&path);
        let handle = self.runner.submit(TaskOp::InstallTheme {
            archive: path,
            target: themes_root,
            limits: self.limits,
        });
        let task_name = self.tr.tf(TextKey::TaskImportTheme, &[("name", &name)]);
        self.push_task(handle, task_name);
        self.status = self.tr.t(TextKey::ImportingTheme);
    }

    pub fn open_path(&mut self, path: PathBuf, password: Option<SecretString>) {
        let name = file_name(&path);
        self.status = self.tr.tf(TextKey::OpeningFile, &[("name", &name)]);
        let handle = self.runner.submit(TaskOp::Open { path, password });
        let task_name = self.tr.tf(TextKey::TaskOpen, &[("name", &name)]);
        self.push_task(handle, task_name);
    }

    /// 解压前预检：选目标目录、生成计划，交给确认弹窗。
    pub fn begin_extract_dialog(&mut self) {
        let start = self
            .archive_path
            .as_ref()
            .and_then(|path| path.parent())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.dir.clone());
        let Some(target) = rfd::FileDialog::new().set_directory(start).pick_folder() else {
            return;
        };
        self.begin_extract_to(target);
    }

    /// 对当前归档提交后台预检；目标目录由调用方决定（对话框 / 右键动词）。
    pub fn begin_extract_to(&mut self, target: PathBuf) {
        let Some(reader) = self.reader.clone() else {
            self.status = self.tr.t(TextKey::EmptyArchive);
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
        let task_name = self.tr.tf(TextKey::TaskPrecheck, &[("name", &name)]);
        self.push_task(handle, task_name);
        self.status = self.tr.t(TextKey::Prechecking);
    }

    /// 删除归档内选中的条目：确认后关闭当前 reader，后台重写并重新打开。
    pub fn delete_selected_entries(&mut self) {
        let Some(path) = self.archive_path.clone() else {
            self.status = self.tr.t(TextKey::EmptyArchive).to_owned();
            return;
        };
        let Some(kind) = self.archive_kind else {
            return;
        };
        if self.selected.is_empty() {
            self.status = self.tr.t(TextKey::SelectBeforeDelete);
            return;
        }
        let count = self.selected.len();
        let title = self.tr.t(TextKey::DeleteTitle);
        let text = self
            .tr
            .tf(TextKey::DeleteConfirm, &[("count", &count.to_string())]);
        let result = rfd::MessageDialog::new()
            .set_title(title)
            .set_description(text)
            .set_level(rfd::MessageLevel::Warning)
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if result != rfd::MessageDialogResult::Yes {
            return;
        }

        let remove: Vec<u32> = self.selected.iter().map(|&index| index as u32).collect();
        // 释放文件句柄，Windows 才能在任务最后由 ReplaceFileW 原子替换。
        self.reader = None;
        let name = self.archive_name.clone();
        let handle = self.runner.submit(TaskOp::Rewrite {
            path: path.clone(),
            kind,
            remove,
            rename: Vec::new(),
            limits: self.limits,
        });
        self.pending_rewrite = Some((handle.id, path));
        let task_name = self.tr.tf(TextKey::TaskDelete, &[("name", &name)]);
        self.push_task(handle, task_name);
        let count_text = count.to_string();
        self.status = self.tr.tf(TextKey::Rewriting, &[("count", &count_text)]);
    }

    /// 打开重命名 / 移动对话框；真正写盘在后台重写任务里完成。
    pub fn begin_relocate(&mut self, mode: RelocateMode) {
        if self.selected.is_empty() {
            self.status = self.tr.t(TextKey::RelocateNoSelection);
            return;
        }
        if mode == RelocateMode::Rename && self.selected.len() != 1 {
            self.status = self.tr.t(TextKey::RelocateOneOnly);
            return;
        }
        let input = match mode {
            RelocateMode::Rename => self
                .selected
                .iter()
                .next()
                .and_then(|&index| self.entries.get(index))
                .map(|entry| entry.display.to_string())
                .unwrap_or_default(),
            RelocateMode::Move => String::new(),
            RelocateMode::BatchRename => "{name}_{n}".to_owned(),
            RelocateMode::RegexRename => String::new(),
        };
        self.relocate = Some(RelocateDialog {
            mode,
            input,
            regex_pattern: String::new(),
            regex_replacement: String::new(),
            regex_case_insensitive: false,
            error: None,
        });
    }

    pub fn apply_relocate(&mut self) {
        let Some(dialog) = self.relocate.as_ref() else {
            return;
        };
        let mode = dialog.mode;
        let input = match mode {
            RelocateMode::RegexRename => RelocateInput::Regex {
                pattern: dialog.regex_pattern.clone(),
                replacement: dialog.regex_replacement.clone(),
                case_insensitive: dialog.regex_case_insensitive,
            },
            _ => RelocateInput::Text(dialog.input.clone()),
        };
        let Some(path) = self.archive_path.clone() else {
            self.relocate = None;
            self.status = self.tr.t(TextKey::EmptyArchive);
            return;
        };
        let Some(kind) = self.archive_kind else {
            return;
        };

        match build_relocate(&self.entries, &self.selected, mode, &input) {
            Ok(rename) => {
                let count = rename.len();
                let name = self.archive_name.clone();
                // 释放文件句柄，Windows 才能在任务最后由 ReplaceFileW 原子替换。
                self.reader = None;
                let handle = self.runner.submit(TaskOp::Rewrite {
                    path: path.clone(),
                    kind,
                    remove: Vec::new(),
                    rename,
                    limits: self.limits,
                });
                self.pending_rewrite = Some((handle.id, path));
                self.relocate = None;
                let task_name = self.tr.tf(TextKey::TaskRelocate, &[("name", &name)]);
                self.push_task(handle, task_name);
                let count_text = count.to_string();
                self.status = self.tr.tf(TextKey::Relocating, &[("count", &count_text)]);
            }
            Err(err) => {
                let text = self.relocate_error_text(&err);
                if let Some(dialog) = self.relocate.as_mut() {
                    dialog.error = Some(text);
                }
            }
        }
    }

    /// 完成摘要 + 前 5 条失败原因，作为任务行悬停详情。
    fn summarize_failures(
        &self,
        summary: &str,
        failed: &[(spell_core::EntryId, spell_core::error::FailReason)],
    ) -> String {
        if failed.is_empty() {
            return summary.to_owned();
        }
        let mut detail = format!("{summary}\n{}", self.tr.t(TextKey::FailedEntries));
        for (id, reason) in failed.iter().take(5) {
            let text = crate::i18n::errors::reason(self.locale, reason);
            detail.push('\n');
            detail.push_str(
                &self
                    .tr
                    .tf(TextKey::FailedEntryLine, &[("id", &id.to_string()), ("error", &text)]),
            );
        }
        if failed.len() > 5 {
            detail.push_str("\n…");
        }
        detail
    }

    fn relocate_error_text(&self, err: &RelocateError) -> String {        match err {
            RelocateError::NoSelection => self.tr.t(TextKey::RelocateNoSelection),
            RelocateError::OneOnly => self.tr.t(TextKey::RelocateOneOnly),
            RelocateError::Invalid => self.tr.t(TextKey::RelocateInvalid),
            RelocateError::InvalidRegex(error) => {
                self.tr.tf(TextKey::RelocateInvalidRegex, &[("error", error)])
            }
            RelocateError::Duplicate(name) => {
                self.tr.tf(TextKey::RelocateDuplicate, &[("name", name)])
            }
            RelocateError::Nested => self.tr.t(TextKey::RelocateNested),
            RelocateError::NoChange => self.tr.t(TextKey::RelocateNoChange),
        }
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
        let task_name = self.tr.tf(TextKey::TaskExtract, &[("name", &name)]);
        self.push_task(handle, task_name);
        let target_text = target.display().to_string();
        self.status = self
            .tr
            .tf(TextKey::ExtractStarted, &[("path", &target_text)]);
    }

    pub fn open_pack_dialog(&mut self) {
        let output = self.dir.join("archive.zip");
        self.open_pack_with(Vec::new(), output);
    }

    /// 用指定输入和输出打开打包对话框（右键“添加到压缩包”）。
    pub fn open_pack_with(&mut self, sources: Vec<PathBuf>, output: PathBuf) {
        self.pack_state = PackState {
            sources,
            output: output.display().to_string(),
            ..Default::default()
        };
        self.show_pack = true;
        if !self.pack_state.sources.is_empty() {
            let count = self.pack_state.sources.len().to_string();
            self.status = self.tr.tf(
                TextKey::PackReady,
                &[("count", &count), ("path", &self.pack_state.output)],
            );
        }
    }

    pub fn pack_add_files(&mut self) {
        let title = self.tr.t(TextKey::ChoosePackFiles);
        let Some(files) = rfd::FileDialog::new()
            .set_directory(&self.dir)
            .set_title(title)
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
        let title = self.tr.t(TextKey::ChoosePackFolder);
        let Some(folder) = rfd::FileDialog::new()
            .set_directory(&self.dir)
            .set_title(title)
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
            let message = self.tr.t(TextKey::PackErrNoSources);
            self.pack_state.error = Some(message);
            return;
        }

        let text = self.pack_state.output.trim();
        if text.is_empty() {
            let message = self.tr.t(TextKey::PackErrNoOutput);
            self.pack_state.error = Some(message);
            return;
        }
        let mut output = PathBuf::from(text);
        if output.extension().is_none() {
            output.set_extension("zip");
        }
        let Some(kind) = kind_for_output(&output) else {
            let message = self.tr.t(TextKey::PackErrUnsupported);
            self.pack_state.error = Some(message);
            return;
        };

        if output.is_dir() {
            let message = self.tr.t(TextKey::PackErrOutputIsDir);
            self.pack_state.error = Some(message);
            return;
        }

        let append = self.pack_state.append;
        if append && kind != ArchiveKind::Zip {
            let message = self.tr.t(TextKey::PackErrAppendZipOnly);
            self.pack_state.error = Some(message);
            return;
        }

        if append {
            if !output.is_file() {
                let message = self.tr.t(TextKey::PackErrAppendMissing);
                self.pack_state.error = Some(message);
                return;
            }
        } else if output.exists() && !self.pack_state.confirm_overwrite {
            self.pack_state.confirm_overwrite = true;
            let message = self.tr.t(TextKey::PackErrConfirmOverwrite);
            self.pack_state.error = Some(message);
            return;
        }

        let encrypt = self.pack_state.encrypt;
        if encrypt && !matches!(kind, ArchiveKind::Zip | ArchiveKind::SevenZ) {
            let message = self.tr.t(TextKey::PackErrEncryptUnsupported);
            self.pack_state.error = Some(message);
            return;
        }
        if append && encrypt {
            let message = self.tr.t(TextKey::PackErrAppendPassword);
            self.pack_state.error = Some(message);
            return;
        }
        if encrypt && self.pack_state.password.is_empty() {
            let message = self.tr.t(TextKey::PackErrPasswordRequired);
            self.pack_state.error = Some(message);
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
        let output_text = output.display().to_string();
        self.status = if append {
            self.tr.tf(TextKey::AppendingTo, &[("path", &output_text)])
        } else {
            self.tr.tf(TextKey::PackingTo, &[("path", &output_text)])
        };
        let sevenz = SevenZOptions {
            level: self.pack_state.sevenz_level.min(9),
            dict_size: self.pack_state.sevenz_dict_mib.saturating_mul(1024 * 1024),
        };
        let handle = self.runner.submit(TaskOp::Pack {
            output,
            inputs: self.pack_state.sources.clone(),
            kind,
            method,
            append,
            password,
            sevenz,
        });
        let task_key = if append {
            TextKey::TaskAppend
        } else {
            TextKey::TaskPack
        };
        let task_name = self.tr.tf(task_key, &[("name", &name)]);
        self.push_task(handle, task_name);
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
        let task_name = self.tr.tf(TextKey::TaskTest, &[("name", &name)]);
        self.push_task(handle, task_name);
        self.status = self.tr.tf(TextKey::TestStarted, &[("name", &name)]);
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
                let count = self.entries.len();
                let count_text = count.to_string();
                let name = self.archive_name.clone();
                self.status = self.tr.tf(
                    TextKey::ArchiveOpened,
                    &[("name", &name), ("count", &count_text)],
                );
                let detail = self.tr.tf(TextKey::EntryCount, &[("count", &count_text)]);
                if let Some(task) = self.task_mut(task) {
                    task.detail = Some(detail);
                }

                if let Some(request) = self.startup.take() {
                    match request.intent {
                        StartupIntent::ExtractHere => {
                            self.begin_extract_to(extract_target(&path, false));
                        }
                        StartupIntent::ExtractTo => {
                            self.begin_extract_to(extract_target(&path, true));
                        }
                        StartupIntent::Open | StartupIntent::Add => {}
                    }
                }
            }
            TaskEvent::NeedPassword { task, path } => {
                let name = file_name(&path);
                self.status = self.tr.tf(TextKey::PasswordNeededFor, &[("name", &name)]);
                let detail = self.tr.t(TextKey::PasswordNeeded);
                if let Some(task) = self.task_mut(task) {
                    task.phase = TaskPhase::Failed;
                    task.detail = Some(detail);
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
                let detail = (total_entries > 0).then(|| {
                    self.tr.tf(
                        TextKey::EntryCount,
                        &[("count", &total_entries.to_string())],
                    )
                });
                if let Some(view) = self.task_mut(task) {
                    view.total = total_bytes;
                    view.written = 0;
                    view.phase = TaskPhase::Running;
                    if view.detail.is_none() {
                        view.detail = detail;
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
                let text = crate::i18n::errors::reason(self.locale, &error);
                if let Some(view) = self.task_mut(task) {
                    view.detail = Some(text);
                }
            }
            TaskEvent::Finished { task, report } => {
                let summary = self.tr.tf(
                    TextKey::ExtractSummary,
                    &[
                        ("ok", &report.succeeded.len().to_string()),
                        ("skipped", &report.skipped.len().to_string()),
                        ("failed", &report.failed.len().to_string()),
                        ("crc", &report.crc_mismatch.len().to_string()),
                    ],
                );
                let detail = self.summarize_failures(&summary, &report.failed);
                if let Some(view) = self.task_mut(task) {
                    view.phase = TaskPhase::Done;
                    view.progress = 1.0;
                    view.detail = Some(detail);
                    if report.bytes_written > 0 {
                        view.total = report.bytes_written;
                        view.written = report.bytes_written;
                    }
                }
                self.status = self.tr.tf(TextKey::TaskFinished, &[("summary", &summary)]);
            }
            TaskEvent::PackFinished {
                task,
                report,
                append,
            } => {
                let summary = self.tr.tf(
                    TextKey::PackSummary,
                    &[
                        ("entries", &report.entries.to_string()),
                        ("bytes_in", &crate::ui::format_bytes(report.bytes_in)),
                        ("bytes_out", &crate::ui::format_bytes(report.bytes_out)),
                    ],
                );
                if let Some(view) = self.task_mut(task) {
                    view.phase = TaskPhase::Done;
                    view.progress = 1.0;
                    view.detail = Some(summary.clone());
                }
                let label = if append {
                    self.tr.t(TextKey::AppendDone)
                } else {
                    self.tr.t(TextKey::PackDone)
                };
                self.status = self.tr.tf(
                    TextKey::SummaryLine,
                    &[("label", &label), ("summary", &summary)],
                );
            }
            TaskEvent::Rewritten { task, path, report } => {
                let summary = self.tr.tf(
                    TextKey::RewriteSummary,
                    &[
                        ("entries", &report.entries.to_string()),
                        ("bytes", &report.bytes_out.to_string()),
                    ],
                );
                if let Some(view) = self.task_mut(task) {
                    view.phase = TaskPhase::Done;
                    view.progress = 1.0;
                    view.detail = Some(summary.clone());
                }
                crate::logging::info_fields(
                    "archive",
                    "rewritten",
                    &[
                        ("path", &path.display().to_string()),
                        ("entries", &report.entries.to_string()),
                        ("bytes_out", &report.bytes_out.to_string()),
                    ],
                );
                self.status = summary;
                if self
                    .pending_rewrite
                    .as_ref()
                    .is_some_and(|(id, _)| *id == task)
                {
                    self.pending_rewrite = None;
                    self.open_path(path, None);
                }
            }
            TaskEvent::PlanReady {
                task,
                plan,
                target,
                reader,
            } => {
                let count = plan.items.len();
                let size = crate::ui::format_bytes(plan.total_bytes);
                self.status = self.tr.tf(
                    TextKey::PrecheckSummary,
                    &[("count", &count.to_string()), ("size", &size)],
                );
                let detail = self
                    .tr
                    .tf(TextKey::EntryCount, &[("count", &count.to_string())]);
                if let Some(view) = self.task_mut(task) {
                    view.detail = Some(detail);
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
                let summary = self.tr.tf(
                    TextKey::ThemePackSummary,
                    &[
                        ("ok", &report.succeeded.len().to_string()),
                        ("skipped", &report.skipped.len().to_string()),
                        ("failed", &report.failed.len().to_string()),
                    ],
                );
                let detail = self.summarize_failures(&summary, &report.failed);
                if let Some(view) = self.task_mut(task) {
                    view.phase = TaskPhase::Done;
                    view.progress = 1.0;
                    view.detail = Some(detail);
                }
                crate::logging::info_fields(
                    "theme",
                    "imported",
                    &[
                        ("source", &source.display().to_string()),
                        ("ok", &report.succeeded.len().to_string()),
                        ("skipped", &report.skipped.len().to_string()),
                        ("failed", &report.failed.len().to_string()),
                    ],
                );
                self.status = if report.failed.is_empty() {
                    summary
                } else {
                    self.tr
                        .tf(TextKey::PartialFailure, &[("summary", &summary)])
                };
                // 让下一次主题热重载轮询立即生效。
                self.next_theme_check = Instant::now();
            }
            TaskEvent::Failed { task, error } => {
                let task_text = task.to_string();
                let error_text = error.to_string();
                crate::logging::error_fields(
                    "task",
                    "failed",
                    &[("task", &task_text), ("error", &error_text)],
                );
                let text = crate::i18n::errors::reason(self.locale, &error);
                if let Some(view) = self.task_mut(task) {
                    view.phase = TaskPhase::Failed;
                    view.detail = Some(text.clone());
                }
                if error.is_password_related() {
                    if let Some(path) = self.archive_path.clone() {
                        self.pending_password = Some(PendingPassword {
                            path,
                            input: String::new(),
                            error: Some(text.clone()),
                        });
                    }
                }
                if let Some(path) = self
                    .pending_rewrite
                    .as_ref()
                    .filter(|(id, _)| *id == task)
                    .map(|(_, path)| path.clone())
                {
                    self.pending_rewrite = None;
                    self.open_path(path, None);
                }
                self.status = self.tr.tf(TextKey::TaskFailed, &[("error", &text)]);
            }
        }
    }

    /// 全局快捷键。模态框打开时不响应；文本输入框聚焦时跳过无修饰键快捷键。
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.show_settings
            || self.show_pack
            || self.pending_password.is_some()
            || self.pending_extract.is_some()
            || self.relocate.is_some()
        {
            return;
        }
        let typing = ctx.egui_wants_keyboard_input();
        let (mut open, mut extract, mut pack, mut test, mut search) = (false, false, false, false, false);
        let (mut select_all, mut delete, mut rename, mut refresh) = (false, false, false, false);
        ctx.input_mut(|input| {
            open = input.consume_key(egui::Modifiers::COMMAND, egui::Key::O);
            extract = input.consume_key(egui::Modifiers::COMMAND, egui::Key::E);
            pack = input.consume_key(egui::Modifiers::COMMAND, egui::Key::P);
            test = input.consume_key(egui::Modifiers::COMMAND, egui::Key::T);
            search = input.consume_key(egui::Modifiers::COMMAND, egui::Key::F);
            if !typing {
                select_all = input.consume_key(egui::Modifiers::COMMAND, egui::Key::A);
                delete = input.consume_key(egui::Modifiers::NONE, egui::Key::Delete);
                rename = input.consume_key(egui::Modifiers::NONE, egui::Key::F2);
                refresh = input.consume_key(egui::Modifiers::NONE, egui::Key::F5);
            }
        });

        if open {
            self.open_archive_dialog();
            return;
        }
        if extract && self.reader.is_some() {
            self.begin_extract_dialog();
        }
        if pack {
            self.open_pack_dialog();
        }
        if test && self.reader.is_some() {
            self.start_test();
        }
        if search {
            self.focus_search = true;
        }
        if select_all {
            self.select_all_filtered();
        }
        if delete && !self.selected.is_empty() {
            self.delete_selected_entries();
        }
        if rename && self.selected.len() == 1 {
            self.begin_relocate(RelocateMode::Rename);
        }
        if refresh {
            self.refresh_dir();
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

        // 记住主窗口大小；只记录逻辑尺寸，位置不持久化，避免换显示器后跑出屏幕。
        if let Some(rect) = ctx.input(|input| input.viewport().inner_rect) {
            let size = [rect.width().round(), rect.height().round()];
            if size[0] >= 640.0 && size[1] >= 480.0 {
                let changed = self.window_size.is_none_or(|old| {
                    (old[0] - size[0]).abs() >= 1.0 || (old[1] - size[1]).abs() >= 1.0
                });
                if changed {
                    self.window_size = Some(size);
                    self.settings_dirty = true;
                }
            }
        }

        self.handle_shortcuts(&ctx);

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
            self.status = self
                .tr
                .tf(TextKey::OpeningMany, &[("count", &files.len().to_string())]);
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

/// 拆分文件名与扩展名；隐藏文件（`.gitignore`）不把点当扩展名分隔符。
fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(position) if position > 0 => (&name[..position], &name[position + 1..]),
        _ => (name, ""),
    }
}

/// 展开批量改名模板：`{name}` 原名、`{n}` 序号、`{ext}` 扩展名。
///
/// 文件默认保留原扩展名；模板里写了 `{ext}` 则由模板决定扩展名。
fn expand_batch_name(
    pattern: &str,
    root: &str,
    is_dir: bool,
    number: usize,
) -> Result<String, RelocateError> {
    if pattern.trim().is_empty() {
        return Err(RelocateError::Invalid);
    }
    let name = root.rsplit('/').next().unwrap_or(root);
    let (stem, ext) = if is_dir { (name, "") } else { split_name(name) };
    let mut result = pattern
        .replace("{name}", stem)
        .replace("{n}", &number.to_string())
        .replace("{ext}", ext);
    if !is_dir && !ext.is_empty() && !pattern.contains("{ext}") {
        result.push('.');
        result.push_str(ext);
    }
    if result.trim().is_empty() {
        return Err(RelocateError::Invalid);
    }
    Ok(result)
}

/// 根据选中条目生成改名映射：目录会自动带上全部子条目。
///
/// `Rename` 只处理一个条目，输入是完整新路径；
/// `Move` 把全部选中条目移动到输入的归档内文件夹，空输入表示根目录。
pub(crate) fn build_relocate(
    entries: &[ArchiveEntry],
    selected: &BTreeSet<usize>,
    mode: RelocateMode,
    input: &RelocateInput,
) -> Result<Vec<(u32, String)>, RelocateError> {
    if selected.is_empty() {
        return Err(RelocateError::NoSelection);
    }
    if mode == RelocateMode::Rename && selected.len() != 1 {
        return Err(RelocateError::OneOnly);
    }

    let mut roots: Vec<(usize, String, bool)> = selected
        .iter()
        .filter_map(|&index| {
            entries.get(index).map(|entry| {
                (
                    index,
                    entry.display.trim_end_matches('/').to_owned(),
                    entry.kind.is_dir(),
                )
            })
        })
        .collect();
    if roots.is_empty() {
        return Err(RelocateError::NoSelection);
    }
    // 浅层目录优先，避免选中目录后其子条目被重复处理。
    roots.sort_by_key(|(_, path, _)| path.matches('/').count());
    let mut chosen: Vec<(usize, String, bool)> = Vec::new();
    for (index, path, is_dir) in roots {
        let nested = chosen
            .iter()
            .any(|(_, root, _)| path == *root || path.starts_with(&format!("{root}/")));
        if !nested {
            chosen.push((index, path, is_dir));
        }
    }

    let raw_input = input.text().trim();
    let regex = match input {
        RelocateInput::Regex {
            pattern,
            replacement,
            case_insensitive,
        } => {
            if pattern.is_empty() {
                return Err(RelocateError::Invalid);
            }
            let compiled = regex::RegexBuilder::new(pattern)
                .case_insensitive(*case_insensitive)
                .build()
                .map_err(|err| RelocateError::InvalidRegex(err.to_string()))?;
            Some((compiled, replacement.clone()))
        }
        RelocateInput::Text(_) => None,
    };

    let mut rename: HashMap<usize, String> = HashMap::new();
    for (order, (_, root, is_dir)) in chosen.iter().enumerate() {
        let final_root = match mode {
            RelocateMode::Rename => {
                spell_core::normalize_entry_name(raw_input).map_err(|_| RelocateError::Invalid)?
            }
            RelocateMode::Move => {
                let dst = if raw_input.is_empty() || raw_input == "/" || raw_input == "." {
                    String::new()
                } else {
                    spell_core::normalize_entry_name(raw_input)
                        .map_err(|_| RelocateError::Invalid)?
                };
                let base = root.rsplit('/').next().unwrap_or(root);
                if dst.is_empty() {
                    base.to_owned()
                } else {
                    format!("{dst}/{base}")
                }
            }
            RelocateMode::BatchRename => {
                let base = expand_batch_name(raw_input, root, *is_dir, order + 1)?;
                match root.rsplit_once('/') {
                    Some((parent, _)) => format!("{parent}/{base}"),
                    None => base,
                }
            }
            RelocateMode::RegexRename => {
                let Some((regex, replacement)) = regex.as_ref() else {
                    return Err(RelocateError::Invalid);
                };
                let base = root.rsplit('/').next().unwrap_or(root);
                let renamed = regex.replace_all(base, replacement.as_str());
                if renamed.trim().is_empty() {
                    return Err(RelocateError::Invalid);
                }
                match root.rsplit_once('/') {
                    Some((parent, _)) => format!("{parent}/{renamed}"),
                    None => renamed.into_owned(),
                }
            }
        };
        let final_root =
            spell_core::normalize_entry_name(&final_root).map_err(|_| RelocateError::Invalid)?;

        if *is_dir && final_root.starts_with(&format!("{root}/")) {
            return Err(RelocateError::Nested);
        }

        let prefix = format!("{root}/");
        for (index, entry) in entries.iter().enumerate() {
            let path = entry.display.trim_end_matches('/');
            if path != root && !path.starts_with(&prefix) {
                continue;
            }
            let suffix = &path[root.len()..];
            let final_name = spell_core::normalize_entry_name(&format!("{final_root}{suffix}"))
                .map_err(|_| RelocateError::Invalid)?;
            if final_name != path {
                rename.insert(index, final_name);
            }
        }
    }

    if rename.is_empty() {
        return Err(RelocateError::NoChange);
    }

    let mut seen: HashMap<String, usize> = HashMap::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let final_name = rename
            .get(&index)
            .cloned()
            .unwrap_or_else(|| entry.display.trim_end_matches('/').to_owned());
        if let Some(previous) = seen.insert(final_name.to_lowercase(), index) {
            if rename.contains_key(&index) || rename.contains_key(&previous) {
                return Err(RelocateError::Duplicate(final_name));
            }
        }
    }

    let mut result: Vec<(u32, String)> = rename
        .into_iter()
        .map(|(index, name)| (index as u32, name))
        .collect();
    result.sort_by_key(|(id, _)| *id);
    Ok(result)
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

const EMBEDDED_CJK: &[u8] = include_bytes!("../assets/NotoSansSC-subset.ttf");

fn install_cjk_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 先挂内嵌子集，保证没有中文字体的系统也能正常显示；
    // 子集未覆盖的生僻字再回退到系统字体。
    fonts.font_data.insert(
        "cjk-embedded".to_owned(),
        egui::FontData::from_static(EMBEDDED_CJK).into(),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("cjk-embedded".to_owned());
    }

    // 系统字体兜底：先试 Windows 字体，再用 fontdb 查常见 CJK 字体（Linux 构建兜底）。
    if let Some(bytes) = cjk_font_bytes() {
        fonts.font_data.insert(
            "cjk-system".to_owned(),
            egui::FontData::from_owned(bytes).into(),
        );
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("cjk-system".to_owned());
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
    use super::{
        RelocateError, RelocateInput, RelocateMode, build_relocate, natural_cmp, search_matches,
        wildcard_match,
    };
    use spell_core::types::{ArchiveEntry, EntryKind, Method};
    use std::cmp::Ordering;
    use std::collections::BTreeSet;

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

    fn entry(id: u32, name: &str, is_dir: bool) -> ArchiveEntry {
        ArchiveEntry {
            id,
            raw_name: name.as_bytes().to_vec().into(),
            display: name.into(),
            kind: if is_dir {
                EntryKind::Dir
            } else {
                EntryKind::File
            },
            size: 0,
            packed: None,
            crc32: None,
            mtime: None,
            encrypted: false,
            method: Method::Store,
        }
    }

    #[test]
    fn relocate_moves_selected_directory_tree() {
        let entries = [
            entry(0, "a.txt", false),
            entry(1, "c", true),
            entry(2, "c/d.txt", false),
        ];
        let selected = BTreeSet::from([1, 2]);
        let map = build_relocate(
            &entries,
            &selected,
            RelocateMode::Move,
            &RelocateInput::Text("moved".to_owned()),
        )
        .unwrap();
        assert_eq!(
            map,
            vec![(1, "moved/c".to_owned()), (2, "moved/c/d.txt".to_owned())]
        );

        let map = build_relocate(
            &entries,
            &BTreeSet::from([0]),
            RelocateMode::Move,
            &RelocateInput::Text(String::new()),
        )
        .unwrap_err();
        assert!(matches!(map, RelocateError::NoChange));
    }

    #[test]
    fn relocate_renames_single_entry() {
        let entries = [entry(0, "a.txt", false)];
        let map = build_relocate(
            &entries,
            &BTreeSet::from([0]),
            RelocateMode::Rename,
            &RelocateInput::Text("docs/a2.txt".to_owned()),
        )
        .unwrap();
        assert_eq!(map, vec![(0, "docs/a2.txt".to_owned())]);
    }

    #[test]
    fn relocate_batch_rename_expands_template() {
        let entries = [
            entry(0, "photo.jpg", false),
            entry(1, "notes.txt", false),
            entry(2, "dir", true),
            entry(3, "dir/a.txt", false),
            entry(4, "other.txt", false),
        ];

        let map = build_relocate(
            &entries,
            &BTreeSet::from([0, 1]),
            RelocateMode::BatchRename,
            &RelocateInput::Text("{name}_{n}".to_owned()),
        )
        .unwrap();
        assert_eq!(
            map,
            vec![(0, "photo_1.jpg".to_owned()), (1, "notes_2.txt".to_owned())]
        );

        let map = build_relocate(
            &entries,
            &BTreeSet::from([0, 1]),
            RelocateMode::BatchRename,
            &RelocateInput::Text("{n}.{ext}".to_owned()),
        )
        .unwrap();
        assert_eq!(map, vec![(0, "1.jpg".to_owned()), (1, "2.txt".to_owned())]);

        // 选中目录时，子条目跟着目录一起换前缀。
        let map = build_relocate(
            &entries,
            &BTreeSet::from([2]),
            RelocateMode::BatchRename,
            &RelocateInput::Text("folder_{n}".to_owned()),
        )
        .unwrap();
        assert_eq!(
            map,
            vec![(2, "folder_1".to_owned()), (3, "folder_1/a.txt".to_owned())]
        );

        let err = build_relocate(
            &entries,
            &BTreeSet::from([1, 4]),
            RelocateMode::BatchRename,
            &RelocateInput::Text("same".to_owned()),
        )
        .unwrap_err();
        assert!(matches!(err, RelocateError::Duplicate(_)));
    }

    #[test]
    fn relocate_rejects_duplicates_and_self_nesting() {
        let entries = [entry(0, "a.txt", false), entry(1, "b.txt", false)];
        let err = build_relocate(
            &entries,
            &BTreeSet::from([0]),
            RelocateMode::Rename,
            &RelocateInput::Text("b.txt".to_owned()),
        )
        .unwrap_err();
        assert!(matches!(err, RelocateError::Duplicate(_)));

        let entries = [entry(0, "a.txt", false), entry(1, "x/a.txt", false)];
        let err = build_relocate(
            &entries,
            &BTreeSet::from([1]),
            RelocateMode::Move,
            &RelocateInput::Text(String::new()),
        )
        .unwrap_err();
        assert!(matches!(err, RelocateError::Duplicate(_)));

        let entries = [entry(0, "dir", true), entry(1, "dir/x.txt", false)];
        let err = build_relocate(
            &entries,
            &BTreeSet::from([0]),
            RelocateMode::Rename,
            &RelocateInput::Text("dir/sub".to_owned()),
        )
        .unwrap_err();
        assert!(matches!(err, RelocateError::Nested));

        let err = build_relocate(
            &entries,
            &BTreeSet::from([0, 1]),
            RelocateMode::Rename,
            &RelocateInput::Text("x".to_owned()),
        )
        .unwrap_err();
        assert!(matches!(err, RelocateError::OneOnly));
    }

    #[test]
    fn regex_rename_replaces_capture_groups() {
        let entries = [
            entry(0, "ep01.txt", false),
            entry(1, "dir/ep02.txt", false),
        ];
        let input = RelocateInput::Regex {
            pattern: r"ep(\d+)".to_owned(),
            replacement: "第$1话".to_owned(),
            case_insensitive: false,
        };
        let map = build_relocate(
            &entries,
            &BTreeSet::from([0, 1]),
            RelocateMode::RegexRename,
            &input,
        )
        .unwrap();
        assert_eq!(
            map,
            vec![(0, "第01话.txt".to_owned()), (1, "dir/第02话.txt".to_owned())]
        );
    }

    #[test]
    fn regex_rename_honours_case_insensitive_and_rejects_invalid() {
        let entries = [entry(0, "EP01.txt", false)];
        let input = RelocateInput::Regex {
            pattern: "ep".to_owned(),
            replacement: "episode".to_owned(),
            case_insensitive: true,
        };
        let map = build_relocate(
            &entries,
            &BTreeSet::from([0]),
            RelocateMode::RegexRename,
            &input,
        )
        .unwrap();
        assert_eq!(map, vec![(0, "episode01.txt".to_owned())]);

        let invalid = RelocateInput::Regex {
            pattern: "(".to_owned(),
            replacement: "x".to_owned(),
            case_insensitive: false,
        };
        let err = build_relocate(
            &entries,
            &BTreeSet::from([0]),
            RelocateMode::RegexRename,
            &invalid,
        )
        .unwrap_err();
        assert!(matches!(err, RelocateError::InvalidRegex(_)));
    }

    #[test]
    fn regex_rename_moves_directory_children() {
        let entries = [
            entry(0, "Season 1", true),
            entry(1, "Season 1/a.txt", false),
        ];
        let input = RelocateInput::Regex {
            pattern: r"Season (\d+)".to_owned(),
            replacement: "第$1季".to_owned(),
            case_insensitive: true,
        };
        let map = build_relocate(
            &entries,
            &BTreeSet::from([0, 1]),
            RelocateMode::RegexRename,
            &input,
        )
        .unwrap();
        assert_eq!(
            map,
            vec![(0, "第1季".to_owned()), (1, "第1季/a.txt".to_owned())]
        );
    }
}
