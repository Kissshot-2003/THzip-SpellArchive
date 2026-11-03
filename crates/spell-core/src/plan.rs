use std::collections::HashMap;
use std::path::PathBuf;

use crate::types::{EntryId, EntryKind};

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
    Reject(String),
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
    BombSuspected { reason: String },
    SymlinkSkipped { id: EntryId },
    SuspiciousDisplayName { id: EntryId, detail: String },
    UnsupportedMethod { id: EntryId, method: String },
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
