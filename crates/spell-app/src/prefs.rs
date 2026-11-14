use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use spell_core::safety::write_atomic;

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
        let Some(path) = config_path() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        let mut prefs: Prefs = toml::from_str(&text).unwrap_or_default();
        // schema 1 没有 close_to_tray；schema 2 没有 sort_key/sort_asc。
        // serde default 已补齐缺失字段，这里只需推进版本号。
        if prefs.schema < CURRENT_SCHEMA {
            prefs.schema = CURRENT_SCHEMA;
        }
        prefs
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = config_path() else {
            return Err(std::io::Error::other("找不到用户配置目录"));
        };
        let text = toml::to_string_pretty(self)
            .map_err(|err| std::io::Error::other(format!("序列化设置失败: {err}")))?;
        write_atomic(&path, |file| {
            use std::io::Write;
            file.write_all(text.as_bytes())
        })
    }
}

/// `%APPDATA%\SpellArchive\prefs.toml`；没有 APPDATA 时退回当前目录。
pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("prefs.toml"))
}

fn config_dir() -> Option<PathBuf> {
    if let Ok(app_data) = std::env::var("APPDATA") {
        if !app_data.is_empty() {
            return Some(PathBuf::from(app_data).join("SpellArchive"));
        }
    }
    Some(PathBuf::from("."))
}

/// 第二实例把“要打开的文件”写在这里，第一实例轮询取走。
pub fn handoff_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("handoff.txt"))
}

pub fn log_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("spellarchive.log"))
}

#[cfg(test)]
mod tests {
    use super::Prefs;

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
}
