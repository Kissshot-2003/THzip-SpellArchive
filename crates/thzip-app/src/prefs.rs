use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thzip_core::limits::Limits;
use thzip_core::safety::write_atomic;

use crate::limit_fields::{LimitField, normalize};

/// 当前配置 schema。新增字段时递增，并在 `load` 里补迁移。
pub const CURRENT_SCHEMA: u32 = 6;

/// 打包任务模板：只保存格式与压缩参数，绝不保存密码。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PackTemplate {
    pub name: String,
    pub store: bool,
    pub zip_method: String,
    pub sevenz_level: u32,
    pub sevenz_dict_mib: u32,
}

/// 限额的落盘形态。`Limits` 不带 serde，这里按字段镜像一份，
/// 读写都走 `LimitField`，字段增减时不会漏项。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LimitPrefs {
    pub max_total_bytes: u64,
    pub max_entry_bytes: u64,
    pub max_entries: u64,
    pub max_ratio_streamed: u64,
    pub max_path_depth: u64,
    pub max_path_len: u64,
    pub max_dict_size: u64,
    /// 套娃层数上限。旧配置文件没有这一项，读出 0 时回落预设值。
    pub max_nesting: u64,
    /// 解压后要保留的可用空间。同上。
    pub min_free_space: u64,
}

impl Default for LimitPrefs {
    fn default() -> Self {
        Self::from_limits(&Limits::default())
    }
}

impl LimitPrefs {
    pub fn from_limits(limits: &Limits) -> Self {
        let mut prefs = Self {
            max_total_bytes: 0,
            max_entry_bytes: 0,
            max_entries: 0,
            max_ratio_streamed: 0,
            max_path_depth: 0,
            max_path_len: 0,
            max_dict_size: 0,
            max_nesting: 0,
            min_free_space: 0,
        };
        for field in LimitField::ALL {
            prefs.write(field, limits.get(field));
        }
        prefs
    }

    /// 还原成 core 的限额。
    ///
    /// 0 表示「这一项没配置」——旧版本写下的文件缺字段、或用户手改删掉了。
    /// 这种情况必须保留预设值：把上限写成 0 会让所有解压都被限额拦住。
    /// 非 0 但越界的值由 `set` 拒绝，同样保留预设值，不做静默截断。
    pub fn to_limits(&self) -> Limits {
        let mut limits = Limits::default();
        for field in LimitField::ALL {
            let value = self.read(field);
            if value == 0 {
                continue;
            }
            let _ = limits.set(field, value);
        }
        normalize(&mut limits);
        limits
    }

    pub fn read(&self, field: LimitField) -> u64 {
        match field {
            LimitField::TotalBytes => self.max_total_bytes,
            LimitField::EntryBytes => self.max_entry_bytes,
            LimitField::Entries => self.max_entries,
            LimitField::RatioStreamed => self.max_ratio_streamed,
            LimitField::PathDepth => self.max_path_depth,
            LimitField::PathLen => self.max_path_len,
            LimitField::DictSize => self.max_dict_size,
            LimitField::Nesting => self.max_nesting,
            LimitField::MinFreeSpace => self.min_free_space,
        }
    }

