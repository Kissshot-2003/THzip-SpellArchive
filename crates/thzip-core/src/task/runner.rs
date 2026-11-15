use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};

use crate::backend;
use crate::error::{ArchiveError, FailReason};
use crate::limits::Limits;
use crate::plan::{DecisionTable, ExtractPlan, OverwritePolicy};
use crate::reader::{ArchiveReader, ExtractSink};
use crate::safety::{ExtractDirSink, MemorySink, VerifySink};
use crate::secret::SecretString;
use crate::task::event::{CancelToken, TaskEvent, TaskId, TaskPhase};
use crate::task::worker;
use crate::types::{ArchiveEntry, ArchiveKind, EntryId, Method};
use crate::writer::{SevenZOptions, scan_inputs, write_entries};

/// 打开后的归档由 UI 与 worker 共享；worker 执行解压 / 测试时短暂持锁。
pub type SharedReader = Arc<Mutex<Box<dyn ArchiveReader>>>;

/// 后台任务种类。UI 只提交意图，不在 UI 线程做重活。
pub enum TaskOp {
    Open {
        path: PathBuf,
        password: Option<SecretString>,
    },
    Plan {
        reader: SharedReader,
        ids: Vec<EntryId>,
        target: PathBuf,
        limits: Limits,
    },
    Extract {
        reader: SharedReader,
        plan: Arc<ExtractPlan>,
        decisions: DecisionTable,
        limits: Limits,
    },
    Test {
        reader: SharedReader,
        limits: Limits,
    },
    /// 读取单个条目到内存用于预览；超过 `max_bytes` 只保留前缀。
    Preview {
        reader: SharedReader,
        id: EntryId,
        limits: Limits,
        max_bytes: u64,
    },
    Pack {
        output: PathBuf,
        inputs: Vec<PathBuf>,
        kind: ArchiveKind,
        method: Method,
        append: bool,
        password: Option<SecretString>,
        sevenz: SevenZOptions,
        comment: Option<String>,
        volume_size: Option<u64>,
        limits: Limits,
    },
    /// 重写归档：删除 `remove` 中的条目，并把 `rename` 中的条目改成新名字。
    Rewrite {
        path: PathBuf,
        kind: ArchiveKind,
        remove: Vec<EntryId>,
        rename: Vec<(EntryId, String)>,
        limits: Limits,
    },
    /// 把主题包解压到目标 themes 目录；冲突默认跳过。
    InstallTheme {
        archive: PathBuf,
        target: PathBuf,
        limits: Limits,
    },
}

impl TaskOp {
    /// 该操作会独占写的磁盘路径。同一路径上的两个写操作必须串行，
    /// 否则「一边打包一边重写同一个归档」会把文件写成半截。
    /// 只读操作（Open / Plan / Test / Preview）返回 None，可以并发。
    fn exclusive_path(&self) -> Option<&PathBuf> {
        match self {
            TaskOp::Pack { output, .. } => Some(output),
            TaskOp::Rewrite { path, .. } => Some(path),
            TaskOp::InstallTheme { target, .. } => Some(target),
            TaskOp::Extract { plan, .. } => Some(&plan.root),
            _ => None,
        }
    }
}

/// `submit` 返回的句柄：UI 用它显示任务、取消或暂停。
#[derive(Clone)]
pub struct TaskHandle {
    pub id: TaskId,
    pub cancel: CancelToken,
}

impl TaskHandle {
    /// 请求暂停。已经进入下一个批次边界时才会真正停下，
    /// 当前这一块数据仍会写完，不会留下半截文件。
    pub fn pause(&self) {
        self.cancel.pause();
    }

    pub fn resume(&self) {
        self.cancel.resume();
    }

    pub fn is_paused(&self) -> bool {
        self.cancel.is_paused()
    }

    pub fn cancel_task(&self) {
        self.cancel.cancel();
    }
}

