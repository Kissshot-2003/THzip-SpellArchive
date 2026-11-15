use std::fmt;

use crate::safety::bomb::LimitHit;
use crate::safety::path_sanitize::SanitizeError;
use crate::types::EntryId;

/// 归档内名字的形状问题。文案由 UI 层翻译。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameIssue {
    Empty,
    Absolute,
    Parent,
    DriveOrAds,
    Nul,
}

impl NameIssue {
    pub fn zh(self) -> &'static str {
        match self {
            NameIssue::Empty => "归档内名字为空",
            NameIssue::Absolute => "归档内名字不能是绝对路径",
            NameIssue::Parent => "归档内名字不能包含 ..",
            NameIssue::DriveOrAds => "归档内名字不能包含盘符或 NTFS 交换数据流",
            NameIssue::Nul => "归档内名字不能包含 NUL",
        }
    }
}

/// 结构化失败原因：只放错误码与参数，不放预翻译文案。
///
/// `Display` 提供中文兜底，供 CLI 与日志使用；GUI 用 [`crate::error::FailReason`]
/// 的结构化字段自行翻译。参数里的路径、方法名、库原始信息不翻译。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailReason {
    /// 库 / 系统返回的原始信息。
    Raw(String),
    PasswordRequired,
    BadPassword,
    OutOfSpace,
    Cancelled,
    /// IO 错误；`detail` 是系统原始信息。
    Io {
        detail: String,
    },
    Limit(LimitHit),
    EntryCorrupt {
        entry: Option<EntryId>,
        cause: Box<FailReason>,
    },

    BackendMissing,
    BackendMissingWrite,
    UnknownFormat,
    CreateUnsupported {
        format: String,
    },
    AppendUnsupported {
        format: String,
    },
    RewriteUnsupported {
        format: String,
    },
    EncryptedRewrite,
    RewriteExtractFailed {
        count: usize,
    },
    RewriteUnsupportedEntries {
        count: usize,
    },
    EncryptUnsupported {
        format: String,
    },
    NotDecompressible {
        format: String,
    },
    SignatureInvalid {
        format: String,
    },
    CentralDirectoryMissing,
    UnsupportedMethod {
        format: String,
        method: String,
    },
    EncryptedXzZipEntry,
    EncryptedLzmaZipEntry,
    ChecksumFailed {
        format: String,
    },
    CorruptLzmaHeader,
    WriterClosed {
        format: String,
    },
    ArchiveClosed {
        format: String,
    },

    EntryMissing {
        id: EntryId,
    },
    RenameTargetMissing {
        id: EntryId,
    },
    EntryCountMismatch {
        format: String,
    },
    EntryReadFailed {
        detail: String,
    },
    EntryExtractFailed {
        detail: String,
    },
    EntryVerifyFailed {
        detail: String,
    },
    EntryCrcFailed {
        detail: String,
    },
    StreamReadFailed {
        detail: String,
    },
    StagedReadFailed {
        detail: String,
    },
    RarFailed {
        detail: String,
    },
    VolumeNameInvalid,
    VolumeNoParent,
    VolumeIncomplete {
        missing: String,
    },

    MoveIntoSelf {
        name: String,
    },
    RenameDuplicate {
        name: String,
    },
    DuplicateArchiveName {
        name: String,
    },
    DuplicateInputName {
        name: String,
    },
    NoInputs,
    InputNotRoot {
        path: String,
    },
    ReadFailed {
        path: String,
        detail: String,
    },
    ListFailed {
        path: String,
        detail: String,
    },
    NotRegularFile {
        path: String,
    },
    SymlinkNotAllowed {
        path: String,
    },
    UnsafeName {
        name: String,
        issue: SanitizeError,
    },
    UnsafeTarget {
        issue: SanitizeError,
    },
    TargetIsReparsePoint,
    ReparsePointWrite {
        path: String,
    },
    ReparsePointOverwrite {
        path: String,
    },
    InvalidName(NameIssue),

    AppendPasswordUnsupported,
    AppendUnsupportedEntries {
        count: usize,
    },
    ThemeManifestMissing,
    ThemeTooManyEntries {
        count: usize,
        limit: usize,
    },
    ThemeFileTooLarge {
        name: String,
        size: u64,
    },
    ThemeTotalTooLarge,
    ThemeForbiddenFile {
        name: String,
    },
    Panic {
        message: String,
    },

    DecisionNotWrite {
        id: EntryId,
    },
    DirHasData {
        id: EntryId,
    },
    EntryNotBegun {
        id: EntryId,
    },
    EntryNotStarted {
        id: EntryId,
    },
    EntryOrder,
}

