use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crate::i18n::{Locale, TextKey, Translator};
use crate::theme::ThemeStore;
use crate::tray::{Tray, TrayAction};
use eframe::egui;
use spell_core::plan::{DecisionTable, ExtractPlan, OverwritePolicy};
use spell_core::safety::ExtractDirSink;
use spell_core::secret::SecretString;
use spell_core::task::{
    CancelToken, SharedReader, TaskEvent, TaskId, TaskOp, TaskPhase, TaskRunner,
};
use spell_core::types::{ArchiveEntry, ArchiveKind, Method};

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
}

#[derive(Default)]
pub struct PackState {
    pub sources: Vec<PathBuf>,
    pub output: String,
    pub store: bool,
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
    pub selected: Option<usize>,

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
        let mut themes = ThemeStore::load(&cc.egui_ctx);
        if let Some(theme) = prefs.theme.as_deref() {
            themes.select_by_id(theme);
        }
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
            selected: None,
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
            tray: Tray::new(),
            quitting: false,
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
            self.touch_settings();
        }
    }

    pub fn touch_settings(&mut self) {
        self.settings_dirty = true;
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
        let text = text.trim();
        self.show_window(ctx);
        if text.is_empty() {
            return;
        }
        let target = PathBuf::from(text);
        if target.is_dir() {
            self.navigate(target);
        } else {
            self.open_path(target, None);
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

        let current_id = self.themes.current_id().to_owned();
        let mut themes = ThemeStore::load(ctx);
        themes.select_by_id(&current_id);
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
            schema: 1,
            theme: Some(self.themes.current_id().to_owned()),
            locale: self.locale.code().to_owned(),
            prefer_standard: self.tr.prefer_standard,
            show_hint: self.show_first_run_hint,
            close_to_tray: self.close_to_tray,
            last_dir: Some(self.dir.display().to_string()),
        };
        if let Err(err) = prefs.save() {
            crate::logging::write(&format!("prefs save failed: {err}"));
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
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                needle.is_empty() || entry.display.to_lowercase().contains(&needle)
            })
            .map(|(index, _)| index)
            .collect()
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

        let mut reader = match spell_core::backend::open(&path, None) {
            Ok(reader) => reader,
            Err(err) => {
                self.status = format!("主题包打开失败: {err}");
                return;
            }
        };
        let has_manifest = reader.entries().iter().any(|entry| {
            let name = entry.display.trim_start_matches("./").replace('\\', "/");
            let parts: Vec<&str> = name.split('/').filter(|part| !part.is_empty()).collect();
            parts.len() == 2 && parts[1] == "theme.toml"
        });
        if !has_manifest {
            self.status = "主题包格式不对：需要 <主题目录>/theme.toml".to_owned();
            return;
        }

        let ids: Vec<u32> = (0..reader.entries().len() as u32).collect();
        let limits = self.limits;
        let plan = match reader.plan(&ids, &themes_root, &limits) {
            Ok(plan) => plan,
            Err(err) => {
                self.status = format!("主题包预检失败: {err}");
                return;
            }
        };
        let decisions = DecisionTable {
            default: OverwritePolicy::Skip,
            ..Default::default()
        };
        let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
        match reader.extract(&plan, &decisions, &CancelToken::new(), &mut sink) {
            Ok(report) if report.failed.is_empty() => {
                self.status = format!("主题包已导入：{} 个条目", report.succeeded.len());
                crate::logging::write(&format!("theme imported from {}", path.display()));
                self.next_theme_check = Instant::now();
            }
            Ok(report) => {
                self.status = format!("主题包导入部分失败：失败 {} 条", report.failed.len());
            }
            Err(err) => {
                self.status = format!("主题包导入失败: {err}");
            }
        }
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

        let ids: Vec<u32> = match self.selected {
            Some(index) => vec![index as u32],
            None => (0..self.entries.len() as u32).collect(),
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
        let (Some(pending), Some(reader)) = (self.pending_extract.take(), self.reader.clone())
        else {
            return;
        };
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
                self.archive_name = file_name(&path);
                self.archive_kind = Some(kind);
                self.archive_path = Some(path.clone());
                self.reader = Some(reader);
                self.entries = entries.to_vec();
                self.search.clear();
                self.selected = None;
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
            TaskEvent::PlanReady { task, plan, target } => {
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
                });
            }
            TaskEvent::Failed { task, error } => {
                crate::logging::write(&format!("task {task} failed: {error}"));
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
        for path in dropped {
            if path.is_dir() {
                self.navigate(path);
            } else {
                self.open_path(path, None);
                break;
            }
        }

        self.poll_tray(&ctx);
        self.poll_handoff(&ctx);
        self.poll_theme(&ctx);
        self.tick();

        crate::ui::hint_banner(self, ui);
        crate::ui::top_bar(self, ui);
        crate::ui::status_bar(self, ui);
        crate::ui::file_browser::show(self, ui);
        crate::ui::task_panel::show(self, ui);
        crate::ui::archive_view::show(self, ui);
        crate::ui::dialogs::show(self, &ctx);
        crate::ui::settings::show(self, ui);
        crate::ui::drag_overlay(self, &ctx);

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
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
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

#[cfg(test)]
mod tests {
    use super::natural_cmp;
    use std::cmp::Ordering;

    #[test]
    fn natural_sort_orders_numbers() {
        assert_eq!(natural_cmp("file2", "file10"), Ordering::Less);
        assert_eq!(natural_cmp("A2", "a10"), Ordering::Less);
        assert_eq!(natural_cmp("文件2", "文件10"), Ordering::Less);
        assert_eq!(natural_cmp("a1", "a1"), Ordering::Equal);
    }
}
