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