impl FailReason {
    /// 中文兜底文案（CLI / 日志 / 中文界面）。
    pub fn zh(&self) -> String {
        match self {
            FailReason::Raw(text) => text.clone(),
            FailReason::PasswordRequired => "需要密码".to_owned(),
            FailReason::BadPassword => "密码错误".to_owned(),
            FailReason::OutOfSpace => "磁盘空间不足".to_owned(),
            FailReason::Cancelled => "已取消".to_owned(),
            FailReason::Io { detail } => detail.clone(),
            FailReason::Limit(hit) => format!("触发安全限制: {hit}"),
            FailReason::EntryCorrupt { entry, cause } => match entry {
                Some(id) => format!("条目损坏: entry={id}: {cause}"),
                None => format!("条目损坏: {cause}"),
            },
            FailReason::BackendMissing => "该格式的后端尚未接入（骨架阶段）".to_owned(),
            FailReason::BackendMissingWrite => "该格式的写入后端尚未接入（骨架阶段）".to_owned(),
            FailReason::UnknownFormat => "无法识别归档格式（magic bytes 不匹配）".to_owned(),
            FailReason::CreateUnsupported { format } => format!("暂不支持创建 {format} 归档"),
            FailReason::AppendUnsupported { format } => format!("暂不支持追加到 {format} 归档"),
            FailReason::RewriteUnsupported { format } => format!("暂不支持修改 {format} 归档"),
            FailReason::EncryptedRewrite => "加密归档暂不支持修改条目".to_owned(),
            FailReason::RewriteExtractFailed { count } => {
                format!("保留条目解压失败 {count} 条，已取消重写")
            }
            FailReason::RewriteUnsupportedEntries { count } => {
                format!("有 {count} 条条目无法写回新归档（符号链接或不支持的方法），已取消重写")
            }
            FailReason::EncryptUnsupported { format } => format!("{format} 暂不支持加密写入"),
            FailReason::NotDecompressible { format } => format!("不是可解压容器：{format}"),
            FailReason::SignatureInvalid { format } => format!("{format} 签名无效"),
            FailReason::CentralDirectoryMissing => "中央目录中找不到条目".to_owned(),
            FailReason::UnsupportedMethod { format, method } => {
                format!("{format} 暂不支持压缩方法 {method}")
            }
            FailReason::EncryptedXzZipEntry => "加密的 XZ zip 条目暂不支持解压".to_owned(),
            FailReason::EncryptedLzmaZipEntry => "加密的 LZMA zip 条目暂不支持解压".to_owned(),
            FailReason::ChecksumFailed { format } => format!("{format} 数据校验失败"),
            FailReason::CorruptLzmaHeader => "zip LZMA 头属性长度异常".to_owned(),
            FailReason::WriterClosed { format } => format!("{format} 写入器已中止"),
            FailReason::ArchiveClosed { format } => format!("{format} 归档已关闭"),
            FailReason::EntryMissing { id } => format!("条目 {id} 不存在"),
            FailReason::RenameTargetMissing { id } => format!("条目 {id} 缺少目标名字"),
            FailReason::EntryCountMismatch { format } => {
                format!("{format} 条目数与列表不一致，无法继续")
            }
            FailReason::EntryReadFailed { detail } => format!("读取条目失败: {detail}"),
            FailReason::EntryExtractFailed { detail } => format!("解压失败: {detail}"),
            FailReason::EntryVerifyFailed { detail } => format!("校验失败: {detail}"),
            FailReason::EntryCrcFailed { detail } => format!("CRC/数据校验失败: {detail}"),
            FailReason::StreamReadFailed { detail } => format!("无法读取压缩流: {detail}"),
            FailReason::StagedReadFailed { detail } => format!("读取 RAR 临时文件失败: {detail}"),
            FailReason::RarFailed { detail } => format!("RAR 处理失败: {detail}"),
            FailReason::VolumeNameInvalid => "分卷文件名无效".to_owned(),
            FailReason::VolumeNoParent => "分卷没有父目录".to_owned(),
            FailReason::VolumeIncomplete { missing } => format!("分卷不完整，缺少：{missing}"),
            FailReason::MoveIntoSelf { name } => {
                format!("不能把目录 {name} 移动到自身内部")
            }
            FailReason::RenameDuplicate { name } => format!("重命名后存在重名条目: {name}"),
            FailReason::DuplicateArchiveName { name } => {
                format!("归档内重名（Windows 不区分大小写）：{name}")
            }
            FailReason::DuplicateInputName { name } => {
                format!("输入中存在同名条目（Windows 不区分大小写）：{name}")
            }
            FailReason::NoInputs => "没有选择要打包的文件".to_owned(),
            FailReason::InputNotRoot { path } => format!("无法作为归档根：{path}"),
            FailReason::ReadFailed { path, detail } => format!("无法读取 {path}: {detail}"),
            FailReason::ListFailed { path, detail } => format!("无法列出 {path}: {detail}"),
            FailReason::NotRegularFile { path } => format!("不是普通文件：{path}"),
            FailReason::SymlinkNotAllowed { path } => format!("不写入符号链接：{path}"),
            FailReason::UnsafeName { name, issue } => format!("不安全的名字 {name}: {issue}"),
            FailReason::UnsafeTarget { issue } => format!("目标路径不安全: {issue}"),
            FailReason::TargetIsReparsePoint => {
                "目标已存在且是重解析点（junction/symlink）".to_owned()
            }
            FailReason::ReparsePointWrite { path } => {
                format!("{path} 是重解析点，拒绝写入")
            }
            FailReason::ReparsePointOverwrite { path } => {
                format!("{path} 已存在且是重解析点，拒绝覆盖")
            }
            FailReason::InvalidName(issue) => issue.zh().to_owned(),
            FailReason::AppendPasswordUnsupported => "追加模式暂不支持设置新密码".to_owned(),
            FailReason::AppendUnsupportedEntries { count } => {
                format!("归档包含 {count} 个无法保留的条目，已取消追加")
            }
            FailReason::ThemeManifestMissing => {
                "主题包格式不对，需要 <主题目录>/theme.toml".to_owned()
            }
            FailReason::ThemeTooManyEntries { count, limit } => {
                format!("主题包条目过多：{count}（上限 {limit}）")
            }
            FailReason::ThemeFileTooLarge { name, size } => {
                format!("主题文件 {name} 超过 8 MiB（{size} 字节）")
            }
            FailReason::ThemeTotalTooLarge => "主题包解压后超过 64 MiB".to_owned(),
            FailReason::ThemeForbiddenFile { name } => {
                format!("主题包包含不允许的文件类型：{name}")
            }
            FailReason::Panic { message } => format!("任务异常: {message}"),
            FailReason::DecisionNotWrite { id } => {
                format!("条目 {id} 的决策不是写入，后端不应调用 on_entry_begin")
            }
            FailReason::DirHasData { id } => format!("目录条目 {id} 不应有数据"),
            FailReason::EntryNotBegun { id } => format!("条目 {id} 未调用 on_entry_begin"),
            FailReason::EntryNotStarted { id } => format!("条目 {id} 未开始"),
            FailReason::EntryOrder => "条目顺序错乱".to_owned(),
        }
    }

