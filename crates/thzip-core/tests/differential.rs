use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

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
            "thzip-differential-{tag}-{}-{}",
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

fn seven_zip() -> Option<PathBuf> {
    let path = std::env::var_os("THZIP_7Z").map(PathBuf::from)?;
    path.is_file().then_some(path)
}

fn build_source(root: &Path) -> PathBuf {
    let src = root.join("src");
    std::fs::create_dir_all(src.join("sub")).unwrap();
    std::fs::create_dir_all(src.join("empty")).unwrap();
    std::fs::write(src.join("hello.txt"), b"hello from thzip").unwrap();
    std::fs::write(src.join("zeros.bin"), vec![0u8; 4096]).unwrap();
    std::fs::write(src.join("sub/data.bin"), (0..=255u8).collect::<Vec<u8>>()).unwrap();
    src
}

fn pack(archive: &Path, kind: ArchiveKind, src: &Path) {
    let method = match kind {
        ArchiveKind::Zip => Method::Deflate,
        ArchiveKind::SevenZ => Method::Lzma2,
        _ => Method::Store,
    };
    let entries = scan_inputs(&[src.to_path_buf()], &Limits::default()).unwrap();
    let mut writer = backend::create_writer(archive, kind, method).unwrap();
    write_entries(
        writer.as_mut(),
        &entries,
        Some(method),
        &CancelToken::new(),
        &|_| {},
    )
    .unwrap();
    writer.finish().unwrap();
}

