use std::path::Path;

use criterion::{Criterion, criterion_group, criterion_main};
use thzip_core::backend;
use thzip_core::limits::Limits;
use thzip_core::safety::TempDir;
use thzip_core::task::CancelToken;
use thzip_core::types::{ArchiveKind, Method};
use thzip_core::writer::{scan_inputs, write_entries};

const PAYLOAD_MIB: usize = 4;

fn build_source(dir: &Path) -> std::path::PathBuf {
    let mut state = 0x1234_5678_9ABC_DEF0u64;
    let mut payload = Vec::with_capacity(PAYLOAD_MIB * 1024 * 1024);
    for _ in 0..PAYLOAD_MIB * 1024 * 1024 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        payload.push((state & 0xFF) as u8);
    }
    let path = dir.join("payload.bin");
    std::fs::write(&path, payload).expect("write payload");
    path
}

fn bench_pack(criterion: &mut Criterion) {
    let dir = TempDir::create("thzip-bench-pack").expect("temp dir");
    let source = build_source(dir.path());
    let entries = scan_inputs(&[source], &Limits::default()).expect("scan");
    let archive = dir.path().join("out.7z");

    let mut group = criterion.benchmark_group("pack");
    group.sample_size(10);
    group.bench_function("sevenz_lzma2_4mib", |bencher| {
        bencher.iter(|| {
            let _ = std::fs::remove_file(&archive);
            let mut writer = backend::create_writer(&archive, ArchiveKind::SevenZ, Method::Lzma2)
                .expect("writer");
            write_entries(
                writer.as_mut(),
                &entries,
                Some(Method::Lzma2),
                &CancelToken::new(),
                &|_| {},
            )
            .expect("write entries");
            writer.finish().expect("finish");
        });
    });
    group.finish();
}

criterion_group!(benches, bench_pack);
criterion_main!(benches);
