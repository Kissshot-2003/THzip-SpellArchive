use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use thzip_core::backend;
use thzip_core::limits::Limits;
use thzip_core::plan::{DecisionTable, OverwritePolicy};
use thzip_core::safety::ExtractDirSink;
use thzip_core::task::CancelToken;
use thzip_core::types::{ArchiveKind, EntryId};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "thzip-spellarchive-stream-{tag}-{}-{}",
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

fn compress(kind: ArchiveKind, payload: &[u8]) -> Vec<u8> {
    match kind {
        ArchiveKind::Gz => {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(payload).unwrap();
            encoder.finish().unwrap()
        }
        ArchiveKind::Bz2 => {
            let mut encoder =
                bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
            encoder.write_all(payload).unwrap();
            encoder.finish().unwrap()
        }
        ArchiveKind::Xz => {
            let mut encoder =
                lzma_rust2::XzWriter::new(Vec::new(), lzma_rust2::XzOptions::default()).unwrap();
            encoder.write_all(payload).unwrap();
            encoder.finish().unwrap()
        }
        ArchiveKind::Zstd => {
            let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 3).unwrap();
            encoder.write_all(payload).unwrap();
            encoder.finish().unwrap()
        }
        ArchiveKind::Lz4 => {
            let mut encoder = lz4_flex::frame::FrameEncoder::new(Vec::new());
            encoder.write_all(payload).unwrap();
            encoder.finish().unwrap()
        }
        other => panic!("不支持测试容器 {other:?}"),
    }
}

fn suffix(kind: ArchiveKind) -> &'static str {
    match kind {
        ArchiveKind::Gz => "gz",
        ArchiveKind::Bz2 => "bz2",
        ArchiveKind::Xz => "xz",
        ArchiveKind::Zstd => "zst",
        ArchiveKind::Lz4 => "lz4",
        _ => "bin",
    }
}

#[test]
fn single_stream_roundtrip() {
    let dir = TempDir::new("roundtrip");
    let payload = "单文件流压缩测试 - THzip-SpellArchive".as_bytes();
    let limits = Limits::default();

    for kind in [
        ArchiveKind::Gz,
        ArchiveKind::Bz2,
        ArchiveKind::Xz,
        ArchiveKind::Zstd,
        ArchiveKind::Lz4,
    ] {
        let archive = dir.path().join(format!("note.txt.{}", suffix(kind)));
        std::fs::write(&archive, compress(kind, payload)).unwrap();

        let mut reader = backend::open(&archive, None).unwrap();
        assert_eq!(reader.kind(), kind);
        assert_eq!(reader.entries().len(), 1);
        assert_eq!(reader.entries()[0].display.as_ref(), "note.txt");
        assert_eq!(reader.entries()[0].size, payload.len() as u64, "{kind:?}");

        let target = dir.path().join(format!("out-{}", suffix(kind)));
        let plan = reader.plan(&[0 as EntryId], &target, &limits).unwrap();
        let decisions = DecisionTable {
            default: OverwritePolicy::Overwrite,
            ..Default::default()
        };
        let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
        let report = reader
            .extract(&plan, &decisions, &CancelToken::new(), &mut sink)
            .unwrap();
        assert!(report.failed.is_empty(), "{kind:?}: {:?}", report.failed);
        assert_eq!(
            std::fs::read(target.join("note.txt")).unwrap(),
            payload,
            "{kind:?}"
        );

        let mut reader = backend::open(&archive, None).unwrap();
        let mut verify = thzip_core::safety::VerifySink::with_limits(limits);
        let report = reader.test(&CancelToken::new(), &mut verify).unwrap();
        assert!(report.failed.is_empty(), "{kind:?}: {:?}", report.failed);
        assert_eq!(report.succeeded.len(), 1);
    }
}
