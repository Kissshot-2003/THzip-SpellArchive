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
            "thzip-spellarchive-tarwrite-{tag}-{}-{}",
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
    std::fs::write(src.join("说明.txt"), "灵梦 tar 写入".as_bytes()).unwrap();
    std::fs::write(src.join("子目录/data.bin"), [5u8, 6, 7]).unwrap();
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
    let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
    reader
        .extract(&plan, &decisions, &CancelToken::new(), &mut sink)
        .unwrap()
}

#[test]
fn tar_family_writes_and_extracts() {
    let dir = TempDir::new("all");
    let src = build_source(dir.path());
    let limits = Limits::default();

    for (kind, suffix) in [
        (ArchiveKind::Tar, "tar"),
        (ArchiveKind::TarGz, "tar.gz"),
        (ArchiveKind::TarBz2, "tar.bz2"),
        (ArchiveKind::TarXz, "tar.xz"),
        (ArchiveKind::TarZst, "tar.zst"),
        (ArchiveKind::TarLz4, "tar.lz4"),
    ] {
        let archive = dir.path().join(format!("created.{suffix}"));
        let entries = scan_inputs(std::slice::from_ref(&src), &limits).unwrap();
        let mut writer = backend::create_writer(&archive, kind, Method::Deflate).unwrap();
        write_entries(
            writer.as_mut(),
            &entries,
            Some(Method::Deflate),
            &CancelToken::new(),
            &|_| {},
        )
        .unwrap();
        let report = writer.finish().unwrap();
        assert_eq!(report.entries as usize, entries.len(), "{kind:?}");

        let target = dir.path().join(format!("out-{suffix}"));
        let report = extract_all(&archive, &target);
        assert!(report.failed.is_empty(), "{kind:?}: {:?}", report.failed);
        assert_eq!(
            std::fs::read(target.join("素材/说明.txt")).unwrap(),
            "灵梦 tar 写入".as_bytes(),
            "{kind:?}"
        );
        assert_eq!(
            std::fs::read(target.join("素材/子目录/data.bin")).unwrap(),
            vec![5u8, 6, 7],
            "{kind:?}"
        );
        assert!(target.join("素材/空目录").is_dir(), "{kind:?}");
    }
}

#[test]
fn tar_rejects_password() {
    let dir = TempDir::new("password");
    let archive = dir.path().join("secret.tar.gz");
    let result = backend::create_writer_with(
        &archive,
        ArchiveKind::TarGz,
        Method::Deflate,
        Some(&thzip_core::SecretString::new("pw")),
    );
    assert!(result.is_err());
}

#[test]
fn tar_family_appends_entries() {
    let dir = TempDir::new("append");
    let src = build_source(dir.path());
    let limits = Limits::default();
    let archive = dir.path().join("append.tar.gz");

    let entries = scan_inputs(std::slice::from_ref(&src), &limits).unwrap();
    let mut writer = backend::create_writer(&archive, ArchiveKind::TarGz, Method::Store).unwrap();
    write_entries(
        writer.as_mut(),
        &entries,
        None,
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    writer.finish().unwrap();

    let extra = dir.path().join("extra.txt");
    std::fs::write(&extra, b"appended").unwrap();
    let extra_entries = scan_inputs(std::slice::from_ref(&extra), &limits).unwrap();
    let mut writer = backend::create_writer_append(
        &archive,
        ArchiveKind::TarGz,
        Method::Store,
        thzip_core::SevenZOptions::default(),
        Limits::default(),
        &CancelToken::new(),
    )
    .unwrap();
    write_entries(
        writer.as_mut(),
        &extra_entries,
        None,
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    writer.finish().unwrap();

    let target = dir.path().join("out");
    let report = extract_all(&archive, &target);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(
        std::fs::read(target.join("素材/说明.txt")).unwrap(),
        "灵梦 tar 写入".as_bytes()
    );
    assert_eq!(
        std::fs::read(target.join("extra.txt")).unwrap(),
        b"appended"
    );

    // 追加同名条目必须失败，且原归档保持不变。
    let duplicate = scan_inputs(std::slice::from_ref(&src), &limits).unwrap();
    let mut writer = backend::create_writer_append(
        &archive,
        ArchiveKind::TarGz,
        Method::Store,
        thzip_core::SevenZOptions::default(),
        Limits::default(),
        &CancelToken::new(),
    )
    .unwrap();
    let result = write_entries(
        writer.as_mut(),
        &duplicate,
        None,
        &CancelToken::new(),
        &|_| {},
    );
    assert!(result.is_err());
    drop(writer);

    let target = dir.path().join("out-again");
    let report = extract_all(&archive, &target);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(target.join("extra.txt").is_file());
}

#[test]
fn tar_append_respects_cancel() {
    let dir = TempDir::new("append-cancel");
    let src = build_source(dir.path());
    let limits = Limits::default();
    let archive = dir.path().join("cancel.tar.gz");
    let entries = scan_inputs(std::slice::from_ref(&src), &limits).unwrap();
    let mut writer = backend::create_writer(&archive, ArchiveKind::TarGz, Method::Store).unwrap();
    write_entries(
        writer.as_mut(),
        &entries,
        None,
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    writer.finish().unwrap();

    let cancel = CancelToken::new();
    cancel.cancel();
    let result = backend::create_writer_append(
        &archive,
        ArchiveKind::TarGz,
        Method::Store,
        thzip_core::SevenZOptions::default(),
        limits,
        &cancel,
    );
    assert!(matches!(result, Err(thzip_core::ArchiveError::Cancelled)));
}

#[test]
fn tar_append_preserves_directory_mtime() {
    let dir = TempDir::new("append-mtime");
    let src = build_source(dir.path());
    let limits = Limits::default();
    let archive = dir.path().join("mtime.tar.gz");
    let entries = scan_inputs(std::slice::from_ref(&src), &limits).unwrap();
    let stamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000);
    let subdir = src.join("子目录");
    thzip_core::safety::set_mtime(&subdir, stamp).unwrap();
    let mut writer = backend::create_writer(&archive, ArchiveKind::TarGz, Method::Store).unwrap();
    write_entries(
        writer.as_mut(),
        &entries,
        None,
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    writer.finish().unwrap();

    let extra = dir.path().join("extra.txt");
    std::fs::write(&extra, b"x").unwrap();
    let extra_entries = scan_inputs(std::slice::from_ref(&extra), &limits).unwrap();
    let mut writer = backend::create_writer_append(
        &archive,
        ArchiveKind::TarGz,
        Method::Store,
        thzip_core::SevenZOptions::default(),
        limits,
        &CancelToken::new(),
    )
    .unwrap();
    write_entries(
        writer.as_mut(),
        &extra_entries,
        None,
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    writer.finish().unwrap();

    let reader = backend::open(&archive, None).unwrap();
    let entry = reader
        .entries()
        .iter()
        .find(|entry| entry.display.as_ref() == "素材/子目录")
        .unwrap();
    let mtime = entry.mtime.unwrap();
    let diff = mtime
        .duration_since(stamp)
        .or_else(|_| stamp.duration_since(mtime))
        .unwrap();
    assert!(diff < std::time::Duration::from_secs(2), "{diff:?}");
}
