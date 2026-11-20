use thzip_core::plan::{Conflict, ConflictKind, PlanWarning};

use super::Locale;
use super::errors::{issue_en, limit_en};

/// 预检警告的中文/英文文案。
pub fn warning(locale: Locale, warning: &PlanWarning) -> String {
    match (locale, warning) {
        (Locale::ZhCn, PlanWarning::BombSuspected { hit }) => {
            format!("疑似压缩炸弹：{hit}")
        }
        (Locale::EnUs, PlanWarning::BombSuspected { hit }) => {
            format!("Possible zip bomb: {}", limit_en(*hit))
        }
        (Locale::ZhCn, PlanWarning::SymlinkSkipped { id }) => {
            format!("跳过链接（条目 {id}）")
        }
        (Locale::EnUs, PlanWarning::SymlinkSkipped { id }) => {
            format!("Skipped link (entry {id})")
        }
        (Locale::ZhCn, PlanWarning::SuspiciousDisplayName { id, detail }) => {
            format!("可疑文件名（条目 {id}）：{detail}")
        }
        (Locale::EnUs, PlanWarning::SuspiciousDisplayName { id, detail }) => {
            format!("Suspicious name (entry {id}): {detail}")
        }
        (Locale::ZhCn, PlanWarning::NameEncodingAmbiguous { id, confidence }) => {
            format!("文件名编码不确定（条目 {id}，置信 {confidence}），可手动切换编码")
        }
        (Locale::EnUs, PlanWarning::NameEncodingAmbiguous { id, confidence }) => {
            format!(
                "Uncertain filename encoding (entry {id}, confidence {confidence}); switch encoding manually"
            )
        }
        (Locale::ZhCn, PlanWarning::UnsupportedMethod { id, method }) => {
            format!("不支持的压缩方法（条目 {id}）：{}", method.label())
        }
        (Locale::EnUs, PlanWarning::UnsupportedMethod { id, method }) => {
            format!("Unsupported method (entry {id}): {}", method.label())
        }
        (Locale::ZhCn, PlanWarning::UnsafePath { id, issue }) => {
            format!("不安全的路径（条目 {id}）：{issue}")
        }
        (Locale::EnUs, PlanWarning::UnsafePath { id, issue }) => {
            format!("Unsafe path (entry {id}): {}", issue_en(*issue))
        }
        (
            Locale::ZhCn,
            PlanWarning::InsufficientSpace {
                required,
                reserved,
                available,
            },
        ) => format!(
            "目标盘空间不足：需要 {}，需保留 {}，当前可用 {}",
            human_size(*required),
            human_size(*reserved),
            human_size(*available)
        ),
        (
            Locale::EnUs,
            PlanWarning::InsufficientSpace {
                required,
                reserved,
                available,
            },
        ) => format!(
            "Not enough space: need {}, {} must stay free, {} available",
            human_size(*required),
            human_size(*reserved),
            human_size(*available)
        ),
        (Locale::ZhCn, PlanWarning::NestedArchive { id, depth }) => {
            format!("包含嵌套归档（条目 {id}，第 {depth} 层）")
        }
        (Locale::EnUs, PlanWarning::NestedArchive { id, depth }) => {
            format!("Nested archive inside (entry {id}, level {depth})")
        }
        (Locale::ZhCn, PlanWarning::LargeFileOnFat32 { id, size }) => format!(
            "单文件 {} 超过 FAT32 的 4 GiB 上限（条目 {id}）",
            human_size(*size)
        ),
        (Locale::EnUs, PlanWarning::LargeFileOnFat32 { id, size }) => format!(
            "{} exceeds the 4 GiB FAT32 limit (entry {id})",
            human_size(*size)
        ),
    }
}

/// 人类可读的字节数。core 里的同类函数不对外公开，界面层自带一份。
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// 预检冲突的中文/英文文案。
pub fn conflict(locale: Locale, conflict: &Conflict) -> String {
    let kind = match (locale, &conflict.kind) {
        (Locale::ZhCn, ConflictKind::ExistingFile) => "已存在同名文件",
        (Locale::ZhCn, ConflictKind::ExistingDir) => "已存在同名文件夹",
        (Locale::ZhCn, ConflictKind::CaseCollision) => "大小写冲突",
        (Locale::ZhCn, ConflictKind::ReparsePoint) => "重解析点",
        (Locale::EnUs, ConflictKind::ExistingFile) => "existing file",
        (Locale::EnUs, ConflictKind::ExistingDir) => "existing folder",
        (Locale::EnUs, ConflictKind::CaseCollision) => "case-only collision",
        (Locale::EnUs, ConflictKind::ReparsePoint) => "reparse point",
    };
    match locale {
        Locale::ZhCn => format!("条目 {}：{kind} — {}", conflict.id, conflict.path.display()),
        Locale::EnUs => format!(
            "Entry {}: {kind} — {}",
            conflict.id,
            conflict.path.display()
        ),
    }
}

/// 列表最多显示多少条，避免预检窗口无限拉长。
pub const MAX_LINES: usize = 12;

#[cfg(test)]
mod tests {
    use super::{conflict, warning};
    use crate::i18n::Locale;
    use std::path::PathBuf;
    use thzip_core::plan::{Conflict, ConflictKind, PlanWarning};
    use thzip_core::safety::LimitHit;

    #[test]
    fn translates_warnings_in_both_languages() {
        let bomb = PlanWarning::BombSuspected {
            hit: LimitHit::Ratio,
        };
        assert!(warning(Locale::ZhCn, &bomb).contains("炸弹"));
        assert!(warning(Locale::EnUs, &bomb).contains("zip bomb"));

        let unsafe_path = PlanWarning::UnsafePath {
            id: 3,
            issue: thzip_core::safety::SanitizeError::Parent,
        };
        assert!(warning(Locale::ZhCn, &unsafe_path).contains("不安全的路径"));
        assert!(warning(Locale::EnUs, &unsafe_path).contains("Unsafe path"));
    }

    #[test]
    fn translates_conflicts_in_both_languages() {
        let item = Conflict {
            id: 2,
            path: PathBuf::from("out/a.txt"),
            kind: ConflictKind::ExistingFile,
        };
        let zh = conflict(Locale::ZhCn, &item);
        let en = conflict(Locale::EnUs, &item);
        assert!(zh.contains("已存在同名文件"), "{zh}");
        assert!(en.contains("existing file"), "{en}");
    }
}
