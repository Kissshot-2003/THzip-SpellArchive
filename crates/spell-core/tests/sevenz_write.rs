use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use spell_core::backend;
use spell_core::limits::Limits;
use spell_core::plan::{DecisionTable, OverwritePolicy};
use spell_core::safety::ExtractDirSink;
use spell_core::task::CancelToken;
use spell_core::types::{ArchiveKind, EntryId, Method};
use spell_core::writer::{SevenZOptions, scan_inputs, write_entries};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "spellarchive-7zwrite-{tag}-{}-{}",
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
    std::fs::write(src.join("说明.txt"), "灵梦 7z 写入".as_bytes()).unwrap();
    std::fs::write(src.join("子目录/data.bin"), [1u8, 2, 3]).unwrap();
    src
}

fn extract_all(archive: &Path, target: &Path, password: Option<&str>) -> spell_core::ExtractReport {
    let limits = Limits::default();
    let secret = password.map(spell_core::SecretString::new);
    let mut reader = backend::open(archive, secret.as_ref()).unwrap();
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

fn pack(archive: &Path, src: &Path, password: Option<&str>) {
    let limits = Limits::default();
    let entries = scan_inputs(&[src.to_path_buf()], &limits).unwrap();
    let secret = password.map(spell_core::SecretString::new);
    let mut writer = backend::create_writer_with(
        archive,
        ArchiveKind::SevenZ,
        Method::Deflate,
        secret.as_ref(),
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
}

#[test]
fn sevenz_writes_and_extracts() {
    let dir = TempDir::new("plain");
    let src = build_source(dir.path());
    let archive = dir.path().join("created.7z");
    pack(&archive, &src, None);

    let reader = backend::open(&archive, None).unwrap();
    assert_eq!(reader.kind(), ArchiveKind::SevenZ);
    assert!(
        reader
            .entries()
            .iter()
            .any(|entry| entry.display.as_ref() == "素材/说明.txt")
    );

    let target = dir.path().join("out");
    let report = extract_all(&archive, &target, None);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(
        std::fs::read(target.join("素材/说明.txt")).unwrap(),
        "灵梦 7z 写入".as_bytes()
    );
    assert_eq!(
        std::fs::read(target.join("素材/子目录/data.bin")).unwrap(),
        vec![1u8, 2, 3]
    );
    assert!(target.join("素材/空目录").is_dir());
}

#[test]
fn sevenz_custom_level_and_dictionary_roundtrip() {
    let dir = TempDir::new("options");
    let src = build_source(dir.path());
    let archive = dir.path().join("options.7z");
    let limits = Limits::default();
    let entries = scan_inputs(&[src], &limits).unwrap();
    let options = SevenZOptions {
        level: 9,
        dict_size: 16 * 1024 * 1024,
    };
    let mut writer = backend::create_writer_with_options(
        &archive,
        ArchiveKind::SevenZ,
        Method::Deflate,
        None,
        options,
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

    let target = dir.path().join("out");
    let report = extract_all(&archive, &target, None);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(
        std::fs::read(target.join("素材/说明.txt")).unwrap(),
        "灵梦 7z 写入".as_bytes()
    );
}

#[test]
fn sevenz_volumes_are_joined() {
    let dir = TempDir::new("volumes");
    let src = build_source(dir.path());
    let archive = dir.path().join("whole.7z");
    pack(&archive, &src, None);

    let data = std::fs::read(&archive).unwrap();
    std::fs::remove_file(&archive).unwrap();
    let chunk = data.len().div_ceil(3).max(1);
    for (index, part) in data.chunks(chunk).enumerate() {
        let path = dir.path().join(format!("split.7z.{:03}", index + 1));
        std::fs::write(path, part).unwrap();
    }

    let first = dir.path().join("split.7z.001");
    let reader = backend::open(&first, None).unwrap();
    assert!(
        reader
            .entries()
            .iter()
            .any(|entry| entry.display.as_ref() == "素材/说明.txt")
    );

    // 从非首卷打开也应能找到同组全部分卷。
    let second = dir.path().join("split.7z.002");
    let reader = backend::open(&second, None).unwrap();
    assert_eq!(reader.entries().len(), 5);

    let target = dir.path().join("vol-out");
    let report = extract_all(&first, &target, None);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(
        std::fs::read(target.join("素材/说明.txt")).unwrap(),
        "灵梦 7z 写入".as_bytes()
    );
}

#[test]
fn sevenz_encrypted_roundtrip() {
    let dir = TempDir::new("encrypted");
    let src = build_source(dir.path());
    let archive = dir.path().join("secret.7z");
    pack(&archive, &src, Some("hunter2"));

    assert!(matches!(
        backend::open(&archive, None),
        Err(spell_core::ArchiveError::PasswordRequired)
    ));

    let target = dir.path().join("out");
    let report = extract_all(&archive, &target, Some("hunter2"));
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(
        std::fs::read(target.join("素材/说明.txt")).unwrap(),
        "灵梦 7z 写入".as_bytes()
    );
}

#[test]
fn sevenz_appends_entries_and_rejects_encrypted() {
    let dir = TempDir::new("append");
    let src = build_source(dir.path());
    let limits = Limits::default();
    let archive = dir.path().join("append.7z");
    pack(&archive, &src, None);

    let extra = dir.path().join("extra.txt");
    std::fs::write(&extra, b"appended").unwrap();
    let entries = scan_inputs(&[extra], &limits).unwrap();
    let mut writer =
        backend::create_writer_append(&archive, ArchiveKind::SevenZ, Method::Store).unwrap();
    write_entries(writer.as_mut(), &entries, None, &CancelToken::new(), &|_| {}).unwrap();
    writer.finish().unwrap();

    let target = dir.path().join("out");
    let report = extract_all(&archive, &target, None);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(
        std::fs::read(target.join("素材/说明.txt")).unwrap(),
        "灵梦 7z 写入".as_bytes()
    );
    assert_eq!(std::fs::read(target.join("extra.txt")).unwrap(), b"appended");

    let encrypted = dir.path().join("secret.7z");
    pack(&encrypted, &src, Some("pw"));
    assert!(backend::create_writer_append(&encrypted, ArchiveKind::SevenZ, Method::Store).is_err());
}