/// 待处理操作队列的容量。无界队列在用户狂点「解压」时会把内存吃光，
/// 而且 `submit` 永不阻塞，UI 也得不到「排队已满」的反馈。
const QUEUE_CAPACITY: usize = 1024;

/// 提交时最长等待时间。队列满到这个程度说明后台已经严重积压，
/// 与其一直卡住 UI，不如直接失败让用户知道。
const SUBMIT_TIMEOUT: Duration = Duration::from_secs(5);

/// 固定数量后台线程 + 双向通道。事件由 UI 侧 `try_recv` 批量取走。
/// 按路径分片的互斥锁表，用于把同路径的写操作串行化。
type PathLocks = Arc<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>>;

pub struct TaskRunner {
    op_tx: Sender<(TaskId, CancelToken, TaskOp)>,
    ev_rx: Receiver<TaskEvent>,
    next_id: TaskId,
    path_locks: PathLocks,
}

impl TaskRunner {
    pub fn new(workers: usize) -> Self {
        let (op_tx, op_rx) = bounded::<(TaskId, CancelToken, TaskOp)>(QUEUE_CAPACITY);
        // 事件通道保持无界：它是 worker 唯一的上报途径，一旦阻塞在这里
        // 就等于把整个后台卡死（UI 线程卡住时尤其致命）。事件本身很小，
        // 上游操作队列又已有界，堆积量天然被限制住。
        let (ev_tx, ev_rx) = unbounded::<TaskEvent>();
        let path_locks: PathLocks = Arc::new(Mutex::new(HashMap::new()));

        for index in 0..workers.max(1) {
            let op_rx = op_rx.clone();
            let ev_tx = ev_tx.clone();
            let locks = Arc::clone(&path_locks);
            let _ = std::thread::Builder::new()
                .name(format!("spell-worker-{index}"))
                .spawn(move || worker_loop(op_rx, ev_tx, locks));
        }

        Self {
            op_tx,
            ev_rx,
            next_id: 1,
            path_locks,
        }
    }

    /// 提交一个操作。队列满时最多等 [`SUBMIT_TIMEOUT`]。
    ///
    /// 等待超时也照样返回句柄：操作没进队列，`cancel` 之后 worker 不会执行它，
    /// 调用方可以从 `try_recv` 里看不到任何事件来判断这一点。
    pub fn submit(&mut self, op: TaskOp) -> TaskHandle {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let cancel = CancelToken::new();
        let _ = self
            .op_tx
            .send_timeout((id, cancel.clone(), op), SUBMIT_TIMEOUT);
        TaskHandle { id, cancel }
    }

    /// 清掉已经没人持有的路径锁，避免长会话里锁表无限增长。
    pub fn prune_locks(&self) {
        let mut guard = lock_map(&self.path_locks);
        guard.retain(|_, lock| Arc::strong_count(lock) > 1);
    }

    /// 非阻塞取一条事件。UI 每帧循环调用直到返回 `None`。
    pub fn try_recv(&self) -> Option<TaskEvent> {
        self.ev_rx.try_recv().ok()
    }
}

/// 取锁表，忽略毒化：某个 worker panic 不该让其余 worker 再也拿不到锁。
fn lock_map(
    locks: &Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>,
) -> std::sync::MutexGuard<'_, HashMap<PathBuf, Arc<Mutex<()>>>> {
    locks.lock().unwrap_or_else(|err| err.into_inner())
}

/// 路径的加锁键：真实路径优先，取不到就退回「父目录真实路径 + 文件名」。
fn lock_key(path: &std::path::Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let name = path.file_name().map(|name| name.to_os_string());
    match (std::fs::canonicalize(parent), name) {
        (Ok(base), Some(name)) => base.join(name),
        _ => path.to_path_buf(),
    }
}

fn archive_lock(locks: &PathLocks, path: &std::path::Path) -> Arc<Mutex<()>> {
    let key = lock_key(path);
    let mut guard = lock_map(locks);
    Arc::clone(guard.entry(key).or_default())
}

