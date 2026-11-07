use std::collections::BTreeMap;

use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    ZhCn,
    EnUs,
}

impl Locale {
    pub fn label(self) -> &'static str {
        match self {
            Locale::ZhCn => "简体中文",
            Locale::EnUs => "English",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Locale::ZhCn => Locale::EnUs,
            Locale::EnUs => Locale::ZhCn,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Locale::ZhCn => "zh-CN",
            Locale::EnUs => "en-US",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "zh-CN" | "zh" => Some(Locale::ZhCn),
            "en-US" | "en" => Some(Locale::EnUs),
            _ => None,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextKey {
    AppTitle,
    Open,
    NewArchive,
    Extract,
    Pack,
    TestArchive,
    Settings,
    LocalFiles,
    ArchiveEntries,
    TaskQueue,
    TaskRunning,
    TaskDone,
    SearchHint,
    Theme,
    Language,
    StandardTerms,
    FirstRunHint,
    Dismiss,
    EmptyArchive,
    DemoBadge,
    GoUp,
    StatusReady,
    About,
    Cancel,
    StartExtract,
    Precheck,
    ExtractTo,
    SkipExisting,
    OverwriteExisting,
    RenameExisting,
    AutoNumberExisting,
    ClearFinished,
    PasswordPrompt,
    TotalSize,
    LargestFile,
    FreeSpace,
    Conflicts,
    Warnings,
    Opening,
    Testing,
    TrayShow,
    TrayHide,
    TrayQuit,
}

impl TextKey {
    /// 与 theme.toml `[text_map]` 的 key 一一对应。
    pub fn key(self) -> &'static str {
        match self {
            TextKey::AppTitle => "app_title",
            TextKey::Open => "open",
            TextKey::NewArchive => "new_archive",
            TextKey::Extract => "extract",
            TextKey::Pack => "pack",
            TextKey::TestArchive => "test",
            TextKey::Settings => "settings",
            TextKey::LocalFiles => "local_files",
            TextKey::ArchiveEntries => "archive_entries",
            TextKey::TaskQueue => "task_queue",
            TextKey::TaskRunning => "task_running",
            TextKey::TaskDone => "task_done",
            TextKey::SearchHint => "search_hint",
            TextKey::Theme => "theme",
            TextKey::Language => "language",
            TextKey::StandardTerms => "standard_terms",
            TextKey::FirstRunHint => "first_run_hint",
            TextKey::Dismiss => "dismiss",
            TextKey::EmptyArchive => "empty_archive",
            TextKey::DemoBadge => "demo_badge",
            TextKey::GoUp => "go_up",
            TextKey::StatusReady => "status_ready",
            TextKey::About => "about",
            TextKey::Cancel => "cancel",
            TextKey::StartExtract => "start_extract",
            TextKey::Precheck => "precheck",
            TextKey::ExtractTo => "extract_to",
            TextKey::SkipExisting => "skip_existing",
            TextKey::OverwriteExisting => "overwrite_existing",
            TextKey::RenameExisting => "rename_existing",
            TextKey::AutoNumberExisting => "auto_number_existing",
            TextKey::ClearFinished => "clear_finished",
            TextKey::PasswordPrompt => "password_prompt",
            TextKey::TotalSize => "total_size",
            TextKey::LargestFile => "largest_file",
            TextKey::FreeSpace => "free_space",
            TextKey::Conflicts => "conflicts",
            TextKey::Warnings => "warnings",
            TextKey::Opening => "opening",
            TextKey::Testing => "testing",
            TextKey::TrayShow => "tray_show",
            TextKey::TrayHide => "tray_hide",
            TextKey::TrayQuit => "tray_quit",
        }
    }
}

pub fn builtin(locale: Locale, key: TextKey) -> &'static str {
    match locale {
        Locale::ZhCn => match key {
            TextKey::AppTitle => "SpellArchive",
            TextKey::Open => "打开归档",
            TextKey::NewArchive => "新建归档",
            TextKey::Extract => "解压",
            TextKey::Pack => "添加到压缩包",
            TextKey::TestArchive => "测试完整性",
            TextKey::Settings => "设置",
            TextKey::LocalFiles => "本地文件",
            TextKey::ArchiveEntries => "包内条目",
            TextKey::TaskQueue => "任务队列",
            TextKey::TaskRunning => "进行中…",
            TextKey::TaskDone => "已完成",
            TextKey::SearchHint => "搜索文件名…",
            TextKey::Theme => "主题",
            TextKey::Language => "语言",
            TextKey::StandardTerms => "显示标准术语（关闭后按钮显示符卡文案）",
            TextKey::FirstRunHint => {
                "界面的符卡文案只是皮肤：功能与标准归档软件一致，鼠标悬停可看到标准术语。"
            }
            TextKey::Dismiss => "知道了",
            TextKey::EmptyArchive => "尚未打开归档",
            TextKey::DemoBadge => "演示数据",
            TextKey::GoUp => "上一级",
            TextKey::StatusReady => "就绪",
            TextKey::About => "关于",
            TextKey::Cancel => "取消",
            TextKey::StartExtract => "开始解压",
            TextKey::Precheck => "解压预检",
            TextKey::ExtractTo => "解压到",
            TextKey::SkipExisting => "跳过已有文件",
            TextKey::OverwriteExisting => "覆盖已有文件",
            TextKey::RenameExisting => "保留已有文件，新文件改名（a~1.txt）",
            TextKey::AutoNumberExisting => "保留已有文件，新文件自动编号（a (1).txt）",
            TextKey::ClearFinished => "清除已完成",
            TextKey::PasswordPrompt => "此归档需要密码",
            TextKey::TotalSize => "解压后大小",
            TextKey::LargestFile => "最大单文件",
            TextKey::FreeSpace => "目标盘剩余",
            TextKey::Conflicts => "冲突",
            TextKey::Warnings => "警告",
            TextKey::Opening => "打开中…",
            TextKey::Testing => "校验中…",
            TextKey::TrayShow => "显示主窗口",
            TextKey::TrayHide => "隐藏主窗口",
            TextKey::TrayQuit => "退出",
        },
        Locale::EnUs => match key {
            TextKey::AppTitle => "SpellArchive",
            TextKey::Open => "Open archive",
            TextKey::NewArchive => "New archive",
            TextKey::Extract => "Extract",
            TextKey::Pack => "Add to archive",
            TextKey::TestArchive => "Test integrity",
            TextKey::Settings => "Settings",
            TextKey::LocalFiles => "Local files",
            TextKey::ArchiveEntries => "Archive entries",
            TextKey::TaskQueue => "Tasks",
            TextKey::TaskRunning => "Running…",
            TextKey::TaskDone => "Done",
            TextKey::SearchHint => "Search file name…",
            TextKey::Theme => "Theme",
            TextKey::Language => "Language",
            TextKey::StandardTerms => "Show standard terms",
            TextKey::FirstRunHint => {
                "Themed labels are cosmetic; every action matches a standard archiver."
            }
            TextKey::Dismiss => "Got it",
            TextKey::EmptyArchive => "No archive opened",
            TextKey::DemoBadge => "Demo data",
            TextKey::GoUp => "Up",
            TextKey::StatusReady => "Ready",
            TextKey::About => "About",
            TextKey::Cancel => "Cancel",
            TextKey::StartExtract => "Start extraction",
            TextKey::Precheck => "Extraction precheck",
            TextKey::ExtractTo => "Extract to",
            TextKey::SkipExisting => "Skip existing files",
            TextKey::OverwriteExisting => "Overwrite existing files",
            TextKey::RenameExisting => "Keep existing, rename new (a~1.txt)",
            TextKey::AutoNumberExisting => "Keep existing, auto-number new (a (1).txt)",
            TextKey::ClearFinished => "Clear finished",
            TextKey::PasswordPrompt => "This archive requires a password",
            TextKey::TotalSize => "Unpacked size",
            TextKey::LargestFile => "Largest file",
            TextKey::FreeSpace => "Free space",
            TextKey::Conflicts => "Conflicts",
            TextKey::Warnings => "Warnings",
            TextKey::Opening => "Opening…",
            TextKey::Testing => "Verifying…",
            TextKey::TrayShow => "Show window",
            TextKey::TrayHide => "Hide window",
            TextKey::TrayQuit => "Quit",
        },
    }
}

pub struct Translator {
    pub locale: Locale,
    /// true = 直接用标准术语（可发现性开关）。
    pub prefer_standard: bool,
    overrides: BTreeMap<String, String>,
}

impl Default for Translator {
    fn default() -> Self {
        Self {
            locale: Locale::ZhCn,
            prefer_standard: false,
            overrides: BTreeMap::new(),
        }
    }
}

impl Translator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_theme(&mut self, theme: &Theme) {
        self.overrides = theme.text_map.clone();
    }

    /// 三层回退：主题覆盖 → 内置语言包。
    pub fn t(&self, key: TextKey) -> String {
        if !self.prefer_standard {
            if let Some(value) = self.overrides.get(key.key()) {
                return value.clone();
            }
        }
        builtin(self.locale, key).to_owned()
    }

    pub fn standard(&self, key: TextKey) -> &'static str {
        builtin(self.locale, key)
    }
}
