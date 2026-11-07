pub mod loader;

use std::collections::{BTreeMap, HashMap};

use eframe::egui;
use egui::Color32;

/// 资源 Key 的唯一真源在 docs/theme_spec.md。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResKey {
    IconArchive7z,
    IconArchiveZip,
    IconArchiveZst,
    IconArchiveEncrypted,
    IconFile,
    IconFolder,
    IconFileUnknown,
    ProgressSpellBg,
    ProgressSpellFill,
    ProgressSpellFail,
    BtnExtract,
    BtnPack,
    BtnAddFile,
    BtnDelete,
    BtnSettings,
    BtnSearch,
    BtnPause,
    BtnResume,
    BtnClose,
    BtnMinimize,
    BtnTheme,
    StateOk,
    StateWarn,
    StateError,
    PanelBg,
    BorderNormal,
    BorderWarning,
    ParticleSparkle,
    ParticleSakura,
    AvatarMain,
}

impl ResKey {
    pub const ALL: &'static [ResKey] = &[
        ResKey::IconArchive7z,
        ResKey::IconArchiveZip,
        ResKey::IconArchiveZst,
        ResKey::IconArchiveEncrypted,
        ResKey::IconFile,
        ResKey::IconFolder,
        ResKey::IconFileUnknown,
        ResKey::ProgressSpellBg,
        ResKey::ProgressSpellFill,
        ResKey::ProgressSpellFail,
        ResKey::BtnExtract,
        ResKey::BtnPack,
        ResKey::BtnAddFile,
        ResKey::BtnDelete,
        ResKey::BtnSettings,
        ResKey::BtnSearch,
        ResKey::BtnPause,
        ResKey::BtnResume,
        ResKey::BtnClose,
        ResKey::BtnMinimize,
        ResKey::BtnTheme,
        ResKey::StateOk,
        ResKey::StateWarn,
        ResKey::StateError,
        ResKey::PanelBg,
        ResKey::BorderNormal,
        ResKey::BorderWarning,
        ResKey::ParticleSparkle,
        ResKey::ParticleSakura,
        ResKey::AvatarMain,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ResKey::IconArchive7z => "IconArchive7z",
            ResKey::IconArchiveZip => "IconArchiveZip",
            ResKey::IconArchiveZst => "IconArchiveZst",
            ResKey::IconArchiveEncrypted => "IconArchiveEncrypted",
            ResKey::IconFile => "IconFile",
            ResKey::IconFolder => "IconFolder",
            ResKey::IconFileUnknown => "IconFileUnknown",
            ResKey::ProgressSpellBg => "ProgressSpellBg",
            ResKey::ProgressSpellFill => "ProgressSpellFill",
            ResKey::ProgressSpellFail => "ProgressSpellFail",
            ResKey::BtnExtract => "BtnExtract",
            ResKey::BtnPack => "BtnPack",
            ResKey::BtnAddFile => "BtnAddFile",
            ResKey::BtnDelete => "BtnDelete",
            ResKey::BtnSettings => "BtnSettings",
            ResKey::BtnSearch => "BtnSearch",
            ResKey::BtnPause => "BtnPause",
            ResKey::BtnResume => "BtnResume",
            ResKey::BtnClose => "BtnClose",
            ResKey::BtnMinimize => "BtnMinimize",
            ResKey::BtnTheme => "BtnTheme",
            ResKey::StateOk => "StateOk",
            ResKey::StateWarn => "StateWarn",
            ResKey::StateError => "StateError",
            ResKey::PanelBg => "PanelBg",
            ResKey::BorderNormal => "BorderNormal",
            ResKey::BorderWarning => "BorderWarning",
            ResKey::ParticleSparkle => "ParticleSparkle",
            ResKey::ParticleSakura => "ParticleSakura",
            ResKey::AvatarMain => "AvatarMain",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|key| key.name() == name)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg_window: Color32,
    pub bg_panel: Color32,
    pub bg_hover: Color32,
    pub border: Color32,
    pub text_normal: Color32,
    pub text_sub: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub progress_bg: Color32,
}

