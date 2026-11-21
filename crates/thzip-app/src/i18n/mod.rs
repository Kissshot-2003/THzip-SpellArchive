use std::collections::BTreeMap;

use thzip_core::limits::LimitProfile;

use crate::limit_fields::LimitField;
use crate::theme::Theme;

pub mod errors;
pub mod warnings;

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
    Pause,
    Resume,
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
    NewTab,
    CloseTab,
    SelectedOne,
    SelectedMany,
    StatusSuffix,
    DropBrowseFolder,
    DropOpenArchive,
    OneFile,
    ItemCount,
    DeleteSelected,
    RenameSelected,
    MoveSelected,
    BatchRenameSelected,
    RegexRenameSelected,
    RelocateRenameTitle,
    RelocateMoveTitle,
    RelocateBatchTitle,
    RelocateRegexTitle,
    RelocatePathLabel,
    RelocateFolderLabel,
    RelocateBatchLabel,
    RelocateRegexPatternLabel,
    RelocateRegexReplaceLabel,
    RelocateRegexHint,
    RelocateCaseInsensitive,
    RelocateApply,
    RelocateNoSelection,
    RelocateOneOnly,
    RelocateInvalid,
    RelocateInvalidRegex,
    RelocateDuplicate,
    RelocateNested,
    RelocateNoChange,
    TaskRelocate,
    Relocating,
    ClearSelection,
    SelectAll,
    EmptyHint,
    ColName,
    ColSize,
    ColPacked,
    ColRatio,
    ColMethod,
    ColTime,
    ColCrc,
    CrcMismatch,
    ColFlags,
    FlagLocked,
    FlagSpoofed,
    SpoofedHint,
    SortHint,
    TaskCount,
    PhasePending,
    PhasePlanning,
    PhaseAwaitingDecision,
    PhaseRunning,
    PhasePaused,
    PhaseFailed,
    CancellingTask,
    BrowserDoubleClick,
    BrowserSelected,
    PackTitle,
    AddFiles,
    AddFolder,
    PackEmpty,
    Output,
    Browse,
    StoreOnly,
    ZipMethod,
    SevenZOptionsLabel,
    SevenZLevel,
    SevenZDict,
    SevenZAuto,
    AppendMode,
    AesEncrypt,
    Password,
    PackFormatsHint,
    PackAppendHint,
    PackOverwriteHint,
    PackAppendMissing,
    StartAppend,
    OverwriteAndPack,
    StartPack,
    Unknown,
    PrecheckEncrypted,
    PrecheckLowSpace,
    ConflictPolicy,
    CloseToTray,
    ShellMenu,
    ImportTheme,
    ExportTheme,
    ThemeExportUnavailable,
    Comment,
    CommentHint,
    Preview,
    PreviewLoading,
    PreviewBinary,
    PreviewTruncated,
    Template,
    TemplateNone,
    TemplateName,
    SaveTemplate,
    DeleteTemplate,
    TemplateSaved,
    VolumeSize,
    /// 预检面板的分卷摘要标签。
    Volumes,
    /// 预检面板的文件名编码选择标签。
    NameEncoding,
    VolumeNone,
    ThemeStats,
    SamplingNearest,
    SamplingLinear,
    LimitsInfo,
    FeatureSummary,
    ConfigPath,
    LogPath,
    ShellMenuAdded,
    ShellMenuRemoved,
    ShellMenuFailed,
    ExePathFailed,
    ThemeSwitched,
    TabSwitched,
    TabClosed,
    ThemeReloaded,
    DirectoryStatus,
    FilterArchives,
    FilterAll,
    FilterThemePack,
    ThemeDirMissing,
    TaskImportTheme,
    ImportingTheme,
    TaskExportTheme,
    ExportingTheme,
    OpeningFile,
    TaskOpen,
    TaskPrecheck,
    Prechecking,
    SelectBeforeDelete,
    DeleteTitle,
    DeleteConfirm,
    TaskDelete,
    Rewriting,
    TaskExtract,
    ExtractStarted,
    PackReady,
    ChoosePackFiles,
    ChoosePackFolder,
    PackErrNoSources,
    PackErrNoOutput,
    PackErrUnsupported,
    PackErrOutputIsDir,
    PackErrAppendZipOnly,
    PackErrAppendMissing,
    PackErrConfirmOverwrite,
    PackErrEncryptUnsupported,
    PackErrAppendPassword,
    PackErrPasswordRequired,
    PackingTo,
    AppendingTo,
    TaskPack,
    TaskAppend,
    TaskTest,
    TestStarted,
    ArchiveOpened,
    EntryCount,
    PasswordNeededFor,
    PasswordNeeded,
    TaskFinished,
    SummaryLine,
    ExtractSummary,
    PackSummary,
    PackDone,
    AppendDone,
    RewriteSummary,
    PrecheckSummary,
    ThemePackSummary,
    FailedEntries,
    FailedEntryLine,
    PartialFailure,
    TaskFailed,
    OpeningMany,
    ShellOpen,
    ShellExtractHere,
    ShellExtractTo,
    ShellAddToZip,
    ShellClassicMenu,
    ShellClassicMenuNote,
    ShellClassicMenuOn,
    ShellClassicMenuOff,
    NoMatches,
    OnlyAnomalies,
    SuspiciousName,
    CommandPalette,
    CommandPaletteHint,
    CommandNoMatch,
    CmdToggleTheme,
    CmdToggleLanguage,
    ReduceMotion,
    HighContrast,
    LimitsTitle,
    LimitsPreset,
    LimitsInvalid,
    RiskTitle,
    RiskAck,
    RiskFat32,
    RememberPassword,
    RememberPasswordNote,
}

