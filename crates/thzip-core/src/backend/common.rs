use std::path::Path;

use crate::error::ArchiveError;
use crate::limits::Limits;
use crate::plan::{Conflict, ConflictKind, PlanWarning};
use crate::safety::is_reparse_point;
use crate::types::{ArchiveEntry, EntryId, EntryKind};

/// 压缩比是否异常。仅在压缩后大小可知（zip/tar 等非固实容器）且样本足够大时判断，
/// 避免对几 KB 的小文件误报。
pub(crate) fn ratio_suspicious(entry: &ArchiveEntry, limits: &Limits) -> bool {
    const MIN_PACKED: u64 = 1024 * 1024;
    if entry.kind.is_dir() || entry.size == 0 {
        return false;
    }
    let Some(packed) = entry.packed else {
        return false;
    };
    if packed < MIN_PACKED {
        return false;
    }
    entry.size as f64 / packed as f64 > limits.max_ratio_streamed as f64
}

/// 整包压缩比是否异常（L-1）。
///
/// 逐条比较 `size / packed` 有天然盲区：攻击者可以把炸弹切成大量小条目，
/// 每条都低于 `MIN_PACKED` 门槛，逐条判断全部放行，总量却已经失控。
/// 这里补一道整包口径的兜底：只要「解压总量 / 归档体积」越过阈值就报警。
pub(crate) fn ratio_suspicious_totals(unpacked: u64, packed: u64, limits: &Limits) -> bool {
    const MIN_PACKED: u64 = 1024 * 1024;
    if packed < MIN_PACKED || unpacked == 0 {
        return false;
    }
    unpacked as f64 / packed as f64 > limits.max_ratio_streamed as f64
}

/// 文件名是否像归档（用于套娃提示）。
pub(crate) fn is_archive_name(name: &Path) -> bool {
    const EXTENSIONS: [&str; 16] = [
        ".zip", ".7z", ".rar", ".tar", ".gz", ".tgz", ".bz2", ".tbz2", ".tbz", ".xz", ".txz",
        ".zst", ".tzst", ".lz4", ".tlz4", ".cbz",
    ];
    let text = name.to_string_lossy().to_ascii_lowercase();
    EXTENSIONS.iter().any(|ext| text.ends_with(ext))
}

/// 预检阶段的统一安全提示：空间、套娃、FAT32 单文件上限、整包压缩比。
///
/// 各后端在 `plan()` 收尾时调用一次即可，避免四份实现各自漂移。
pub(crate) fn plan_safety_warnings(
    target: &Path,
    items: &[crate::plan::PlannedItem],
    total_bytes: u64,
    packed_total: u64,
    limits: &crate::limits::Limits,
) -> Vec<PlanWarning> {
    use crate::safety::disk::{FileSystem, available_space, filesystem_of};

    let mut warnings = Vec::new();

    if let Some(available) = available_space(target) {
        let reserved = limits.min_free_space;
        if available < total_bytes.saturating_add(reserved) {
            warnings.push(PlanWarning::InsufficientSpace {
                required: total_bytes,
                reserved,
                available,
            });
        }
    }

    if ratio_suspicious_totals(total_bytes, packed_total, limits) {
        // 整包口径异常时借用既有的 Ratio 提示，UI 文案已经就位。
        warnings.push(PlanWarning::BombSuspected {
            hit: crate::safety::bomb::LimitHit::Ratio,
        });
    }

    if matches!(filesystem_of(target), FileSystem::Fat32)
        && let Some(limit) = FileSystem::Fat32.max_file_size()
    {
        for item in items {
            if !item.kind.is_dir() && item.size > limit {
                warnings.push(PlanWarning::LargeFileOnFat32 {
                    id: item.id,
                    size: item.size,
                });
            }
        }
    }

    // 套娃提示只看顶层「文件名本身就是归档」的条目，深度固定为 1；
    // 真正的多层嵌套在解压时由 `max_nesting` 拦截。
    for item in items {
        if item.kind.is_dir() {
            continue;
        }
        if is_archive_name(&item.rel_path) {
            warnings.push(PlanWarning::NestedArchive {
                id: item.id,
                depth: 1,
            });
        }
    }

    warnings
}

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
