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
