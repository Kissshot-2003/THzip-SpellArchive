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
fn extract_reads_password_from_file_and_same_dir() {
    let dir = TempDir::new("pwfile");
    let source = dir.path().join("secret.txt");
    std::fs::write(&source, b"classified").unwrap();
    let archive = dir.path().join("locked.zip");

    let pack = run(&[
        "pack",
        archive.to_str().unwrap(),
        source.to_str().unwrap(),
        "--password",
        "hunter2",
    ]);
    assert!(pack.status.success(), "{}", stdout(&pack));

    // --password-file，文件带 UTF-8 BOM 与尾随空行。
    let pwfile = dir.path().join("pw.txt");
    std::fs::write(&pwfile, b"\xEF\xBB\xBFhunter2\n\n").unwrap();
    let target1 = dir.path().join("out1");
    let ex1 = run(&[
        "extract",
        archive.to_str().unwrap(),
        target1.to_str().unwrap(),
        "--password-file",
        pwfile.to_str().unwrap(),
    ]);
    assert!(ex1.status.success(), "{}", stdout(&ex1));
    assert_eq!(
        std::fs::read(target1.join("secret.txt")).unwrap(),
        b"classified"
    );

    // 同目录 password.txt 约定，显式不传密码。
    std::fs::write(dir.path().join("password.txt"), b"hunter2\n").unwrap();
    let target2 = dir.path().join("out2");
    let ex2 = run(&[
        "extract",
        archive.to_str().unwrap(),
        target2.to_str().unwrap(),
    ]);
    assert!(ex2.status.success(), "{}", stdout(&ex2));
    assert_eq!(
        std::fs::read(target2.join("secret.txt")).unwrap(),
        b"classified"
    );

    // 密码不进 JSON。
    let ex3 = run(&[
        "extract",
        archive.to_str().unwrap(),
        dir.path().join("out3").to_str().unwrap(),
        "--json",
    ]);
    assert!(!stdout(&ex3).contains("hunter2"), "{}", stdout(&ex3));
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

#[test]
fn checksum_and_verify_roundtrip() {
    let dir = TempDir::new("sums");
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&a, b"alpha").unwrap();
    std::fs::write(&b, b"beta").unwrap();
    let archive = dir.path().join("data.zip");

    let pack = run(&[
        "pack",
        archive.to_str().unwrap(),
        a.to_str().unwrap(),
        b.to_str().unwrap(),
    ]);
    assert!(pack.status.success(), "{}", stdout(&pack));

    let ca = run(&["checksum", a.to_str().unwrap()]);
    let cb = run(&["checksum", b.to_str().unwrap()]);
    assert!(ca.status.success(), "{}", stdout(&ca));
    assert!(cb.status.success(), "{}", stdout(&cb));
    let ha = stdout(&ca).split_whitespace().next().unwrap().to_owned();
    let hb = stdout(&cb).split_whitespace().next().unwrap().to_owned();

    let sums = dir.path().join("SHA256SUMS.txt");
    std::fs::write(&sums, format!("{ha}  a.txt\n{hb}  b.txt\n")).unwrap();

    let ok = run(&[
        "verify",
        archive.to_str().unwrap(),
        "--checksum-file",
        sums.to_str().unwrap(),
    ]);
    assert!(ok.status.success(), "{}", stdout(&ok));

    std::fs::write(&sums, format!("{ha}  a.txt\n0000  b.txt\n")).unwrap();
    let bad = run(&[
        "verify",
        archive.to_str().unwrap(),
        "--checksum-file",
        sums.to_str().unwrap(),
        "--json",
    ]);
    assert!(!bad.status.success(), "{}", stdout(&bad));
    assert!(
        stdout(&bad).contains("\"mismatch\":[\"b.txt\"]"),
        "{}",
        stdout(&bad)
    );
}
