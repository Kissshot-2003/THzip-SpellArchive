use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use thzip_core::backend;
use thzip_core::limits::Limits;
use thzip_core::plan::{DecisionTable, OverwritePolicy};
use thzip_core::safety::ExtractDirSink;
use thzip_core::task::CancelToken;
use thzip_core::types::{ArchiveKind, EntryId, Method};
use thzip_core::writer::{scan_inputs, write_entries};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "thzip-spellarchive-write-{tag}-{}-{}",
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

fn build_source(root: &Path) -> PathBuf {
    let src = root.join("素材");
    std::fs::create_dir_all(src.join("子目录")).unwrap();
    std::fs::create_dir_all(src.join("空目录")).unwrap();
    std::fs::write(src.join("说明.txt"), "灵梦的符卡".as_bytes()).unwrap();
    std::fs::write(src.join("子目录/data.bin"), [0u8, 1, 2, 3, 255]).unwrap();
    src
}

fn extract_all(archive: &Path, target: &Path) -> thzip_core::ExtractReport {
    let limits = Limits::default();
    let mut reader = backend::open(archive, None).unwrap();
    let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
    let plan = reader.plan(&ids, target, &limits).unwrap();
    let decisions = DecisionTable {
        default: OverwritePolicy::Overwrite,
        ..Default::default()
    };
    let cancel = CancelToken::new();
    let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
    reader
        .extract(&plan, &decisions, &cancel, &mut sink)
        .unwrap()
}

