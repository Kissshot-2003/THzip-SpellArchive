use std::path::Path;

/// 把任意字节写成归档文件后走一遍打开与列表。
/// 大小上限用于控制 fuzz 语料体积，避免每轮都写一个巨型临时文件。
pub fn exercise(data: &[u8], ext: &str) {
    if data.len() > 1024 * 1024 {
        return;
    }
    let Ok(dir) = thzip_core::safety::TempDir::create("thzip-fuzz") else {
        return;
    };
    let path = dir.path().join(format!("input.{ext}"));
    if std::fs::write(&path, data).is_err() {
        return;
    }
    open_and_list(&path);
}

fn open_and_list(path: &Path) {
    let Ok(reader) = thzip_core::backend::open(path, None) else {
        return;
    };
    let entries = reader.entries();
    std::hint::black_box(entries.len());
    for entry in entries.iter().take(256) {
        std::hint::black_box(entry.size);
        std::hint::black_box(entry.packed);
        std::hint::black_box(&entry.display);
    }
}

/// 归档内名字可以是任意字符串（甚至是任意字节的 lossy 结果）。净化函数要么拒绝，
/// 要么给出一个「不含分隔符、不含上跳段、非绝对路径」的相对路径，且再净化一次结果不变。
pub fn sanitize_name(raw: &str) {
    let limits = thzip_core::limits::Limits::default();
    let Ok(rel) = thzip_core::safety::sanitize_rel(raw, &limits) else {
        return;
    };
    std::hint::black_box(&rel);
    let again = thzip_core::safety::sanitize_rel(&rel.to_string_lossy(), &limits);
    std::hint::black_box(again.ok().map(|path| path == rel));
}
