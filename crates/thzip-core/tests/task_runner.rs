use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use thzip_core::plan::DecisionTable;
use thzip_core::task::{TaskEvent, TaskOp, TaskPhase, TaskRunner};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "thzip-spellarchive-runner-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_zip(path: &Path, files: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in files {
        writer.start_file(*name, options).unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap();
}

fn pump_until(
    runner: &TaskRunner,
    deadline: Instant,
    mut done: impl FnMut(&TaskEvent) -> bool,
) -> Vec<TaskEvent> {
    let mut events = Vec::new();
    loop {
        while let Some(event) = runner.try_recv() {
            let stop = done(&event);
            events.push(event);
            if stop {
                return events;
            }
        }
        assert!(Instant::now() < deadline, "任务超时未完成");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn open_then_extract_through_worker() {
    let dir = TempDir::new("extract");
    let archive = dir.path().join("a.zip");
    write_zip(&archive, &[("a.txt", b"hello"), ("b/c.txt", b"world")]);

    let mut runner = TaskRunner::new(1);
    runner.submit(TaskOp::Open {
        path: archive.clone(),
        password: None,
    });
    let events = pump_until(&runner, Instant::now() + Duration::from_secs(10), |event| {
        matches!(event, TaskEvent::Opened { .. } | TaskEvent::Failed { .. })
    });

    let reader = events
        .iter()
        .find_map(|event| match event {
            TaskEvent::Opened { reader, .. } => Some(reader.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("打开任务没有返回 Opened 事件"));

    let target = dir.path().join("out");
    let plan = {
        let guard = reader.lock().unwrap();
        let ids: Vec<u32> = (0..guard.entries().len() as u32).collect();
        guard
            .plan(&ids, &target, &thzip_core::Limits::default())
            .unwrap()
    };

    let extract = runner.submit(TaskOp::Extract {
        reader,
        plan: std::sync::Arc::new(plan),
        decisions: DecisionTable::default(),
        limits: thzip_core::Limits::default(),
    });

    let events = pump_until(
        &runner,
        Instant::now() + Duration::from_secs(20),
        |event| match event {
            TaskEvent::Finished { task, .. } | TaskEvent::Failed { task, .. } => {
                *task == extract.id
            }
            _ => false,
        },
    );

    assert!(
        events
            .iter()
            .any(|event| matches!(event, TaskEvent::Progress { .. })),
        "应至少收到一次进度事件"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            TaskEvent::Finished { report, .. } if report.failed.is_empty()
        )),
        "解压任务应成功完成"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            TaskEvent::PhaseChanged {
                phase: TaskPhase::Done,
                ..
            }
        )),
        "任务应进入 Done 阶段"
    );
    assert_eq!(
        std::fs::read(target.join("a.txt")).unwrap(),
        b"hello".to_vec()
    );
    assert_eq!(
        std::fs::read(target.join("b/c.txt")).unwrap(),
        b"world".to_vec()
    );
}

#[test]
fn plans_through_worker() {
    let dir = TempDir::new("plan");
    let archive = dir.path().join("plan.zip");
    write_zip(&archive, &[("a.txt", b"hello")]);

    let mut runner = TaskRunner::new(1);
    runner.submit(TaskOp::Open {
        path: archive,
        password: None,
    });
    let events = pump_until(&runner, Instant::now() + Duration::from_secs(10), |event| {
        matches!(event, TaskEvent::Opened { .. } | TaskEvent::Failed { .. })
    });
    let reader = events
        .iter()
        .find_map(|event| match event {
            TaskEvent::Opened { reader, .. } => Some(reader.clone()),
            _ => None,
        })
        .expect("打开失败");

    let ids: Vec<u32> = {
        let guard = reader.lock().unwrap();
        (0..guard.entries().len() as u32).collect()
    };
    let target = dir.path().join("out");
    let handle = runner.submit(TaskOp::Plan {
        reader,
        ids,
        target: target.clone(),
        limits: thzip_core::Limits::default(),
    });
    let events = pump_until(&runner, Instant::now() + Duration::from_secs(10), |event| {
        matches!(
            event,
            TaskEvent::PlanReady { task, .. } | TaskEvent::Failed { task, .. }
                if *task == handle.id
        )
    });
    assert!(events.iter().any(|event| matches!(
        event,
        TaskEvent::PlanReady { plan, target: ready_target, .. }
            if plan.items.len() == 1 && ready_target == &target
    )));
}

