use super::{
    RelocateError, RelocateInput, RelocateMode, build_relocate, natural_cmp, search_matches,
    wildcard_match,
};
use std::cmp::Ordering;
use std::collections::BTreeSet;
use thzip_core::types::{ArchiveEntry, EntryKind, Method};

#[test]
fn natural_sort_orders_numbers() {
    assert_eq!(natural_cmp("file2", "file10"), Ordering::Less);
    assert_eq!(natural_cmp("A2", "a10"), Ordering::Less);
    assert_eq!(natural_cmp("文件2", "文件10"), Ordering::Less);
    assert_eq!(natural_cmp("a1", "a1"), Ordering::Equal);
}

#[test]
fn wildcard_matches_globs() {
    assert!(wildcard_match("*.txt", "a.txt"));
    assert!(wildcard_match("*.txt", "dir/a.txt"));
    assert!(wildcard_match("a?c", "abc"));
    assert!(wildcard_match("*", "anything"));
    assert!(wildcard_match("a*b*c", "aXXbYYc"));
    assert!(!wildcard_match("*.txt", "a.zip"));
    assert!(!wildcard_match("a?c", "abbc"));
    assert!(!wildcard_match("b*", "abc"));
}

#[test]
fn supports_regex_search() {
    assert!(search_matches("re:^a.*\\.txt$", "A1.TXT"));
    assert!(!search_matches("re:^a.*\\.txt$", "b.txt"));
    // 非法正则退回子串匹配。
    assert!(search_matches("re:[", "["));
    assert!(search_matches("", "anything"));
}

fn entry(id: u32, name: &str, is_dir: bool) -> ArchiveEntry {
    ArchiveEntry {
        id,
        raw_name: name.as_bytes().to_vec().into(),
        display: name.into(),
        kind: if is_dir {
            EntryKind::Dir
        } else {
            EntryKind::File
        },
        size: 0,
        packed: None,
        crc32: None,
        mtime: None,
        encrypted: false,
        method: Method::Store,
    }
}

#[test]
fn relocate_moves_selected_directory_tree() {
    let entries = [
        entry(0, "a.txt", false),
        entry(1, "c", true),
        entry(2, "c/d.txt", false),
    ];
    let selected = BTreeSet::from([1, 2]);
    let map = build_relocate(
        &entries,
        &selected,
        RelocateMode::Move,
        &RelocateInput::Text("moved".to_owned()),
    )
    .unwrap();
    assert_eq!(
        map,
        vec![(1, "moved/c".to_owned()), (2, "moved/c/d.txt".to_owned())]
    );

    let map = build_relocate(
        &entries,
        &BTreeSet::from([0]),
        RelocateMode::Move,
        &RelocateInput::Text(String::new()),
    )
    .unwrap_err();
    assert!(matches!(map, RelocateError::NoChange));
}

#[test]
fn relocate_renames_single_entry() {
    let entries = [entry(0, "a.txt", false)];
    let map = build_relocate(
        &entries,
        &BTreeSet::from([0]),
        RelocateMode::Rename,
        &RelocateInput::Text("docs/a2.txt".to_owned()),
    )
    .unwrap();
    assert_eq!(map, vec![(0, "docs/a2.txt".to_owned())]);
}

#[test]
fn relocate_batch_rename_expands_template() {
    let entries = [
        entry(0, "photo.jpg", false),
        entry(1, "notes.txt", false),
        entry(2, "dir", true),
        entry(3, "dir/a.txt", false),
        entry(4, "other.txt", false),
    ];

    let map = build_relocate(
        &entries,
        &BTreeSet::from([0, 1]),
        RelocateMode::BatchRename,
        &RelocateInput::Text("{name}_{n}".to_owned()),
    )
    .unwrap();
    assert_eq!(
        map,
        vec![(0, "photo_1.jpg".to_owned()), (1, "notes_2.txt".to_owned())]
    );

    let map = build_relocate(
        &entries,
        &BTreeSet::from([0, 1]),
        RelocateMode::BatchRename,
        &RelocateInput::Text("{n}.{ext}".to_owned()),
    )
    .unwrap();
    assert_eq!(map, vec![(0, "1.jpg".to_owned()), (1, "2.txt".to_owned())]);

    // 选中目录时，子条目跟着目录一起换前缀。
    let map = build_relocate(
        &entries,
        &BTreeSet::from([2]),
        RelocateMode::BatchRename,
        &RelocateInput::Text("folder_{n}".to_owned()),
    )
    .unwrap();
    assert_eq!(
        map,
        vec![(2, "folder_1".to_owned()), (3, "folder_1/a.txt".to_owned())]
    );

    let err = build_relocate(
        &entries,
        &BTreeSet::from([1, 4]),
        RelocateMode::BatchRename,
        &RelocateInput::Text("same".to_owned()),
    )
    .unwrap_err();
    assert!(matches!(err, RelocateError::Duplicate(_)));
}