    pub fn is_password_related(&self) -> bool {
        matches!(self, FailReason::PasswordRequired | FailReason::BadPassword)
    }
}

impl fmt::Display for FailReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.zh())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("打开失败: {0}")]
    Open(FailReason),

    #[error("需要密码")]
    PasswordRequired,

    #[error("密码错误")]
    BadPassword,

    #[error("不支持的格式或特性: {0}")]
    Unsupported(FailReason),

    #[error("条目损坏: {cause}")]
    CorruptEntry {
        entry: Option<EntryId>,
        cause: FailReason,
    },

    #[error("触发安全限制: {0}")]
    LimitExceeded(LimitHit),

    #[error("磁盘空间不足")]
    OutOfSpace,

    #[error("已取消")]
    Cancelled,

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(FailReason),
}

impl ArchiveError {
    /// 转成可跨线程、可克隆、可被 UI 翻译的结构化原因。
    pub fn reason(&self) -> FailReason {
        match self {
            ArchiveError::Open(reason)
            | ArchiveError::Unsupported(reason)
            | ArchiveError::Other(reason) => reason.clone(),
            ArchiveError::PasswordRequired => FailReason::PasswordRequired,
            ArchiveError::BadPassword => FailReason::BadPassword,
            ArchiveError::CorruptEntry { entry, cause } => FailReason::EntryCorrupt {
                entry: *entry,
                cause: Box::new(cause.clone()),
            },
            ArchiveError::LimitExceeded(hit) => FailReason::Limit(*hit),
            ArchiveError::OutOfSpace => FailReason::OutOfSpace,
            ArchiveError::Cancelled => FailReason::Cancelled,
            ArchiveError::Io(err) => FailReason::Io {
                detail: err.to_string(),
            },
        }
    }

    pub fn is_password_related(&self) -> bool {
        matches!(
            self,
            ArchiveError::PasswordRequired | ArchiveError::BadPassword
        ) || matches!(self, ArchiveError::Other(reason) if reason.is_password_related())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SinkError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("触发安全限制: {0}")]
    Limit(LimitHit),

    #[error("{0}")]
    Other(FailReason),
}

impl SinkError {
    pub fn reason(&self) -> FailReason {
        match self {
            SinkError::Io(err) => FailReason::Io {
                detail: err.to_string(),
            },
            SinkError::Limit(hit) => FailReason::Limit(*hit),
            SinkError::Other(reason) => reason.clone(),
        }
    }
}

/// 便捷构造：`FailReason::Raw(format!(...))`。
pub fn raw(text: impl Into<String>) -> FailReason {
    FailReason::Raw(text.into())
}

/// 便捷构造：`path` 的展示字符串。
pub fn path_text(path: &std::path::Path) -> String {
    path.display().to_string()
}
