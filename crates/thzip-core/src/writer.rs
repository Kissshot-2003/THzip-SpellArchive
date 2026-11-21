use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{ArchiveError, FailReason, NameIssue, path_text};
use crate::limits::Limits;
use crate::safety::bomb::LimitHit;
use crate::task::CancelToken;
use crate::types::Method;

const MAX_NAME_LEN: usize = 32_767;
const MAX_DEPTH: usize = 64;

#[derive(Debug, Default)]
pub struct FinishReport {
    pub entries: u32,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub elapsed: Duration,
}

/// 7z LZMA2 写入参数；`dict_size = 0` 表示沿用压缩级别的默认字典。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SevenZOptions {
    /// 压缩级别 0–9。
    pub level: u32,
    /// 字典大小（字节）；0 = 自动。
    pub dict_size: u32,
    /// LZMA2 并行分块使用的线程数；0 = 自动（按 CPU 核数，上限 8）。
    ///
    /// LZMA2 原生支持「独立分块 + 多流」，每块可独立编解码，因此并行是
    /// 无损的正确性前提下最快的手段。代价是块边界会切断跨块匹配，压缩率
    /// 略降——把块大小设成字典的几倍即可把这个损失压到很低。
    pub threads: u32,
}

impl Default for SevenZOptions {
    fn default() -> Self {
        // level 3 使用 Fast + Hc4 match finder，绕开 level 4–9 的 Bt4 树搜索。
        // 实测（4 MiB 数据）：随机数据与 level 6 持平（~1.0s），但文本从 343ms
        // 降到 83ms（4x），压缩率仅从 872 B 降到 950 B（约 9%）。
        // 追求极致压缩率的用户仍可用 --7z-level 6–9。
        Self {
            level: 3,
            dict_size: 0,
            threads: 0,
        }
    }
}

/// 把用户输入 / 磁盘文件名规范成归档内名字。
///
/// 统一用 `/`，折叠空段与 `.`，拒绝绝对路径、`..`、盘符、NTFS ADS 与 NUL。
pub fn normalize_entry_name(name: &str) -> Result<String, ArchiveError> {
    let name = name.replace('\\', "/");
    if name.starts_with('/') {
        return Err(ArchiveError::Other(FailReason::InvalidName(
            NameIssue::Absolute,
        )));
    }

    let mut parts = Vec::new();
    for segment in name.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Err(ArchiveError::Other(FailReason::InvalidName(
                NameIssue::Parent,
            )));
        }
        if segment.contains(':') {
            return Err(ArchiveError::Other(FailReason::InvalidName(
                NameIssue::DriveOrAds,
            )));
        }
        if segment.contains('\0') {
            return Err(ArchiveError::Other(FailReason::InvalidName(NameIssue::Nul)));
        }

        // L-4：写入侧必须不弱于读取侧，否则「打得进去、解不出来」。
        // 之前只查了绝对路径 / `..` / 盘符 / NUL，保留设备名、非法字符、
        // 段尾句点空格、段长全部漏检，产物在 Windows 上根本落不了盘。
        if let Some(bad) = crate::safety::path_sanitize::find_illegal(segment) {
            return Err(ArchiveError::Other(FailReason::InvalidName(
                NameIssue::IllegalChar(bad),
            )));
        }
        // Win32 会静默截断段尾的句点与空格，两端统一按截断后的结果记录，
        // 保证「写入 → 解出」得到同一个名字。
        let trimmed = segment.trim_end_matches([' ', '.']);
        if trimmed.is_empty() {
            return Err(ArchiveError::Other(FailReason::InvalidName(
                NameIssue::Empty,
            )));
        }
        if crate::safety::path_sanitize::is_reserved_name(trimmed) {
            return Err(ArchiveError::Other(FailReason::InvalidName(
                NameIssue::Reserved,
            )));
        }
        if trimmed.len() > 255 {
            return Err(ArchiveError::Other(FailReason::InvalidName(
                NameIssue::NameTooLong,
            )));
        }
        parts.push(trimmed);
    }

    if parts.is_empty() {
        return Err(ArchiveError::Other(FailReason::InvalidName(
            NameIssue::Empty,
        )));
    }
    if parts.len() > MAX_DEPTH {
        return Err(ArchiveError::LimitExceeded(LimitHit::EntryPathDepth));
    }

    let joined = parts.join("/");
    if joined.len() > MAX_NAME_LEN {
        return Err(ArchiveError::LimitExceeded(LimitHit::EntryNameLength));
    }
    Ok(joined)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackKind {
    File,
    Dir,
}

#[derive(Debug, Clone)]
pub struct PackEntry {
    pub source: PathBuf,
    pub name: String,
    pub kind: PackKind,
    pub size: u64,
}

impl PackEntry {
    pub fn is_dir(&self) -> bool {
        self.kind == PackKind::Dir
    }
}

