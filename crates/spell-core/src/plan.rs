use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::error::FailReason;
use crate::safety::bomb::LimitHit;
use crate::safety::path_sanitize::SanitizeError;
use crate::types::{EntryId, EntryKind, Method};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverwritePolicy {
    Ask,
    Overwrite,
    Skip,
    Rename,
    AutoNumber,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanAction {
    Create,
    Overwrite,
    Skip,
    Rename(String),
    Reject(FailReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictKind {
    ExistingFile,
    ExistingDir,
    CaseCollision,
    ReparsePoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub id: EntryId,
    pub path: PathBuf,
    pub kind: ConflictKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanWarning {
    BombSuspected { hit: LimitHit },
    SymlinkSkipped { id: EntryId },
    SuspiciousDisplayName { id: EntryId, detail: String },
    UnsupportedMethod { id: EntryId, method: Method },
    UnsafePath { id: EntryId, issue: SanitizeError },
    DuplicatePath { id: EntryId, renamed_to: String },
    LongPath,
}

#[derive(Debug, Clone)]
pub struct PlannedItem {
    pub id: EntryId,
    pub rel_path: PathBuf,
    pub kind: EntryKind,
    pub size: u64,
    pub action: PlanAction,
}

#[derive(Debug, Clone, Default)]
pub struct ExtractPlan {
    pub items: Vec<PlannedItem>,
    pub conflicts: Vec<Conflict>,
    pub total_bytes: u64,
    pub largest_entry: u64,
    pub warnings: Vec<PlanWarning>,
    pub needs_password: bool,
    pub free_space: Option<u64>,
}

impl ExtractPlan {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn item(&self, id: EntryId) -> Option<&PlannedItem> {
        self.items.iter().find(|item| item.id == id)
    }
}

/// UI 在预检之后填好，worker 只读执行，避免在后台线程同步询问用户。
#[derive(Debug, Clone)]
pub struct DecisionTable {
    pub default: OverwritePolicy,
    pub per_entry: HashMap<EntryId, OverwritePolicy>,
}

impl Default for DecisionTable {
    fn default() -> Self {
        Self {
            default: OverwritePolicy::Ask,
            per_entry: HashMap::new(),
        }
    }
}

impl DecisionTable {
    pub fn resolve(&self, id: EntryId) -> OverwritePolicy {
        self.per_entry.get(&id).copied().unwrap_or(self.default)
    }
}

/// 决策表 + 预检结果解析出的最终动作。后端与 sink 各自调用一次
/// [`resolve_all`]，得到完全一致的结果（`plan.items` 顺序确定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedAction {
    /// 写入（或覆盖）到该路径。目录条目走同一条分支。
    Write {
        path: PathBuf,
        overwrite: bool,
    },
    Skip,
    Reject(FailReason),
}

pub fn resolve_all(
    plan: &ExtractPlan,
    decisions: &DecisionTable,
) -> HashMap<EntryId, ResolvedAction> {
    let conflicted: HashSet<EntryId> = plan.conflicts.iter().map(|c| c.id).collect();
    let mut used: HashSet<String> = HashSet::new();
    let mut resolved = HashMap::with_capacity(plan.items.len());

    for item in &plan.items {
        let action = match &item.action {
            PlanAction::Reject(reason) => ResolvedAction::Reject(reason.clone()),
            PlanAction::Skip => ResolvedAction::Skip,
            PlanAction::Create | PlanAction::Overwrite | PlanAction::Rename(_) => {
                let mut path = item.rel_path.clone();
                let mut overwrite = item.action == PlanAction::Overwrite;
                let mut rename_conflict = false;
                let mut style = SuffixStyle::Tilde;

                if conflicted.contains(&item.id) {
                    match decisions.resolve(item.id) {
                        // worker 无法同步询问，未决的 Ask 一律按跳过处理。
                        OverwritePolicy::Ask | OverwritePolicy::Skip => {
                            resolved.insert(item.id, ResolvedAction::Skip);
                            continue;
                        }
                        OverwritePolicy::Overwrite => overwrite = true,
                        OverwritePolicy::Rename => {
                            // 目录条目不能单独改名，否则子条目仍会写到旧路径。
                            rename_conflict = !item.kind.is_dir();
                            overwrite = false;
                        }
                        OverwritePolicy::AutoNumber => {
                            rename_conflict = !item.kind.is_dir();
                            style = SuffixStyle::Paren;
                            overwrite = false;
                        }
                    }
                }

                // 包内同名 / 大小写冲突：始终改名，不能静默丢文件；
                // Rename/AutoNumber 还要避开磁盘上已存在的文件。
                let mut candidate = path.clone();
                let mut n = 0u32;
                while used.contains(&collision_key(&candidate))
                    || (rename_conflict && candidate.exists())
                {
                    n += 1;
                    candidate = suffix_path(&path, n, style);
                }
                if n > 0 {
                    overwrite = false;
                }
                used.insert(collision_key(&candidate));
                path = candidate;

                ResolvedAction::Write { path, overwrite }
            }
        };
        resolved.insert(item.id, action);
    }

    resolved
}

fn collision_key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

#[derive(Debug, Clone, Copy)]
enum SuffixStyle {
    /// `a.txt` → `a~1.txt`
    Tilde,
    /// `a.txt` → `a (1).txt`
    Paren,
}

fn suffix_path(path: &Path, n: u32, style: SuffixStyle) -> PathBuf {
    match style {
        SuffixStyle::Tilde => with_suffix(path, n),
        SuffixStyle::Paren => with_numbered_suffix(path, n),
    }
}

/// 在扩展名前插入 `~n`：`a/b.txt` → `a/b~1.txt`。
pub fn with_suffix(path: &Path, n: u32) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match path.extension() {
        Some(ext) => format!("{stem}~{n}.{}", ext.to_string_lossy()),
        None => format!("{stem}~{n}"),
    };
    match path.parent() {
        Some(parent) => parent.join(name),
        None => PathBuf::from(name),
    }
}

/// 在扩展名前插入 ` (n)`：`a/b.txt` → `a/b (1).txt`。
pub fn with_numbered_suffix(path: &Path, n: u32) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match path.extension() {
        Some(ext) => format!("{stem} ({n}).{}", ext.to_string_lossy()),
        None => format!("{stem} ({n})"),
    };
    match path.parent() {
        Some(parent) => parent.join(name),
        None => PathBuf::from(name),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DecisionTable, ExtractPlan, OverwritePolicy, PlanAction, PlannedItem, ResolvedAction,
        resolve_all,
    };
    use crate::plan::Conflict;
    use crate::types::EntryKind;
    use std::path::PathBuf;

    fn item(id: u32, name: &str, action: PlanAction) -> PlannedItem {
        PlannedItem {
            id,
            rel_path: PathBuf::from(name),
            kind: EntryKind::File,
            size: 0,
            action,
        }
    }

    #[test]
    fn skips_unresolved_conflicts() {
        let mut plan = ExtractPlan::default();
        plan.items
            .push(item(0, "C:/out/a.txt", PlanAction::Overwrite));
        plan.conflicts.push(Conflict {
            id: 0,
            path: PathBuf::from("C:/out/a.txt"),
            kind: crate::plan::ConflictKind::ExistingFile,
        });

        let resolved = resolve_all(&plan, &DecisionTable::default());
        assert_eq!(resolved[&0], ResolvedAction::Skip);

        let decisions = DecisionTable {
            default: OverwritePolicy::Overwrite,
            ..Default::default()
        };
        let resolved = resolve_all(&plan, &decisions);
        assert_eq!(
            resolved[&0],
            ResolvedAction::Write {
                path: PathBuf::from("C:/out/a.txt"),
                overwrite: true,
            }
        );
    }

    #[test]
    fn renames_duplicates_without_overwriting() {
        let mut plan = ExtractPlan::default();
        plan.items.push(item(0, "a.txt", PlanAction::Create));
        plan.items.push(item(1, "a.txt", PlanAction::Create));

        let resolved = resolve_all(&plan, &DecisionTable::default());
        assert!(matches!(
            resolved[&0],
            ResolvedAction::Write {
                overwrite: false,
                ..
            }
        ));
        assert_eq!(
            resolved[&1],
            ResolvedAction::Write {
                path: PathBuf::from("a~1.txt"),
                overwrite: false,
            }
        );
    }

    fn conflict_plan(path: &std::path::Path) -> ExtractPlan {
        let mut plan = ExtractPlan::default();
        plan.items
            .push(item(0, path.to_str().unwrap(), PlanAction::Overwrite));
        plan.conflicts.push(Conflict {
            id: 0,
            path: path.to_path_buf(),
            kind: crate::plan::ConflictKind::ExistingFile,
        });
        plan
    }

    #[test]
    fn rename_policy_keeps_existing_file() {
        let base = std::env::temp_dir().join(format!(
            "spellarchive-rename-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        let target = base.join("a.txt");
        std::fs::write(&target, b"old").unwrap();
        std::fs::write(base.join("a~1.txt"), b"also old").unwrap();

        let plan = conflict_plan(&target);
        let decisions = DecisionTable {
            default: OverwritePolicy::Rename,
            ..Default::default()
        };
        let resolved = resolve_all(&plan, &decisions);
        assert_eq!(
            resolved[&0],
            ResolvedAction::Write {
                path: base.join("a~2.txt"),
                overwrite: false,
            }
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn auto_number_policy_uses_parenthesized_suffix() {
        let base = std::env::temp_dir().join(format!(
            "spellarchive-number-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        let target = base.join("a.txt");
        std::fs::write(&target, b"old").unwrap();

        let plan = conflict_plan(&target);
        let decisions = DecisionTable {
            default: OverwritePolicy::AutoNumber,
            ..Default::default()
        };
        let resolved = resolve_all(&plan, &decisions);
        assert_eq!(
            resolved[&0],
            ResolvedAction::Write {
                path: base.join("a (1).txt"),
                overwrite: false,
            }
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}
