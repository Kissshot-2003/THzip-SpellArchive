//! 长路径回归：验证超过 MAX_PATH(260) 的深层路径确实能写盘并读回。
//!
//! 关键在于**不依赖**注册表里的 `LongPathsEnabled` 开关：实现给超过 240 字符的
//! 路径统一补 `\\?\` 前缀（`critical/wide_path`），这条路径在默认配置的 Windows
//! 上就必须通过。若哪天有人把前缀去掉，这里会立刻红——而 std 自己会偷偷加前缀，
//! 所以只测 std 是测不出问题的，必须走真正的解压写盘链路。

use std::path::PathBuf;

use thzip_core::limits::Limits;
use thzip_core::plan::{DecisionTable, ExtractPlan, OverwritePolicy, PlanAction, PlannedItem};
use thzip_core::reader::ExtractSink;
use thzip_core::safety::ExtractDirSink;
use thzip_core::types::{ArchiveEntry, EntryKind};

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "thzip-long-path-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn limits() -> Limits {
    Limits {
        max_path_len: 32_767,
        max_path_depth: 64,
        ..Default::default()
    }
}

/// 目标路径总长超过 300 个字符时仍然能落盘、能读回。
#[test]
fn writes_and_reads_beyond_max_path() {
    let root = scratch("deep");
    let segment = "d".repeat(40);
    let target = (0..8).fold(root.clone(), |mut path, _| {
        path.push(&segment);
        path
    });
    assert!(
        target.as_os_str().len() > 300,
        "构造出的路径不够长: {}",
        target.as_os_str().len()
    );

    let limits = limits();
    let mut plan = ExtractPlan {
        root: root.clone(),
        limits,
        ..Default::default()
    };
    plan.items.push(PlannedItem {
        id: 0,
        rel_path: target.clone(),
        kind: EntryKind::File,
        size: 4,
        action: PlanAction::Create,
    });

    let decisions = DecisionTable {
        default: OverwritePolicy::Overwrite,
        ..Default::default()
    };
    let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
    let mut entry = ArchiveEntry::new(0, b"deep.bin", "deep.bin", EntryKind::File);
    entry.crc32 = Some(crc32fast::hash(b"deep"));
    sink.on_entry_begin(0, &entry).unwrap();
    sink.on_data(0, b"deep").unwrap();
    sink.on_entry_end(0, true).unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), b"deep");
    let _ = std::fs::remove_dir_all(&root);
}

/// 长路径下覆盖已有文件：原子替换走的是 `ReplaceFileW`，同样要吃前缀。
#[test]
fn replaces_existing_file_beyond_max_path() {
    let root = scratch("replace");
    let dir = (0..6).fold(root.clone(), |mut path, _| {
        path.push("e".repeat(40));
        path
    });
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("payload.bin");
    std::fs::write(&target, b"old").unwrap();
    assert!(target.as_os_str().len() > 260);

    let limits = limits();
    let mut plan = ExtractPlan {
        root: root.clone(),
        limits,
        ..Default::default()
    };
    plan.items.push(PlannedItem {
        id: 0,
        rel_path: target.clone(),
        kind: EntryKind::File,
        size: 3,
        action: PlanAction::Overwrite,
    });
    let decisions = DecisionTable {
        default: OverwritePolicy::Overwrite,
        ..Default::default()
    };
    let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
    let mut entry = ArchiveEntry::new(0, b"payload.bin", "payload.bin", EntryKind::File);
    entry.crc32 = Some(crc32fast::hash(b"new"));
    sink.on_entry_begin(0, &entry).unwrap();
    sink.on_data(0, b"new").unwrap();
    sink.on_entry_end(0, true).unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), b"new");
    let _ = std::fs::remove_dir_all(&root);
}

/// 长路径下 CRC 不符仍然不得落盘（原子写 + 前缀两条路径叠加时不回退）。
#[test]
fn crc_mismatch_beyond_max_path_leaves_nothing() {
    let root = scratch("crc");
    let target = (0..7).fold(root.clone(), |mut path, _| {
        path.push("f".repeat(40));
        path
    });

    let limits = limits();
    let mut plan = ExtractPlan {
        root: root.clone(),
        limits,
        ..Default::default()
    };
    plan.items.push(PlannedItem {
        id: 0,
        rel_path: target.clone(),
        kind: EntryKind::File,
        size: 4,
        action: PlanAction::Create,
    });
    let decisions = DecisionTable {
        default: OverwritePolicy::Overwrite,
        ..Default::default()
    };
    let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
    let mut entry = ArchiveEntry::new(0, b"bad.bin", "bad.bin", EntryKind::File);
    entry.crc32 = Some(0xDEAD_BEEF);
    sink.on_entry_begin(0, &entry).unwrap();
    sink.on_data(0, b"deep").unwrap();
    assert!(sink.on_entry_end(0, true).is_err());
    assert!(!target.exists(), "CRC 不符的文件不得落盘");
    assert_eq!(sink.take_crc_mismatch(), vec![0]);

    let _ = std::fs::remove_dir_all(&root);
}
