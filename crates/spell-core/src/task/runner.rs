use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::backend;
use crate::error::{ArchiveError, FailReason};
use crate::limits::Limits;
use crate::plan::{DecisionTable, ExtractPlan, OverwritePolicy};
use crate::reader::{ArchiveReader, ExtractSink};
use crate::safety::{ExtractDirSink, VerifySink};
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
    Pack {
        output: PathBuf,
        inputs: Vec<PathBuf>,
        kind: ArchiveKind,
        method: Method,
        append: bool,
        password: Option<SecretString>,
        sevenz: SevenZOptions,
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

/// `submit` 返回的句柄：UI 用它显示任务并按需取消。
#[derive(Clone)]
pub struct TaskHandle {
    pub id: TaskId,
    pub cancel: CancelToken,
}

/// 固定数量后台线程 + 双向通道。事件由 UI 侧 `try_recv` 批量取走。
pub struct TaskRunner {
    op_tx: Sender<(TaskId, CancelToken, TaskOp)>,
    ev_rx: Receiver<TaskEvent>,
    next_id: TaskId,
}

impl TaskRunner {
    pub fn new(workers: usize) -> Self {
        let (op_tx, op_rx) = unbounded::<(TaskId, CancelToken, TaskOp)>();
        let (ev_tx, ev_rx) = unbounded::<TaskEvent>();

        for index in 0..workers.max(1) {
            let op_rx = op_rx.clone();
            let ev_tx = ev_tx.clone();
            let _ = std::thread::Builder::new()
                .name(format!("spell-worker-{index}"))
                .spawn(move || worker_loop(op_rx, ev_tx));
        }

        Self {
            op_tx,
            ev_rx,
            next_id: 1,
        }
    }

    pub fn submit(&mut self, op: TaskOp) -> TaskHandle {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let cancel = CancelToken::new();
        let _ = self.op_tx.send((id, cancel.clone(), op));
        TaskHandle { id, cancel }
    }

    /// 非阻塞取一条事件。UI 每帧循环调用直到返回 `None`。
    pub fn try_recv(&self) -> Option<TaskEvent> {
        self.ev_rx.try_recv().ok()
    }
}

fn worker_loop(op_rx: Receiver<(TaskId, CancelToken, TaskOp)>, ev_tx: Sender<TaskEvent>) {
    while let Ok((task, cancel, op)) = op_rx.recv() {
        let tx = ev_tx.clone();
        let result = worker::guarded(|| run_task(task, &cancel, op, &tx));
        if let Err(message) = result {
            let _ = ev_tx.send(TaskEvent::Failed {
                task,
                error: FailReason::Panic {
                    message,
                },
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

            let sink = ExtractDirSink::new(&plan, &decisions, limits);
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
            match guard.test(cancel, &mut sink) {
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
        TaskOp::Pack {
            output,
            inputs,
            kind,
            method,
            append,
            password,
            sevenz,
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
            let limits = Limits::default();
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
                backend::create_writer_append(&output, kind, method)
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
            let sink = ExtractDirSink::new(&plan, &decisions, limits);
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
}
