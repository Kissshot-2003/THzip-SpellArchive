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
            "thzip-spellarchive-tar-{tag}-{}-{}",
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

fn build_tar(dir: &Path) -> Vec<u8> {
    let src = dir.join("src");
    std::fs::create_dir_all(src.join("子目录")).unwrap();
    std::fs::create_dir_all(src.join("空目录")).unwrap();
    let a = src.join("a.txt");
    let b = src.join("子目录/data.bin");
    std::fs::write(&a, b"hello tar").unwrap();
    std::fs::write(&b, [9u8, 8, 7]).unwrap();

    let mut builder = tar::Builder::new(Vec::new());
    builder.append_path_with_name(&a, "a.txt").unwrap();
    builder.append_dir("空目录", src.join("空目录")).unwrap();
    builder
        .append_path_with_name(&b, "子目录/data.bin")
        .unwrap();
    builder.into_inner().unwrap()
}

fn compress(kind: ArchiveKind, tar_bytes: &[u8]) -> Vec<u8> {
    match kind {
        ArchiveKind::Tar => tar_bytes.to_vec(),
        ArchiveKind::TarGz => {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(tar_bytes).unwrap();
            encoder.finish().unwrap()
        }
        ArchiveKind::TarBz2 => {
            let mut encoder =
                bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
            encoder.write_all(tar_bytes).unwrap();
            encoder.finish().unwrap()
        }
        ArchiveKind::TarXz => {
            let mut encoder =
                lzma_rust2::XzWriter::new(Vec::new(), lzma_rust2::XzOptions::default()).unwrap();
            encoder.write_all(tar_bytes).unwrap();
            encoder.finish().unwrap()
        }
        ArchiveKind::TarZst => {
            let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 3).unwrap();
            encoder.write_all(tar_bytes).unwrap();
            encoder.finish().unwrap()
        }
        ArchiveKind::TarLz4 => {
            let mut encoder = lz4_flex::frame::FrameEncoder::new(Vec::new());
            encoder.write_all(tar_bytes).unwrap();
            encoder.finish().unwrap()
        }
        other => panic!("不支持测试容器 {other:?}"),
    }
}

fn extension(kind: ArchiveKind) -> &'static str {
    match kind {
        ArchiveKind::Tar => "tar",
        ArchiveKind::TarGz => "tar.gz",
        ArchiveKind::TarBz2 => "tar.bz2",
        ArchiveKind::TarXz => "tar.xz",
        ArchiveKind::TarZst => "tar.zst",
        ArchiveKind::TarLz4 => "tar.lz4",
        _ => "bin",
    }
}

#[test]
fn tar_family_lists_and_extracts() {
    let dir = TempDir::new("all");
    let tar_bytes = build_tar(dir.path());
    let limits = Limits::default();

    for kind in [
        ArchiveKind::Tar,
        ArchiveKind::TarGz,
        ArchiveKind::TarBz2,
        ArchiveKind::TarXz,
        ArchiveKind::TarZst,
        ArchiveKind::TarLz4,
    ] {
        let archive = dir.path().join(format!("sample.{}", extension(kind)));
        std::fs::write(&archive, compress(kind, &tar_bytes)).unwrap();

        let mut reader = backend::open(&archive, None).unwrap();
        assert_eq!(reader.kind(), kind);
        assert!(
            reader
                .entries()
                .iter()
                .any(|entry| entry.display.as_ref() == "子目录/data.bin"),
            "{kind:?} 缺少条目"
        );

        let target = dir.path().join(format!("out-{}", extension(kind)));
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
        assert!(report.failed.is_empty(), "{kind:?}: {:?}", report.failed);
        assert_eq!(
            std::fs::read(target.join("a.txt")).unwrap(),
            b"hello tar".to_vec(),
            "{kind:?}"
        );
        assert_eq!(
            std::fs::read(target.join("子目录/data.bin")).unwrap(),
            vec![9u8, 8, 7],
            "{kind:?}"
        );
        assert!(target.join("空目录").is_dir(), "{kind:?}");

        let mut reader = backend::open(&archive, None).unwrap();
        let mut verify = thzip_core::safety::VerifySink::with_limits(limits);
        let report = reader.test(&CancelToken::new(), &mut verify).unwrap();
        assert!(report.failed.is_empty(), "{kind:?}: {:?}", report.failed);
        assert!(!report.succeeded.is_empty());
    }
}