impl TextKey {
    /// 所有 Key；用于文案完整性测试。
    #[allow(dead_code)]
    pub const ALL: &'static [TextKey] = &[
        TextKey::AppTitle,
        TextKey::Open,
        TextKey::NewArchive,
        TextKey::Extract,
        TextKey::Pack,
        TextKey::TestArchive,
        TextKey::Settings,
        TextKey::LocalFiles,
        TextKey::ArchiveEntries,
        TextKey::TaskQueue,
        TextKey::TaskRunning,
        TextKey::TaskDone,
        TextKey::SearchHint,
        TextKey::Theme,
        TextKey::Language,
        TextKey::StandardTerms,
        TextKey::FirstRunHint,
        TextKey::Dismiss,
        TextKey::EmptyArchive,
        TextKey::DemoBadge,
        TextKey::GoUp,
        TextKey::StatusReady,
        TextKey::About,
        TextKey::Cancel,
        TextKey::Pause,
        TextKey::Resume,
        TextKey::StartExtract,
        TextKey::Precheck,
        TextKey::ExtractTo,
        TextKey::SkipExisting,
        TextKey::OverwriteExisting,
        TextKey::RenameExisting,
        TextKey::AutoNumberExisting,
        TextKey::ClearFinished,
        TextKey::PasswordPrompt,
        TextKey::TotalSize,
        TextKey::LargestFile,
        TextKey::FreeSpace,
        TextKey::Conflicts,
        TextKey::Warnings,
        TextKey::Opening,
        TextKey::Testing,
        TextKey::TrayShow,
        TextKey::TrayHide,
        TextKey::TrayQuit,
        TextKey::NewTab,
        TextKey::CloseTab,
        TextKey::SelectedOne,
        TextKey::SelectedMany,
        TextKey::StatusSuffix,
        TextKey::DropBrowseFolder,
        TextKey::DropOpenArchive,
        TextKey::OneFile,
        TextKey::ItemCount,
        TextKey::ClearSelection,
        TextKey::SelectAll,
        TextKey::DeleteSelected,
        TextKey::RenameSelected,
        TextKey::MoveSelected,
        TextKey::BatchRenameSelected,
        TextKey::RegexRenameSelected,
        TextKey::RelocateRenameTitle,
        TextKey::RelocateMoveTitle,
        TextKey::RelocateBatchTitle,
        TextKey::RelocateRegexTitle,
        TextKey::RelocatePathLabel,
        TextKey::RelocateFolderLabel,
        TextKey::RelocateBatchLabel,
        TextKey::RelocateRegexPatternLabel,
        TextKey::RelocateRegexReplaceLabel,
        TextKey::RelocateRegexHint,
        TextKey::RelocateCaseInsensitive,
        TextKey::RelocateApply,
        TextKey::RelocateNoSelection,
        TextKey::RelocateOneOnly,
        TextKey::RelocateInvalid,
        TextKey::RelocateInvalidRegex,
        TextKey::RelocateDuplicate,
        TextKey::RelocateNested,
        TextKey::RelocateNoChange,
        TextKey::TaskRelocate,
        TextKey::Relocating,
        TextKey::EmptyHint,
        TextKey::ColName,
        TextKey::ColSize,
        TextKey::ColPacked,
        TextKey::ColRatio,
        TextKey::ColMethod,
        TextKey::ColTime,
        TextKey::ColCrc,
        TextKey::CrcMismatch,
        TextKey::ColFlags,
        TextKey::FlagLocked,
        TextKey::FlagSpoofed,
        TextKey::SpoofedHint,
        TextKey::SortHint,
        TextKey::TaskCount,
        TextKey::PhasePending,
        TextKey::PhasePlanning,
        TextKey::PhaseAwaitingDecision,
        TextKey::PhaseRunning,
        TextKey::PhasePaused,
        TextKey::PhaseFailed,
        TextKey::CancellingTask,
        TextKey::BrowserDoubleClick,
        TextKey::BrowserSelected,
        TextKey::PackTitle,
        TextKey::AddFiles,
        TextKey::AddFolder,
        TextKey::PackEmpty,
        TextKey::Output,
        TextKey::Browse,
        TextKey::StoreOnly,
        TextKey::ZipMethod,
        TextKey::SevenZOptionsLabel,
        TextKey::SevenZLevel,
        TextKey::SevenZDict,
        TextKey::SevenZAuto,
        TextKey::AppendMode,
        TextKey::AesEncrypt,
        TextKey::Password,
        TextKey::PackFormatsHint,
        TextKey::PackAppendHint,
        TextKey::PackOverwriteHint,
        TextKey::PackAppendMissing,
        TextKey::StartAppend,
        TextKey::OverwriteAndPack,
        TextKey::StartPack,
        TextKey::Unknown,
        TextKey::PrecheckEncrypted,
        TextKey::PrecheckLowSpace,
        TextKey::ConflictPolicy,
        TextKey::CloseToTray,
        TextKey::ShellMenu,
        TextKey::ImportTheme,
        TextKey::ExportTheme,
        TextKey::ThemeExportUnavailable,
        TextKey::Comment,
        TextKey::CommentHint,
        TextKey::Preview,
        TextKey::PreviewLoading,
        TextKey::PreviewBinary,
        TextKey::PreviewTruncated,
        TextKey::Template,
        TextKey::TemplateNone,
        TextKey::TemplateName,
        TextKey::SaveTemplate,
        TextKey::DeleteTemplate,
        TextKey::TemplateSaved,
        TextKey::VolumeSize,
        TextKey::Volumes,
        TextKey::NameEncoding,
        TextKey::VolumeNone,
        TextKey::ThemeStats,
        TextKey::SamplingNearest,
        TextKey::SamplingLinear,
        TextKey::LimitsInfo,
        TextKey::FeatureSummary,
        TextKey::ConfigPath,
        TextKey::LogPath,
        TextKey::ShellMenuAdded,
        TextKey::ShellMenuRemoved,
        TextKey::ShellMenuFailed,
        TextKey::ExePathFailed,
        TextKey::ThemeSwitched,
        TextKey::TabSwitched,
        TextKey::TabClosed,
        TextKey::ThemeReloaded,
        TextKey::DirectoryStatus,
        TextKey::FilterArchives,
        TextKey::FilterAll,
        TextKey::FilterThemePack,
        TextKey::ThemeDirMissing,
        TextKey::TaskImportTheme,
        TextKey::ImportingTheme,
        TextKey::TaskExportTheme,
        TextKey::ExportingTheme,
        TextKey::OpeningFile,
        TextKey::TaskOpen,
        TextKey::TaskPrecheck,
        TextKey::Prechecking,
        TextKey::SelectBeforeDelete,
        TextKey::DeleteTitle,
        TextKey::DeleteConfirm,
        TextKey::TaskDelete,
        TextKey::Rewriting,
        TextKey::TaskExtract,
        TextKey::ExtractStarted,
        TextKey::PackReady,
        TextKey::ChoosePackFiles,
        TextKey::ChoosePackFolder,
        TextKey::PackErrNoSources,
        TextKey::PackErrNoOutput,
        TextKey::PackErrUnsupported,
        TextKey::PackErrOutputIsDir,
        TextKey::PackErrAppendZipOnly,
        TextKey::PackErrAppendMissing,
        TextKey::PackErrConfirmOverwrite,
        TextKey::PackErrEncryptUnsupported,
        TextKey::PackErrAppendPassword,
        TextKey::PackErrPasswordRequired,
        TextKey::PackingTo,
        TextKey::AppendingTo,
        TextKey::TaskPack,
        TextKey::TaskAppend,
        TextKey::TaskTest,
        TextKey::TestStarted,
        TextKey::ArchiveOpened,
        TextKey::EntryCount,
        TextKey::PasswordNeededFor,
        TextKey::PasswordNeeded,
        TextKey::TaskFinished,
        TextKey::SummaryLine,
        TextKey::ExtractSummary,
        TextKey::PackSummary,
        TextKey::PackDone,
        TextKey::AppendDone,
        TextKey::RewriteSummary,
        TextKey::PrecheckSummary,
        TextKey::ThemePackSummary,
        TextKey::FailedEntries,
        TextKey::FailedEntryLine,
        TextKey::PartialFailure,
        TextKey::TaskFailed,
        TextKey::OpeningMany,
        TextKey::ShellOpen,
        TextKey::ShellExtractHere,
        TextKey::ShellExtractTo,
        TextKey::ShellAddToZip,
        TextKey::ShellClassicMenu,
        TextKey::ShellClassicMenuNote,
        TextKey::ShellClassicMenuOn,
        TextKey::ShellClassicMenuOff,
        TextKey::NoMatches,
        TextKey::OnlyAnomalies,
        TextKey::SuspiciousName,
        TextKey::CommandPalette,
        TextKey::CommandPaletteHint,
        TextKey::CommandNoMatch,
        TextKey::CmdToggleTheme,
        TextKey::CmdToggleLanguage,
        TextKey::ReduceMotion,
        TextKey::HighContrast,
        TextKey::LimitsTitle,
        TextKey::LimitsPreset,
        TextKey::LimitsInvalid,
        TextKey::RiskTitle,
        TextKey::RiskAck,
        TextKey::RiskFat32,
        TextKey::RememberPassword,
        TextKey::RememberPasswordNote,
    ];

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
            TextKey::Pause => "pause",
            TextKey::Resume => "resume",
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
            TextKey::NewTab => "new_tab",
            TextKey::CloseTab => "close_tab",
            TextKey::SelectedOne => "selected_one",
            TextKey::SelectedMany => "selected_many",
            TextKey::StatusSuffix => "status_suffix",
            TextKey::DropBrowseFolder => "drop_browse_folder",
            TextKey::DropOpenArchive => "drop_open_archive",
            TextKey::OneFile => "one_file",
            TextKey::ItemCount => "item_count",
            TextKey::ClearSelection => "clear_selection",
            TextKey::SelectAll => "select_all",
            TextKey::DeleteSelected => "delete_selected",
            TextKey::RenameSelected => "rename_selected",
            TextKey::MoveSelected => "move_selected",
            TextKey::BatchRenameSelected => "batch_rename_selected",
            TextKey::RegexRenameSelected => "regex_rename_selected",
            TextKey::RelocateRenameTitle => "relocate_rename_title",
            TextKey::RelocateMoveTitle => "relocate_move_title",
            TextKey::RelocateBatchTitle => "relocate_batch_title",
            TextKey::RelocateRegexTitle => "relocate_regex_title",
            TextKey::RelocatePathLabel => "relocate_path_label",
            TextKey::RelocateFolderLabel => "relocate_folder_label",
            TextKey::RelocateBatchLabel => "relocate_batch_label",
            TextKey::RelocateRegexPatternLabel => "relocate_regex_pattern_label",
            TextKey::RelocateRegexReplaceLabel => "relocate_regex_replace_label",
            TextKey::RelocateRegexHint => "relocate_regex_hint",
            TextKey::RelocateCaseInsensitive => "relocate_case_insensitive",
            TextKey::RelocateApply => "relocate_apply",
            TextKey::RelocateNoSelection => "relocate_no_selection",
            TextKey::RelocateOneOnly => "relocate_one_only",
            TextKey::RelocateInvalid => "relocate_invalid",
            TextKey::RelocateInvalidRegex => "relocate_invalid_regex",
            TextKey::RelocateDuplicate => "relocate_duplicate",
            TextKey::RelocateNested => "relocate_nested",
            TextKey::RelocateNoChange => "relocate_no_change",
            TextKey::TaskRelocate => "task_relocate",
            TextKey::Relocating => "relocating",
            TextKey::EmptyHint => "empty_hint",
            TextKey::ColName => "col_name",
            TextKey::ColSize => "col_size",
            TextKey::ColPacked => "col_packed",
            TextKey::ColRatio => "col_ratio",
            TextKey::ColMethod => "col_method",
            TextKey::ColTime => "col_time",
            TextKey::ColCrc => "col_crc",
            TextKey::CrcMismatch => "crc_mismatch",
            TextKey::ColFlags => "col_flags",
            TextKey::FlagLocked => "flag_locked",
            TextKey::FlagSpoofed => "flag_spoofed",
            TextKey::SpoofedHint => "spoofed_hint",
            TextKey::SortHint => "sort_hint",
            TextKey::TaskCount => "task_count",
            TextKey::PhasePending => "phase_pending",
            TextKey::PhasePlanning => "phase_planning",
            TextKey::PhaseAwaitingDecision => "phase_awaiting_decision",
            TextKey::PhaseRunning => "phase_running",
            TextKey::PhasePaused => "phase_paused",
            TextKey::PhaseFailed => "phase_failed",
            TextKey::CancellingTask => "cancelling_task",
            TextKey::BrowserDoubleClick => "browser_double_click",
            TextKey::BrowserSelected => "browser_selected",
            TextKey::PackTitle => "pack_title",
            TextKey::AddFiles => "add_files",
            TextKey::AddFolder => "add_folder",
            TextKey::PackEmpty => "pack_empty",
            TextKey::Output => "output",
            TextKey::Browse => "browse",
            TextKey::StoreOnly => "store_only",
            TextKey::ZipMethod => "zip_method",
            TextKey::SevenZOptionsLabel => "sevenz_options",
            TextKey::SevenZLevel => "sevenz_level",
            TextKey::SevenZDict => "sevenz_dict",
            TextKey::SevenZAuto => "sevenz_auto",
            TextKey::AppendMode => "append_mode",
            TextKey::AesEncrypt => "aes_encrypt",
            TextKey::Password => "password",
            TextKey::PackFormatsHint => "pack_formats_hint",
            TextKey::PackAppendHint => "pack_append_hint",
            TextKey::PackOverwriteHint => "pack_overwrite_hint",
            TextKey::PackAppendMissing => "pack_append_missing",
            TextKey::StartAppend => "start_append",
            TextKey::OverwriteAndPack => "overwrite_and_pack",
            TextKey::StartPack => "start_pack",
            TextKey::Unknown => "unknown",
            TextKey::PrecheckEncrypted => "precheck_encrypted",
            TextKey::PrecheckLowSpace => "precheck_low_space",
            TextKey::ConflictPolicy => "conflict_policy",
            TextKey::CloseToTray => "close_to_tray",
            TextKey::ShellMenu => "shell_menu",
            TextKey::ImportTheme => "import_theme",
            TextKey::ExportTheme => "export_theme",
            TextKey::ThemeExportUnavailable => "theme_export_unavailable",
            TextKey::Comment => "comment",
            TextKey::CommentHint => "comment_hint",
            TextKey::Preview => "preview",
            TextKey::PreviewLoading => "preview_loading",
            TextKey::PreviewBinary => "preview_binary",
            TextKey::PreviewTruncated => "preview_truncated",
            TextKey::Template => "template",
            TextKey::TemplateNone => "template_none",
            TextKey::TemplateName => "template_name",
            TextKey::SaveTemplate => "save_template",
            TextKey::DeleteTemplate => "delete_template",
            TextKey::TemplateSaved => "template_saved",
            TextKey::VolumeSize => "volume_size",
            TextKey::Volumes => "volumes",
            TextKey::NameEncoding => "name_encoding",
            TextKey::VolumeNone => "volume_none",
            TextKey::ThemeStats => "theme_stats",
            TextKey::SamplingNearest => "sampling_nearest",
            TextKey::SamplingLinear => "sampling_linear",
            TextKey::LimitsInfo => "limits_info",
            TextKey::FeatureSummary => "feature_summary",
            TextKey::ConfigPath => "config_path",
            TextKey::LogPath => "log_path",
            TextKey::ShellMenuAdded => "shell_menu_added",
            TextKey::ShellMenuRemoved => "shell_menu_removed",
            TextKey::ShellMenuFailed => "shell_menu_failed",
            TextKey::ExePathFailed => "exe_path_failed",
            TextKey::ThemeSwitched => "theme_switched",
            TextKey::TabSwitched => "tab_switched",
            TextKey::TabClosed => "tab_closed",
            TextKey::ThemeReloaded => "theme_reloaded",
            TextKey::DirectoryStatus => "directory_status",
            TextKey::FilterArchives => "filter_archives",
            TextKey::FilterAll => "filter_all",
            TextKey::FilterThemePack => "filter_theme_pack",
            TextKey::ThemeDirMissing => "theme_dir_missing",
            TextKey::TaskImportTheme => "task_import_theme",
            TextKey::ImportingTheme => "importing_theme",
            TextKey::TaskExportTheme => "task_export_theme",
            TextKey::ExportingTheme => "exporting_theme",
            TextKey::OpeningFile => "opening_file",
            TextKey::TaskOpen => "task_open",
            TextKey::TaskPrecheck => "task_precheck",
            TextKey::Prechecking => "prechecking",
            TextKey::SelectBeforeDelete => "select_before_delete",
            TextKey::DeleteTitle => "delete_title",
            TextKey::DeleteConfirm => "delete_confirm",
            TextKey::TaskDelete => "task_delete",
            TextKey::Rewriting => "rewriting",
            TextKey::TaskExtract => "task_extract",
            TextKey::ExtractStarted => "extract_started",
            TextKey::PackReady => "pack_ready",
            TextKey::ChoosePackFiles => "choose_pack_files",
            TextKey::ChoosePackFolder => "choose_pack_folder",
            TextKey::PackErrNoSources => "pack_err_no_sources",
            TextKey::PackErrNoOutput => "pack_err_no_output",
            TextKey::PackErrUnsupported => "pack_err_unsupported",
            TextKey::PackErrOutputIsDir => "pack_err_output_is_dir",
            TextKey::PackErrAppendZipOnly => "pack_err_append_zip_only",
            TextKey::PackErrAppendMissing => "pack_err_append_missing",
            TextKey::PackErrConfirmOverwrite => "pack_err_confirm_overwrite",
            TextKey::PackErrEncryptUnsupported => "pack_err_encrypt_unsupported",
            TextKey::PackErrAppendPassword => "pack_err_append_password",
            TextKey::PackErrPasswordRequired => "pack_err_password_required",
            TextKey::PackingTo => "packing_to",
            TextKey::AppendingTo => "appending_to",
            TextKey::TaskPack => "task_pack",
            TextKey::TaskAppend => "task_append",
            TextKey::TaskTest => "task_test",
            TextKey::TestStarted => "test_started",
            TextKey::ArchiveOpened => "archive_opened",
            TextKey::EntryCount => "entry_count",
            TextKey::PasswordNeededFor => "password_needed_for",
            TextKey::PasswordNeeded => "password_needed",
            TextKey::TaskFinished => "task_finished",
            TextKey::SummaryLine => "summary_line",
            TextKey::ExtractSummary => "extract_summary",
            TextKey::PackSummary => "pack_summary",
            TextKey::PackDone => "pack_done",
            TextKey::AppendDone => "append_done",
            TextKey::RewriteSummary => "rewrite_summary",
            TextKey::PrecheckSummary => "precheck_summary",
            TextKey::ThemePackSummary => "theme_pack_summary",
            TextKey::FailedEntries => "failed_entries",
            TextKey::FailedEntryLine => "failed_entry_line",
            TextKey::PartialFailure => "partial_failure",
            TextKey::TaskFailed => "task_failed",
            TextKey::OpeningMany => "opening_many",
            TextKey::ShellOpen => "shell_open",
            TextKey::ShellExtractHere => "shell_extract_here",
            TextKey::ShellExtractTo => "shell_extract_to",
            TextKey::ShellAddToZip => "shell_add_to_zip",
            TextKey::ShellClassicMenu => "shell_classic_menu",
            TextKey::ShellClassicMenuNote => "shell_classic_menu_note",
            TextKey::ShellClassicMenuOn => "shell_classic_menu_on",
            TextKey::ShellClassicMenuOff => "shell_classic_menu_off",
            TextKey::NoMatches => "no_matches",
            TextKey::OnlyAnomalies => "only_anomalies",
            TextKey::SuspiciousName => "suspicious_name",
            TextKey::CommandPalette => "command_palette",
            TextKey::CommandPaletteHint => "command_palette_hint",
            TextKey::CommandNoMatch => "command_no_match",
            TextKey::CmdToggleTheme => "cmd_toggle_theme",
            TextKey::CmdToggleLanguage => "cmd_toggle_language",
            TextKey::ReduceMotion => "reduce_motion",
            TextKey::HighContrast => "high_contrast",
            TextKey::LimitsTitle => "limits_title",
            TextKey::LimitsPreset => "limits_preset",
            TextKey::LimitsInvalid => "limits_invalid",
            TextKey::RiskTitle => "risk_title",
            TextKey::RiskAck => "risk_ack",
            TextKey::RiskFat32 => "risk_fat32",
            TextKey::RememberPassword => "remember_password",
            TextKey::RememberPasswordNote => "remember_password_note",
        }
    }
}

