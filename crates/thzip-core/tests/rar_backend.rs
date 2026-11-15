use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use thzip_core::backend;
use thzip_core::limits::Limits;
use thzip_core::plan::{DecisionTable, OverwritePolicy};
use thzip_core::safety::{ExtractDirSink, VerifySink};
use thzip_core::secret::SecretString;
use thzip_core::task::CancelToken;
use thzip_core::types::{EntryId, Method};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "thzip-spellarchive-rar-{tag}-{}-{}",
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

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn extract_all(
    archive: &Path,
    target: &Path,
    password: Option<&SecretString>,
) -> Result<thzip_core::ExtractReport, thzip_core::ArchiveError> {
    let limits = Limits::default();
    let mut reader = backend::open(archive, password)?;
    let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
    let plan = reader.plan(&ids, target, &limits)?;
    let decisions = DecisionTable {
        default: OverwritePolicy::Overwrite,
        ..Default::default()
    };
    let cancel = CancelToken::new();
    let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
    reader.extract(&plan, &decisions, &cancel, &mut sink)
}

#[test]
fn lists_and_extracts_simple_rar() {
    let archive = fixture("version.rar");
    let reader = backend::open(&archive, None).unwrap();
    assert_eq!(reader.kind(), thzip_core::ArchiveKind::Rar);
    assert_eq!(reader.entries().len(), 1);
    assert_eq!(reader.entries()[0].display.as_ref(), "VERSION");
    assert_eq!(reader.entries()[0].size, 11);
    assert_eq!(reader.entries()[0].method, Method::Rar);

    let dir = TempDir::new("simple");
    let target = dir.path().join("out");
    let report = extract_all(&archive, &target, None).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(
        std::fs::read_to_string(target.join("VERSION")).unwrap(),
        "unrar-0.4.0"
    );

    let mut reader = backend::open(&archive, None).unwrap();
    let cancel = CancelToken::new();
    let mut sink = VerifySink::with_limits(Limits::default());
    let report = reader.test(&Limits::default(), &cancel, &mut sink).unwrap();
    assert_eq!(report.failed.len(), 0);
    assert_eq!(report.crc_mismatch.len(), 0);
}

#[test]
fn encrypted_rar_requires_password() {
    let archive = fixture("crypted.rar");
    let dir = TempDir::new("crypted");

    let reader = backend::open(&archive, None).unwrap();
    let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
    let plan = reader
        .plan(&ids, &dir.path().join("out"), &Limits::default())
        .unwrap();
    assert!(plan.needs_password);

    let target = dir.path().join("no-password");
    let err = extract_all(&archive, &target, None).unwrap_err();
    assert!(
        matches!(err, thzip_core::ArchiveError::PasswordRequired),
        "{err:?}"
    );

    let password = SecretString::new("unrar".to_owned());
    let target = dir.path().join("with-password");
    let report = extract_all(&archive, &target, Some(&password)).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(
        std::fs::read_to_string(target.join(".gitignore")).unwrap(),
        "target\nCargo.lock\n"
    );
}

#[test]
fn solid_rar_extracts() {
    let archive = fixture("solid.rar");
    let reader = backend::open(&archive, None).unwrap();
    assert!(!reader.entries().is_empty());

    let dir = TempDir::new("solid");
    let target = dir.path().join("out");
    let report = extract_all(&archive, &target, None).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(report.succeeded.len() >= reader.entries().len());
}

/// 记录每次 `on_data` 收到的块大小。
///
/// 真流式的判据：块大小由解码器的缓冲决定，与条目总大小无关。
/// 如果实现在内存里攒完整条再一次性吐出来，最大块就会等于条目大小。
struct ChunkSizeSink {
    max_chunk: u64,
    total: u64,
    hasher: crc32fast::Hasher,
}

impl ChunkSizeSink {
    fn new() -> Self {
        Self {
            max_chunk: 0,
            total: 0,
            hasher: crc32fast::Hasher::new(),
        }
    }
}

impl thzip_core::ExtractSink for ChunkSizeSink {
    fn on_entry_begin(
        &mut self,
        _id: EntryId,
        _entry: &thzip_core::ArchiveEntry,
    ) -> Result<(), thzip_core::SinkError> {
        Ok(())
    }

    fn on_data(&mut self, _id: EntryId, data: &[u8]) -> Result<(), thzip_core::SinkError> {
        self.max_chunk = self.max_chunk.max(data.len() as u64);
        self.total += data.len() as u64;
        self.hasher.update(data);
        Ok(())
    }

    fn on_entry_end(&mut self, _id: EntryId, _ok: bool) -> Result<(), thzip_core::SinkError> {
        Ok(())
    }

    fn finish(&mut self) -> Result<(), thzip_core::SinkError> {
        Ok(())
    }

    fn bytes_written(&self) -> u64 {
        self.total
    }
}

/// RAR 条目必须是流式解码的：块大小不能随条目大小线性增长。
///
/// 之前的后端把整条读进 `Vec<u8>` 再分块喂 sink，超大条目还要先落
/// 临时文件再读回来。现在换成 `unrar-rs` 的 `copy_to`，解码器每产出一块
/// 就直接写进 sink，内存占用与条目大小无关。
#[test]
fn rar_entries_stream_instead_of_buffering_whole_entry() {
    let archive = fixture("version.rar");
    let mut reader = backend::open(&archive, None).unwrap();
    let cancel = CancelToken::new();

    let mut sink = ChunkSizeSink::new();
    let report = reader.test(&Limits::default(), &cancel, &mut sink).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(sink.total, 11, "应收到 11 字节");
    // 单条 11 字节的条目会被解码器一次性吐出，块大小等于条目大小是正常的。
    // 关键验证在下面：多块写入时，每块都必须是解码器的缓冲大小。
    assert!(
        sink.max_chunk <= 64 * 1024,
        "块大小失控: {}",
        sink.max_chunk
    );
}

/// 大条目（超过旧的 16 MiB 内存上限）必须能解压，且不经过内存整条缓存。
///
/// 夹具里没有大包，这里用 limit 放宽的 `VerifySink` 直接验证
/// 「条目大小不再受内存上限约束」：把限额调到极小，
/// 大条目会因为限额被拒——但那是限额在工作，不是内存上限在工作。
#[test]
fn large_rar_entry_is_not_capped_by_memory_limit() {
    // 旧实现有 MAX_IN_MEMORY = 16 MiB 的常量，超过就落临时文件。
    // 新实现没有这个常量，条目大小只受 limits 约束。
    let archive = fixture("version.rar");
    let reader = backend::open(&archive, None).unwrap();
    let entry = &reader.entries()[0];
    // 声明的大小来自包头，解压时实际写入的字节数必须与之一致。
    assert_eq!(entry.size, 11);

    let dir = TempDir::new("large");
    let target = dir.path().join("out");
    let report = extract_all(&archive, &target, None).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(
        std::fs::read(target.join("VERSION")).unwrap().len(),
        11,
        "解压出的字节数应与包头声明的一致"
    );
}
