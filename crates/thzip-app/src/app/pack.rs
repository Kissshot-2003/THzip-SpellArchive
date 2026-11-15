use std::path::PathBuf;

use super::*;
use crate::i18n::TextKey;

impl ThzipApp {
    pub fn open_pack_dialog(&mut self) {
        let output = self.dir.join("archive.zip");
        self.open_pack_with(Vec::new(), output);
    }

    /// 用指定输入和输出打开打包对话框（右键“添加到压缩包”）。
    pub fn open_pack_with(&mut self, sources: Vec<PathBuf>, output: PathBuf) {
        self.pack_state = PackState {
            sources,
            output: output.display().to_string(),
            ..Default::default()
        };
        self.show_pack = true;
        if !self.pack_state.sources.is_empty() {
            let count = self.pack_state.sources.len().to_string();
            self.status = self.tr.tf(
                TextKey::PackReady,
                &[("count", &count), ("path", &self.pack_state.output)],
            );
        }
    }

    pub fn pack_add_files(&mut self) {
        let title = self.tr.t(TextKey::ChoosePackFiles);
        let Some(files) = rfd::FileDialog::new()
            .set_directory(&self.dir)
            .set_title(title)
            .pick_files()
        else {
            return;
        };
        for path in files {
            if !self.pack_state.sources.contains(&path) {
                self.pack_state.sources.push(path);
            }
        }
        self.pack_state.error = None;
    }

    pub fn pack_add_folder(&mut self) {
        let title = self.tr.t(TextKey::ChoosePackFolder);
        let Some(folder) = rfd::FileDialog::new()
            .set_directory(&self.dir)
            .set_title(title)
            .pick_folder()
        else {
            return;
        };
        if !self.pack_state.sources.contains(&folder) {
            self.pack_state.sources.push(folder);
        }
        self.pack_state.error = None;
    }