pub fn builtin(locale: Locale, key: TextKey) -> &'static str {
    match locale {
        Locale::ZhCn => match key {
            TextKey::AppTitle => "THzip-SpellArchive",
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
            TextKey::SearchHint => "搜索文件名…（* / ? 通配，re: 正则）",
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
            TextKey::Pause => "暂停",
            TextKey::Resume => "继续",
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
            TextKey::NewTab => "新标签页",
            TextKey::CloseTab => "关闭标签页",
            TextKey::SelectedOne => "已选：{name}",
            TextKey::SelectedMany => "已选 {count} 条",
            TextKey::StatusSuffix => "{theme} · {locale} · {total} 条",
            TextKey::DropBrowseFolder => "松开以浏览文件夹",
            TextKey::DropOpenArchive => "松开以打开归档",
            TextKey::OneFile => "1 个文件",
            TextKey::ItemCount => "{count} 个项目",
            TextKey::ClearSelection => "清空",
            TextKey::SelectAll => "全选",
            TextKey::DeleteSelected => "删除选中",
            TextKey::RenameSelected => "重命名…",
            TextKey::MoveSelected => "移动…",
            TextKey::BatchRenameSelected => "批量改名…",
            TextKey::RegexRenameSelected => "正则改名…",
            TextKey::RelocateRenameTitle => "重命名归档条目",
            TextKey::RelocateMoveTitle => "移动归档条目",
            TextKey::RelocateBatchTitle => "批量重命名归档条目",
            TextKey::RelocateRegexTitle => "正则替换重命名",
            TextKey::RelocatePathLabel => "新路径（归档内）",
            TextKey::RelocateFolderLabel => "目标文件夹（留空为根目录）",
            TextKey::RelocateBatchLabel => {
                "命名模板：{name}=原名，{n}=序号，{ext}=扩展名；不写 {ext} 时自动保留"
            }
            TextKey::RelocateRegexPatternLabel => "正则表达式（只匹配文件名）",
            TextKey::RelocateRegexReplaceLabel => "替换为（$1、${name} 引用分组）",
            TextKey::RelocateRegexHint => "示例：ep(\\d+) → 第$1话；目录改名时子条目会一起跟随",
            TextKey::RelocateCaseInsensitive => "忽略大小写",
            TextKey::RelocateApply => "应用",
            TextKey::RelocateNoSelection => "请先选择条目",
            TextKey::RelocateOneOnly => "重命名一次只能处理一个条目",
            TextKey::RelocateInvalid => "归档内路径不合法",
            TextKey::RelocateInvalidRegex => "正则表达式无效：{error}",
            TextKey::RelocateDuplicate => "重命名后存在重名：{name}",
            TextKey::RelocateNested => "不能把文件夹移动到自身内部",
            TextKey::RelocateNoChange => "路径没有变化",
            TextKey::TaskRelocate => "调整条目：{name}",
            TextKey::Relocating => "正在重写归档，调整 {count} 个条目…",
            TextKey::EmptyHint => "也可以双击左侧列表中的 zip / 7z，或把文件拖到窗口",
            TextKey::ColName => "名称",
            TextKey::ColSize => "大小",
            TextKey::ColPacked => "压缩后",
            TextKey::ColRatio => "比率",
            TextKey::ColMethod => "方法",
            TextKey::ColTime => "修改时间",
            TextKey::ColCrc => "CRC32",
            TextKey::CrcMismatch => "CRC 不符：解出的内容与目录记录的校验值不一致，文件可能已损坏",
            TextKey::ColFlags => "标记",
            TextKey::FlagLocked => "锁",
            TextKey::FlagSpoofed => "伪装",
            TextKey::SpoofedHint => "文件名包含双向控制字符，显示的扩展名可能与真实扩展名不同",
            TextKey::SortHint => "点击切换升序 / 降序",
            TextKey::TaskCount => "{count} 个任务",
            TextKey::PhasePending => "排队中",
            TextKey::PhasePlanning => "预检中",
            TextKey::PhaseAwaitingDecision => "等待确认",
            TextKey::PhaseRunning => "进行中",
            TextKey::PhasePaused => "已暂停",
            TextKey::PhaseFailed => "失败",
            TextKey::CancellingTask => "正在取消：{name}",
            TextKey::BrowserDoubleClick => "{name} · {kind} · 双击打开",
            TextKey::BrowserSelected => "选中：{name}",
            TextKey::PackTitle => "新建压缩包",
            TextKey::AddFiles => "添加文件…",
            TextKey::AddFolder => "添加文件夹…",
            TextKey::PackEmpty => "尚未添加任何文件或文件夹",
            TextKey::Output => "输出",
            TextKey::Browse => "浏览…",
            TextKey::StoreOnly => "仅存储（不压缩）",
            TextKey::ZipMethod => "zip 压缩方法",
            TextKey::SevenZOptionsLabel => "7z 参数",
            TextKey::SevenZLevel => "压缩级别",
            TextKey::SevenZDict => "字典大小",
            TextKey::SevenZAuto => "自动",
            TextKey::AppendMode => "追加到现有 zip（保留原有条目）",
            TextKey::AesEncrypt => "AES-256 加密",
            TextKey::Password => "密码",
            TextKey::PackFormatsHint => {
                "支持 .zip / .7z / .tar / .tar.gz / .tar.bz2 / .tar.xz / .tar.zst / .tar.lz4"
            }
            TextKey::PackAppendHint => "将追加到现有 zip / 7z / tar；完成后原子替换原文件。",
            TextKey::PackOverwriteHint => "目标文件已存在：确认后会原子替换。",
            TextKey::PackAppendMissing => "追加模式需要目标 zip 已存在。",
            TextKey::StartAppend => "开始追加",
            TextKey::OverwriteAndPack => "覆盖并打包",
            TextKey::StartPack => "开始打包",
            TextKey::Unknown => "未知",
            TextKey::PrecheckEncrypted => "包含加密条目；若尚未提供密码，解压会中止。",
            TextKey::PrecheckLowSpace => "预计写出的数据超过目标盘剩余空间。",
            TextKey::ConflictPolicy => "冲突处理",
            TextKey::CloseToTray => "关闭窗口时最小化到托盘",
            TextKey::ShellMenu => "资源管理器右键菜单（Win11 在“显示更多选项”里）",
            TextKey::ImportTheme => "导入主题包…",
            TextKey::ExportTheme => "导出主题包…",
            TextKey::ThemeExportUnavailable => "当前主题没有可导出的目录（素体为内置主题）",
            TextKey::Comment => "归档注释",
            TextKey::CommentHint => "仅 zip 支持，可留空",
            TextKey::Preview => "预览",
            TextKey::PreviewLoading => "正在读取条目…",
            TextKey::PreviewBinary => "二进制内容，暂不支持预览",
            TextKey::PreviewTruncated => "内容过大，仅显示前部",
            TextKey::Template => "任务模板",
            TextKey::TemplateNone => "不使用模板",
            TextKey::TemplateName => "模板名",
            TextKey::SaveTemplate => "保存模板",
            TextKey::DeleteTemplate => "删除模板",
            TextKey::TemplateSaved => "打包模板已保存",
            TextKey::VolumeSize => "分卷大小",
            TextKey::Volumes => "分卷",
            TextKey::NameEncoding => "文件名编码",
            TextKey::VolumeNone => "不分卷",
            TextKey::ThemeStats => "资源：{resources} 个纹理 · 采样：{sampling} · 基准 {scale}x",
            TextKey::SamplingNearest => "NEAREST（像素）",
            TextKey::SamplingLinear => "LINEAR",
            TextKey::LimitsInfo => "安全限额：总量 {total} GiB · 单条 {entry} GiB · 条目 {entries}",
            TextKey::FeatureSummary => {
                "已实现：zip / 7z / rar / tar 读取，zip / 7z / tar 写入、单文件流读取、AES-256、\n预检、CRC、后台任务、取消、密码重试、主题与 i18n 回退、路径净化、限额熔断、\n原子覆盖、设置持久化、日志、单实例、托盘、右键菜单、主题包导入、内嵌字体、正则改名。\n待接入：代码签名。"
            }
            TextKey::ConfigPath => "设置文件：{path}",
            TextKey::LogPath => "日志文件：{path}",
            TextKey::ShellMenuAdded => "已添加资源管理器右键菜单",
            TextKey::ShellMenuRemoved => "已移除资源管理器右键菜单",
            TextKey::ShellMenuFailed => "右键菜单更新失败: {error}",
            TextKey::ExePathFailed => "取 exe 路径失败: {error}",
            TextKey::ThemeSwitched => "已切换到主题：{name}",
            TextKey::TabSwitched => "切换到：{name}",
            TextKey::TabClosed => "已关闭标签页，剩余 {count} 个",
            TextKey::ThemeReloaded => "主题文件已重新加载",
            TextKey::DirectoryStatus => "目录：{path}",
            TextKey::FilterArchives => "归档",
            TextKey::FilterAll => "全部文件",
            TextKey::FilterThemePack => "主题包",
            TextKey::ThemeDirMissing => "找不到 themes 目录",
            TextKey::TaskImportTheme => "导入主题：{name}",
            TextKey::ImportingTheme => "正在后台导入主题包…",
            TextKey::TaskExportTheme => "导出主题：{name}",
            TextKey::ExportingTheme => "正在后台导出主题包…",
            TextKey::OpeningFile => "正在打开：{name}",
            TextKey::TaskOpen => "打开：{name}",
            TextKey::TaskPrecheck => "预检：{name}",
            TextKey::Prechecking => "正在后台预检…",
            TextKey::SelectBeforeDelete => "请先选择要删除的条目",
            TextKey::DeleteTitle => "删除归档条目",
            TextKey::DeleteConfirm => {
                "确定从归档中删除 {count} 个条目？\n归档会被重写，原文件在成功前保持不变。"
            }
            TextKey::TaskDelete => "删除条目：{name}",
            TextKey::Rewriting => "正在重写归档，删除 {count} 个条目…",
            TextKey::TaskExtract => "解压：{name}",
            TextKey::ExtractStarted => "开始解压到：{path}",
            TextKey::PackReady => "准备打包 {count} 个输入到 {path}",
            TextKey::ChoosePackFiles => "选择要打包的文件",
            TextKey::ChoosePackFolder => "选择要打包的文件夹",
            TextKey::PackErrNoSources => "请先添加文件或文件夹",
            TextKey::PackErrNoOutput => "请填写输出文件路径",
            TextKey::PackErrUnsupported => {
                "不支持的输出格式；可用 .zip / .7z / .tar / .tar.gz / .tar.bz2 / .tar.xz / .tar.zst / .tar.lz4"
            }
            TextKey::PackErrOutputIsDir => "输出路径是一个文件夹，请指定文件名",
            TextKey::PackErrAppendZipOnly => "追加模式支持 zip / 7z / tar 家族",
            TextKey::PackErrAppendMissing => "追加模式要求目标归档已存在",
            TextKey::PackErrConfirmOverwrite => "目标已存在；再次点击「开始打包」将覆盖它",
            TextKey::PackErrEncryptUnsupported => "只有 zip / 7z 支持加密写入",
            TextKey::PackErrAppendPassword => "追加模式暂不支持设置新密码",
            TextKey::PackErrPasswordRequired => "启用加密时必须填写密码",
            TextKey::PackingTo => "打包到：{path}",
            TextKey::AppendingTo => "追加到：{path}",
            TextKey::TaskPack => "打包：{name}",
            TextKey::TaskAppend => "追加：{name}",
            TextKey::TaskTest => "测试：{name}",
            TextKey::TestStarted => "开始完整性测试：{name}",
            TextKey::ArchiveOpened => "已打开 {name}：{count} 个条目",
            TextKey::EntryCount => "{count} 个条目",
            TextKey::PasswordNeededFor => "{name} 需要密码",
            TextKey::PasswordNeeded => "需要密码",
            TextKey::TaskFinished => "任务完成：{summary}",
            TextKey::SummaryLine => "{label}：{summary}",
            TextKey::ExtractSummary => "成功 {ok} · 跳过 {skipped} · 失败 {failed} · CRC {crc}",
            TextKey::PackSummary => "{entries} 个条目 · {bytes_in} → {bytes_out}",
            TextKey::PackDone => "打包完成",
            TextKey::AppendDone => "追加完成",
            TextKey::RewriteSummary => "重写完成：{entries} 个条目 · {bytes} 字节",
            TextKey::PrecheckSummary => "预检完成：{count} 个条目，解压后 {size}",
            TextKey::ThemePackSummary => "主题包：成功 {ok} · 跳过 {skipped} · 失败 {failed}",
            TextKey::FailedEntries => "失败条目：",
            TextKey::FailedEntryLine => "条目 {id}：{error}",
            TextKey::PartialFailure => "{summary}（部分条目失败）",
            TextKey::TaskFailed => "任务失败：{error}",
            TextKey::OpeningMany => "正在打开 {count} 个归档（各占一个标签页）",
            TextKey::ShellOpen => "用 THzip-SpellArchive 打开",
            TextKey::ShellExtractHere => "解压到当前文件夹",
            TextKey::ShellExtractTo => "解压到单独文件夹",
            TextKey::ShellAddToZip => "添加到 zip 压缩包",
            TextKey::ShellClassicMenu => "恢复 Win10 经典右键菜单",
            TextKey::ShellClassicMenuNote => {
                "让菜单项直接出现在右键第一层，不用点“显示更多选项”。需重启资源管理器生效。"
            }
            TextKey::ShellClassicMenuOn => "已恢复 Win10 经典右键菜单，请重启资源管理器",
            TextKey::ShellClassicMenuOff => "已恢复 Win11 新版右键菜单，请重启资源管理器",
            TextKey::NoMatches => "没有匹配的条目",
            TextKey::OnlyAnomalies => "只看异常项",
            TextKey::SuspiciousName => "可疑文件名：{detail}",
            TextKey::CommandPalette => "命令面板",
            TextKey::CommandPaletteHint => "输入命令过滤，回车执行，Esc 关闭",
            TextKey::CommandNoMatch => "没有匹配的命令",
            TextKey::CmdToggleTheme => "切换主题",
            TextKey::CmdToggleLanguage => "切换语言",
            TextKey::ReduceMotion => "减少动效（关闭过渡动画与粒子）",
            TextKey::HighContrast => "高对比度模式（正文对比度 ≥ 7:1）",
            TextKey::LimitsTitle => "安全限额",
            TextKey::LimitsPreset => "预设",
            TextKey::LimitsInvalid => "数值超出可表示范围，未写入",
            TextKey::RiskTitle => "高风险操作，需要二次确认",
            TextKey::RiskAck => "我已了解上述风险，仍要继续",
            TextKey::RiskFat32 => {
                "存在超过 4 GiB 的条目，而目标盘是 FAT32：写入会失败，建议改用 NTFS/exFAT"
            }
            TextKey::RememberPassword => "记住密码（本机本用户）",
            TextKey::RememberPasswordNote => {
                "密码用 Windows DPAPI 加密后存盘，只能在本机当前用户下解密；换机、重装或换用户都会失效。"
            }
        },
        Locale::EnUs => match key {
            TextKey::AppTitle => "THzip-SpellArchive",
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
            TextKey::SearchHint => "Search file name… (* / ? glob, re: regex)",
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
            TextKey::Pause => "Pause",
            TextKey::Resume => "Resume",
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
            TextKey::NewTab => "New tab",
            TextKey::CloseTab => "Close tab",
            TextKey::SelectedOne => "Selected: {name}",
            TextKey::SelectedMany => "{count} selected",
            TextKey::StatusSuffix => "{theme} · {locale} · {total} entries",
            TextKey::DropBrowseFolder => "Release to browse folder",
            TextKey::DropOpenArchive => "Release to open archive",
            TextKey::OneFile => "1 file",
            TextKey::ItemCount => "{count} items",
            TextKey::ClearSelection => "Clear",
            TextKey::SelectAll => "Select all",
            TextKey::DeleteSelected => "Delete selected",
            TextKey::RenameSelected => "Rename…",
            TextKey::MoveSelected => "Move…",
            TextKey::BatchRenameSelected => "Batch rename…",
            TextKey::RegexRenameSelected => "Regex rename…",
            TextKey::RelocateRenameTitle => "Rename archive entry",
            TextKey::RelocateMoveTitle => "Move archive entries",
            TextKey::RelocateBatchTitle => "Batch rename archive entries",
            TextKey::RelocateRegexTitle => "Regex replace rename",
            TextKey::RelocatePathLabel => "New path (inside archive)",
            TextKey::RelocateFolderLabel => "Target folder (empty for root)",
            TextKey::RelocateBatchLabel => {
                "Pattern: {name}=original, {n}=number, {ext}=extension; extension is kept unless {ext} is used"
            }
            TextKey::RelocateRegexPatternLabel => "Regular expression (file name only)",
            TextKey::RelocateRegexReplaceLabel => "Replace with ($1, ${name} for groups)",
            TextKey::RelocateRegexHint => {
                "Example: ep(\\d+) → episode$1; renaming a folder moves its children too"
            }
            TextKey::RelocateCaseInsensitive => "Case-insensitive",
            TextKey::RelocateApply => "Apply",
            TextKey::RelocateNoSelection => "Select entries first",
            TextKey::RelocateOneOnly => "Rename handles one entry at a time",
            TextKey::RelocateInvalid => "Invalid archive path",
            TextKey::RelocateInvalidRegex => "Invalid regular expression: {error}",
            TextKey::RelocateDuplicate => "Duplicate name after rename: {name}",
            TextKey::RelocateNested => "Cannot move a folder into itself",
            TextKey::RelocateNoChange => "Path unchanged",
            TextKey::TaskRelocate => "Relocate entries: {name}",
            TextKey::Relocating => "Rewriting archive, relocating {count} entries…",
            TextKey::EmptyHint => {
                "You can also double-click a zip / 7z in the list, or drag a file into the window"
            }
            TextKey::ColName => "Name",
            TextKey::ColSize => "Size",
            TextKey::ColPacked => "Packed",
            TextKey::ColRatio => "Ratio",
            TextKey::ColMethod => "Method",
            TextKey::ColTime => "Modified",
            TextKey::ColCrc => "CRC32",
            TextKey::CrcMismatch => {
                "CRC mismatch: extracted data does not match the recorded checksum; the file may be corrupt"
            }
            TextKey::ColFlags => "Flags",
            TextKey::FlagLocked => "Locked",
            TextKey::FlagSpoofed => "Spoofed",
            TextKey::SpoofedHint => {
                "The name contains bidi control characters; the shown extension may differ from the real one"
            }
            TextKey::SortHint => "Click to toggle ascending / descending",
            TextKey::TaskCount => "{count} tasks",
            TextKey::PhasePending => "Pending",
            TextKey::PhasePlanning => "Planning",
            TextKey::PhaseAwaitingDecision => "Awaiting confirmation",
            TextKey::PhaseRunning => "Running",
            TextKey::PhasePaused => "Paused",
            TextKey::PhaseFailed => "Failed",
            TextKey::CancellingTask => "Cancelling: {name}",
            TextKey::BrowserDoubleClick => "{name} · {kind} · double-click to open",
            TextKey::BrowserSelected => "Selected: {name}",
            TextKey::PackTitle => "Create archive",
            TextKey::AddFiles => "Add files…",
            TextKey::AddFolder => "Add folder…",
            TextKey::PackEmpty => "No files or folders added yet",
            TextKey::Output => "Output",
            TextKey::Browse => "Browse…",
            TextKey::StoreOnly => "Store only (no compression)",
            TextKey::ZipMethod => "Zip method",
            TextKey::SevenZOptionsLabel => "7z options",
            TextKey::SevenZLevel => "Compression level",
            TextKey::SevenZDict => "Dictionary size",
            TextKey::SevenZAuto => "Auto",
            TextKey::AppendMode => "Append to existing zip (keep existing entries)",
            TextKey::AesEncrypt => "AES-256 encryption",
            TextKey::Password => "Password",
            TextKey::PackFormatsHint => {
                "Supports .zip / .7z / .tar / .tar.gz / .tar.bz2 / .tar.xz / .tar.zst / .tar.lz4"
            }
            TextKey::PackAppendHint => {
                "Will append to the existing zip; the original file is atomically replaced on success."
            }
            TextKey::PackOverwriteHint => {
                "The target file already exists: it will be atomically replaced after confirmation."
            }
            TextKey::PackAppendMissing => "Append mode requires an existing archive.",
            TextKey::StartAppend => "Start appending",
            TextKey::OverwriteAndPack => "Overwrite and pack",
            TextKey::StartPack => "Start packing",
            TextKey::Unknown => "Unknown",
            TextKey::PrecheckEncrypted => {
                "Archive contains encrypted entries; extraction stops without a password."
            }
            TextKey::PrecheckLowSpace => {
                "Estimated output exceeds the free space on the target drive."
            }
            TextKey::ConflictPolicy => "Conflict handling",
            TextKey::CloseToTray => "Minimize to tray when closing the window",
            TextKey::ShellMenu => "Explorer context menu (Windows 11: under \"Show more options\")",
            TextKey::ImportTheme => "Import theme pack…",
            TextKey::ExportTheme => "Export theme pack…",
            TextKey::ThemeExportUnavailable => {
                "The current theme has no exportable directory (the plain theme is built in)"
            }
            TextKey::Comment => "Archive comment",
            TextKey::CommentHint => "zip only; leave empty for none",
            TextKey::Preview => "Preview",
            TextKey::PreviewLoading => "Reading entry…",
            TextKey::PreviewBinary => "Binary content; preview is not available",
            TextKey::PreviewTruncated => "Content too large; showing the beginning",
            TextKey::Template => "Pack template",
            TextKey::TemplateNone => "No template",
            TextKey::TemplateName => "Template name",
            TextKey::SaveTemplate => "Save template",
            TextKey::DeleteTemplate => "Delete template",
            TextKey::TemplateSaved => "Pack template saved",
            TextKey::VolumeSize => "Volume size",
            TextKey::Volumes => "Volumes",
            TextKey::NameEncoding => "Name encoding",
            TextKey::VolumeNone => "No split",
            TextKey::ThemeStats => {
                "Resources: {resources} textures · Sampling: {sampling} · Base {scale}x"
            }
            TextKey::SamplingNearest => "NEAREST (pixel)",
            TextKey::SamplingLinear => "LINEAR",
            TextKey::LimitsInfo => {
                "Safety limits: total {total} GiB · per entry {entry} GiB · {entries} entries"
            }
            TextKey::FeatureSummary => {
                "Implemented: read zip / 7z / rar / tar; write zip / 7z / tar; single-stream reading; AES-256;\nprecheck, CRC, background tasks, cancellation, password retry, theme and i18n fallback,\npath sanitizing, limit circuit breaker, atomic overwrite, settings persistence, logging,\nsingle instance, tray, context menu, theme pack import, embedded fonts, regex rename.\nPending: code signing."
            }
            TextKey::ConfigPath => "Settings file: {path}",
            TextKey::LogPath => "Log file: {path}",
            TextKey::ShellMenuAdded => "Explorer context menu installed",
            TextKey::ShellMenuRemoved => "Explorer context menu removed",
            TextKey::ShellMenuFailed => "Failed to update context menu: {error}",
            TextKey::ExePathFailed => "Failed to get exe path: {error}",
            TextKey::ThemeSwitched => "Switched to theme: {name}",
            TextKey::TabSwitched => "Switched to: {name}",
            TextKey::TabClosed => "Tab closed, {count} remaining",
            TextKey::ThemeReloaded => "Theme files reloaded",
            TextKey::DirectoryStatus => "Directory: {path}",
            TextKey::FilterArchives => "Archives",
            TextKey::FilterAll => "All files",
            TextKey::FilterThemePack => "Theme pack",
            TextKey::ThemeDirMissing => "themes directory not found",
            TextKey::TaskImportTheme => "Import theme: {name}",
            TextKey::ImportingTheme => "Importing theme pack in the background…",
            TextKey::TaskExportTheme => "Export theme: {name}",
            TextKey::ExportingTheme => "Exporting theme pack in the background…",
            TextKey::OpeningFile => "Opening: {name}",
            TextKey::TaskOpen => "Open: {name}",
            TextKey::TaskPrecheck => "Precheck: {name}",
            TextKey::Prechecking => "Prechecking in the background…",
            TextKey::SelectBeforeDelete => "Select entries to delete first",
            TextKey::DeleteTitle => "Delete archive entries",
            TextKey::DeleteConfirm => {
                "Delete {count} entries from the archive?\nThe archive will be rewritten; the original stays unchanged until success."
            }
            TextKey::TaskDelete => "Delete entries: {name}",
            TextKey::Rewriting => "Rewriting archive, deleting {count} entries…",
            TextKey::TaskExtract => "Extract: {name}",
            TextKey::ExtractStarted => "Extracting to: {path}",
            TextKey::PackReady => "Ready to pack {count} inputs to {path}",
            TextKey::ChoosePackFiles => "Select files to pack",
            TextKey::ChoosePackFolder => "Select folder to pack",
            TextKey::PackErrNoSources => "Add files or folders first",
            TextKey::PackErrNoOutput => "Enter an output file path",
            TextKey::PackErrUnsupported => {
                "Unsupported output format; use .zip / .7z / .tar / .tar.gz / .tar.bz2 / .tar.xz / .tar.zst / .tar.lz4"
            }
            TextKey::PackErrOutputIsDir => "The output path is a folder; specify a file name",
            TextKey::PackErrAppendZipOnly => "Append mode supports zip, 7z and tar",
            TextKey::PackErrAppendMissing => "Append mode requires an existing archive",
            TextKey::PackErrConfirmOverwrite => {
                "Target already exists; click \"Start packing\" again to overwrite it"
            }
            TextKey::PackErrEncryptUnsupported => "Only zip / 7z support encrypted writing",
            TextKey::PackErrAppendPassword => "Append mode cannot set a new password",
            TextKey::PackErrPasswordRequired => "Enter a password when encryption is enabled",
            TextKey::PackingTo => "Packing to: {path}",
            TextKey::AppendingTo => "Appending to: {path}",
            TextKey::TaskPack => "Pack: {name}",
            TextKey::TaskAppend => "Append: {name}",
            TextKey::TaskTest => "Test: {name}",
            TextKey::TestStarted => "Starting integrity test: {name}",
            TextKey::ArchiveOpened => "Opened {name}: {count} entries",
            TextKey::EntryCount => "{count} entries",
            TextKey::PasswordNeededFor => "{name} requires a password",
            TextKey::PasswordNeeded => "Password required",
            TextKey::TaskFinished => "Task finished: {summary}",
            TextKey::SummaryLine => "{label}: {summary}",
            TextKey::ExtractSummary => "OK {ok} · skipped {skipped} · failed {failed} · CRC {crc}",
            TextKey::PackSummary => "{entries} entries · {bytes_in} → {bytes_out}",
            TextKey::PackDone => "Packing finished",
            TextKey::AppendDone => "Appending finished",
            TextKey::RewriteSummary => "Rewrite finished: {entries} entries · {bytes} bytes",
            TextKey::PrecheckSummary => "Precheck finished: {count} entries, {size} unpacked",
            TextKey::ThemePackSummary => {
                "Theme pack: OK {ok} · skipped {skipped} · failed {failed}"
            }
            TextKey::FailedEntries => "Failed entries:",
            TextKey::FailedEntryLine => "Entry {id}: {error}",
            TextKey::PartialFailure => "{summary} (some entries failed)",
            TextKey::TaskFailed => "Task failed: {error}",
            TextKey::OpeningMany => "Opening {count} archives (one tab each)",
            TextKey::ShellOpen => "Open with THzip-SpellArchive",
            TextKey::ShellExtractHere => "Extract here",
            TextKey::ShellExtractTo => "Extract to folder",
            TextKey::ShellAddToZip => "Add to zip",
            TextKey::ShellClassicMenu => "Restore Win10 classic context menu",
            TextKey::ShellClassicMenuNote => {
                "Shows menu items directly on right-click instead of under \"Show more options\". Requires restarting Explorer."
            }
            TextKey::ShellClassicMenuOn => {
                "Win10 classic context menu restored, please restart Explorer"
            }
            TextKey::ShellClassicMenuOff => {
                "Win11 modern context menu restored, please restart Explorer"
            }
            TextKey::NoMatches => "No matching entries",
            TextKey::OnlyAnomalies => "Only anomalies",
            TextKey::SuspiciousName => "Suspicious name: {detail}",
            TextKey::CommandPalette => "Command palette",
            TextKey::CommandPaletteHint => "Type to filter, Enter to run, Esc to close",
            TextKey::CommandNoMatch => "No matching command",
            TextKey::CmdToggleTheme => "Switch theme",
            TextKey::CmdToggleLanguage => "Switch language",
            TextKey::ReduceMotion => "Reduce motion (no transitions or particles)",
            TextKey::HighContrast => "High contrast mode (body text ≥ 7:1)",
            TextKey::LimitsTitle => "Safety limits",
            TextKey::LimitsPreset => "Presets",
            TextKey::LimitsInvalid => "Value out of range; not applied",
            TextKey::RiskTitle => "High-risk operation, confirmation required",
            TextKey::RiskAck => "I understand the risks and want to continue",
            TextKey::RiskFat32 => {
                "An entry exceeds 4 GiB while the target is FAT32: the write will fail; use NTFS/exFAT"
            }
            TextKey::RememberPassword => "Remember password (this machine, this user)",
            TextKey::RememberPasswordNote => {
                "The password is encrypted with Windows DPAPI and can only be decrypted by the current user on this machine; moving to another machine, reinstalling or switching users invalidates it."
            }
        },
    }
}

