use criterion::{Criterion, black_box, criterion_group, criterion_main};
use thzip_core::backend::classify;

fn corpus() -> Vec<Vec<u8>> {
    let mut samples = Vec::new();
    let magics: [&[u8]; 9] = [
        &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C],
        b"PK\x03\x04",
        b"Rar!\x1A\x07",
        &[0x1F, 0x8B],
        b"BZh",
        &[0xFD, b'7', b'z', b'X', b'Z', 0x00],
        &[0x28, 0xB5, 0x2F, 0xFD],
        &[0x04, 0x22, 0x4D, 0x18],
        b"ustar",
    ];

    for (index, magic) in magics.iter().enumerate() {
        let mut bytes = vec![0u8; 600];
        let offset = if index == 8 { 257 } else { 0 };
        bytes[offset..offset + magic.len()].copy_from_slice(magic);
        for (position, byte) in bytes.iter_mut().enumerate() {
            *byte ^= (position as u8).wrapping_mul(31);
        }
        samples.push(bytes);
    }

    for seed in 0..8u64 {
        let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let bytes = (0..600)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state & 0xFF) as u8
            })
            .collect();
        samples.push(bytes);
    }

    samples
}

fn bench_classify(criterion: &mut Criterion) {
    let samples = corpus();
    criterion.bench_function("classify_600b", |bencher| {
        bencher.iter(|| {
            for sample in &samples {
                black_box(classify(black_box(sample), Some("a.zip")));
                black_box(classify(black_box(sample), None));
            }
        });
    });
}

criterion_group!(benches, bench_classify);
criterion_main!(benches);
