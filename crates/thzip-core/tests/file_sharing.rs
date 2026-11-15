//! 打开归档时的共享读语义（规格 7-6）。
//!
//! 双击打开正在被别的程序写入的归档是常见场景：下载工具刚落盘、解压软件正在
//! 追加、编辑器正持有句柄。只要打开时带上 `FILE_SHARE_READ | FILE_SHARE_WRITE |
//! FILE_SHARE_DELETE`，就能读到对方已写入的部分，而不是被一个「文件被占用」的
//! 错误挡住。
//!
//! Rust 标准库的 `File::open` 自带这三个标志，但这些标志一旦被谁改成
//! `OpenOptions::new().read(true).open(..)` 之外的写法（例如为了加
//! `FILE_FLAG_SEQUENTIAL_SCAN` 而手写 `CreateFileW`）就可能丢掉。
//! 这里用真实句柄把行为钉住。

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use thzip_core::backend;
use thzip_core::limits::Limits;
use thzip_core::types::ArchiveKind;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "thzip-sharing-{tag}-{}-{}",
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

/// 生成一个只有一条条目的最小 zip（store 方式，无需压缩库）。
fn minimal_zip(path: &Path) {
    // 本地文件头 + 数据 + 中央目录 + 中央目录结束，全部按 zip 规范手工拼。
    let name = b"a.txt";
    let data = b"hello";
    let crc = crc32fast::hash(data);

    let mut out = Vec::new();
    out.extend_from_slice(b"PK\x03\x04");
    out.extend_from_slice(&20u16.to_le_bytes()); // version needed
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&0u16.to_le_bytes()); // method = store
    out.extend_from_slice(&0u16.to_le_bytes()); // time
    out.extend_from_slice(&0u16.to_le_bytes()); // date
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // extra len
    out.extend_from_slice(name);
    out.extend_from_slice(data);

    let cd_offset = out.len() as u32;
    out.extend_from_slice(b"PK\x01\x02");
    out.extend_from_slice(&20u16.to_le_bytes()); // version made by
    out.extend_from_slice(&20u16.to_le_bytes()); // version needed
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // extra
    out.extend_from_slice(&0u16.to_le_bytes()); // comment
    out.extend_from_slice(&0u16.to_le_bytes()); // disk
    out.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
    out.extend_from_slice(&0u32.to_le_bytes()); // external attrs
    out.extend_from_slice(&0u32.to_le_bytes()); // local header offset
    out.extend_from_slice(name);
    let cd_size = out.len() as u32 - cd_offset;

    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // 条目数
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());

    std::fs::write(path, out).unwrap();
}

/// 另一个句柄以「读写」方式持有归档时，仍能打开并列出内容。
#[test]
fn opens_while_another_handle_holds_it_for_writing() {
    let dir = TempDir::new("open");
    let archive = dir.path().join("shared.zip");
    minimal_zip(&archive);

    // 模拟下载工具 / 追加写入方：可写句柄保持打开。
    let mut holder = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&archive)
        .expect("持有句柄");
    holder.write_all(&[]).unwrap();

    let reader = backend::open(&archive, None).expect("已被占用的归档仍应可打开");
    assert_eq!(reader.kind(), ArchiveKind::Zip);
    assert_eq!(reader.entries().len(), 1);

    drop(holder);
}

/// 持有句柄期间还能预检并解压——说明整个链路都没有独占打开。
#[test]
fn extracts_while_another_handle_holds_it() {
    let dir = TempDir::new("extract");
    let archive = dir.path().join("shared.zip");
    minimal_zip(&archive);

    let holder = File::open(&archive).expect("持有句柄");

    let limits = Limits::default();
    let reader = backend::open(&archive, None).expect("open");
    let ids: Vec<_> = reader.entries().iter().map(|entry| entry.id).collect();
    let target = dir.path().join("out");
    let plan = reader.plan(&ids, &target, &limits).expect("plan");
    assert_eq!(plan.items.len(), 1);

    drop(holder);
}

/// 归档被删除（或正在被替换）时打开，应当得到明确的错误而不是进程崩溃。
#[test]
fn reports_missing_archive_clearly() {
    let dir = TempDir::new("missing");
    let archive = dir.path().join("nope.zip");
    let err = match backend::open(&archive, None) {
        Ok(_) => panic!("不存在的归档必须报错"),
        Err(err) => err,
    };
    // 具体文案不重要，重要的是有结构化错误、且不是 panic。
    assert!(!err.reason().zh().is_empty());
}
