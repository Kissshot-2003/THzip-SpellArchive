use std::path::Path;

use crate::error::ArchiveError;
use crate::plan::{Conflict, ConflictKind};
use crate::safety::is_reparse_point;
use crate::types::{EntryId, EntryKind};

/// 检测目标路径是否已存在，以及是否是重解析点。zip / 7z / tar 共用。
pub(crate) fn detect_conflict(
    id: EntryId,
    path: &Path,
    kind: EntryKind,
) -> Result<Option<Conflict>, ArchiveError> {
    if is_reparse_point(path)? {
        return Ok(Some(Conflict {
            id,
            path: path.to_path_buf(),
            kind: ConflictKind::ReparsePoint,
        }));
    }

    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(ArchiveError::Io(err)),
    };

    let existing_is_dir = meta.is_dir();
    let conflict_kind = if existing_is_dir && kind.is_dir() {
        return Ok(None);
    } else if existing_is_dir {
        ConflictKind::ExistingDir
    } else {
        ConflictKind::ExistingFile
    };

    Ok(Some(Conflict {
        id,
        path: path.to_path_buf(),
        kind: conflict_kind,
    }))
}