/// 把输入路径展开成归档条目列表（目录递归、跳过符号链接、拒绝重名）。
pub fn scan_inputs(inputs: &[PathBuf], limits: &Limits) -> Result<Vec<PackEntry>, ArchiveError> {
    if inputs.is_empty() {
        return Err(ArchiveError::Other(FailReason::NoInputs));
    }

    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    for input in inputs {
        let base = input
            .file_name()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                ArchiveError::Other(FailReason::InputNotRoot {
                    path: path_text(input),
                })
            })?
            .to_string_lossy();
        let name = normalize_entry_name(&base)?;
        walk(input, &name, 0, limits, &mut entries, &mut seen)?;
    }
    Ok(entries)
}

fn walk(
    path: &Path,
    name: &str,
    depth: usize,
    limits: &Limits,
    entries: &mut Vec<PackEntry>,
    seen: &mut HashSet<String>,
) -> Result<(), ArchiveError> {
    if depth > MAX_DEPTH || depth > limits.max_path_depth {
        return Err(ArchiveError::LimitExceeded(LimitHit::ScanDepth));
    }
    if entries.len() >= limits.max_entries as usize {
        return Err(ArchiveError::LimitExceeded(LimitHit::ScanEntries));
    }

    let meta = std::fs::symlink_metadata(path).map_err(|err| {
        ArchiveError::Other(FailReason::ReadFailed {
            path: path_text(path),
            detail: err.to_string(),
        })
    })?;
    let file_type = meta.file_type();

    // 符号链接不跟随，避免把链接目标复制进归档或形成递归。
    if file_type.is_symlink() {
        return Ok(());
    }

    let key = name.to_lowercase();
    if !seen.insert(key) {
        return Err(ArchiveError::Other(FailReason::DuplicateInputName {
            name: name.to_owned(),
        }));
    }

    if file_type.is_dir() {
        entries.push(PackEntry {
            source: path.to_path_buf(),
            name: name.to_owned(),
            kind: PackKind::Dir,
            size: 0,
        });

        let mut children = Vec::new();
        for item in std::fs::read_dir(path).map_err(|err| {
            ArchiveError::Other(FailReason::ListFailed {
                path: path_text(path),
                detail: err.to_string(),
            })
        })? {
            let item = item.map_err(|err| {
                ArchiveError::Other(FailReason::ListFailed {
                    path: path_text(path),
                    detail: err.to_string(),
                })
            })?;
            let child_name = item.file_name();
            if child_name.is_empty() {
                continue;
            }
            children.push((child_name, item.path()));
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));

        for (child_name, child_path) in children {
            let child_name = child_name.to_string_lossy();
            let child = normalize_entry_name(&format!("{name}/{child_name}"))?;
            walk(&child_path, &child, depth + 1, limits, entries, seen)?;
        }
    } else if file_type.is_file() {
        entries.push(PackEntry {
            source: path.to_path_buf(),
            name: name.to_owned(),
            kind: PackKind::File,
            size: meta.len(),
        });
    }
    Ok(())
}

/// 按扫描顺序写入归档。`progress` 收到的是**全部输入累计**的已读字节。
pub fn write_entries(
    writer: &mut dyn ArchiveWriter,
    entries: &[PackEntry],
    method: Option<Method>,
    cancel: &CancelToken,
    progress: &dyn Fn(u64),
) -> Result<(), ArchiveError> {
    let mut done = 0u64;
    for entry in entries {
        if cancel.is_cancelled() {
            return Err(ArchiveError::Cancelled);
        }
        if entry.is_dir() {
            writer.add_dir(&entry.source, &entry.name)?;
            continue;
        }

        let base = done;
        writer.add(&entry.source, &entry.name, method, cancel, &|bytes| {
            progress(base.saturating_add(bytes));
        })?;
        done = done.saturating_add(entry.size);
        progress(done);
    }
    Ok(())
}

pub trait ArchiveWriter: Send {
    fn add(
        &mut self,
        _src: &Path,
        _name_in_archive: &str,
        _method: Option<Method>,
        _cancel: &CancelToken,
        _progress: &dyn Fn(u64),
    ) -> Result<(), ArchiveError> {
        Err(ArchiveError::Unsupported(FailReason::BackendMissingWrite))
    }

    fn add_dir(&mut self, _src: &Path, _name_in_archive: &str) -> Result<(), ArchiveError> {
        Err(ArchiveError::Unsupported(FailReason::BackendMissingWrite))
    }

    fn set_comment(&mut self, _text: &str) -> Result<(), ArchiveError> {
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<FinishReport, ArchiveError> {
        Err(ArchiveError::Unsupported(FailReason::BackendMissingWrite))
    }
}

#[cfg(test)]
mod tests {
    use super::SevenZOptions;

    /// 默认级别锁定在 3（Fast + Hc4）：文本数据比 level 6 快约 4 倍，
    /// 随机数据持平，压缩率仅降约 9%。改回 6 会让性能回退且 CI 全绿无人察觉，
    /// 所以用一个断言把它钉住。理由见 README 性能基线与 HANDOVER §16.6。
    #[test]
    fn sevenz_default_level_is_speed_optimized() {
        assert_eq!(SevenZOptions::default().level, 3);
    }
}
