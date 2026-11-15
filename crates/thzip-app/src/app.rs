use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crate::i18n::{Locale, TextKey, Translator};
use crate::startup::{StartupIntent, StartupRequest, add_output, extract_target};
use crate::theme::ThemeStore;
use crate::tray::{Tray, TrayAction};
use eframe::egui;
use thzip_core::SevenZOptions;
use thzip_core::plan::{DecisionTable, OverwritePolicy};
use thzip_core::secret::SecretString;
use thzip_core::task::{SharedReader, TaskEvent, TaskId, TaskOp, TaskPhase, TaskRunner};
use thzip_core::types::{ArchiveEntry, ArchiveKind, Method};

mod state;
pub use state::*;
mod filter;
mod pack;
mod relocate;
pub(crate) use filter::*;
pub(crate) use pack::*;
#[cfg(test)]
pub(crate) use relocate::*;
#[cfg(test)]
mod tests;

pub struct ThzipApp {
    pub tr: Translator,
    pub themes: ThemeStore,
    pub locale: Locale,
    pub show_settings: bool,
    pub show_first_run_hint: bool,
    pub close_to_tray: bool,
    pub pack_templates: Vec<crate::prefs::PackTemplate>,

    pub dir: PathBuf,
    pub dir_items: Vec<DirItem>,

    pub archive_name: String,
    pub archive_kind: Option<ArchiveKind>,
    pub archive_path: Option<PathBuf>,
    pub reader: Option<SharedReader>,
    pub entries: Vec<ArchiveEntry>,
    pub comment: Option<String>,
    pub preview: Option<PreviewView>,
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
    pub limits: thzip_core::Limits,

    settings_dirty: bool,
    last_settings_save: Instant,
    next_handoff_check: Instant,
    theme_signature: Option<(u64, SystemTime)>,
    next_theme_check: Instant,
    window_size: Option<[f32; 2]>,
    tray: Option<Tray>,
    quitting: bool,
}

impl ThzipApp {
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
            pack_templates: prefs.pack_templates.clone(),
            dir,
            dir_items: Vec::new(),
            archive_name: String::new(),
            archive_kind: None,
            archive_path: None,
            reader: None,
            entries: Vec::new(),
            comment: None,
            preview: None,
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
            limits: thzip_core::Limits::default(),
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
        if crate::shell_menu::is_installed()
            && let Ok(exe) = std::env::current_exe()
        {
            let _ = crate::shell_menu::install(&exe, self.locale);
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
            comment: self.comment.take(),
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
        self.comment = tab.comment;
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
            pack_templates: self.pack_templates.clone(),
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

    /// 把当前主题目录打包成 zip，输出位置由用户选择。
    pub fn export_theme_dialog(&mut self) {
        let Some(themes_root) = crate::theme::loader::themes_root() else {
            self.status = self.tr.t(TextKey::ThemeDirMissing);
            return;
        };
        let id = self.themes.current_id().to_owned();
        let theme_dir = themes_root.join(&id);
        if !theme_dir.is_dir() {
            self.status = self.tr.t(TextKey::ThemeExportUnavailable);
            return;
        }
        let filter = self.tr.t(TextKey::FilterThemePack);
        let Some(output) = rfd::FileDialog::new()
            .set_directory(&self.dir)
            .set_file_name(format!("{id}-theme.zip"))
            .add_filter(filter, &["zip"])
            .save_file()
        else {
            return;
        };
        let name = file_name(&output);
        let handle = self.runner.submit(TaskOp::Pack {
            output,
            inputs: vec![theme_dir],
            kind: ArchiveKind::Zip,
            method: Method::Deflate,
            append: false,
            password: None,
            sevenz: SevenZOptions::default(),
            comment: None,
            volume_size: None,
            limits: self.limits,
        });
        let task_name = self.tr.tf(TextKey::TaskExportTheme, &[("name", &name)]);
        self.push_task(handle, task_name);
        self.status = self.tr.t(TextKey::ExportingTheme);
    }

    /// 双击条目时读取该条目到内存，弹窗预览文本或常见图片。
    pub fn preview_entry(&mut self, index: usize) {
        let Some(reader) = self.reader.clone() else {
            return;
        };
        let Some(entry) = self.entries.get(index).cloned() else {
            return;
        };
        if entry.kind.is_dir() {
            return;
        }
        let id = entry.id;
        self.runner.submit(TaskOp::Preview {
            reader,
            id,
            limits: self.limits,
            max_bytes: 512 * 1024,
        });
        self.preview = Some(PreviewView {
            id,
            name: entry.display.to_string(),
            data: Arc::<[u8]>::from(Vec::new()),
            truncated: false,
            failed: None,
            loading: true,
            text: None,
            image: None,
            texture: None,
        });
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

    fn push_task(&mut self, handle: thzip_core::task::TaskHandle, name: String) {
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
                let comment = reader.lock().ok().and_then(|reader| reader.comment());
                self.archive_name = file_name(&path);
                self.archive_kind = Some(kind);
                self.archive_path = Some(path.clone());
                self.reader = Some(reader);
                self.entries = entries.to_vec();
                self.comment = comment;
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
            TaskEvent::PreviewReady {
                id,
                data,
                truncated,
                failed,
                ..
            } => {
                let Some(preview) = self.preview.as_mut() else {
                    return;
                };
                if preview.id != id {
                    return;
                }
                preview.loading = false;
                preview.truncated = truncated;
                preview.failed = failed;
                preview.data = data;
                let lower = preview.name.to_ascii_lowercase();
                if is_preview_image(&lower) {
                    if let Ok(image) = image::load_from_memory(&preview.data) {
                        let rgba = image.to_rgba8();
                        preview.image = Some((
                            [rgba.width() as usize, rgba.height() as usize],
                            rgba.into_raw(),
                        ));
                    }
                } else {
                    let limit = 64 * 1024;
                    let slice = &preview.data[..preview.data.len().min(limit)];
                    if let Ok(text) = std::str::from_utf8(slice) {
                        if preview.data.len() > limit {
                            preview.truncated = true;
                        }
                        preview.text = Some(text.to_owned());
                    }
                }
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
                if error.is_password_related()
                    && let Some(path) = self.archive_path.clone()
                {
                    self.pending_password = Some(PendingPassword {
                        path,
                        input: String::new(),
                        error: Some(text.clone()),
                    });
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
        let typing = ctx.text_edit_focused();
        let (mut open, mut extract, mut pack, mut test, mut search) =
            (false, false, false, false, false);
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

impl eframe::App for ThzipApp {
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

fn is_preview_image(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".bmp"]
        .iter()
        .any(|extension| name.ends_with(extension))
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
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