#[test]
fn relocate_rejects_duplicates_and_self_nesting() {
    let entries = [entry(0, "a.txt", false), entry(1, "b.txt", false)];
    let err = build_relocate(
        &entries,
        &BTreeSet::from([0]),
        RelocateMode::Rename,
        &RelocateInput::Text("b.txt".to_owned()),
    )
    .unwrap_err();
    assert!(matches!(err, RelocateError::Duplicate(_)));

    let entries = [entry(0, "a.txt", false), entry(1, "x/a.txt", false)];
    let err = build_relocate(
        &entries,
        &BTreeSet::from([1]),
        RelocateMode::Move,
        &RelocateInput::Text(String::new()),
    )
    .unwrap_err();
    assert!(matches!(err, RelocateError::Duplicate(_)));

    let entries = [entry(0, "dir", true), entry(1, "dir/x.txt", false)];
    let err = build_relocate(
        &entries,
        &BTreeSet::from([0]),
        RelocateMode::Rename,
        &RelocateInput::Text("dir/sub".to_owned()),
    )
    .unwrap_err();
    assert!(matches!(err, RelocateError::Nested));

    let err = build_relocate(
        &entries,
        &BTreeSet::from([0, 1]),
        RelocateMode::Rename,
        &RelocateInput::Text("x".to_owned()),
    )
    .unwrap_err();
    assert!(matches!(err, RelocateError::OneOnly));
}

#[test]
fn regex_rename_replaces_capture_groups() {
    let entries = [entry(0, "ep01.txt", false), entry(1, "dir/ep02.txt", false)];
    let input = RelocateInput::Regex {
        pattern: r"ep(\d+)".to_owned(),
        replacement: "第$1话".to_owned(),
        case_insensitive: false,
    };
    let map = build_relocate(
        &entries,
        &BTreeSet::from([0, 1]),
        RelocateMode::RegexRename,
        &input,
    )
    .unwrap();
    assert_eq!(
        map,
        vec![
            (0, "第01话.txt".to_owned()),
            (1, "dir/第02话.txt".to_owned())
        ]
    );
}

#[test]
fn regex_rename_honours_case_insensitive_and_rejects_invalid() {
    let entries = [entry(0, "EP01.txt", false)];
    let input = RelocateInput::Regex {
        pattern: "ep".to_owned(),
        replacement: "episode".to_owned(),
        case_insensitive: true,
    };
    let map = build_relocate(
        &entries,
        &BTreeSet::from([0]),
        RelocateMode::RegexRename,
        &input,
    )
    .unwrap();
    assert_eq!(map, vec![(0, "episode01.txt".to_owned())]);

    let invalid = RelocateInput::Regex {
        pattern: "(".to_owned(),
        replacement: "x".to_owned(),
        case_insensitive: false,
    };
    let err = build_relocate(
        &entries,
        &BTreeSet::from([0]),
        RelocateMode::RegexRename,
        &invalid,
    )
    .unwrap_err();
    assert!(matches!(err, RelocateError::InvalidRegex(_)));
}

#[test]
fn regex_rename_moves_directory_children() {
    let entries = [
        entry(0, "Season 1", true),
        entry(1, "Season 1/a.txt", false),
    ];
    let input = RelocateInput::Regex {
        pattern: r"Season (\d+)".to_owned(),
        replacement: "第$1季".to_owned(),
        case_insensitive: true,
    };
    let map = build_relocate(
        &entries,
        &BTreeSet::from([0, 1]),
        RelocateMode::RegexRename,
        &input,
    )
    .unwrap();
    assert_eq!(
        map,
        vec![(0, "第1季".to_owned()), (1, "第1季/a.txt".to_owned())]
    );
}
