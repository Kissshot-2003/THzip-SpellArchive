use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "thzip-cli-{tag}-{}-{}",
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

fn thzip() -> Command {
    Command::new(env!("CARGO_BIN_EXE_thzip"))
}

fn run(args: &[&str]) -> std::process::Output {
    thzip().args(args).output().expect("run thzip")
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn json_comment_and_extract_roundtrip() {
    let dir = TempDir::new("json");
    let source = dir.path().join("a.txt");
    std::fs::write(&source, b"hello cli").unwrap();
    let archive = dir.path().join("demo.zip");

    let pack = run(&[
        "pack",
        archive.to_str().unwrap(),
        source.to_str().unwrap(),
        "--comment",
        "符卡注释",
        "--json",
    ]);
    assert!(pack.status.success(), "{}", stdout(&pack));
    assert!(stdout(&pack).contains("\"format\":\"zip\""));

    let list = run(&["list", archive.to_str().unwrap(), "--json"]);
    assert!(list.status.success(), "{}", stdout(&list));
    let body = stdout(&list);
    assert!(body.contains("\"comment\":\"符卡注释\""), "{body}");
    assert!(body.contains("\"name\":\"a.txt\""), "{body}");

    let test = run(&["test", archive.to_str().unwrap(), "--json"]);
    assert!(test.status.success(), "{}", stdout(&test));
    assert!(stdout(&test).contains("\"ok\":true"));

    let target = dir.path().join("out");
    let extract = run(&[
        "extract",
        archive.to_str().unwrap(),
        target.to_str().unwrap(),
        "--json",
    ]);
    assert!(extract.status.success(), "{}", stdout(&extract));
    assert!(stdout(&extract).contains("\"succeeded\":1"));
    assert_eq!(std::fs::read(target.join("a.txt")).unwrap(), b"hello cli");
}

#[test]
fn split_7z_volumes_are_readable() {
    let dir = TempDir::new("split");
    let source = dir.path().join("payload.bin");
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let payload: Vec<u8> = (0..1_500_000)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state & 0xFF) as u8
        })
        .collect();
    std::fs::write(&source, &payload).unwrap();
    let archive = dir.path().join("split.7z");

    let pack = run(&[
        "pack",
        archive.to_str().unwrap(),
        source.to_str().unwrap(),
        "--volume-size",
        "1",
        "--json",
    ]);
    assert!(pack.status.success(), "{}", stdout(&pack));
    let body = stdout(&pack);
    assert!(body.contains("\"volumes\":2"), "{body}");
    assert!(dir.path().join("split.7z.001").is_file());
    assert!(dir.path().join("split.7z.002").is_file());

    let list = run(&[
        "list",
        dir.path().join("split.7z.001").to_str().unwrap(),
        "--json",
    ]);
    assert!(list.status.success(), "{}", stdout(&list));
    assert!(stdout(&list).contains("\"name\":\"payload.bin\""));
}

#[test]
fn version_uses_product_name() {
    let output = run(&["--version"]);
    assert!(output.status.success());
    let body = stdout(&output);
    assert!(body.starts_with("thzip "), "{body}");
    assert!(body.contains("THzip-SpellArchive"), "{body}");
}
