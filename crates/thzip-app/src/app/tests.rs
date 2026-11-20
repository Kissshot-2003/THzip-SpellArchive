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
        name_encoding: thzip_core::backend::name_codec::NameEncoding::Utf8,
        name_confidence: 100,
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

#[test]
fn detects_preview_image_extensions() {
    use super::is_preview_image;
    assert!(is_preview_image("a.png"));
    assert!(is_preview_image("a.JPG"));
    assert!(is_preview_image("a.jpeg"));
    assert!(is_preview_image("a.gif"));
    assert!(is_preview_image("a.bmp"));
    assert!(!is_preview_image("a.txt"));
    assert!(!is_preview_image("png"));
}

// ---------------------------------------------------------------------------
// 流程冒烟：打开 → 预检 → 解压 → 取消
// ---------------------------------------------------------------------------
//
// 完整的 egui_kittest 窗口测试需要 `eframe::CreationContext`（真实 wgpu 后端），
// 无头环境跑不了。但这条流程的**业务逻辑**不依赖渲染：打开、预检、解压、
// 取消都在 core 的 `ArchiveReader` 上，走的是 GUI 完全相同的路径。
//
// 这里覆盖的是「逻辑是否还通」，不是「像素是否还对」。

use thzip_core::backend;
use thzip_core::limits::Limits;
use thzip_core::plan::{DecisionTable, OverwritePolicy};
use thzip_core::safety::ExtractDirSink;
use thzip_core::task::CancelToken;
use thzip_core::types::EntryId;
use thzip_core::writer::{PackEntry, PackKind, write_entries};