/// 限额字段的本地化名。
pub fn limit_field_label(locale: Locale, field: LimitField) -> &'static str {
    match locale {
        Locale::ZhCn => match field {
            LimitField::TotalBytes => "解压总量上限",
            LimitField::EntryBytes => "单条目大小上限",
            LimitField::Entries => "条目数上限",
            LimitField::RatioStreamed => "膨胀倍率上限",
            LimitField::PathDepth => "路径层级上限",
            LimitField::PathLen => "路径长度上限",
            LimitField::DictSize => "压缩字典上限",
            LimitField::Nesting => "套娃层数上限",
            LimitField::MinFreeSpace => "保留可用空间",
        },
        Locale::EnUs => match field {
            LimitField::TotalBytes => "Max extracted size",
            LimitField::EntryBytes => "Max entry size",
            LimitField::Entries => "Max entry count",
            LimitField::RatioStreamed => "Max expansion ratio",
            LimitField::PathDepth => "Max path depth",
            LimitField::PathLen => "Max path length",
            LimitField::DictSize => "Max dictionary size",
            LimitField::Nesting => "Max nesting depth",
            LimitField::MinFreeSpace => "Free space to keep",
        },
    }
}

/// 三档限额预设的名称。
pub fn limit_profile_label(locale: Locale, profile: LimitProfile) -> &'static str {
    match locale {
        Locale::ZhCn => match profile {
            LimitProfile::Strict => "严格",
            LimitProfile::Default => "默认",
            LimitProfile::Relaxed => "宽松",
        },
        Locale::EnUs => match profile {
            LimitProfile::Strict => "Strict",
            LimitProfile::Default => "Default",
            LimitProfile::Relaxed => "Relaxed",
        },
    }
}

