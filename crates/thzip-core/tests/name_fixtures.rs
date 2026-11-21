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
fn missing_volume_lists_all_numbers() {
    let err = match thzip_core::backend::open(&fixture("split-missing.7z.001"), None) {
        Ok(_) => panic!("expected missing-volume error"),
        Err(err) => err,
    };
    let text = err.to_string();
    assert!(text.contains("003"), "{text}");
    assert!(!text.contains("001"), "{text}");
}