fn worker_loop(
    op_rx: Receiver<(TaskId, CancelToken, TaskOp)>,
    ev_tx: Sender<TaskEvent>,
    locks: PathLocks,
) {
    while let Ok((task, cancel, op)) = op_rx.recv() {
        // 同路径的写操作串行执行。先取锁再进 run_task，
        // 保证「打包 / 重写 / 解压到同一目录」不会交错。
        let lock = op.exclusive_path().map(|path| archive_lock(&locks, path));
        let _guard = lock
            .as_ref()
            .map(|lock| lock.lock().unwrap_or_else(|err| err.into_inner()));

        let tx = ev_tx.clone();
        let result = worker::guarded(|| run_task(task, &cancel, op, &tx));
        if let Err(message) = result {
            let _ = ev_tx.send(TaskEvent::Failed {
                task,
                error: FailReason::Panic { message },
            });
        }
    }
}

fn run_task(
    task: TaskId,
    cancel: &CancelToken,
    op: TaskOp,
    tx: &Sender<TaskEvent>,
) -> Result<(), ()> {
    match op {
        TaskOp::Open { path, password } => {
            send(
                tx,
                TaskEvent::PhaseChanged {
                    task,
                    phase: TaskPhase::Planning,
                },
            );
            match backend::open(&path, password.as_ref()) {
                Ok(reader) => {
                    let kind = reader.kind();
                    let entries: Arc<[ArchiveEntry]> =
                        Arc::from(reader.entries().to_vec().into_boxed_slice());
                    let reader = Arc::new(Mutex::new(reader));
                    send(
                        tx,
                        TaskEvent::Opened {
                            task,
                            path,
                            kind,
                            entries,
                            reader,
                        },
                    );
                    send(
                        tx,
                        TaskEvent::PhaseChanged {
                            task,
                            phase: TaskPhase::Done,
                        },
                    );
                }
                Err(ArchiveError::PasswordRequired) => {
                    send(tx, TaskEvent::NeedPassword { task, path });
                }
                Err(err) => {
                    send(
                        tx,
                        TaskEvent::Failed {
                            task,
                            error: err.reason(),
                        },
                    );
                }
            }
        }
        TaskOp::Plan {
            reader,
            ids,
            target,
            limits,
        } => {
            send(
                tx,
                TaskEvent::PhaseChanged {
                    task,
                    phase: TaskPhase::Planning,
                },
            );
            let plan = {
                let guard = lock_reader(&reader);
                guard.plan(&ids, &target, &limits)
            };
            match plan {
                Ok(plan) => {
                    send(
                        tx,
                        TaskEvent::PlanReady {
                            task,
                            plan: Arc::new(plan),
                            target,
                            reader,
                        },
                    );
                    send(
                        tx,
                        TaskEvent::PhaseChanged {
                            task,
                            phase: TaskPhase::Done,
                        },
                    );
                }
                Err(err) => {
                    send(
                        tx,
                        TaskEvent::Failed {
                            task,
                            error: err.reason(),
                        },
                    );
                }
            }
        }
        TaskOp::Extract {
            reader,
            plan,
            decisions,
            limits,
        } => {
            let total = plan.total_bytes;
            send(
                tx,
                TaskEvent::Started {
                    task,
                    total_bytes: total,
                    total_entries: plan.items.len() as u32,
                },
            );
            send(
                tx,
                TaskEvent::PhaseChanged {
                    task,
                    phase: TaskPhase::Running,
                },
            );

            let sink =
                ExtractDirSink::with_control(&plan, &decisions, limits, Some(cancel.clone()));
            let mut sink = ProgressSink::new(sink, tx.clone(), task);
            let mut guard = lock_reader(&reader);
            match guard.extract(&plan, &decisions, cancel, &mut sink) {
                Ok(report) => {
                    send(
                        tx,
                        TaskEvent::Finished {
                            task,
                            report: Arc::new(report),
                        },
                    );
                    send(
                        tx,
                        TaskEvent::PhaseChanged {
                            task,
                            phase: TaskPhase::Done,
                        },
                    );
                }
                Err(err) => {
                    send(
                        tx,
                        TaskEvent::Failed {
                            task,
                            error: err.reason(),
                        },
                    );
                }
            }
        }
        TaskOp::Test { reader, limits } => {
            let total = {
                let guard = lock_reader(&reader);
                guard
                    .entries()
                    .iter()
                    .filter(|entry| !entry.kind.is_dir())
                    .map(|entry| entry.size)
                    .sum()
            };
            send(
                tx,
                TaskEvent::Started {
                    task,
                    total_bytes: total,
                    total_entries: 0,
                },
            );
            send(
                tx,
                TaskEvent::PhaseChanged {
                    task,
                    phase: TaskPhase::Running,
                },
            );

            let sink = VerifySink::with_limits(limits);
            let mut sink = ProgressSink::new(sink, tx.clone(), task);
            let mut guard = lock_reader(&reader);
            match guard.test(&limits, cancel, &mut sink) {
                Ok(report) => {
                    send(
                        tx,
                        TaskEvent::Finished {
                            task,
                            report: Arc::new(report),
                        },
                    );
                    send(
                        tx,
                        TaskEvent::PhaseChanged {
                            task,
                            phase: TaskPhase::Done,
                        },
                    );
                }
                Err(err) => {
                    send(
                        tx,
                        TaskEvent::Failed {
                            task,
                            error: err.reason(),
                        },
                    );
                }
            }
        }
        TaskOp::Preview {
            reader,
            id,
            limits,
            max_bytes,
        } => {
            send(
                tx,
                TaskEvent::PhaseChanged {
                    task,
                    phase: TaskPhase::Running,
                },
            );
            let mut guard = lock_reader(&reader);
            let Some(entry) = guard.entries().get(id as usize).cloned() else {
                send(
                    tx,
                    TaskEvent::Failed {
                        task,
                        error: FailReason::EntryMissing { id },
                    },
                );
                return Ok(());
            };
            if entry.kind.is_dir() {
                send(
                    tx,
                    TaskEvent::PreviewReady {
                        task,
                        id,
                        data: Arc::<[u8]>::from(Vec::new()),
                        truncated: false,
                        failed: None,
                    },
                );
                return Ok(());
            }

            let target = std::env::temp_dir();
            let plan = match guard.plan(&[id], &target, &limits) {
                Ok(plan) => plan,
                Err(err) => {
                    send(
                        tx,
                        TaskEvent::Failed {
                            task,
                            error: err.reason(),
                        },
                    );
                    return Ok(());
                }
            };
            let decisions = DecisionTable {
                default: OverwritePolicy::Skip,
                ..Default::default()
            };
            let mut sink = MemorySink::with_limits(max_bytes, limits);
            let report = match guard.extract(&plan, &decisions, cancel, &mut sink) {
                Ok(report) => report,
                Err(err) => {
                    send(
                        tx,
                        TaskEvent::Failed {
                            task,
                            error: err.reason(),
                        },
                    );
                    return Ok(());
                }
            };
            let (data, truncated, ok) = sink.into_parts();
            let failed = if ok {
                report.failed.first().map(|(_, reason)| reason.clone())
            } else {
                Some(FailReason::ChecksumFailed {
                    format: "archive".to_owned(),
                })
            };
            send(
                tx,
                TaskEvent::PreviewReady {
                    task,
                    id,
                    data: Arc::<[u8]>::from(data),
                    truncated,
                    failed,
                },
            );
            send(
                tx,
                TaskEvent::PhaseChanged {
                    task,
                    phase: TaskPhase::Done,
                },
            );
        }
        TaskOp::Pack {
            output,
            inputs,
            kind,
            method,
            append,
            password,
            sevenz,
            comment,
            volume_size,
            limits,
        } => {
            if append && password.is_some() {
                send(
                    tx,
                    TaskEvent::Failed {
                        task,
                        error: FailReason::AppendPasswordUnsupported,
                    },
                );
                return Ok(());
            }
            let entries = match scan_inputs(&inputs, &limits) {
                Ok(entries) => entries,
                Err(err) => {
                    send(
                        tx,
                        TaskEvent::Failed {
                            task,
                            error: err.reason(),
                        },
                    );
                    return Ok(());
                }
            };
            let total: u64 = entries.iter().map(|entry| entry.size).sum();
            send(
                tx,
                TaskEvent::Started {
                    task,
                    total_bytes: total,
                    total_entries: entries.len() as u32,
                },
            );
            send(
                tx,
                TaskEvent::PhaseChanged {
                    task,
                    phase: TaskPhase::Running,
                },
            );

            let created = if append {
                backend::create_writer_append(&output, kind, method, sevenz, limits, cancel)
            } else {
                backend::create_writer_with_options(
                    &output,
                    kind,
                    method,
                    password.as_ref(),
                    sevenz,
                )
            };
            let mut writer = match created {
                Ok(writer) => writer,
                Err(err) => {
                    send(
                        tx,
                        TaskEvent::Failed {
                            task,
                            error: err.reason(),
                        },
                    );
                    return Ok(());
                }
            };

            if kind == ArchiveKind::Zip
                && !append
                && let Some(text) = comment.as_deref()
                && let Err(err) = writer.set_comment(text)
            {
                send(
                    tx,
                    TaskEvent::Failed {
                        task,
                        error: err.reason(),
                    },
                );
                return Ok(());
            }

            let progress_state = std::cell::RefCell::new((Instant::now(), 0u64));
            let progress_tx = tx.clone();
            let progress = |done: u64| {
                let mut state = progress_state.borrow_mut();
                let (last_time, last_bytes) = *state;
                if last_time.elapsed().as_millis() >= 100
                    || done.saturating_sub(last_bytes) >= 1_048_576
                {
                    *state = (Instant::now(), done);
                    let _ = progress_tx.send(TaskEvent::Progress {
                        task,
                        processed_compressed: 0,
                        written: done,
                        entry: None,
                    });
                }
            };

            match write_entries(writer.as_mut(), &entries, Some(method), cancel, &progress) {
                Ok(()) => match writer.finish() {
                    Ok(report) => {
                        if let Some(part_size) = volume_size
                            && let Err(err) =
                                crate::backend::volume::split_archive(&output, part_size, true)
                        {
                            send(
                                tx,
                                TaskEvent::Failed {
                                    task,
                                    error: err.reason(),
                                },
                            );
                            return Ok(());
                        }
                        send(
                            tx,
                            TaskEvent::PackFinished {
                                task,
                                report: Arc::new(report),
                                append,
                            },
                        );
                        send(
                            tx,
                            TaskEvent::PhaseChanged {
                                task,
                                phase: TaskPhase::Done,
                            },
                        );
                    }
                    Err(err) => {
                        send(
                            tx,
                            TaskEvent::Failed {
                                task,
                                error: err.reason(),
                            },
                        );
                    }
                },
                Err(err) => {
                    send(
                        tx,
                        TaskEvent::Failed {
                            task,
                            error: err.reason(),
                        },
                    );
                }
            }
        }
        TaskOp::Rewrite {
            path,
            kind,
            remove,
            rename,
            limits,
        } => {
            send(
                tx,
                TaskEvent::PhaseChanged {
                    task,
                    phase: TaskPhase::Running,
                },
            );
            let remove: HashSet<EntryId> = remove.into_iter().collect();
            let rename: HashMap<EntryId, String> = rename.into_iter().collect();
            let progress_state = std::cell::RefCell::new((Instant::now(), 0u64));
            let progress_tx = tx.clone();
            let progress = |done: u64| {
                let mut state = progress_state.borrow_mut();
                let (last_time, last_bytes) = *state;
                if last_time.elapsed().as_millis() >= 100
                    || done.saturating_sub(last_bytes) >= 1_048_576
                {
                    *state = (Instant::now(), done);
                    let _ = progress_tx.send(TaskEvent::Progress {
                        task,
                        processed_compressed: 0,
                        written: done,
                        entry: None,
                    });
                }
            };
            match crate::backend::rewrite::rewrite_with_names(
                &path, kind, &remove, &rename, limits, cancel, &progress,
            ) {
                Ok(report) => {
                    send(
                        tx,
                        TaskEvent::Rewritten {
                            task,
                            path,
                            report: Arc::new(report),
                        },
                    );
                    send(
                        tx,
                        TaskEvent::PhaseChanged {
                            task,
                            phase: TaskPhase::Done,
                        },
                    );
                }
                Err(err) => {
                    send(
                        tx,
                        TaskEvent::Failed {
                            task,
                            error: err.reason(),
                        },
                    );
                }
            }
        }
        TaskOp::InstallTheme {
            archive,
            target,
            limits,
        } => {
            send(
                tx,
                TaskEvent::PhaseChanged {
                    task,
                    phase: TaskPhase::Planning,
                },
            );
            let mut reader = match backend::open(&archive, None) {
                Ok(reader) => reader,
                Err(err) => {
                    send(
                        tx,
                        TaskEvent::Failed {
                            task,
                            error: err.reason(),
                        },
                    );
                    return Ok(());
                }
            };

            let has_manifest = reader.entries().iter().any(|entry| {
                let name = entry.display.trim_start_matches("./").replace('\\', "/");
                let parts: Vec<&str> = name.split('/').filter(|part| !part.is_empty()).collect();
                parts.len() == 2 && parts[1] == "theme.toml"
            });
            if !has_manifest {
                send(
                    tx,
                    TaskEvent::Failed {
                        task,
                        error: FailReason::ThemeManifestMissing,
                    },
                );
                return Ok(());
            }

            if let Err(error) = validate_theme_entries(reader.entries()) {
                send(tx, TaskEvent::Failed { task, error });
                return Ok(());
            }

            let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
            let plan = match reader.plan(&ids, &target, &limits) {
                Ok(plan) => plan,
                Err(err) => {
                    send(
                        tx,
                        TaskEvent::Failed {
                            task,
                            error: err.reason(),
                        },
                    );
                    return Ok(());
                }
            };

            send(
                tx,
                TaskEvent::Started {
                    task,
                    total_bytes: plan.total_bytes,
                    total_entries: plan.items.len() as u32,
                },
            );
            send(
                tx,
                TaskEvent::PhaseChanged {
                    task,
                    phase: TaskPhase::Running,
                },
            );

            let decisions = DecisionTable {
                default: OverwritePolicy::Skip,
                ..Default::default()
            };
            let sink =
                ExtractDirSink::with_control(&plan, &decisions, limits, Some(cancel.clone()));
            let mut sink = ProgressSink::new(sink, tx.clone(), task);
            match reader.extract(&plan, &decisions, cancel, &mut sink) {
                Ok(report) => {
                    send(
                        tx,
                        TaskEvent::ThemeInstalled {
                            task,
                            report: Arc::new(report),
                            source: archive,
                        },
                    );
                    send(
                        tx,
                        TaskEvent::PhaseChanged {
                            task,
                            phase: TaskPhase::Done,
                        },
                    );
                }
                Err(err) => {
                    send(
                        tx,
                        TaskEvent::Failed {
                            task,
                            error: err.reason(),
                        },
                    );
                }
            }
        }
    }
    Ok(())
}