impl Translator {
    /// 伪本地化：把文案包进 ⟦ ⟧ 并把每个字母换成带重音的等价字符，
    /// 同时按固定比例拉长。用于在真实界面里一眼看出「哪些字符串没走 i18n」
    /// （裸 ASCII 一眼就露出来）和「哪些控件会被译文撑破」。
    ///
    /// 通过环境变量 `THZIP_PSEUDO_LOCALE=1` 开启，只在开发排障时用。
    pub fn pseudo_enabled() -> bool {
        std::env::var("THZIP_PSEUDO_LOCALE")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    }
}

/// 伪本地化单个字符串。占位符 `{name}` 原样保留，否则参数会替换失败。
///
/// 标记字符只用 ASCII（`[!!` / `!!]`）：egui 对缺字形会直接 panic，
/// 而内置比例字体只保证基础拉丁与 CJK，装饰性括号（⟦⟧、«»）不保险。
fn pseudo_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2 + 6);
    out.push_str("[!!");
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        push_pseudo(&mut out, &rest[..start]);
        match rest[start..].find('}') {
            Some(end) => {
                out.push_str(&rest[start..start + end + 1]);
                rest = &rest[start + end + 1..];
            }
            None => {
                push_pseudo(&mut out, &rest[start..]);
                rest = "";
            }
        }
    }
    push_pseudo(&mut out, rest);
    out.push_str("!!]");
    out
}

