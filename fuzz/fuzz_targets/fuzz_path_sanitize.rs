#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // 合法 UTF-8 直接测；非 UTF-8 也要走一遍 lossy 转换——归档内名字可能
    // 是任意字节，展示层就是这么处理的。
    if let Ok(text) = std::str::from_utf8(data) {
        thzip_fuzz::sanitize_name(text);
    }
    thzip_fuzz::sanitize_name(&String::from_utf8_lossy(data));
});
