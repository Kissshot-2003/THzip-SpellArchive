use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::plan::ExtractPlan;
use crate::reader::{ArchiveReader, ExtractReport};
use crate::types::{ArchiveEntry, ArchiveKind, EntryId};
use crate::writer::FinishReport;

pub type TaskId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskPhase {
    Pending,
    Planning,
    AwaitingDecision,
    Running,
    Paused,
    Done,
    Failed,
}

impl TaskPhase {
    pub fn label(self) -> &'static str {
        match self {
            TaskPhase::Pending => "排队中",
            TaskPhase::Planning => "预检中",
            TaskPhase::AwaitingDecision => "等待确认",
            TaskPhase::Running => "进行中",
            TaskPhase::Paused => "已暂停",
            TaskPhase::Done => "已完成",
            TaskPhase::Failed => "失败",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub enum TaskEvent {
    /// 后台成功打开归档。`reader` 保留给后续预检 / 解压 / 测试使用。
    Opened {
        task: TaskId,
        path: PathBuf,
        kind: ArchiveKind,
        entries: Arc<[ArchiveEntry]>,
        reader: Arc<Mutex<Box<dyn ArchiveReader>>>,
    },
    /// 需要密码才能继续（打开被中止，等待 UI 重试）。
    NeedPassword {
        task: TaskId,
        path: PathBuf,
    },
    PhaseChanged {
        task: TaskId,
        phase: TaskPhase,
    },
    Started {
        task: TaskId,
        total_bytes: u64,
        total_entries: u32,
    },
    Progress {
        task: TaskId,
        processed_compressed: u64,
        written: u64,
        entry: Option<EntryId>,
    },
    Warning {
        task: TaskId,
        text: String,
    },
    NeedDecision {
        task: TaskId,
        plan: Arc<ExtractPlan>,
    },
    /// 后台预检完成，UI 可以弹出确认框。
    PlanReady {
        task: TaskId,
        plan: Arc<ExtractPlan>,
        target: PathBuf,
        /// 预检时使用的 reader；切换标签页后仍能对正确的归档解压。
        reader: Arc<Mutex<Box<dyn ArchiveReader>>>,
    },
    EntryFailed {
        task: TaskId,
        entry: EntryId,
        error: String,
    },
    Finished {
        task: TaskId,
        report: Arc<ExtractReport>,
    },
    PackFinished {
        task: TaskId,
        report: Arc<FinishReport>,
        append: bool,
    },
    /// 归档重写完成（删除条目）。
    Rewritten {
        task: TaskId,
        path: PathBuf,
        report: Arc<FinishReport>,
    },
    /// 主题包已在后台解压到 themes 目录。
    ThemeInstalled {
        task: TaskId,
        report: Arc<ExtractReport>,
        source: PathBuf,
    },
    Failed {
        task: TaskId,
        error: String,
    },
}

#[derive(Debug, Clone)]
pub struct ArchiveOpened {
    pub task: TaskId,
    pub path: PathBuf,
}