fn push_pseudo(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            'a' => out.push('á'),
            'e' => out.push('é'),
            'i' => out.push('í'),
            'o' => out.push('ó'),
            'u' => out.push('ú'),
            'A' => out.push('Á'),
            'E' => out.push('É'),
            'I' => out.push('Í'),
            'O' => out.push('Ó'),
            'U' => out.push('Ú'),
            _ => out.push(ch),
        }
    }
    // 日语假名与汉字的宽度和拉丁字母差别大，统一再补 4 个占位字符，
    // 模拟「德语 / 俄语译文普遍长三成」的最坏情况。
    out.push_str("···");
}

pub struct Translator {
    pub locale: Locale,
    /// true = 直接用标准术语（可发现性开关）。
    pub prefer_standard: bool,
    /// 是否输出伪本地化文本（见 [`Translator::pseudo_enabled`]）。
    pub pseudo: bool,
    overrides: BTreeMap<String, String>,
}

impl Default for Translator {
    fn default() -> Self {
        Self {
            locale: Locale::ZhCn,
            prefer_standard: false,
            pseudo: Self::pseudo_enabled(),
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

    /// 三层回退：主题覆盖 → 内置语言包。开启伪本地化时再包一层。
    pub fn t(&self, key: TextKey) -> String {
        let text = if !self.prefer_standard
            && let Some(value) = self.overrides.get(key.key())
        {
            value.clone()
        } else {
            builtin(self.locale, key).to_owned()
        };
        if self.pseudo {
            return pseudo_text(&text);
        }
        text
    }

    /// 在文案中替换 `{name}` 形式的占位符。
    pub fn tf(&self, key: TextKey, vars: &[(&str, &str)]) -> String {
        let mut text = self.t(key);
        for (name, value) in vars {
            text = text.replace(&format!("{{{name}}}"), value);
        }
        text
    }

    pub fn standard(&self, key: TextKey) -> &'static str {
        builtin(self.locale, key)
    }
}

#[cfg(test)]
mod tests {
    use super::{Locale, TextKey, Translator, builtin};
    use std::collections::HashSet;

