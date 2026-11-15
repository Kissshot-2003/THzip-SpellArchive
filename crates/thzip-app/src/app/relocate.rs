use std::collections::HashMap;

use super::*;

impl ThzipApp {
    /// 打开重命名 / 移动对话框；真正写盘在后台重写任务里完成。
    pub fn begin_relocate(&mut self, mode: RelocateMode) {
        if self.selected.is_empty() {
            self.status = self.tr.t(TextKey::RelocateNoSelection);
            return;
        }
        if mode == RelocateMode::Rename && self.selected.len() != 1 {
            self.status = self.tr.t(TextKey::RelocateOneOnly);
            return;
        }
        let input = match mode {
            RelocateMode::Rename => self
                .selected
                .iter()
                .next()
                .and_then(|&index| self.entries.get(index))
                .map(|entry| entry.display.to_string())
                .unwrap_or_default(),
            RelocateMode::Move => String::new(),
            RelocateMode::BatchRename => "{name}_{n}".to_owned(),
            RelocateMode::RegexRename => String::new(),
        };
        self.relocate = Some(RelocateDialog {
            mode,
            input,
            regex_pattern: String::new(),
            regex_replacement: String::new(),
            regex_case_insensitive: false,
            error: None,
        });
    }

    pub fn apply_relocate(&mut self) {
        let Some(dialog) = self.relocate.as_ref() else {
            return;
        };
        let mode = dialog.mode;
        let input = match mode {
            RelocateMode::RegexRename => RelocateInput::Regex {
                pattern: dialog.regex_pattern.clone(),
                replacement: dialog.regex_replacement.clone(),
                case_insensitive: dialog.regex_case_insensitive,
            },
            _ => RelocateInput::Text(dialog.input.clone()),
        };
        let Some(path) = self.archive_path.clone() else {
            self.relocate = None;
            self.status = self.tr.t(TextKey::EmptyArchive);
            return;
        };
        let Some(kind) = self.archive_kind else {
            return;
        };

        match build_relocate(&self.entries, &self.selected, mode, &input) {
            Ok(rename) => {
                let count = rename.len();
                let name = self.archive_name.clone();
                // 释放文件句柄，Windows 才能在任务最后由 ReplaceFileW 原子替换。
                self.reader = None;
                let handle = self.runner.submit(TaskOp::Rewrite {
                    path: path.clone(),
                    kind,
                    remove: Vec::new(),
                    rename,
                    limits: self.limits,
                });
                self.pending_rewrite = Some((handle.id, path));
                self.relocate = None;
                let task_name = self.tr.tf(TextKey::TaskRelocate, &[("name", &name)]);
                self.push_task(handle, task_name);
                let count_text = count.to_string();
                self.status = self.tr.tf(TextKey::Relocating, &[("count", &count_text)]);
            }
            Err(err) => {
                let text = self.relocate_error_text(&err);
                if let Some(dialog) = self.relocate.as_mut() {
                    dialog.error = Some(text);
                }
            }
        }
    }

    /// 完成摘要 + 前 5 条失败原因，作为任务行悬停详情。
    pub(crate) fn summarize_failures(
        &self,
        summary: &str,
        failed: &[(thzip_core::EntryId, thzip_core::error::FailReason)],
    ) -> String {
        if failed.is_empty() {
            return summary.to_owned();
        }
        let mut detail = format!("{summary}\n{}", self.tr.t(TextKey::FailedEntries));
        for (id, reason) in failed.iter().take(5) {
            let text = crate::i18n::errors::reason(self.locale, reason);
            detail.push('\n');
            detail.push_str(&self.tr.tf(
                TextKey::FailedEntryLine,
                &[("id", &id.to_string()), ("error", &text)],
            ));
        }
        if failed.len() > 5 {
            detail.push_str("\n…");
        }
        detail
    }

    fn relocate_error_text(&self, err: &RelocateError) -> String {
        match err {
            RelocateError::NoSelection => self.tr.t(TextKey::RelocateNoSelection),
            RelocateError::OneOnly => self.tr.t(TextKey::RelocateOneOnly),
            RelocateError::Invalid => self.tr.t(TextKey::RelocateInvalid),
            RelocateError::InvalidRegex(error) => self
                .tr
                .tf(TextKey::RelocateInvalidRegex, &[("error", error)]),
            RelocateError::Duplicate(name) => {
                self.tr.tf(TextKey::RelocateDuplicate, &[("name", name)])
            }
            RelocateError::Nested => self.tr.t(TextKey::RelocateNested),
            RelocateError::NoChange => self.tr.t(TextKey::RelocateNoChange),
        }
    }
}

/// 拆分文件名与扩展名；隐藏文件（`.gitignore`）不把点当扩展名分隔符。
pub(crate) fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(position) if position > 0 => (&name[..position], &name[position + 1..]),
        _ => (name, ""),
    }
}

/// 展开批量改名模板：`{name}` 原名、`{n}` 序号、`{ext}` 扩展名。
///
/// 文件默认保留原扩展名；模板里写了 `{ext}` 则由模板决定扩展名。
pub(crate) fn expand_batch_name(
    pattern: &str,
    root: &str,
    is_dir: bool,
    number: usize,
) -> Result<String, RelocateError> {
    if pattern.trim().is_empty() {
        return Err(RelocateError::Invalid);
    }
    let name = root.rsplit('/').next().unwrap_or(root);
    let (stem, ext) = if is_dir { (name, "") } else { split_name(name) };
    let mut result = pattern
        .replace("{name}", stem)
        .replace("{n}", &number.to_string())
        .replace("{ext}", ext);
    if !is_dir && !ext.is_empty() && !pattern.contains("{ext}") {
        result.push('.');
        result.push_str(ext);
    }
    if result.trim().is_empty() {
        return Err(RelocateError::Invalid);
    }
    Ok(result)
}