impl Palette {
    pub fn base(accent: u32, accent_soft: u32) -> Self {
        Self {
            bg_window: Color32::from_rgb(0x0F, 0x12, 0x21),
            bg_panel: Color32::from_rgb(0x17, 0x1A, 0x2E),
            bg_hover: Color32::from_rgb(0x29, 0x2B, 0x47),
            border: Color32::from_rgb(0x40, 0x42, 0x63),
            text_normal: Color32::from_rgb(0xF5, 0xF2, 0xE8),
            text_sub: Color32::from_rgb(0xA6, 0xA6, 0xBD),
            accent: rgb(accent),
            accent_soft: rgb(accent_soft),
            success: Color32::from_rgb(0x61, 0xDE, 0x9C),
            warning: Color32::from_rgb(0xF5, 0xB8, 0x40),
            error: Color32::from_rgb(0xFF, 0x5E, 0x6E),
            progress_bg: Color32::from_rgb(0x10, 0x13, 0x26),
        }
    }

    pub fn plain() -> Self {
        let mut palette = Self::base(0x8A8FA3, 0x24273D);
        palette.progress_bg = Color32::from_rgb(0x1B, 0x1E, 0x33);
        palette
    }
}

fn rgb(value: u32) -> Color32 {
    Color32::from_rgb(
        ((value >> 16) & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        (value & 0xFF) as u8,
    )
}

#[derive(Clone)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub license: String,
    pub description: String,
    pub sampling_pixel: bool,
    pub scale: u32,
    pub palette: Palette,
    /// 主题文案覆盖（theme.toml `[text_map]`）。
    pub text_map: BTreeMap<String, String>,
    pub resources: HashMap<ResKey, egui::TextureHandle>,
    /// 九宫格切片（左/上/右/下，单位 1x 像素）。
    pub nine_patch: HashMap<ResKey, [u32; 4]>,
    /// 解析后的纹理路径；切换主题时再解码，避免启动加载全部主题。
    pub(crate) paths: HashMap<ResKey, std::path::PathBuf>,
}

impl Theme {
    pub fn texture(&self, key: ResKey) -> Option<&egui::TextureHandle> {
        self.resources.get(&key)
    }

    fn empty(
        id: &str,
        name: &str,
        description: &str,
        palette: Palette,
        text_map: BTreeMap<String, String>,
    ) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            author: "SpellArchive".to_owned(),
            version: "0.1".to_owned(),
            license: "CC-BY-NC-SA-4.0".to_owned(),
            description: description.to_owned(),
            sampling_pixel: true,
            scale: 2,
            palette,
            text_map,
            resources: HashMap::new(),
            nine_patch: HashMap::new(),
            paths: HashMap::new(),
        }
    }
}

pub struct ThemeStore {
    themes: Vec<Theme>,
    current: usize,
}

impl ThemeStore {
    /// 扫描 `themes/` 解析配色与资源路径；纹理在 `ensure_loaded` 里按需解码。
    pub fn load() -> Self {
        let mut themes = loader::load_all();
        if themes.is_empty() {
            themes = builtin();
        } else {
            themes.push(plain_theme());
        }
        Self { themes, current: 0 }
    }

    /// 解码当前主题的纹理。切换主题后调用；已加载则直接返回。
    pub fn ensure_loaded(&mut self, ctx: &egui::Context) {
        let theme = &mut self.themes[self.current];
        if !theme.resources.is_empty() || theme.paths.is_empty() {
            return;
        }
        let options = if theme.sampling_pixel {
            egui::TextureOptions::NEAREST
        } else {
            egui::TextureOptions::LINEAR
        };
        let paths = theme.paths.clone();
        let id = theme.id.clone();
        for (key, path) in paths {
            match loader::load_texture(ctx, &path, options, &format!("{id}/{}", key.name())) {
                Ok(texture) => {
                    theme.resources.insert(key, texture);
                }
                Err(err) => eprintln!("[theme] {id}: {} 载入失败: {err}", key.name()),
            }
        }
    }