    pub fn write(&mut self, field: LimitField, value: u64) {
        match field {
            LimitField::TotalBytes => self.max_total_bytes = value,
            LimitField::EntryBytes => self.max_entry_bytes = value,
            LimitField::Entries => self.max_entries = value,
            LimitField::RatioStreamed => self.max_ratio_streamed = value,
            LimitField::PathDepth => self.max_path_depth = value,
            LimitField::PathLen => self.max_path_len = value,
            LimitField::DictSize => self.max_dict_size = value,
            LimitField::Nesting => self.max_nesting = value,
            LimitField::MinFreeSpace => self.min_free_space = value,
        }
    }
}

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
    /// 主窗口内尺寸（逻辑点）。
    pub window_size: Option<[f32; 2]>,
    /// 主窗口左上角（逻辑点）。启动时若不在任何显示器内会被拉回主屏。
    pub window_pos: Option<[f32; 2]>,
    /// 归档表各列宽度（名称/大小/压缩后/比率/方法/时间/CRC/标记）。
    /// 存空或长度不足时按内置默认值补齐，旧配置无需迁移。
    pub column_widths: Option<Vec<f32>>,
    /// 打包任务模板；密码永远不入库。
    pub pack_templates: Vec<PackTemplate>,
    /// 安全限额；缺省时用 core 的默认档。
    pub limits: LimitPrefs,
    pub reduce_motion: bool,
    pub high_contrast: bool,
    /// 列表只看异常条目（失败/跳过/CRC 不符）。
    pub only_anomalies: bool,
    /// 用 DPAPI 加密保存最近一次输入的密码；默认关闭。
    pub remember_password: bool,
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
            window_pos: None,
            column_widths: None,
            pack_templates: Vec::new(),
            limits: LimitPrefs::default(),
            reduce_motion: false,
            high_contrast: false,
            only_anomalies: false,
            remember_password: false,
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
    if dir.join("portable.flag").is_file() {
        return true;
    }
    // 安装版同样带 themes/，用 Inno 的卸载器把两者区分开。
    let installed = dir.join("unins000.exe").is_file() || dir.join("unins000.dat").is_file();
    dir.join("themes").is_dir() && !installed
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
        std::fs::remove_file(dir.join("portable.flag")).unwrap();

        // 安装版带 themes/，但有卸载器，不应误判为便携。
        std::fs::create_dir_all(dir.join("themes")).unwrap();
        std::fs::write(dir.join("unins000.exe"), b"").unwrap();
        assert!(!super::is_portable_dir(&dir));

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
        assert_eq!(prefs.window_pos, None);
        assert_eq!(prefs.column_widths, None);
        assert!(!prefs.reduce_motion);
        assert!(!prefs.high_contrast);
        assert!(!prefs.only_anomalies);
        assert!(!prefs.remember_password);
        assert_eq!(prefs.schema, super::CURRENT_SCHEMA);
    }

    #[test]
    fn missing_limits_fall_back_to_core_defaults() {
        let prefs: Prefs = toml::from_str("locale = \"en-US\"\n").unwrap();
        let limits = prefs.limits.to_limits();
        let default = thzip_core::limits::Limits::default();
        for field in crate::limit_fields::LimitField::ALL {
            assert_eq!(limits.get(field), default.get(field), "{field:?}");
        }
    }

    #[test]
    fn limits_survive_save_and_load() {
        let mut limits = thzip_core::limits::Limits::default();
        limits.set(crate::limit_fields::LimitField::Entries, 12_345);
        limits.set(crate::limit_fields::LimitField::PathLen, 512);
        let prefs = Prefs {
            limits: super::LimitPrefs::from_limits(&limits),
            ..Prefs::default()
        };
        let text = toml::to_string_pretty(&prefs).unwrap();
        let parsed: Prefs = toml::from_str(&text).unwrap();
        let restored = parsed.limits.to_limits();
        assert_eq!(restored.max_entries, 12_345);
        // normalize 会把过小的路径长度抬到 260，512 应原样保留。
        assert_eq!(restored.max_path_len, 512);
    }

    #[test]
    fn parses_window_geometry_and_column_widths() {
        let prefs: Prefs = toml::from_str(
            "window_pos = [-12.0, 40.0]\ncolumn_widths = [300.0, 90.0, 90.0, 60.0, 80.0, 120.0, 70.0, 40.0]\n",
        )
        .unwrap();
        assert_eq!(prefs.window_pos, Some([-12.0, 40.0]));
        assert_eq!(prefs.column_widths.map(|widths| widths.len()), Some(8));
    }

    #[test]
    fn parses_window_size() {
        let prefs: Prefs = toml::from_str("window_size = [1280.0, 800.0]\n").unwrap();
        assert_eq!(prefs.window_size, Some([1280.0, 800.0]));
    }

    #[test]
    fn parses_pack_templates() {
        let prefs: Prefs = toml::from_str(
            "[[pack_templates]]\nname = \"快速\"\nstore = true\nzip_method = \"deflate\"\nsevenz_level = 6\n",
        )
        .unwrap();
        assert_eq!(prefs.pack_templates.len(), 1);
        assert_eq!(prefs.pack_templates[0].name, "快速");
        assert!(prefs.pack_templates[0].store);
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