fn listing_from_7z(sevenzip: &Path, archive: &Path) -> BTreeMap<String, u64> {
    let output = Command::new(sevenzip)
        .args(["l", "-slt", "-ba"])
        .arg(archive)
        .output()
        .expect("run 7z l");
    assert!(
        output.status.success(),
        "7z l failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let text = String::from_utf8_lossy(&output.stdout);
    let mut map = BTreeMap::new();
    let mut path: Option<String> = None;
    let mut size: Option<u64> = None;
    let mut is_dir = false;

    let flush = |path: &mut Option<String>,
                 size: &mut Option<u64>,
                 is_dir: &mut bool,
                 map: &mut BTreeMap<String, u64>| {
        if let (Some(name), Some(bytes)) = (path.take(), size.take())
            && !*is_dir
        {
            map.insert(name, bytes);
        }
        *is_dir = false;
    };

    for line in text.lines() {
        if let Some(value) = line.strip_prefix("Path = ") {
            path = Some(value.trim().replace('\\', "/"));
            size = None;
            is_dir = false;
        } else if let Some(value) = line.strip_prefix("Folder = ") {
            is_dir = value.trim() == "+";
        } else if let Some(value) = line.strip_prefix("Attributes = ") {
            // 7z 对 7z 包不输出 Folder 行，只能用属性首字母 D 判断目录。
            is_dir |= value.trim_start().starts_with('D');
        } else if let Some(value) = line.strip_prefix("Size = ") {
            size = value.trim().parse().ok();
        } else if line.is_empty() {
            flush(&mut path, &mut size, &mut is_dir, &mut map);
        }
    }
    flush(&mut path, &mut size, &mut is_dir, &mut map);
    map
}

fn listing_from_core(archive: &Path) -> BTreeMap<String, u64> {
    let reader = backend::open(archive, None).expect("open archive");
    reader
        .entries()
        .iter()
        .filter(|entry| !entry.kind.is_dir())
        .map(|entry| (entry.display.to_string(), entry.size))
        .collect()
}

fn extract_all(archive: &Path, target: &Path) {
    let limits = Limits::default();
    let mut reader = backend::open(archive, None).expect("open for extract");
    let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
    let plan = reader.plan(&ids, target, &limits).expect("plan");
    let decisions = DecisionTable {
        default: OverwritePolicy::Overwrite,
        ..Default::default()
    };
    let cancel = CancelToken::new();
    let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
    let report = reader
        .extract(&plan, &decisions, &cancel, &mut sink)
        .expect("extract");
    assert!(report.failed.is_empty(), "failed: {:?}", report.failed);
}

#[test]
fn listings_agree_with_7zip() {
    let Some(sevenzip) = seven_zip() else {
        eprintln!("THZIP_7Z not set; skipping differential listing test");
        return;
    };

    for (kind, name) in [
        (ArchiveKind::Zip, "ours.zip"),
        (ArchiveKind::SevenZ, "ours.7z"),
        (ArchiveKind::Tar, "ours.tar"),
    ] {
        let dir = TempDir::new(name);
        let src = build_source(dir.path());
        let archive = dir.path().join(name);
        pack(&archive, kind, &src);

        let ours = listing_from_core(&archive);
        let theirs = listing_from_7z(&sevenzip, &archive);
        assert_eq!(ours, theirs, "listing mismatch for {name}");
        assert!(!ours.is_empty(), "archive should not be empty: {name}");
    }

    // 7z 不递归 tar.gz，先把 gzip 层解出来再比内层 tar。
    let dir = TempDir::new("ours.tar.gz");
    let src = build_source(dir.path());
    let archive = dir.path().join("ours.tar.gz");
    pack(&archive, ArchiveKind::TarGz, &src);
    let inner = dir.path().join("inner.tar");
    let output = Command::new(&sevenzip)
        .args(["e", "-so"])
        .arg(&archive)
        .output()
        .expect("run 7z e -so");
    assert!(
        output.status.success(),
        "7z could not decompress tar.gz: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(&inner, &output.stdout).expect("write inner tar");
    assert_eq!(
        listing_from_core(&archive),
        listing_from_7z(&sevenzip, &inner),
        "listing mismatch for ours.tar.gz"
    );
}

#[test]
fn reads_archives_created_by_7zip() {
    let Some(sevenzip) = seven_zip() else {
        eprintln!("THZIP_7Z not set; skipping differential read test");
        return;
    };

    for (flag, name) in [
        ("-tzip", "theirs.zip"),
        ("-t7z", "theirs.7z"),
        ("-ttar", "theirs.tar"),
    ] {
        let dir = TempDir::new(name);
        let src = build_source(dir.path());
        let archive = dir.path().join(name);

        let output = Command::new(&sevenzip)
            .arg("a")
            .arg(flag)
            .arg(&archive)
            .arg(&src)
            .output()
            .expect("run 7z a");
        assert!(
            output.status.success(),
            "7z a failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let ours = listing_from_core(&archive);
        let theirs = listing_from_7z(&sevenzip, &archive);
        assert_eq!(ours, theirs, "listing mismatch for 7z-created {name}");

        let target = dir.path().join("out");
        extract_all(&archive, &target);
        assert_eq!(
            std::fs::read(target.join("src/hello.txt")).unwrap(),
            b"hello from thzip"
        );
        assert_eq!(
            std::fs::read(target.join("src/sub/data.bin")).unwrap(),
            (0..=255u8).collect::<Vec<u8>>()
        );
    }
}

#[test]
fn passwords_interoperate_with_7zip() {
    let Some(sevenzip) = seven_zip() else {
        eprintln!("THZIP_7Z not set; skipping differential password test");
        return;
    };

    let password = thzip_core::SecretString::new("hunter2");

    let dir = TempDir::new("password");
    let src = build_source(dir.path());

    let ours = dir.path().join("ours-aes.zip");
    let entries = scan_inputs(&[src.join("hello.txt")], &Limits::default()).unwrap();
    let mut writer =
        backend::create_writer_with(&ours, ArchiveKind::Zip, Method::Deflate, Some(&password))
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

    let output = Command::new(&sevenzip)
        .arg("t")
        .arg("-phunter2")
        .arg(&ours)
        .output()
        .expect("run 7z t");
    assert!(
        output.status.success(),
        "7z could not test our AES zip: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let theirs = dir.path().join("theirs-encrypted.7z");
    let output = Command::new(&sevenzip)
        .args(["a", "-t7z", "-phunter2", "-mhe=on"])
        .arg(&theirs)
        .arg(src.join("hello.txt"))
        .output()
        .expect("run 7z a -p");
    assert!(
        output.status.success(),
        "7z could not create encrypted archive: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let reader = backend::open(&theirs, Some(&password)).expect("open encrypted 7z");
    assert!(!reader.entries().is_empty());
    assert!(backend::open(&theirs, None).is_err());
}
