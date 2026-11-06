use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use spell_core::safety::write_atomic;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub schema: u32,
    pub theme: Option<String>,
    pub locale: String,
    pub prefer_standard: bool,
    pub show_hint: bool,
    pub last_dir: Option<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            schema: 1,
            theme: None,
            locale: "zh-CN".to_owned(),
            prefer_standard: false,
            show_hint: true,
            last_dir: None,
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
        toml::from_str(&text).unwrap_or_default()
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
        assert_eq!(prefs.schema, 1);
    }
}
