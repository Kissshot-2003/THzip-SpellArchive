#![no_main]

use libfuzzer_sys::fuzz_target;
use thzip_core::backend::classify;

const NAMES: [&str; 10] = [
    "a.zip", "a.7z", "a.tar.gz", "a.tgz", "a.txz", "a.tzst", "a.tlz4", "a.7z.001", "a.rar", "noext",
];

fuzz_target!(|data: &[u8]| {
    for name in NAMES {
        std::hint::black_box(classify(data, Some(name)));
    }
    std::hint::black_box(classify(data, None));
});
