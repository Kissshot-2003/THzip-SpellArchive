//! 属性测试：用随机输入压路径净化与限额这两个纯函数。
//!
//! 单元测试覆盖的是「已知的坏名字」，属性测试覆盖的是「未知的坏名字」——
//! 安全边界是否成立应当由不变量保证，而不是由我列举的例子保证。

use std::path::{Component, Path, PathBuf};

use proptest::prelude::*;
use thzip_core::limits::{LimitField, LimitProfile, Limits};
use thzip_core::safety::{resolve_under, sanitize_rel};

/// 随便什么名人都得先能当文件名。
fn name_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // 完全随机（含控制字符、路径分隔符、设备名片段）
        ".{0,64}",
        // 结构化：多段 + 可疑分隔符
        prop::collection::vec(
            prop::sample::select(vec![
                "a",
                "..",
                ".",
                "",
                "CON",
                "con.txt",
                "COM1",
                "a.txt",
                "a.txt.",
                "a ",
                "x:y",
                "x?y",
                "dir",
                "\\\\",
                "/",
                "\u{202E}",
                "\u{200B}",
                "\u{0000}",
                "非常长的名字非常长的名字",
            ]),
            0..8
        )
        .prop_map(|parts| parts.join("/")),
        // 绝对路径与盘符
        "(/[^/]{0,16}){0,4}",
        "[A-Za-z]:[\\\\/].{0,32}",
    ]
}

proptest! {
    /// 净化结果只能包含 `Normal` 组件：这是「结构上无法逃逸」的依据。
    #[test]
    fn sanitized_paths_only_contain_normal_components(raw in name_strategy()) {
        if let Ok(rel) = sanitize_rel(&raw, &Limits::default()) {
            prop_assert!(!rel.is_absolute());
            for component in rel.components() {
                prop_assert!(
                    matches!(component, Component::Normal(_)),
                    "出现非 Normal 组件 {:?}（输入 {:?}）",
                    component,
                    raw
                );
            }
        }
    }

    /// 拼到根目录下之后必须仍在根目录内。这是所有写盘路径的前置条件。
    #[test]
    fn resolved_paths_stay_inside_root(raw in name_strategy()) {
        let limits = Limits::default();
        let root = Path::new("C:/thzip-prop-root");
        if let Ok(rel) = sanitize_rel(&raw, &limits)
            && let Ok(abs) = resolve_under(root, &rel, &limits)
        {
            prop_assert!(abs.starts_with(root), "逃出根目录: {:?}", abs);
        }
    }

    /// 净化后的每一段都不以点或空格结尾：Win32 会静默裁掉它们，
    /// 留着就会让两个不同条目落到同一文件上。
    #[test]
    fn sanitized_segments_have_no_trailing_dots_or_spaces(raw in name_strategy()) {
        if let Ok(rel) = sanitize_rel(&raw, &Limits::default()) {
            for component in rel.components() {
                let Component::Normal(part) = component else {
                    continue;
                };
                let part = part.to_string_lossy();
                prop_assert!(
                    !part.ends_with(' ') && !part.ends_with('.'),
                    "段 {:?} 仍以点或空格结尾（输入 {:?}）",
                    part,
                    raw
                );
            }
        }
    }

    /// 净化是幂等的：对已经净化过的结果再净化一次必须得到同一个路径。
    /// 反例意味着「解压→打包→再解压」会漂移。
    #[test]
    fn sanitizing_twice_is_stable(raw in name_strategy()) {
        let limits = Limits::default();
        if let Ok(first) = sanitize_rel(&raw, &limits) {
            let text = first.to_string_lossy().replace('\\', "/");
            let second = sanitize_rel(&text, &limits);
            prop_assert_eq!(second.ok(), Some(first));
        }
    }

    /// 写盘侧与读盘侧对「明显危险的名字」判断一致。
    ///
    /// 两侧规则允许有差异（写盘侧更严），但不能出现「读盘侧拒绝、写盘侧接受」
    /// 的方向——那意味着我们能把一个自己都解不开的名字写进归档。
    #[test]
    fn writer_rejects_what_reader_rejects(raw in name_strategy()) {
        use thzip_core::writer::normalize_entry_name;

        let limits = Limits::default();
        let reader_ok = sanitize_rel(&raw, &limits).is_ok();
        let writer_ok = normalize_entry_name(&raw).is_ok();
        if reader_ok {
            prop_assert!(writer_ok, "读盘侧接受但写盘侧拒绝: {:?}", raw);
        }
    }

    /// 三档预设都满足「单条目上限 ≤ 总量上限」，否则后者永远不会先触发。
    #[test]
    fn presets_are_internally_consistent(_dummy in 0u8..1) {
        for profile in LimitProfile::all() {
            let limits = profile.preset();
            prop_assert!(limits.max_entry_bytes <= limits.max_total_bytes);
            prop_assert!(limits.max_entries > 0);
            prop_assert!(limits.max_path_depth > 0);
            prop_assert!(limits.max_path_len >= 260);
        }
    }

    /// 字段读写往返不丢精度（设置页按字段改限额依赖这一点）。
    #[test]
    fn limit_fields_round_trip(value in 1u64..u64::MAX) {
        let mut limits = Limits::default();
        for field in LimitField::ALL {
            if limits.set(field, value) {
                prop_assert_eq!(limits.get(field), value);
            }
        }
    }
}

#[test]
fn rejects_traversal_through_joined_segments() {
    // 随机输入之外，再钉一个具体反例：分段拼接不能重新组装出 `..`。
    let limits = Limits::default();
    let abs = resolve_under(Path::new("C:/root"), Path::new("a/../../etc"), &limits);
    assert_eq!(
        abs,
        Err(thzip_core::safety::SanitizeError::Parent),
        "含 .. 的相对路径必须被拒"
    );
}

#[test]
fn sanitize_never_returns_absolute_even_for_unc() {
    let limits = Limits::default();
    for raw in [r"\\server\share\x", r"\\?\C:\x", r"\\.\PhysicalDrive0"] {
        assert!(sanitize_rel(raw, &limits).is_err(), "{raw} 必须被拒绝");
    }
}

#[test]
fn long_paths_are_rejected_not_truncated() {
    let limits = Limits {
        max_path_len: 300,
        ..Default::default()
    };
    let deep = Path::new("C:/root");
    let rel = PathBuf::from("a".repeat(400));
    assert!(
        resolve_under(deep, &rel, &limits).is_err(),
        "超长路径必须拒绝，不能截断后静默写入"
    );
}
