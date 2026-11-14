use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use eframe::egui;
use thzip_core::plan::{ExtractPlan, OverwritePolicy};
use thzip_core::task::{CancelToken, SharedReader, TaskId, TaskPhase};
use thzip_core::types::{ArchiveEntry, ArchiveKind, EntryId, Method};

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
    pub comment: Option<String>,
}

pub struct DirItem {
    pub name: String,
    pub is_dir: bool,
}

pub struct PreviewView {
    pub id: EntryId,
    pub name: String,
    pub data: Arc<[u8]>,
    pub truncated: bool,
    pub failed: Option<thzip_core::error::FailReason>,
    pub loading: bool,
    pub text: Option<String>,
    pub image: Option<([usize; 2], Vec<u8>)>,
    pub texture: Option<egui::TextureHandle>,
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
    pub(crate) last_bytes: u64,
    pub(crate) last_sample: Instant,
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
    pub fn key(self) -> &'static str {
        match self {
            Self::Deflate => "deflate",
            Self::Bzip2 => "bzip2",
            Self::Zstd => "zstd",
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            "bzip2" => Self::Bzip2,
            "zstd" => Self::Zstd,
            _ => Self::Deflate,
        }
    }

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
    pub volume_mib: u32,
    pub append: bool,
    pub encrypt: bool,
    pub password: String,
    pub comment: String,
    pub template_name: String,
    pub template_index: Option<usize>,
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
            volume_mib: 0,
            append: false,
            encrypt: false,
            password: String::new(),
            comment: String::new(),
            template_name: String::new(),
            template_index: None,
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
    pub(crate) fn text(&self) -> &str {
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
