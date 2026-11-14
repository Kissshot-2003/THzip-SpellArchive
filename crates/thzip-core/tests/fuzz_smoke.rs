use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;

use thzip_core::backend;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

fn fixtures() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("fixtures directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    paths.sort();
    paths
}

fn mutate(original: &[u8], seed: u64) -> Vec<u8> {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1));
    let mut data = original.to_vec();
    let flips = 1 + (rng.next() % 24) as usize;
    for _ in 0..flips {
        if data.is_empty() {
            break;
        }
        let index = (rng.next() as usize) % data.len();
        data[index] ^= (rng.next() & 0xFF) as u8;
    }
    if seed.is_multiple_of(5) && !data.is_empty() {
        let keep = (rng.next() as usize) % data.len();
        data.truncate(keep);
    }
    if seed.is_multiple_of(11) {
        let extra = (rng.next() % 32) as usize;
        for _ in 0..extra {
            data.push((rng.next() & 0xFF) as u8);
        }
    }
    data
}

#[test]
fn malformed_inputs_never_panic() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));

    let mut failures = Vec::new();
    let dir = thzip_core::safety::TempDir::create("thzip-smoke").expect("temp dir");

    for fixture in fixtures() {
        let original = std::fs::read(&fixture).expect("read fixture");
        let name = fixture
            .file_name()
            .expect("fixture file name")
            .to_string_lossy()
            .into_owned();
        let file = dir.path().join(&name);

        for seed in 0..48u64 {
            let data = mutate(&original, seed);
            if std::fs::write(&file, &data).is_err() {
                continue;
            }
            let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
                let _ = backend::sniff_file(&file);
                if let Ok(reader) = backend::open(&file, None) {
                    std::hint::black_box(reader.entries().len());
                }
            }));
            if outcome.is_err() {
                failures.push(format!("{} seed={seed}", fixture.display()));
            }
        }
    }

    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    for _ in 0..512 {
        let len = (rng.next() % 600) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| (rng.next() & 0xFF) as u8).collect();
        std::hint::black_box(backend::classify(&bytes, Some("a.7z")));
        std::hint::black_box(backend::classify(&bytes, Some("a.tar.gz")));
        std::hint::black_box(backend::classify(&bytes, None));
    }

    panic::set_hook(default_hook);
    assert!(
        failures.is_empty(),
        "mutated inputs caused panics: {failures:?}"
    );
}