/// 造一个带两个条目的 zip。
///
/// 走 core 自己的写入器而不是直接用 zip crate——这样测的就是项目真实的
/// 写入路径，也不会给 app crate 引入一个只在测试里用到的依赖。
fn scratch_zip(dir: &std::path::Path) -> std::path::PathBuf {
    let source = dir.join("src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("a.txt"), b"first").unwrap();
    std::fs::write(source.join("b.txt"), b"second").unwrap();

    let path = dir.join("scratch.zip");
    let mut writer = backend::create_writer(
        &path,
        thzip_core::types::ArchiveKind::Zip,
        thzip_core::types::Method::Deflate,
    )
    .expect("创建写入器");

    let entries = vec![
        PackEntry {
            source: source.join("a.txt"),
            name: "a.txt".to_owned(),
            kind: PackKind::File,
            size: 5,
        },
        PackEntry {
            source: source.join("b.txt"),
            name: "b.txt".to_owned(),
            kind: PackKind::File,
            size: 6,
        },
    ];
    write_entries(
        &mut *writer,
        &entries,
        Some(thzip_core::types::Method::Deflate),
        &CancelToken::new(),
        &|_| {},
    )
    .expect("写入 zip");
    writer.finish().expect("收尾");
    path
}

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "thzip-flow-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 打开 → 预检 → 解压 → 校验落盘内容。
///
/// 这一条对应 GUI 里「右键解压到当前文件夹」的完整链路。
#[test]
fn flow_open_plan_extract_roundtrip() {
    let dir = scratch_dir("flow");
    let archive = scratch_zip(&dir);
    let target = dir.join("out");

    // 打开
    let limits = Limits::default();
    let mut reader = backend::open(&archive, None).expect("打开归档");
    assert_eq!(reader.entries().len(), 2, "应列出两个条目");

    // 预检
    let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
    let plan = reader.plan(&ids, &target, &limits).expect("预检");
    assert_eq!(plan.items.len(), 2);
    assert!(
        plan.warnings.is_empty(),
        "预检不该有警告: {:?}",
        plan.warnings
    );
    assert!(!plan.needs_password);

    // 解压
    let decisions = DecisionTable {
        default: OverwritePolicy::Overwrite,
        ..Default::default()
    };
    let cancel = CancelToken::new();
    let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
    let report = reader
        .extract(&plan, &decisions, &cancel, &mut sink)
        .expect("解压");

    assert!(report.failed.is_empty(), "不该有失败: {:?}", report.failed);
    assert_eq!(report.succeeded.len(), 2);
    assert_eq!(std::fs::read(target.join("a.txt")).unwrap(), b"first");
    assert_eq!(std::fs::read(target.join("b.txt")).unwrap(), b"second");

    let _ = std::fs::remove_dir_all(&dir);
}

/// 解压途中取消：必须停下，不能把数据全写出来。
///
/// 这是 GUI 上「取消」按钮背后的路径。取消令牌在第一条数据前就被触发，
/// 结果应当是 Cancelled 而不是「悄悄解压完了」。
#[test]
fn flow_cancel_stops_extraction() {
    let dir = scratch_dir("cancel");
    let archive = scratch_zip(&dir);
    let target = dir.join("out");

    let limits = Limits::default();
    let mut reader = backend::open(&archive, None).expect("打开归档");
    let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
    let plan = reader.plan(&ids, &target, &limits).expect("预检");
    let decisions = DecisionTable {
        default: OverwritePolicy::Overwrite,
        ..Default::default()
    };

    // 先取消，再开始解压
    let cancel = CancelToken::new();
    cancel.cancel();

    let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
    let err = reader
        .extract(&plan, &decisions, &cancel, &mut sink)
        .expect_err("已取消的解压应当报错");

    assert!(
        matches!(err, thzip_core::ArchiveError::Cancelled),
        "应为 Cancelled: {err:?}"
    );
    // 取消得足够早，目标目录不该出现完整内容。
    assert!(!target.join("b.txt").exists(), "取消后不应写完所有条目");

    let _ = std::fs::remove_dir_all(&dir);
}

/// 预检阶段就能看出问题：不需要等到写盘才发现。
///
/// 对应 GUI 上「解压前先看一眼预检面板」的设计。
///
/// 恶意条目用原始字节构造：core 自己的写入器会净化条目名，造不出穿越名。
#[test]
fn flow_precheck_flags_unsafe_names_before_writing() {
    let dir = scratch_dir("precheck");
    let target = dir.join("out");

    // 最小的 zip：本地头 + 数据 + 中央目录 + EOCD，条目名带 `../`。
    let path = dir.join("evil.zip");
    let name = b"../escape.txt";
    let data = b"nope";
    let mut bytes = Vec::new();
    // local file header
    bytes.extend_from_slice(b"PK\x03\x04");
    bytes.extend_from_slice(&20u16.to_le_bytes()); // version
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    bytes.extend_from_slice(&0u16.to_le_bytes()); // method: Stored
    bytes.extend_from_slice(&0u16.to_le_bytes()); // time
    bytes.extend_from_slice(&0u16.to_le_bytes()); // date
    bytes.extend_from_slice(&0u32.to_le_bytes()); // crc
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes()); // csize
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes()); // usize
    bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes()); // extra len
    bytes.extend_from_slice(name);
    bytes.extend_from_slice(data);

    let cd_offset = bytes.len() as u32;
    // central directory
    bytes.extend_from_slice(b"PK\x01\x02");
    bytes.extend_from_slice(&20u16.to_le_bytes()); // version made by
    bytes.extend_from_slice(&20u16.to_le_bytes()); // version needed
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    bytes.extend_from_slice(&0u16.to_le_bytes()); // method
    bytes.extend_from_slice(&0u16.to_le_bytes()); // time
    bytes.extend_from_slice(&0u16.to_le_bytes()); // date
    bytes.extend_from_slice(&0u32.to_le_bytes()); // crc
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes()); // extra len
    bytes.extend_from_slice(&0u16.to_le_bytes()); // comment len
    bytes.extend_from_slice(&0u16.to_le_bytes()); // disk
    bytes.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
    bytes.extend_from_slice(&0u32.to_le_bytes()); // external attrs
    bytes.extend_from_slice(&0u32.to_le_bytes()); // local header offset
    bytes.extend_from_slice(name);

    let cd_size = bytes.len() as u32 - cd_offset;
    // end of central directory
    bytes.extend_from_slice(b"PK\x05\x06");
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&cd_size.to_le_bytes());
    bytes.extend_from_slice(&cd_offset.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());

    std::fs::write(&path, &bytes).unwrap();

    let limits = Limits::default();
    let reader = backend::open(&path, None).expect("打开归档");
    let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
    let plan = reader.plan(&ids, &target, &limits).expect("预检");

    // 预检就该把它挡下来：一个警告 + 一个被拒绝的条目。
    assert!(!plan.warnings.is_empty(), "路径穿越应在预检阶段报警");
    let rejected = plan
        .items
        .iter()
        .filter(|item| matches!(item.action, thzip_core::plan::PlanAction::Reject(_)))
        .count();
    assert_eq!(rejected, 1, "穿越条目应被拒绝");

    let _ = std::fs::remove_dir_all(&dir);
}
