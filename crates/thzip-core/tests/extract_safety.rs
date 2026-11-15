//! 安全解压的端到端回归。
//!
//! 重点：祖先链上的 junction / symlink 不得成为越界写盘的通道。
//! 只检查直接父目录是不够的 —— 直接父目录不存在时检查必然通过，
//! 随后的建目录操作却会穿过祖先链接。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use thzip_core::backend;
use thzip_core::limits::Limits;
use thzip_core::plan::{DecisionTable, OverwritePolicy};
use thzip_core::safety::ExtractDirSink;
use thzip_core::task::CancelToken;
use thzip_core::types::EntryId;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "thzip-spellarchive-safety-{tag}-{}-{}",
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

fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    for (name, data) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap();
}

fn extract(archive: &Path, target: &Path) -> thzip_core::ExtractReport {
    let limits = Limits::default();
    let mut reader = backend::open(archive, None).unwrap();
    let ids: Vec<EntryId> = reader.entries().iter().map(|e| e.id).collect();
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

#[cfg(windows)]
fn make_junction(link: &Path, target: &Path) -> bool {
    std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 归档内的条目路径穿过祖先 junction 时，必须被拒绝，内容不得落到解压根目录之外。
#[cfg(windows)]
#[test]
fn extract_refuses_to_escape_through_junction_ancestor() {
    let dir = TempDir::new("junction");
    let root = dir.path().join("root");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();

    let link = root.join("esc");
    assert!(make_junction(&link, &outside), "创建 junction 失败");

    let archive = dir.path().join("evil.zip");
    write_zip(&archive, &[("esc/sub/payload.txt", b"ESCAPED-PAYLOAD")]);

    let report = extract(&archive, &root);

    assert!(
        report.succeeded.is_empty(),
        "穿越 junction 的条目不应解压成功: {:?}",
        report.succeeded
    );
    assert!(
        !outside.join("sub").join("payload.txt").exists(),
        "内容不得写到解压根目录之外"
    );
    assert!(
        !outside.join("sub").exists(),
        "不得在解压根目录之外创建目录"
    );

    let _ = std::fs::remove_dir(&link);
}

/// 深度为 1（junction 直接是父目录）时同样必须被拒绝。
#[cfg(windows)]
#[test]
fn extract_refuses_to_write_through_direct_junction() {
    let dir = TempDir::new("junction-direct");
    let root = dir.path().join("root");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();

    let link = root.join("esc");
    assert!(make_junction(&link, &outside), "创建 junction 失败");

    let archive = dir.path().join("evil.zip");
    write_zip(&archive, &[("esc/payload.txt", b"ESCAPED-PAYLOAD")]);

    let report = extract(&archive, &root);

    assert!(report.succeeded.is_empty(), "不应解压成功");
    assert!(
        !outside.join("payload.txt").exists(),
        "内容不得写到解压根目录之外"
    );

    let _ = std::fs::remove_dir(&link);
}

/// 正常归档不受影响：多层目录照常创建并解压。
#[test]
fn extract_creates_nested_directories_normally() {
    let dir = TempDir::new("nested");
    let root = dir.path().join("root");
    let archive = dir.path().join("ok.zip");
    write_zip(&archive, &[("a/b/c/payload.txt", b"OK")]);

    let report = extract(&archive, &root);

    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(
        std::fs::read(root.join("a/b/c/payload.txt")).unwrap(),
        b"OK"
    );
}