#[test]
fn packs_tree_and_extracts_roundtrip() {
    let dir = TempDir::new("roundtrip");
    let src = build_source(dir.path());
    let archive = dir.path().join("out.zip");

    let entries = scan_inputs(&[src], &Limits::default()).unwrap();
    let mut writer = backend::create_writer(&archive, ArchiveKind::Zip, Method::Deflate).unwrap();
    write_entries(
        writer.as_mut(),
        &entries,
        Some(Method::Deflate),
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    let report = writer.finish().unwrap();
    assert_eq!(report.entries as usize, entries.len());
    assert!(report.bytes_out > 0);

    let target = dir.path().join("extracted");
    let extracted = extract_all(&archive, &target);
    assert!(extracted.failed.is_empty(), "{:?}", extracted.failed);
    assert_eq!(
        std::fs::read(target.join("素材/说明.txt")).unwrap(),
        "灵梦的符卡".as_bytes()
    );
    assert_eq!(
        std::fs::read(target.join("素材/子目录/data.bin")).unwrap(),
        vec![0u8, 1, 2, 3, 255]
    );
    assert!(target.join("素材/空目录").is_dir());
}

#[test]
fn cancelled_write_leaves_no_output() {
    let dir = TempDir::new("cancel");
    let src = build_source(dir.path());
    let archive = dir.path().join("cancelled.zip");

    let entries = scan_inputs(&[src], &Limits::default()).unwrap();
    let mut writer = backend::create_writer(&archive, ArchiveKind::Zip, Method::Store).unwrap();
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = write_entries(
        writer.as_mut(),
        &entries,
        Some(Method::Store),
        &cancel,
        &|_| {},
    );
    assert!(matches!(result, Err(thzip_core::ArchiveError::Cancelled)));
    drop(writer);
    assert!(!archive.exists(), "取消后不应留下目标文件");
}

#[test]
fn dropped_writer_keeps_existing_archive() {
    let dir = TempDir::new("atomic");
    let src = build_source(dir.path());
    let archive = dir.path().join("keep.zip");
    std::fs::write(&archive, b"old archive").unwrap();

    let entries = scan_inputs(&[src], &Limits::default()).unwrap();
    let mut writer = backend::create_writer(&archive, ArchiveKind::Zip, Method::Deflate).unwrap();
    write_entries(
        writer.as_mut(),
        &entries,
        Some(Method::Deflate),
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    drop(writer);

    assert_eq!(std::fs::read(&archive).unwrap(), b"old archive");
}

#[test]
fn appends_to_existing_archive() {
    let dir = TempDir::new("append");
    let archive = dir.path().join("base.zip");
    let first = dir.path().join("a.txt");
    let second = dir.path().join("b.txt");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    let limits = Limits::default();

    let entries = scan_inputs(&[first], &limits).unwrap();
    let mut writer = backend::create_writer(&archive, ArchiveKind::Zip, Method::Deflate).unwrap();
    write_entries(
        writer.as_mut(),
        &entries,
        Some(Method::Deflate),
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    writer.finish().unwrap();

    let entries = scan_inputs(&[second], &limits).unwrap();
    let mut writer = backend::create_writer_append(
        &archive,
        ArchiveKind::Zip,
        Method::Deflate,
        thzip_core::SevenZOptions::default(),
        Limits::default(),
        &CancelToken::new(),
    )
    .unwrap();
    write_entries(
        writer.as_mut(),
        &entries,
        Some(Method::Deflate),
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    writer.finish().unwrap();

    let reader = backend::open(&archive, None).unwrap();
    let names: Vec<String> = reader
        .entries()
        .iter()
        .map(|entry| entry.display.to_string())
        .collect();
    assert!(names.iter().any(|name| name == "a.txt"));
    assert!(names.iter().any(|name| name == "b.txt"));
}

#[test]
fn append_rejects_duplicate_name() {
    let dir = TempDir::new("append-dup");
    let archive = dir.path().join("base.zip");
    let file = dir.path().join("a.txt");
    std::fs::write(&file, b"first").unwrap();
    let limits = Limits::default();

    let entries = scan_inputs(std::slice::from_ref(&file), &limits).unwrap();
    let mut writer = backend::create_writer(&archive, ArchiveKind::Zip, Method::Deflate).unwrap();
    write_entries(
        writer.as_mut(),
        &entries,
        Some(Method::Deflate),
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    writer.finish().unwrap();

    let mut writer = backend::create_writer_append(
        &archive,
        ArchiveKind::Zip,
        Method::Deflate,
        thzip_core::SevenZOptions::default(),
        Limits::default(),
        &CancelToken::new(),
    )
    .unwrap();
    let result = write_entries(
        writer.as_mut(),
        &entries,
        Some(Method::Deflate),
        &CancelToken::new(),
        &|_| {},
    );
    assert!(result.is_err(), "同名条目应被拒绝");
    drop(writer);

    let reader = backend::open(&archive, None).unwrap();
    assert_eq!(reader.entries().len(), 1);
    assert_eq!(reader.entries()[0].display.as_ref(), "a.txt");
}

#[test]
fn encrypted_archive_needs_password() {
    let dir = TempDir::new("encrypted");
    let archive = dir.path().join("secret.zip");
    let file = dir.path().join("secret.txt");
    std::fs::write(&file, b"top secret").unwrap();
    let limits = Limits::default();

    let entries = scan_inputs(std::slice::from_ref(&file), &limits).unwrap();
    let mut writer = backend::create_writer_with(
        &archive,
        ArchiveKind::Zip,
        Method::Deflate,
        Some(&thzip_core::SecretString::new("hunter2")),
    )
    .unwrap();
    write_entries(
        writer.as_mut(),
        &entries,
        Some(Method::Deflate),
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    writer.finish().unwrap();

    // 无密码可以读目录，但解压条目会失败。
    let target = dir.path().join("no-password");
    let report = extract_all(&archive, &target);
    assert!(report.succeeded.is_empty());
    assert!(!report.failed.is_empty());
    assert!(!target.join("secret.txt").exists());

    // 正确密码可以解出原文。
    let mut reader =
        backend::open(&archive, Some(&thzip_core::SecretString::new("hunter2"))).unwrap();
    let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
    let plan = reader.plan(&ids, &target, &limits).unwrap();
    let decisions = DecisionTable {
        default: OverwritePolicy::Overwrite,
        ..Default::default()
    };
    let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
    let report = reader
        .extract(&plan, &decisions, &CancelToken::new(), &mut sink)
        .unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(
        report.crc_mismatch.is_empty(),
        "AES 条目不应用 CRC=0 误报: {:?}",
        report.crc_mismatch
    );
    assert_eq!(
        std::fs::read(target.join("secret.txt")).unwrap(),
        b"top secret".to_vec()
    );

    // 加密包拒绝追加，避免新条目以明文写入。
    let result = backend::create_writer_append(
        &archive,
        ArchiveKind::Zip,
        Method::Deflate,
        thzip_core::SevenZOptions::default(),
        Limits::default(),
        &CancelToken::new(),
    );
    assert!(matches!(
        result,
        Err(thzip_core::ArchiveError::Unsupported(_))
    ));
}

#[test]
fn rejects_unsafe_archive_names() {
    assert!(thzip_core::writer::normalize_entry_name("../evil").is_err());
    assert!(thzip_core::writer::normalize_entry_name("C:/Windows/evil").is_err());
    assert!(thzip_core::writer::normalize_entry_name("a/../../b").is_err());
    assert!(thzip_core::writer::normalize_entry_name("a\\b\\c").is_ok());
}

#[test]
fn creates_bzip2_and_zstd_zips() {
    for method in [Method::Bzip2, Method::Zstd] {
        let dir = TempDir::new(&format!("{}-method", method.label()));
        let src = build_source(dir.path());
        let archive = dir.path().join("out.zip");

        let entries = scan_inputs(&[src], &Limits::default()).unwrap();
        let mut writer = backend::create_writer(&archive, ArchiveKind::Zip, method).unwrap();
        write_entries(
            writer.as_mut(),
            &entries,
            Some(method),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();
        writer.finish().unwrap();

        let reader = backend::open(&archive, None).unwrap();
        let file_entry = reader
            .entries()
            .iter()
            .find(|entry| entry.display.ends_with("说明.txt"))
            .unwrap();
        assert_eq!(file_entry.method, method);

        let target = dir.path().join("out");
        let report = extract_all(&archive, &target);
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(
            std::fs::read(target.join("素材/说明.txt")).unwrap(),
            "灵梦的符卡".as_bytes()
        );
    }
}

#[test]
fn zip_append_respects_cancel() {
    let dir = TempDir::new("append-cancel");
    let archive = dir.path().join("cancel.zip");
    let mut writer = backend::create_writer(&archive, ArchiveKind::Zip, Method::Deflate).unwrap();
    writer.add_dir(std::path::Path::new("."), "dir").unwrap();
    writer.finish().unwrap();

    let cancel = CancelToken::new();
    cancel.cancel();
    let result = backend::create_writer_append(
        &archive,
        ArchiveKind::Zip,
        Method::Deflate,
        thzip_core::SevenZOptions::default(),
        Limits::default(),
        &cancel,
    );
    assert!(matches!(result, Err(thzip_core::ArchiveError::Cancelled)));
}
