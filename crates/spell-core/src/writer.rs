use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::ArchiveError;
use crate::limits::Limits;
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

/// 把用户输入 / 磁盘文件名规范成归档内名字。
///
/// 统一用 `/`，折叠空段与 `.`，拒绝绝对路径、`..`、盘符、NTFS ADS 与 NUL。
pub fn normalize_entry_name(name: &str) -> Result<String, ArchiveError> {
    let name = name.replace('\\', "/");
    if name.starts_with('/') {
        return Err(ArchiveError::Other("归档内名字不能是绝对路径".to_owned()));
    }

    let mut parts = Vec::new();
    for segment in name.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Err(ArchiveError::Other("归档内名字不能包含 ..".to_owned()));
        }
        if segment.contains(':') {
            return Err(ArchiveError::Other(
                "归档内名字不能包含盘符或 NTFS 交换数据流".to_owned(),
            ));
        }
        if segment.contains('\0') {
            return Err(ArchiveError::Other("归档内名字不能包含 NUL".to_owned()));
        }
        parts.push(segment);
    }

    if parts.is_empty() {
        return Err(ArchiveError::Other("归档内名字为空".to_owned()));
    }
    if parts.len() > MAX_DEPTH {
        return Err(ArchiveError::LimitExceeded("归档内路径层级超过上限"));
    }

    let joined = parts.join("/");
    if joined.len() > MAX_NAME_LEN {
        return Err(ArchiveError::LimitExceeded("归档内名字过长"));
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
        return Err(ArchiveError::Other("没有选择要打包的文件".to_owned()));
    }

    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    for input in inputs {
        let base = input
            .file_name()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| ArchiveError::Other(format!("无法作为归档根：{}", input.display())))?
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
        return Err(ArchiveError::LimitExceeded("打包路径层级超过上限"));
    }
    if entries.len() >= limits.max_entries as usize {
        return Err(ArchiveError::LimitExceeded("打包条目数超过上限"));
    }

    let meta = std::fs::symlink_metadata(path)
        .map_err(|err| ArchiveError::Other(format!("无法读取 {}: {err}", path.display())))?;
    let file_type = meta.file_type();

    // 符号链接不跟随，避免把链接目标复制进归档或形成递归。
    if file_type.is_symlink() {
        return Ok(());
    }

    let key = name.to_lowercase();
    if !seen.insert(key) {
        return Err(ArchiveError::Other(format!(
            "输入中存在同名条目（Windows 不区分大小写）：{name}"
        )));
    }

    if file_type.is_dir() {
        entries.push(PackEntry {
            source: path.to_path_buf(),
            name: name.to_owned(),
            kind: PackKind::Dir,
            size: 0,
        });

        let mut children = Vec::new();
        for item in std::fs::read_dir(path)
            .map_err(|err| ArchiveError::Other(format!("无法列出 {}: {err}", path.display())))?
        {
            let item = item.map_err(|err| {
                ArchiveError::Other(format!("无法列出 {}: {err}", path.display()))
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
        Err(ArchiveError::Unsupported(
            "该格式的写入后端尚未接入（骨架阶段）".to_owned(),
        ))
    }

    fn add_dir(&mut self, _src: &Path, _name_in_archive: &str) -> Result<(), ArchiveError> {
        Err(ArchiveError::Unsupported(
            "该格式的写入后端尚未接入（骨架阶段）".to_owned(),
        ))
    }

    fn set_comment(&mut self, _text: &str) -> Result<(), ArchiveError> {
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<FinishReport, ArchiveError> {
        Err(ArchiveError::Unsupported(
            "该格式的写入后端尚未接入（骨架阶段）".to_owned(),
        ))
    }
}