/// 主题包安全限制：总数 / 单文件 / 解压总量 / 禁止可执行扩展名。
fn validate_theme_entries(entries: &[ArchiveEntry]) -> Result<(), FailReason> {
    const MAX_FILES: usize = 500;
    const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
    const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
    const FORBIDDEN: &[&str] = &[
        "exe", "dll", "bat", "cmd", "com", "msi", "ps1", "scr", "vbs", "js", "jse", "wsf", "lnk",
    ];

    if entries.len() > MAX_FILES {
        return Err(FailReason::ThemeTooManyEntries {
            count: entries.len(),
            limit: MAX_FILES,
        });
    }

    let mut total = 0u64;
    for entry in entries {
        if entry.kind.is_dir() {
            continue;
        }
        if entry.size > MAX_FILE_BYTES {
            return Err(FailReason::ThemeFileTooLarge {
                name: entry.display.to_string(),
                size: entry.size,
            });
        }
        total = total.saturating_add(entry.size);
        if total > MAX_TOTAL_BYTES {
            return Err(FailReason::ThemeTotalTooLarge);
        }
        let name = entry.display.to_ascii_lowercase();
        if let Some(ext) = name.rsplit('.').next()
            && name.contains('.')
            && FORBIDDEN.contains(&ext)
        {
            return Err(FailReason::ThemeForbiddenFile {
                name: entry.display.to_string(),
            });
        }
    }
    Ok(())
}

