use std::path::{Path, PathBuf};

use thzip_core::backend::name_codec::{NameEncoding, NameOverride};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn cp932_fixture_is_detected() {
    let reader = thzip_core::backend::open(&fixture("cp932-names.zip"), None).unwrap();
    let entry = &reader.entries()[0];
    assert_eq!(entry.display.as_ref(), "テスト.txt");
    assert_eq!(entry.name_encoding, NameEncoding::ShiftJis);
    assert!(
        entry.name_confidence >= 60,
        "confidence={}",
        entry.name_confidence
    );
}

#[test]
fn gbk_fixture_is_detected() {
    let reader = thzip_core::backend::open(&fixture("gbk-names.zip"), None).unwrap();
    let entry = &reader.entries()[0];
    assert_eq!(entry.display.as_ref(), "测试.txt");
    assert_eq!(entry.name_encoding, NameEncoding::Gbk);
}

#[test]
fn mixed_fixture_decodes_per_entry() {
    let reader = thzip_core::backend::open(&fixture("mixed-names.zip"), None).unwrap();
    assert_eq!(reader.entries()[0].name_encoding, NameEncoding::Utf8);
    assert_eq!(reader.entries()[1].display.as_ref(), "テスト.txt");
    assert_eq!(reader.entries()[1].name_encoding, NameEncoding::ShiftJis);
}

#[test]
fn name_override_changes_display_without_reopening() {
    let mut reader = thzip_core::backend::open(&fixture("cp932-names.zip"), None).unwrap();

    reader.apply_name_override(NameOverride::Raw);
    let escaped = reader.entries()[0].display.to_string();
    // 「テ」的 CP932 字节 0x83 0x65：0x65 是可打印 ASCII 'e'，按规则保留原样。
    assert!(escaped.starts_with("%83e"), "escaped={escaped}");
    assert_eq!(reader.entries()[0].name_encoding, NameEncoding::Lossy);

    reader.apply_name_override(NameOverride::Auto);
    assert_eq!(reader.entries()[0].display.as_ref(), "テスト.txt");
}

#[test]
fn utf8_flag_bit11_is_authoritative() {
    let reader = thzip_core::backend::open(&fixture("utf8-flag.zip"), None).unwrap();
    let entry = &reader.entries()[0];
    assert_eq!(entry.display.as_ref(), "utf8-名前.txt");
    assert_eq!(entry.name_encoding, NameEncoding::Utf8);
    assert_eq!(entry.name_confidence, 100);
    // bit 11 置位时不做打分：原始字节即 UTF-8。
    assert_eq!(entry.raw_name.as_ref(), "utf8-名前.txt".as_bytes());
}

#[test]
fn info_zip_unicode_path_extension_wins_over_guessing() {
    let reader = thzip_core::backend::open(&fixture("unicode-path.zip"), None).unwrap();
    let entry = &reader.entries()[0];
    // 扩展 CRC 有效时 zip crate 会把 0x7075 的 UTF-8 名字同时也回填到 name_raw()，
    // 因此原始 CP932 字节在这里不可见——但权威名字仍来自扩展。
    assert_eq!(entry.display.as_ref(), "テスト.txt");
    assert_eq!(entry.name_encoding, NameEncoding::Utf8);
    assert_eq!(entry.name_confidence, 100);
}

#[test]
fn broken_unicode_path_crc_falls_back_to_scoring() {
    let reader = thzip_core::backend::open(&fixture("unicode-path-bad-crc.zip"), None).unwrap();
    let entry = &reader.entries()[0];
    // name_crc 不符：0x7075 必须被忽略，原始 CP932 字节保留并回落到打分。
    assert!(
        std::str::from_utf8(entry.raw_name.as_ref()).is_err(),
        "raw 名字应保留 CP932 原始字节"
    );
    assert_eq!(entry.display.as_ref(), "テスト.txt");
    assert_eq!(entry.name_encoding, NameEncoding::ShiftJis);
    // 关键：不能沿用扩展给出的「UTF-8 / 100」，必须是自己打分得出的结论。
    assert!(
        entry.name_confidence >= 60,
        "confidence={}",
        entry.name_confidence
    );
}

#[test]
fn missing_volume_lists_all_numbers() {
    let err = match thzip_core::backend::open(&fixture("split-missing.7z.001"), None) {
        Ok(_) => panic!("expected missing-volume error"),
        Err(err) => err,
    };
    let text = err.to_string();
    assert!(text.contains("003"), "{text}");
    assert!(!text.contains("001"), "{text}");
}
