use std::io::Write;
use std::path::Path;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use thzip_core::safety::TempDir;

const ENTRY_COUNT: usize = 10_000;

fn build_zip(path: &Path, count: usize) {
    let file = std::fs::File::create(path).expect("create zip");
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for index in 0..count {
        let name = format!("dir{:02}/file{index:05}.txt", index % 100);
        writer.start_file(name, options).expect("start entry");
        writer.write_all(b"thzip benchmark payload").expect("write");
    }
    writer.finish().expect("finish zip");
}

fn bench_list(criterion: &mut Criterion) {
    let dir = TempDir::create("thzip-bench-list").expect("temp dir");
    let archive = dir.path().join("many.zip");
    build_zip(&archive, ENTRY_COUNT);

    criterion.bench_function("open_and_list_10k_zip", |bencher| {
        bencher.iter(|| {
            let reader = thzip_core::backend::open(&archive, None).expect("open");
            black_box(reader.entries().len())
        });
    });

    criterion.bench_function("open_10k_zip_central_directory_only", |bencher| {
        bencher.iter(|| {
            let file = std::fs::File::open(&archive).expect("open file");
            let reader = std::io::BufReader::new(file);
            let archive = zip::ZipArchive::new(reader).expect("parse central directory");
            black_box(archive.len())
        });
    });
}

criterion_group!(benches, bench_list);
criterion_main!(benches);
