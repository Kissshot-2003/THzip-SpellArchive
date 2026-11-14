use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thzip_core::safety::write_atomic;

/// 当前配置 schema。新增字段时递增，并在 `load` 里补迁移。
pub const CURRENT_SCHEMA: u32 = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub schema: u32,
    pub theme: Option<String>,
    pub locale: String,
    pub prefer_standard: bool,
    pub show_hint: bool,
    pub close_to_tray: bool,
    pub last_dir: Option<String>,
    /// 归档表排序键：name / size / packed / ratio / method / time。
    pub sort_key: Option<String>,
    pub sort_asc: bool,
    /// 主窗口内尺寸（逻辑点）；只恢复大小，不恢复位置，避免换显示器后跑出屏幕。
    pub window_size: Option<[f32; 2]>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            schema: CURRENT_SCHEMA,
            theme: None,
            locale: "zh-CN".to_owned(),
            prefer_standard: false,
            show_hint: true,
            close_to_tray: true,
            last_dir: None,
            sort_key: None,
            sort_asc: true,
            window_size: None,
        }
    }
}

impl Prefs {
    pub fn load() -> Self {
        let config = config_path();
        if let Some(path) = config.as_ref()
            && path.is_file()
        {
            return Self::load_at(config, legacy_config_path());
        }
        // 首次便携启动：优先沿用 %APPDATA% 里的新名配置，再退回旧名配置。
        if is_portable() {
            let appdata = app_data_dir("THzip-SpellArchive").map(|dir| dir.join("prefs.toml"));
            let prefs = Self::load_at(appdata, legacy_config_path());
            if let Some(path) = config.as_ref() {
                let _ = prefs.save_to(path);
            }
            return prefs;
        }
        Self::load_at(config, legacy_config_path())
    }

    fn load_at(config: Option<PathBuf>, legacy: Option<PathBuf>) -> Self {
        if let Some(path) = config.as_ref()
            && let Ok(text) = std::fs::read_to_string(path)
        {
            return Self::parse(&text);
        }
        // 首次以新名字启动：把旧 SpellArchive 配置复制过来，保留主题/语言/目录等设置。
        if let Some(legacy) = legacy.as_ref()
            && let Ok(text) = std::fs::read_to_string(legacy)
        {
            let prefs = Self::parse(&text);
            if let Some(path) = config.as_ref() {
                let _ = prefs.save_to(path);
            }
            return prefs;
        }
        Self::default()
    }

    fn parse(text: &str) -> Self {
        let mut prefs: Prefs = toml::from_str(text).unwrap_or_default();
        // schema 1/2/3 缺字段由 serde default 补齐，这里只需推进版本号。
        if prefs.schema < CURRENT_SCHEMA {
            prefs.schema = CURRENT_SCHEMA;
        }
        prefs
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = config_path() else {
            return Err(std::io::Error::other("找不到用户配置目录"));
        };
        self.save_to(&path)
    }

    fn save_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let text = toml::to_string_pretty(self)
            .map_err(|err| std::io::Error::other(format!("序列化设置失败: {err}")))?;
        write_atomic(path, |file| {
            use std::io::Write;
            file.write_all(text.as_bytes())
        })
    }
}

/// 便携配置路径：`<exe 目录>\config\prefs.toml`。
pub fn portable_config_path(root: &Path) -> PathBuf {
    root.join("config").join("prefs.toml")
}

/// exe 同目录存在 `portable.flag` 或 `themes/` 即视为便携模式。
fn is_portable_dir(dir: &Path) -> bool {
    dir.join("portable.flag").is_file() || dir.join("themes").is_dir()
}

pub fn is_portable() -> bool {
    portable_root().is_some()
}

fn portable_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.to_path_buf();
    is_portable_dir(&dir).then_some(dir)
}

/// 便携模式写 `<exe 目录>\config\`，否则写 `%APPDATA%\THzip-SpellArchive\`。
pub fn config_path() -> Option<PathBuf> {
    if let Some(root) = portable_root() {
        return Some(portable_config_path(&root));
    }
    config_dir().map(|dir| dir.join("prefs.toml"))
}

/// 旧名 `%APPDATA%\SpellArchive\prefs.toml`，只用于一次性迁移。
fn legacy_config_path() -> Option<PathBuf> {
    legacy_config_dir().map(|dir| dir.join("prefs.toml"))
}