    fn placeholders(text: &str) -> HashSet<String> {
        let mut names = HashSet::new();
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            let after = &rest[start + 1..];
            let Some(end) = after.find('}') else {
                break;
            };
            let name = &after[..end];
            if !name.is_empty() {
                names.insert(name.to_owned());
            }
            rest = &after[end + 1..];
        }
        names
    }

    #[test]
    fn every_key_has_unique_id_and_both_locales() {
        let mut ids = HashSet::new();
        for &key in TextKey::ALL {
            let id = key.key();
            assert!(!id.is_empty(), "empty id for {key:?}");
            assert!(ids.insert(id), "duplicate id: {id}");
            for locale in [Locale::ZhCn, Locale::EnUs] {
                let text = builtin(locale, key);
                assert!(!text.trim().is_empty(), "missing {locale:?} for {id}");
            }
        }
        assert_eq!(ids.len(), TextKey::ALL.len());
    }

    #[test]
    fn translations_share_the_same_placeholders() {
        for &key in TextKey::ALL {
            let zh = placeholders(builtin(Locale::ZhCn, key));
            let en = placeholders(builtin(Locale::EnUs, key));
            assert_eq!(zh, en, "placeholder mismatch for {}", key.key());
        }
    }

    /// 伪本地化必须保留占位符原文，否则参数替换会静默失效；
    /// 同时必须真的变长，才能压出被截断的控件。
    #[test]
    fn pseudo_locale_keeps_placeholders_and_grows() {
        for &key in TextKey::ALL {
            let original = builtin(Locale::ZhCn, key);
            let pseudo = super::pseudo_text(original);
            assert!(
                pseudo.chars().count() > original.chars().count(),
                "{} 没有变长",
                key.key()
            );
            assert_eq!(
                placeholders(&pseudo),
                placeholders(original),
                "{} 的占位符被破坏",
                key.key()
            );
            assert!(pseudo.starts_with("[!!") && pseudo.ends_with("!!]"));
        }
    }

