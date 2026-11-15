use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::FailReason;
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

/// 取消 / 暂停共用的控制位。取消与暂停是两个独立状态：
/// 取消是单向且终态，暂停可反复切换。
#[derive(Default)]
struct ControlState {
    cancelled: AtomicBool,
    paused: AtomicBool,
}

/// 后台任务的取消 / 暂停开关。克隆成本只有一次 `Arc` 计数增加，
/// 因此可以放心地分发给执行层（`ExtractDirSink` 等）。
#[derive(Clone, Default)]
pub struct CancelToken(Arc<ControlState>);

/// 暂停期间 `checkpoint()` 的轮询间隔。取 20 ms 是为了既不让出一整个
/// 时隙（恢复响应的迟滞），也不至于空转烧 CPU。
const PAUSE_POLL: Duration = Duration::from_millis(20);

/// 暂停最长持续时间。超过则自动放行并视为未暂停，避免残留的暂停标志
/// 把工作线程永久挂死（例如 UI 异常退出没来得及 resume）。
const PAUSE_TIMEOUT: Duration = Duration::from_secs(3600);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Relaxed);
        // 取消必须能打断暂停，否则 pause 中的线程永远看不到取消位。
        self.0.paused.store(false, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Relaxed)
    }

    pub fn pause(&self) {
        self.0.paused.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.0.paused.store(false, Ordering::Relaxed);
    }

    pub fn is_paused(&self) -> bool {
        self.0.paused.load(Ordering::Relaxed)
    }

    /// 在批次边界调用：已取消返回 `false`；处于暂停则原地阻塞直到恢复、
    /// 取消或超时。返回值语义是「可以继续」。
    pub fn checkpoint(&self) -> bool {
        if self.is_cancelled() {
            return false;
        }
        if !self.is_paused() {
            return true;
        }
        let started = Instant::now();
        while self.is_paused() {
            if self.is_cancelled() {
                return false;
            }
            if started.elapsed() >= PAUSE_TIMEOUT {
                // 超时自愈：清掉暂停位，让任务继续跑完，避免线程挂死。
                self.0.paused.store(false, Ordering::Relaxed);
                return true;
            }
            thread::sleep(PAUSE_POLL);
        }
        !self.is_cancelled()
    }
}

impl fmt::Debug for CancelToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CancelToken")
            .field("cancelled", &self.is_cancelled())
            .field("paused", &self.is_paused())
            .finish()
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
    /// 后台预检完成，UI 可以弹出确认框。
    PlanReady {
        task: TaskId,
        plan: Arc<ExtractPlan>,
        target: PathBuf,
        /// 预检时使用的 reader；切换标签页后仍能对正确的归档解压。
        reader: Arc<Mutex<Box<dyn ArchiveReader>>>,
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
    /// 单条目预览数据已读入内存。
    PreviewReady {
        task: TaskId,
        id: EntryId,
        data: Arc<[u8]>,
        truncated: bool,
        failed: Option<FailReason>,
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
        error: FailReason,
    },
}