fn config_dir() -> Option<PathBuf> {
    app_data_dir("THzip-SpellArchive")
}

fn legacy_config_dir() -> Option<PathBuf> {
    app_data_dir("SpellArchive")
}

fn app_data_dir(name: &str) -> Option<PathBuf> {
    if let Ok(app_data) = std::env::var("APPDATA")
        && !app_data.is_empty()
    {
        return Some(PathBuf::from(app_data).join(name));
    }
    Some(PathBuf::from("."))
}

/// 第二实例把“要打开的文件”写在这里，第一实例轮询取走。
pub fn handoff_path() -> Option<PathBuf> {
    config_path().map(|path| path.with_file_name("handoff.txt"))
}

/// 旧名的 handoff 路径：升级期间若旧版仍在运行，把启动参数交给它。
pub fn legacy_handoff_path() -> Option<PathBuf> {
    legacy_config_dir().map(|dir| dir.join("handoff.txt"))
}

pub fn log_path() -> Option<PathBuf> {
    config_path().map(|path| path.with_file_name("thzip-spellarchive.log"))
}

#[cfg(test)]
mod tests {
    use super::Prefs;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "thzip-prefs-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn portable_detection_uses_flag_or_themes() {
        let dir = temp_dir("portable");
        assert!(!super::is_portable_dir(&dir));

        std::fs::create_dir_all(dir.join("themes")).unwrap();
        assert!(super::is_portable_dir(&dir));
        std::fs::remove_dir_all(dir.join("themes")).unwrap();

        std::fs::write(dir.join("portable.flag"), b"").unwrap();
        assert!(super::is_portable_dir(&dir));

        assert_eq!(
            super::portable_config_path(&dir),
            dir.join("config").join("prefs.toml")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_partial_config_with_defaults() {
        let prefs: Prefs = toml::from_str("theme = \"marisa\"\nprefer_standard = true\n").unwrap();
        assert_eq!(prefs.theme.as_deref(), Some("marisa"));
        assert!(prefs.prefer_standard);
        assert_eq!(prefs.locale, "zh-CN");
        assert!(prefs.show_hint);
        assert!(prefs.close_to_tray);
        assert!(prefs.sort_asc);
        assert_eq!(prefs.sort_key, None);
        assert_eq!(prefs.window_size, None);
        assert_eq!(prefs.schema, super::CURRENT_SCHEMA);
    }

    #[test]
    fn parses_window_size() {
        let prefs: Prefs = toml::from_str("window_size = [1280.0, 800.0]\n").unwrap();
        assert_eq!(prefs.window_size, Some([1280.0, 800.0]));
    }

    #[test]
    fn parses_sort_preferences() {
        let prefs: Prefs = toml::from_str("sort_key = \"size\"\nsort_asc = false\n").unwrap();
        assert_eq!(prefs.sort_key.as_deref(), Some("size"));
        assert!(!prefs.sort_asc);
    }

    #[test]
    fn migrates_legacy_prefs_into_new_path() {
        let dir = std::env::temp_dir().join(format!(
            "thzip-prefs-migrate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let old_dir = dir.join("SpellArchive");
        let new_dir = dir.join("THzip-SpellArchive");
        std::fs::create_dir_all(&old_dir).unwrap();
        let legacy = old_dir.join("prefs.toml");
        std::fs::write(&legacy, "schema = 3\ntheme = \"marisa\"\n").unwrap();

        let prefs = Prefs::load_at(Some(new_dir.join("prefs.toml")), Some(legacy));
        assert_eq!(prefs.theme.as_deref(), Some("marisa"));
        assert_eq!(prefs.schema, super::CURRENT_SCHEMA);
        let migrated = std::fs::read_to_string(new_dir.join("prefs.toml")).unwrap();
        assert!(migrated.contains("marisa"), "{migrated}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn defaults_when_no_config_exists() {
        let dir = std::env::temp_dir().join(format!(
            "thzip-prefs-empty-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let prefs = Prefs::load_at(Some(dir.join("prefs.toml")), Some(dir.join("old.toml")));
        assert_eq!(prefs.locale, "zh-CN");
        assert_eq!(prefs.schema, super::CURRENT_SCHEMA);
    }
}