    /// 译文与原文完全相同，通常意味着漏翻。
    ///
    /// 产品名、缩写、纯符号（`CRC32`、`THzip-SpellArchive`、`×`）两种写法本就
    /// 一致，用「不含小写 ASCII 字母」把它们排除掉：真正需要翻译的散文一定
    /// 含小写单词。这样白名单不必手工维护，新增技术名词不会误报。
    #[test]
    fn translations_are_not_pure_copies() {
        // 产品名两种语言都不翻译，且含小写字母，只能显式列出。
        const PRODUCT_NAMES: &[&str] = &["app_name", "app_title"];
        let mut offenders = Vec::new();
        for &key in TextKey::ALL {
            if PRODUCT_NAMES.contains(&key.key()) {
                continue;
            }
            let zh = builtin(Locale::ZhCn, key);
            let en = builtin(Locale::EnUs, key);
            if zh != en {
                continue;
            }
            if !zh.chars().any(|ch| ch.is_ascii_lowercase()) {
                continue;
            }
            offenders.push(key.key());
        }
        assert!(offenders.is_empty(), "疑似漏翻: {offenders:?}");
    }

    #[test]
    fn tf_replaces_named_placeholders() {
        let tr = Translator::new();
        let text = tr.tf(TextKey::EntryCount, &[("count", "3")]);
        assert_eq!(text, "3 个条目");

        let text = tr.tf(TextKey::ArchiveOpened, &[("name", "a.zip"), ("count", "2")]);
        assert_eq!(text, "已打开 a.zip：2 个条目");
    }
}