    pub fn pack_browse_output(&mut self) {
        let current = PathBuf::from(self.pack_state.output.trim());
        let dir = current
            .parent()
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| self.dir.clone());
        let name = current
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "archive.zip".to_owned());
        let Some(mut output) = rfd::FileDialog::new()
            .set_directory(dir)
            .set_file_name(name)
            .add_filter("zip", &["zip"])
            .save_file()
        else {
            return;
        };
        if output.extension().is_none() {
            output.set_extension("zip");
        }
        self.pack_state.output = output.display().to_string();
        self.pack_state.confirm_overwrite = false;
        self.pack_state.error = None;
    }

    pub fn pack_start(&mut self) {
        if self.pack_state.sources.is_empty() {
            let message = self.tr.t(TextKey::PackErrNoSources);
            self.pack_state.error = Some(message);
            return;
        }

        let text = self.pack_state.output.trim();
        if text.is_empty() {
            let message = self.tr.t(TextKey::PackErrNoOutput);
            self.pack_state.error = Some(message);
            return;
        }
        let mut output = PathBuf::from(text);
        if output.extension().is_none() {
            output.set_extension("zip");
        }
        let Some(kind) = kind_for_output(&output) else {
            let message = self.tr.t(TextKey::PackErrUnsupported);
            self.pack_state.error = Some(message);
            return;
        };

        if output.is_dir() {
            let message = self.tr.t(TextKey::PackErrOutputIsDir);
            self.pack_state.error = Some(message);
            return;
        }

        let append = self.pack_state.append;
        if append && !thzip_core::backend::supports_append(kind) {
            let message = self.tr.t(TextKey::PackErrAppendZipOnly);
            self.pack_state.error = Some(message);
            return;
        }

        if append {
            if !output.is_file() {
                let message = self.tr.t(TextKey::PackErrAppendMissing);
                self.pack_state.error = Some(message);
                return;
            }
        } else if output.exists() && !self.pack_state.confirm_overwrite {
            self.pack_state.confirm_overwrite = true;
            let message = self.tr.t(TextKey::PackErrConfirmOverwrite);
            self.pack_state.error = Some(message);
            return;
        }

        let encrypt = self.pack_state.encrypt;
        if encrypt && !matches!(kind, ArchiveKind::Zip | ArchiveKind::SevenZ) {
            let message = self.tr.t(TextKey::PackErrEncryptUnsupported);
            self.pack_state.error = Some(message);
            return;
        }
        if append && encrypt {
            let message = self.tr.t(TextKey::PackErrAppendPassword);
            self.pack_state.error = Some(message);
            return;
        }
        if encrypt && self.pack_state.password.is_empty() {
            let message = self.tr.t(TextKey::PackErrPasswordRequired);
            self.pack_state.error = Some(message);
            return;
        }

        let method = if self.pack_state.store {
            Method::Store
        } else if kind == ArchiveKind::Zip {
            self.pack_state.zip_method.method()
        } else {
            Method::Deflate
        };
        let password = if encrypt {
            Some(SecretString::new(self.pack_state.password.clone()))
        } else {
            None
        };
        let name = file_name(&output);
        let output_text = output.display().to_string();
        self.status = if append {
            self.tr.tf(TextKey::AppendingTo, &[("path", &output_text)])
        } else {
            self.tr.tf(TextKey::PackingTo, &[("path", &output_text)])
        };
        let sevenz = SevenZOptions {
            level: self.pack_state.sevenz_level.min(9),
            dict_size: self.pack_state.sevenz_dict_mib.saturating_mul(1024 * 1024),
            // 自动按 CPU 核数并行分块：7z 创建慢主要慢在单线程 LZMA2 上。
            threads: 0,
        };
        let comment = if kind == ArchiveKind::Zip && !append {
            let text = self.pack_state.comment.trim();
            (!text.is_empty()).then(|| text.to_owned())
        } else {
            None
        };
        let volume_size =
            if kind == ArchiveKind::SevenZ && !append && self.pack_state.volume_mib > 0 {
                Some(u64::from(self.pack_state.volume_mib) * 1024 * 1024)
            } else {
                None
            };
        let handle = self.runner.submit(TaskOp::Pack {
            output,
            inputs: self.pack_state.sources.clone(),
            kind,
            method,
            append,
            password,
            sevenz,
            comment,
            volume_size,
            limits: self.limits,
        });
        let task_key = if append {
            TextKey::TaskAppend
        } else {
            TextKey::TaskPack
        };
        let task_name = self.tr.tf(task_key, &[("name", &name)]);
        self.push_task(handle, task_name);
        self.show_pack = false;
        self.pack_state.password.clear();
        self.pack_state.error = None;
    }

    pub fn apply_pack_template(&mut self, index: usize) {
        let Some(template) = self.pack_templates.get(index).cloned() else {
            return;
        };
        self.pack_state.store = template.store;
        self.pack_state.zip_method = ZipPackMethod::from_key(&template.zip_method);
        self.pack_state.sevenz_level = template.sevenz_level.min(9);
        self.pack_state.sevenz_dict_mib = template.sevenz_dict_mib.min(1024);
        self.pack_state.confirm_overwrite = false;
        self.pack_state.error = None;
    }

    pub fn save_pack_template(&mut self) {
        let name = self.pack_state.template_name.trim().to_owned();
        if name.is_empty() {
            return;
        }
        let template = crate::prefs::PackTemplate {
            name: name.clone(),
            store: self.pack_state.store,
            zip_method: self.pack_state.zip_method.key().to_owned(),
            sevenz_level: self.pack_state.sevenz_level.min(9),
            sevenz_dict_mib: self.pack_state.sevenz_dict_mib.min(1024),
        };
        if let Some(existing) = self
            .pack_templates
            .iter_mut()
            .find(|existing| existing.name == name)
        {
            *existing = template;
        } else {
            self.pack_templates.push(template);
        }
        self.pack_state.template_name.clear();
        self.touch_settings();
        self.status = self.tr.t(TextKey::TemplateSaved);
    }

    pub fn delete_pack_template(&mut self, index: usize) {
        if index < self.pack_templates.len() {
            self.pack_templates.remove(index);
            self.touch_settings();
        }
    }
}

pub(crate) fn kind_for_output(path: &std::path::Path) -> Option<ArchiveKind> {
    let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    for (suffix, kind) in [
        (".tar.gz", ArchiveKind::TarGz),
        (".tgz", ArchiveKind::TarGz),
        (".tar.bz2", ArchiveKind::TarBz2),
        (".tbz2", ArchiveKind::TarBz2),
        (".tbz", ArchiveKind::TarBz2),
        (".tar.xz", ArchiveKind::TarXz),
        (".txz", ArchiveKind::TarXz),
        (".tar.zst", ArchiveKind::TarZst),
        (".tzst", ArchiveKind::TarZst),
        (".tar.lz4", ArchiveKind::TarLz4),
        (".tlz4", ArchiveKind::TarLz4),
        (".zip", ArchiveKind::Zip),
        (".7z", ArchiveKind::SevenZ),
        (".tar", ArchiveKind::Tar),
    ] {
        if name.ends_with(suffix) {
            return Some(kind);
        }
    }
    None
}