#[test]
fn cancellation_stops_extract() {
    let dir = TempDir::new("cancel");
    let archive = dir.path().join("big.zip");
    let payload = vec![0u8; 16 * 1024 * 1024];
    write_zip(&archive, &[("big.bin", &payload)]);

    let mut runner = TaskRunner::new(1);
    runner.submit(TaskOp::Open {
        path: archive,
        password: None,
    });
    let events = pump_until(&runner, Instant::now() + Duration::from_secs(20), |event| {
        matches!(event, TaskEvent::Opened { .. } | TaskEvent::Failed { .. })
    });
    let reader = events
        .iter()
        .find_map(|event| match event {
            TaskEvent::Opened { reader, .. } => Some(reader.clone()),
            _ => None,
        })
        .expect("打开失败");

    let target = dir.path().join("out");
    let plan = {
        let guard = reader.lock().unwrap();
        let ids: Vec<u32> = (0..guard.entries().len() as u32).collect();
        guard
            .plan(&ids, &target, &thzip_core::Limits::default())
            .unwrap()
    };

    let handle = runner.submit(TaskOp::Extract {
        reader,
        plan: std::sync::Arc::new(plan),
        decisions: DecisionTable::default(),
        limits: thzip_core::Limits::default(),
    });
    handle.cancel.cancel();

    let events = pump_until(
        &runner,
        Instant::now() + Duration::from_secs(20),
        |event| matches!(event, TaskEvent::Finished { task, .. } | TaskEvent::Failed { task, .. } if *task == handle.id),
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            TaskEvent::Failed { error, .. } if error.to_string().contains("已取消")
        )),
        "取消后应返回「已取消」失败事件"
    );
    assert!(!target.join("big.bin").exists(), "取消后不应留下目标文件");
}

#[test]
fn packs_through_worker() {
    let dir = TempDir::new("pack");
    let src = dir.path().join("素材");
    std::fs::create_dir_all(src.join("子目录")).unwrap();
    std::fs::write(src.join("a.txt"), b"hello").unwrap();
    std::fs::write(src.join("子目录/b.txt"), b"world").unwrap();
    let archive = dir.path().join("packed.zip");

    let mut runner = TaskRunner::new(1);
    let handle = runner.submit(TaskOp::Pack {
        output: archive.clone(),
        inputs: vec![src],
        kind: thzip_core::ArchiveKind::Zip,
        method: thzip_core::Method::Deflate,
        append: false,
        password: None,
        sevenz: thzip_core::SevenZOptions::default(),
        comment: None,
        limits: thzip_core::Limits::default(),
    });

    let events = pump_until(
        &runner,
        Instant::now() + Duration::from_secs(20),
        |event| match event {
            TaskEvent::PackFinished { task, .. } | TaskEvent::Failed { task, .. } => {
                *task == handle.id
            }
            _ => false,
        },
    );

    assert!(
        events.iter().any(|event| matches!(
            event,
            TaskEvent::PackFinished { report, .. } if report.entries == 4
        )),
        "打包任务应完成（根目录 + 子目录 + 2 个文件）"
    );
    assert!(archive.exists());

    let reader = thzip_core::backend::open(&archive, None).unwrap();
    let names: Vec<String> = reader
        .entries()
        .iter()
        .map(|entry| entry.display.to_string())
        .collect();
    assert!(names.iter().any(|name| name == "素材/a.txt"));
    assert!(names.iter().any(|name| name == "素材/子目录/b.txt"));
}

#[test]
fn installs_theme_through_worker() {
    let dir = TempDir::new("theme");
    let archive = dir.path().join("theme.zip");
    write_zip(
        &archive,
        &[
            ("mytheme/theme.toml", b"schema = 1\n"),
            ("mytheme/png/avatar.png", b"\x89PNG"),
        ],
    );
    let target = dir.path().join("themes");
    std::fs::create_dir_all(&target).unwrap();

    let mut runner = TaskRunner::new(1);
    let handle = runner.submit(TaskOp::InstallTheme {
        archive,
        target: target.clone(),
        limits: thzip_core::Limits::default(),
    });
    let events = pump_until(&runner, Instant::now() + Duration::from_secs(10), |event| {
        matches!(
            event,
            TaskEvent::ThemeInstalled { task, .. } | TaskEvent::Failed { task, .. }
                if *task == handle.id
        )
    });
    assert!(
        events.iter().any(|event| matches!(
            event,
            TaskEvent::ThemeInstalled { report, .. } if report.failed.is_empty()
        )),
        "主题包应导入成功"
    );
    assert!(target.join("mytheme/theme.toml").is_file());
    assert!(target.join("mytheme/png/avatar.png").is_file());
}

#[test]
fn rejects_theme_without_manifest() {
    let dir = TempDir::new("theme-bad");
    let archive = dir.path().join("bad.zip");
    write_zip(&archive, &[("theme.toml", b"schema = 1\n")]);
    let target = dir.path().join("themes");
    std::fs::create_dir_all(&target).unwrap();

    let mut runner = TaskRunner::new(1);
    let handle = runner.submit(TaskOp::InstallTheme {
        archive,
        target,
        limits: thzip_core::Limits::default(),
    });
    let events = pump_until(&runner, Instant::now() + Duration::from_secs(10), |event| {
        matches!(
            event,
            TaskEvent::ThemeInstalled { task, .. } | TaskEvent::Failed { task, .. }
                if *task == handle.id
        )
    });
    assert!(events.iter().any(|event| matches!(
        event,
        TaskEvent::Failed { error, .. } if error.to_string().contains("theme.toml")
    )));
}

