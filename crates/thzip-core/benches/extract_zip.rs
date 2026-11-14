use std::io::Write;
use std::path::Path;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use thzip_core::backend;
use thzip_core::error::SinkError;
use thzip_core::limits::Limits;
use thzip_core::reader::ExtractSink;
use thzip_core::safety::TempDir;
use thzip_core::task::CancelToken;
use thzip_core::types::{ArchiveEntry, EntryId};

const PAYLOAD_MIB: usize = 16;

struct CountingSink {
    bytes: u64,
}

impl ExtractSink for CountingSink {
    fn on_entry_begin(&mut self, _id: EntryId, _entry: &ArchiveEntry) -> Result<(), SinkError> {
        Ok(())
    }

    fn on_data(&mut self, _id: EntryId, chunk: &[u8]) -> Result<(), SinkError> {
        self.bytes += chunk.len() as u64;
        Ok(())
    }

    fn on_entry_end(&mut self, _id: EntryId, _ok: bool) -> Result<(), SinkError> {
        Ok(())
    }

    fn bytes_written(&self) -> u64 {
        self.bytes
    }
}

fn build_zip(path: &Path) {
    let payload = "thzip benchmark payload line\n"
        .repeat(PAYLOAD_MIB * 1024 * 1024 / 27)
        .into_bytes();
    let file = std::fs::File::create(path).expect("create zip");
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer.start_file("payload.txt", options).expect("start");
    writer.write_all(&payload).expect("write payload");
    writer.finish().expect("finish");
}

fn bench_extract(criterion: &mut Criterion) {
    let dir = TempDir::create("thzip-bench-extract").expect("temp dir");
    let archive = dir.path().join("payload.zip");
    build_zip(&archive);
    let target = dir.path().to_path_buf();
    let limits = Limits::default();

    criterion.bench_function("extract_16mib_zip_to_sink", |bencher| {
        bencher.iter_batched(
            || {
                let reader = backend::open(&archive, None).expect("open");
                let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
                let plan = reader.plan(&ids, &target, &limits).expect("plan");
                (reader, plan)
            },
            |(mut reader, plan)| {
                let cancel = CancelToken::new();
                let mut sink = CountingSink { bytes: 0 };
                reader
                    .extract(&plan, &Default::default(), &cancel, &mut sink)
                    .expect("extract");
                sink.bytes
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, bench_extract);
criterion_main!(benches);