fn lock_reader(reader: &SharedReader) -> std::sync::MutexGuard<'_, Box<dyn ArchiveReader>> {
    // worker 被 catch_unwind 兜住后互斥量可能中毒；归档对象本身仍可用。
    reader
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn send(tx: &Sender<TaskEvent>, event: TaskEvent) {
    let _ = tx.send(event);
}

/// 把底层 sink 包一层，按时间 / 字节节流发出 `Progress` 事件。
pub struct ProgressSink<S> {
    inner: S,
    tx: Sender<TaskEvent>,
    task: TaskId,
    written: u64,
    last_emit: Instant,
    last_bytes: u64,
    entry: Option<EntryId>,
}

impl<S: ExtractSink> ProgressSink<S> {
    pub fn new(inner: S, tx: Sender<TaskEvent>, task: TaskId) -> Self {
        Self {
            inner,
            tx,
            task,
            written: 0,
            last_emit: Instant::now(),
            last_bytes: 0,
            entry: None,
        }
    }

    fn emit(&mut self, force: bool) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_emit).as_millis();
        if force || elapsed >= 100 || self.written.saturating_sub(self.last_bytes) >= 1_048_576 {
            self.last_emit = now;
            self.last_bytes = self.written;
            let _ = self.tx.send(TaskEvent::Progress {
                task: self.task,
                processed_compressed: 0,
                written: self.written,
                entry: self.entry,
            });
        }
    }
}

impl<S: ExtractSink> ExtractSink for ProgressSink<S> {
    fn on_entry_begin(
        &mut self,
        id: EntryId,
        entry: &ArchiveEntry,
    ) -> Result<(), crate::error::SinkError> {
        self.entry = Some(id);
        self.inner.on_entry_begin(id, entry)?;
        self.emit(false);
        Ok(())
    }

    fn on_data(&mut self, id: EntryId, chunk: &[u8]) -> Result<(), crate::error::SinkError> {
        self.inner.on_data(id, chunk)?;
        self.written = self.written.saturating_add(chunk.len() as u64);
        self.emit(false);
        Ok(())
    }

    fn on_entry_end(&mut self, id: EntryId, ok: bool) -> Result<(), crate::error::SinkError> {
        let result = self.inner.on_entry_end(id, ok);
        self.emit(true);
        result
    }

    fn bytes_written(&self) -> u64 {
        self.inner.bytes_written()
    }

    fn take_crc_mismatch(&mut self) -> Vec<EntryId> {
        self.inner.take_crc_mismatch()
    }

    fn finish(&mut self) -> Result<(), crate::error::SinkError> {
        self.inner.finish()
    }
}