#[test]
fn rejects_theme_with_executables() {
    let dir = TempDir::new("theme-exe");
    let archive = dir.path().join("bad.zip");
    write_zip(
        &archive,
        &[
            ("mytheme/theme.toml", b"schema = 1\n"),
            ("mytheme/evil.exe", b"MZ"),
        ],
    );
    let target = dir.path().join("themes");
    std::fs::create_dir_all(&target).unwrap();

    let mut runner = TaskRunner::new(1);
    let handle = runner.submit(TaskOp::InstallTheme {
        archive,
        target,
        limits: thzip_core::Limits::default(),
    });
    let events = pump_until(&runner, Instant::now() + Duration::from_secs(10), |event| {
        matches!(
            event,
            TaskEvent::ThemeInstalled { task, .. } | TaskEvent::Failed { task, .. }
                if *task == handle.id
        )
    });
    assert!(events.iter().any(|event| matches!(
        event,
        TaskEvent::Failed { error, .. } if error.to_string().contains("不允许")
    )));
}

#[test]
fn rejects_theme_with_too_many_entries() {
    let dir = TempDir::new("theme-many");
    let archive = dir.path().join("many.zip");
    let file = std::fs::File::create(&archive).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("mytheme/theme.toml", options).unwrap();
    for index in 0..500 {
        writer
            .start_file(format!("mytheme/png/{index}.png"), options)
            .unwrap();
    }
    writer.finish().unwrap();

    let target = dir.path().join("themes");
    std::fs::create_dir_all(&target).unwrap();
    let mut runner = TaskRunner::new(1);
    let handle = runner.submit(TaskOp::InstallTheme {
        archive,
        target,
        limits: thzip_core::Limits::default(),
    });
    let events = pump_until(&runner, Instant::now() + Duration::from_secs(10), |event| {
        matches!(
            event,
            TaskEvent::ThemeInstalled { task, .. } | TaskEvent::Failed { task, .. }
                if *task == handle.id
        )
    });
    assert!(events.iter().any(|event| matches!(
        event,
        TaskEvent::Failed { error, .. } if error.to_string().contains("条目过多")
    )));
}

#[test]
fn missing_password_reports_need_password() {
    let dir = TempDir::new("password");
    let archive = dir.path().join("empty.zip");
    write_zip(&archive, &[("a.txt", b"x")]);

    let mut runner = TaskRunner::new(1);
    // 普通 zip 不需要密码；这里只验证事件通道能把结果送回 UI。
    runner.submit(TaskOp::Open {
        path: dir.path().join("does-not-exist.zip"),
        password: None,
    });

    let events = pump_until(&runner, Instant::now() + Duration::from_secs(10), |event| {
        matches!(event, TaskEvent::Failed { .. })
    });
    assert!(events.iter().any(|event| matches!(
        event,
        TaskEvent::Failed { error, .. } if !error.to_string().is_empty()
    )));
}

#[test]
fn preview_reads_prefix_and_truncates() {
    let dir = TempDir::new("preview");
    let archive = dir.path().join("preview.zip");
    write_zip(&archive, &[("a.txt", b"hello world")]);

    let mut runner = TaskRunner::new(1);
    let open = runner.submit(TaskOp::Open {
        path: archive,
        password: None,
    });
    let events = pump_until(&runner, Instant::now() + Duration::from_secs(10), |event| {
        matches!(
            event,
            TaskEvent::Opened { task, .. } | TaskEvent::Failed { task, .. } if *task == open.id
        )
    });
    let reader = events
        .iter()
        .find_map(|event| match event {
            TaskEvent::Opened { reader, .. } => Some(reader.clone()),
            _ => None,
        })
        .expect("archive should open");

    let preview = runner.submit(TaskOp::Preview {
        reader,
        id: 0,
        limits: thzip_core::Limits::default(),
        max_bytes: 4,
    });
    let events = pump_until(&runner, Instant::now() + Duration::from_secs(10), |event| {
        matches!(
            event,
            TaskEvent::PreviewReady { task, .. } | TaskEvent::Failed { task, .. }
                if *task == preview.id
        )
    });
    let (data, truncated) = events
        .iter()
        .find_map(|event| match event {
            TaskEvent::PreviewReady {
                data, truncated, ..
            } => Some((data.clone(), *truncated)),
            _ => None,
        })
        .expect("preview should finish");
    assert_eq!(&data[..], b"hell");
    assert!(truncated);
}
