use crate::types::EntryId;

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("打开失败: {0}")]
    Open(String),

    #[error("需要密码")]
    PasswordRequired,

    #[error("密码错误")]
    BadPassword,

    #[error("不支持的格式或特性: {0}")]
    Unsupported(String),

    #[error("条目损坏: entry={entry:?}: {detail}")]
    CorruptEntry {
        entry: Option<EntryId>,
        detail: String,
    },

    #[error("触发安全限制: {0}")]
    LimitExceeded(&'static str),

    #[error("磁盘空间不足")]
    OutOfSpace,

    #[error("已取消")]
    Cancelled,

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

#[derive(Debug, thiserror::Error)]
pub enum SinkError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}