    pub fn current(&self) -> &Theme {
        &self.themes[self.current]
    }

    pub fn current_index(&self) -> usize {
        self.current
    }

    pub fn current_id(&self) -> &str {
        &self.current().id
    }

    /// 按主题 id 选择；返回是否真的切换了。
    pub fn select_by_id(&mut self, id: &str) -> bool {
        if let Some(index) = self.themes.iter().position(|theme| theme.id == id) {
            return self.set(index);
        }
        false
    }

    pub fn names(&self) -> Vec<String> {
        self.themes.iter().map(|theme| theme.name.clone()).collect()
    }

    pub fn set(&mut self, index: usize) -> bool {
        if index < self.themes.len() && index != self.current {
            self.current = index;
            true
        } else {
            false
        }
    }

    pub fn texture(&self, key: ResKey) -> Option<&egui::TextureHandle> {
        self.current().texture(key)
    }

    pub fn apply(&self, ctx: &egui::Context) {
        let palette = self.current().palette;

        ctx.set_theme(egui::ThemePreference::Dark);
        ctx.all_styles_mut(|style| {
            style.visuals = egui::Visuals::dark();
            style.visuals.panel_fill = palette.bg_panel;
            style.visuals.window_fill = palette.bg_window;
            style.visuals.faint_bg_color = palette.bg_hover;
            style.visuals.extreme_bg_color = palette.bg_window;
            style.visuals.override_text_color = Some(palette.text_normal);
            style.visuals.selection.bg_fill = palette.accent;
            style.visuals.selection.stroke = egui::Stroke::new(1.0, palette.text_normal);
            style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, palette.border);
            style.visuals.widgets.inactive.bg_fill = palette.bg_hover;
            style.visuals.widgets.inactive.weak_bg_fill = palette.bg_hover;
            style.visuals.widgets.hovered.bg_fill = palette.accent_soft;
            style.visuals.widgets.hovered.weak_bg_fill = palette.accent_soft;
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(10.0, 5.0);
        });
    }
}

fn builtin() -> Vec<Theme> {
    let mut themes = vec![
        themed(
            "reimu",
            "灵梦 · 结界",
            "红白符卡，像素硬边",
            0xF03D52,
            0x3D1F33,
        ),
        themed(
            "marisa",
            "魔理沙 · 星尘",
            "金色星屑，暖色调",
            0xF5B840,
            0x403824,
        ),
        themed("sanae", "早苗 · 青蛙", "御币绿，清爽", 0x3DC794, 0x1A3B36),
        themed("cirno", "琪露诺 · 冰晶", "冷蓝冰翼", 0x52B8F5, 0x1A3347),
    ];
    themes.push(plain_theme());
    themes
}

fn plain_theme() -> Theme {
    let mut theme = Theme::empty(
        "plain",
        "素体 · 无主题",
        "无贴图、无特效、无彩蛋",
        Palette::plain(),
        BTreeMap::new(),
    );
    theme.sampling_pixel = false;
    theme
}

fn themed(id: &'static str, name: &str, description: &str, accent: u32, accent_soft: u32) -> Theme {
    let mut text_map = BTreeMap::new();
    text_map.insert("extract".to_owned(), "解除符卡封印".to_owned());
    text_map.insert("pack".to_owned(), "构筑封印结界".to_owned());
    text_map.insert("new_archive".to_owned(), "构筑新结界".to_owned());
    text_map.insert("test".to_owned(), "结界稳固性检测".to_owned());
    text_map.insert("task_running".to_owned(), "咏唱进行中…".to_owned());
    text_map.insert("task_done".to_owned(), "符卡咏唱完成".to_owned());

    Theme::empty(
        id,
        name,
        description,
        Palette::base(accent, accent_soft),
        text_map,
    )
}