/// 根据选中条目生成改名映射：目录会自动带上全部子条目。
///
/// `Rename` 只处理一个条目，输入是完整新路径；
/// `Move` 把全部选中条目移动到输入的归档内文件夹，空输入表示根目录。
pub(crate) fn build_relocate(
    entries: &[ArchiveEntry],
    selected: &BTreeSet<usize>,
    mode: RelocateMode,
    input: &RelocateInput,
) -> Result<Vec<(u32, String)>, RelocateError> {
    if selected.is_empty() {
        return Err(RelocateError::NoSelection);
    }
    if mode == RelocateMode::Rename && selected.len() != 1 {
        return Err(RelocateError::OneOnly);
    }

    let mut roots: Vec<(usize, String, bool)> = selected
        .iter()
        .filter_map(|&index| {
            entries.get(index).map(|entry| {
                (
                    index,
                    entry.display.trim_end_matches('/').to_owned(),
                    entry.kind.is_dir(),
                )
            })
        })
        .collect();
    if roots.is_empty() {
        return Err(RelocateError::NoSelection);
    }
    // 浅层目录优先，避免选中目录后其子条目被重复处理。
    roots.sort_by_key(|(_, path, _)| path.matches('/').count());
    let mut chosen: Vec<(usize, String, bool)> = Vec::new();
    for (index, path, is_dir) in roots {
        let nested = chosen
            .iter()
            .any(|(_, root, _)| path == *root || path.starts_with(&format!("{root}/")));
        if !nested {
            chosen.push((index, path, is_dir));
        }
    }

    let raw_input = input.text().trim();
    let regex = match input {
        RelocateInput::Regex {
            pattern,
            replacement,
            case_insensitive,
        } => {
            if pattern.is_empty() {
                return Err(RelocateError::Invalid);
            }
            let compiled = regex::RegexBuilder::new(pattern)
                .case_insensitive(*case_insensitive)
                .build()
                .map_err(|err| RelocateError::InvalidRegex(err.to_string()))?;
            Some((compiled, replacement.clone()))
        }
        RelocateInput::Text(_) => None,
    };

    let mut rename: HashMap<usize, String> = HashMap::new();
    for (order, (_, root, is_dir)) in chosen.iter().enumerate() {
        let final_root = match mode {
            RelocateMode::Rename => {
                thzip_core::normalize_entry_name(raw_input).map_err(|_| RelocateError::Invalid)?
            }
            RelocateMode::Move => {
                let dst = if raw_input.is_empty() || raw_input == "/" || raw_input == "." {
                    String::new()
                } else {
                    thzip_core::normalize_entry_name(raw_input)
                        .map_err(|_| RelocateError::Invalid)?
                };
                let base = root.rsplit('/').next().unwrap_or(root);
                if dst.is_empty() {
                    base.to_owned()
                } else {
                    format!("{dst}/{base}")
                }
            }
            RelocateMode::BatchRename => {
                let base = expand_batch_name(raw_input, root, *is_dir, order + 1)?;
                match root.rsplit_once('/') {
                    Some((parent, _)) => format!("{parent}/{base}"),
                    None => base,
                }
            }
            RelocateMode::RegexRename => {
                let Some((regex, replacement)) = regex.as_ref() else {
                    return Err(RelocateError::Invalid);
                };
                let base = root.rsplit('/').next().unwrap_or(root);
                let renamed = regex.replace_all(base, replacement.as_str());
                if renamed.trim().is_empty() {
                    return Err(RelocateError::Invalid);
                }
                match root.rsplit_once('/') {
                    Some((parent, _)) => format!("{parent}/{renamed}"),
                    None => renamed.into_owned(),
                }
            }
        };
        let final_root =
            thzip_core::normalize_entry_name(&final_root).map_err(|_| RelocateError::Invalid)?;

        if *is_dir && final_root.starts_with(&format!("{root}/")) {
            return Err(RelocateError::Nested);
        }

        let prefix = format!("{root}/");
        for (index, entry) in entries.iter().enumerate() {
            let path = entry.display.trim_end_matches('/');
            if path != root && !path.starts_with(&prefix) {
                continue;
            }
            let suffix = &path[root.len()..];
            let final_name = thzip_core::normalize_entry_name(&format!("{final_root}{suffix}"))
                .map_err(|_| RelocateError::Invalid)?;
            if final_name != path {
                rename.insert(index, final_name);
            }
        }
    }

    if rename.is_empty() {
        return Err(RelocateError::NoChange);
    }

    let mut seen: HashMap<String, usize> = HashMap::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let final_name = rename
            .get(&index)
            .cloned()
            .unwrap_or_else(|| entry.display.trim_end_matches('/').to_owned());
        if let Some(previous) = seen.insert(final_name.to_lowercase(), index)
            && (rename.contains_key(&index) || rename.contains_key(&previous))
        {
            return Err(RelocateError::Duplicate(final_name));
        }
    }

    let mut result: Vec<(u32, String)> = rename
        .into_iter()
        .map(|(index, name)| (index as u32, name))
        .collect();
    result.sort_by_key(|(id, _)| *id);
    Ok(result)
}
