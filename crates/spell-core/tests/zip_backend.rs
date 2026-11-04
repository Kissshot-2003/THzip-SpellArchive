use std::io::Write;
use std::path::{Path, PathBuf};

use spell_core::backend;
use spell_core::limits::{LimitProfile, Limits};
use spell_core::plan::{DecisionTable, OverwritePolicy, PlanAction};
use spell_core::safety::ExtractDirSink;
use spell_core::task::CancelToken;
use spell_core::types::EntryId;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "spellarchive-zip-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
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

fn write_zip(path: &Path, files: &[(&str, &[u8])], dirs: &[&str]) {
    let file = std::fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for dir in dirs {
        writer.add_directory(*dir, options).unwrap();
    }
    for (name, data) in files {
        writer.start_file(*name, options).unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap();
}

fn extract_all(
    archive: &Path,
    target: &Path,
    decisions: DecisionTable,
) -> spell_core::ExtractReport {
    let limits = Limits::default();
    let mut reader = backend::open(archive, None).unwrap();
    let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
    let plan = reader.plan(&ids, target, &limits).unwrap();
    let cancel = CancelToken::new();
    let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
    reader
        .extract(&plan, &decisions, &cancel, &mut sink)
        .unwrap()
}

#[test]
fn lists_and_extracts_roundtrip() {
    let dir = TempDir::new("roundtrip");
    let archive = dir.path().join("a.zip");
    write_zip(
        &archive,
        &[
            ("文档/说明.txt", "灵梦的符卡".as_bytes()),
            ("root.bin", &[0u8, 1, 2, 3, 255]),
            ("empty.txt", b""),
        ],
        &["空目录/"],
    );

    let reader = backend::open(&archive, None).unwrap();
    assert_eq!(reader.entries().len(), 4);

    let target = dir.path().join("out");
    let report = extract_all(&archive, &target, DecisionTable::default());
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(report.succeeded.len(), 4);
    assert_eq!(
        std::fs::read(target.join("文档/说明.txt")).unwrap(),
        "灵梦的符卡".as_bytes()
    );
    assert_eq!(
        std::fs::read(target.join("root.bin")).unwrap(),
        vec![0u8, 1, 2, 3, 255]
    );
    assert!(target.join("空目录").is_dir());
}

#[test]
fn rejects_parent_traversal() {
    let dir = TempDir::new("traversal");
    let archive = dir.path().join("evil.zip");
    write_zip(&archive, &[("../evil.txt", b"pwned")], &[]);

    let target = dir.path().join("out");
    let limits = Limits::default();
    let reader = backend::open(&archive, None).unwrap();
    let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
    let plan = reader.plan(&ids, &target, &limits).unwrap();

    assert!(matches!(plan.items[0].action, PlanAction::Reject(_)));
    assert!(!dir.path().join("evil.txt").exists());

    let report = extract_all(&archive, &target, DecisionTable::default());
    assert!(report.succeeded.is_empty());
    assert_eq!(report.failed.len(), 1);
    assert!(!dir.path().join("evil.txt").exists());
}

#[test]
fn rejects_absolute_and_ads_names() {
    let dir = TempDir::new("absolute");
    let archive = dir.path().join("abs.zip");
    write_zip(&archive, &[("C:/Windows/evil.dll", b"x")], &[]);

    let target = dir.path().join("out");
    let report = extract_all(&archive, &target, DecisionTable::default());
    assert!(report.succeeded.is_empty());
    assert_eq!(report.failed.len(), 1);
    assert!(!dir.path().join("out/C:/Windows/evil.dll").exists());
}

#[test]
fn case_collisions_are_renamed_not_lost() {
    let dir = TempDir::new("duplicate");
    let archive = dir.path().join("dup.zip");
    write_zip(
        &archive,
        &[("Same.txt", b"first"), ("same.txt", b"second")],
        &[],
    );

    let target = dir.path().join("out");
    let report = extract_all(&archive, &target, DecisionTable::default());
    assert_eq!(report.succeeded.len(), 2);
    assert_eq!(std::fs::read(target.join("Same.txt")).unwrap(), b"first");
    assert_eq!(std::fs::read(target.join("same~1.txt")).unwrap(), b"second");
}

#[test]
fn skip_policy_keeps_existing_file() {
    let dir = TempDir::new("skip");
    let archive = dir.path().join("conflict.zip");
    write_zip(&archive, &[("a.txt", b"new")], &[]);

    let target = dir.path().join("out");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("a.txt"), b"old").unwrap();

    let decisions = DecisionTable {
        default: OverwritePolicy::Skip,
        ..Default::default()
    };
    let report = extract_all(&archive, &target, decisions);
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(std::fs::read(target.join("a.txt")).unwrap(), b"old");
}

#[test]
fn overwrite_policy_replaces_existing_file() {
    let dir = TempDir::new("overwrite");
    let archive = dir.path().join("conflict.zip");
    write_zip(&archive, &[("a.txt", b"new")], &[]);

    let target = dir.path().join("out");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("a.txt"), b"old").unwrap();

    let decisions = DecisionTable {
        default: OverwritePolicy::Overwrite,
        ..Default::default()
    };
    let report = extract_all(&archive, &target, decisions);
    assert_eq!(report.succeeded.len(), 1);
    assert_eq!(std::fs::read(target.join("a.txt")).unwrap(), b"new");
}

#[test]
fn entry_limit_is_enforced() {
    let dir = TempDir::new("limit");
    let archive = dir.path().join("many.zip");
    write_zip(
        &archive,
        &[("a.txt", b"1"), ("b.txt", b"2"), ("c.txt", b"3")],
        &[],
    );

    let mut limits = LimitProfile::Strict.preset();
    limits.max_entries = 2;

    let reader = backend::open(&archive, None).unwrap();
    let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
    let result = reader.plan(&ids, &dir.path().join("out"), &limits);
    assert!(result.is_err());
}

#[test]
fn test_command_verifies_crc() {
    let dir = TempDir::new("verify");
    let archive = dir.path().join("verify.zip");
    write_zip(&archive, &[("a.txt", b"hello"), ("b/c.txt", b"world")], &[]);

    let mut reader = backend::open(&archive, None).unwrap();
    let cancel = CancelToken::new();
    let mut sink = spell_core::safety::VerifySink::with_limits(Limits::default());
    let report = reader.test(&cancel, &mut sink).unwrap();
    assert_eq!(report.failed.len(), 0);
    assert_eq!(report.crc_mismatch.len(), 0);
}
