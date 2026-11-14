use std::io::Write;
use std::path::{Path, PathBuf};

use thzip_core::backend;
use thzip_core::limits::{LimitProfile, Limits};
use thzip_core::plan::{DecisionTable, OverwritePolicy, PlanAction, PlanWarning};
use thzip_core::safety::ExtractDirSink;
use thzip_core::task::CancelToken;
use thzip_core::types::EntryId;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "thzip-spellarchive-zip-{tag}-{}-{}",
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
) -> thzip_core::ExtractReport {
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
    let mut sink = thzip_core::safety::VerifySink::with_limits(Limits::default());
    let report = reader.test(&cancel, &mut sink).unwrap();
    assert_eq!(report.failed.len(), 0);
    assert_eq!(report.crc_mismatch.len(), 0);
}

fn write_zip_with_method(path: &Path, name: &str, data: &[u8], method: zip::CompressionMethod) {
    let file = std::fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(method);
    writer.start_file(name, options).unwrap();
    writer.write_all(data).unwrap();
    writer.finish().unwrap();
}

#[test]
fn non_deflate_methods_read_and_extract() {
    use thzip_core::types::Method;

    let dir = TempDir::new("methods");
    let payload = b"THzip-SpellArchive non-deflate method test line\n".repeat(64);
    // LZMA / XZ 由 zip 写入端以外的方式生成，见下方 fixtures 用例。
    let cases = [
        (zip::CompressionMethod::Bzip2, Method::Bzip2),
        (zip::CompressionMethod::Zstd, Method::Zstd),
        (zip::CompressionMethod::Ppmd, Method::Ppmd),
    ];

    for (index, (zip_method, expected)) in cases.into_iter().enumerate() {
        let archive = dir.path().join(format!("method-{index}.zip"));
        write_zip_with_method(&archive, "sample.txt", &payload, zip_method);

        let reader = backend::open(&archive, None).unwrap();
        assert_eq!(reader.entries()[0].method, expected, "{zip_method:?}");

        let target = dir.path().join(format!("out-{index}"));
        let report = extract_all(&archive, &target, DecisionTable::default());
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(std::fs::read(target.join("sample.txt")).unwrap(), payload);

        let mut reader = backend::open(&archive, None).unwrap();
        let cancel = CancelToken::new();
        let mut sink = thzip_core::safety::VerifySink::with_limits(Limits::default());
        let report = reader.test(&cancel, &mut sink).unwrap();
        assert_eq!(report.failed.len(), 0, "{zip_method:?}");
        assert_eq!(report.crc_mismatch.len(), 0, "{zip_method:?}");
    }
}

#[test]
fn extraction_restores_entry_mtime() {
    let dir = TempDir::new("mtime");
    let archive = dir.path().join("time.zip");
    let file = std::fs::File::create(&archive).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .last_modified_time(zip::DateTime::from_date_and_time(2021, 4, 5, 6, 7, 8).unwrap());
    writer.start_file("a.txt", options).unwrap();
    writer.write_all(b"x").unwrap();
    writer.finish().unwrap();

    let target = dir.path().join("out");
    let report = extract_all(&archive, &target, DecisionTable::default());
    assert!(report.failed.is_empty(), "{:?}", report.failed);

    let modified = std::fs::metadata(target.join("a.txt"))
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    // 2021-04-05 落在 2020-09 与 2023-11 之间即可证明写回的是归档时间。
    assert!(
        modified > 1_600_000_000 && modified < 1_700_000_000,
        "{modified}"
    );
}

#[test]
fn xz_entries_extract() {
    use thzip_core::types::Method;

    let archive = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join("xz.zip");
    let dir = TempDir::new("xz");
    let target = dir.path().join("out");
    let expected = (1..=200)
        .map(|index| format!("SpellArchive compression fixture line {index} 0123456789ABCDEF"))
        .collect::<Vec<_>>()
        .join("\r\n");

    let reader = backend::open(&archive, None).unwrap();
    assert_eq!(reader.entries()[0].method, Method::Xz);
    let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
    let plan = reader.plan(&ids, &target, &Limits::default()).unwrap();
    assert!(matches!(
        plan.items[0].action,
        PlanAction::Create | PlanAction::Overwrite
    ));

    let report = extract_all(&archive, &target, DecisionTable::default());
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(report.succeeded.len(), 1);
    assert_eq!(
        std::fs::read_to_string(target.join("payload.txt")).unwrap(),
        expected
    );
}

#[test]
fn lzma_and_deflate64_fixtures_extract() {
    use thzip_core::types::Method;

    let expected = (1..=200)
        .map(|index| format!("SpellArchive compression fixture line {index} 0123456789ABCDEF"))
        .collect::<Vec<_>>()
        .join("\r\n");

    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let dir = TempDir::new("fixtures");
    let cases = [
        ("lzma.zip", Method::Lzma),
        ("deflate64.zip", Method::Deflate64),
    ];

    for (file, expected_method) in cases {
        let archive = fixtures.join(file);
        let reader = backend::open(&archive, None).unwrap();
        assert_eq!(reader.entries()[0].method, expected_method, "{file}");
        assert_eq!(reader.entries()[0].size, expected.len() as u64, "{file}");

        let target = dir.path().join(file);
        let report = extract_all(&archive, &target, DecisionTable::default());
        assert!(report.failed.is_empty(), "{file}: {:?}", report.failed);
        let extracted = std::fs::read(target.join("payload.txt")).unwrap();
        assert_eq!(String::from_utf8(extracted).unwrap(), expected, "{file}");

        let mut reader = backend::open(&archive, None).unwrap();
        let cancel = CancelToken::new();
        let mut sink = thzip_core::safety::VerifySink::with_limits(Limits::default());
        let report = reader.test(&cancel, &mut sink).unwrap();
        assert_eq!(report.failed.len(), 0, "{file}");
        assert_eq!(report.crc_mismatch.len(), 0, "{file}");
    }
}

#[test]
fn plan_reports_and_rejects_suspicious_ratio() {
    let dir = TempDir::new("ratio");
    let archive = dir.path().join("bomb.zip");
    let file = std::fs::File::create(&archive).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer.start_file("payload.bin", options).unwrap();
    // 一半随机、一半零：总 4 MiB，压缩后约 2 MiB，倍率约 2。
    let mut state = 0x1234_5678u32;
    let mut data = Vec::with_capacity(4 * 1024 * 1024);
    for _ in 0..(2 * 1024 * 1024) {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        data.push((state >> 24) as u8);
    }
    data.resize(4 * 1024 * 1024, 0);
    writer.write_all(&data).unwrap();
    writer.finish().unwrap();

    let limits = Limits {
        max_ratio_streamed: 1,
        ..Limits::default()
    };
    let reader = backend::open(&archive, None).unwrap();
    let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
    let plan = reader.plan(&ids, &dir.path().join("out"), &limits).unwrap();
    assert!(matches!(plan.items[0].action, PlanAction::Reject(_)));
    assert!(
        plan.warnings
            .iter()
            .any(|warning| matches!(warning, PlanWarning::BombSuspected { .. }))
    );
}
