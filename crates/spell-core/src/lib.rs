//! SpellArchive 核心库：不依赖任何 GUI 框架。
//!
//! 分层约定：
//! - `types` / `plan` / `reader` / `writer`：纯数据与契约
//! - `backend`：格式实现（目前为占位，待接入 sevenz-rust2 / zip / tar）
//! - `safety`：路径净化、显示名防护、限额、原子覆盖（必须被所有写盘入口复用）
//! - `task`：任务事件、取消令牌、`catch_unwind` 保护

#![deny(unsafe_code)]

pub mod backend;
pub mod error;
pub mod limits;
pub mod plan;
pub mod reader;
pub mod safety;
pub mod secret;
pub mod task;
pub mod types;
pub mod writer;

pub use error::{ArchiveError, SinkError};
pub use limits::{LimitProfile, Limits};
pub use plan::{DecisionTable, ExtractPlan, OverwritePolicy};
pub use reader::{ArchiveReader, ExtractReport, ExtractSink};
pub use secret::SecretString;
pub use types::{ArchiveEntry, ArchiveKind, EntryId, EntryKind, Method};
pub use writer::{
    ArchiveWriter, FinishReport, PackEntry, PackKind, SevenZOptions, normalize_entry_name,
    scan_inputs, write_entries,
};
