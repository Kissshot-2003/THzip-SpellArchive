use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use eframe::egui;
use egui::Color32;
use serde::Deserialize;

use crate::theme::{Palette, ResKey, Theme};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Manifest {
    #[serde(default = "schema_version")]
    schema: u32,
    meta: Meta,
    #[serde(default)]
    text_map: BTreeMap<String, String>,
    #[serde(default)]
    resource: BTreeMap<String, String>,
    #[serde(default)]
    effects: Effects,
    #[serde(default)]
    resource_nine_patch: BTreeMap<String, [u32; 4]>,
    #[serde(default)]
    color: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Meta {
    name: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    homepage: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    license: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_true")]
    sampling_pixel: bool,
    #[serde(default = "default_scale")]
    scale: u32,
}

#[derive(Debug, Default, Deserialize)]
struct Effects {
    #[serde(default)]
    particles: bool,
}

fn schema_version() -> u32 {
    1
}

fn default_true() -> bool {
    true
}

fn default_scale() -> u32 {
    1
}

/// 主题文件签名：数量 + 最新修改时间。用于热重载轮询。
pub fn signature() -> Option<(u64, std::time::SystemTime)> {
    let root = theme_root()?;
    let mut count = 0u64;
    let mut latest: Option<std::time::SystemTime> = None;
    let mut note = |path: &Path| {
        if let Ok(meta) = std::fs::metadata(path)
            && let Ok(modified) = meta.modified()
        {
            count += 1;
            latest = Some(match latest {
                Some(current) if current >= modified => current,
                _ => modified,
            });
        }
    };

    for entry in std::fs::read_dir(&root).ok()?.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let manifest = dir.join("theme.toml");
        if manifest.is_file() {
            note(&manifest);
        }
        if let Ok(pngs) = std::fs::read_dir(dir.join("png")) {
            for png in pngs.flatten() {
                if png.path().is_file() {
                    note(&png.path());
                }
            }
        }
    }

    latest.map(|latest| (count, latest))
}

pub fn load_all() -> Vec<Theme> {
    let Some(root) = theme_root() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };

    let mut themes = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let manifest = dir.join("theme.toml");
        if !manifest.is_file() {
            continue;
        }
        match load_theme(&dir, &manifest) {
            Ok(theme) => themes.push(theme),
            Err(err) => eprintln!("[theme] {} 加载失败: {err}", dir.display()),
        }
    }

    themes.sort_by(|a, b| {
        rank(&a.id)
            .cmp(&rank(&b.id))
            .then_with(|| a.name.cmp(&b.name))
    });
    themes
}

fn rank(id: &str) -> u8 {
    match id {
        "reimu" => 0,
        "marisa" => 1,
        "sanae" => 2,
        "cirno" => 3,
        _ => 9,
    }
}

/// 运行时主题目录；主题包导入需要写到这里。
pub fn themes_root() -> Option<PathBuf> {
    theme_root()
}

/// 查找主题目录：工作目录优先，其次 exe 同级，最后 `target/debug` 的开发路径。
fn theme_root() -> Option<PathBuf> {
    let mut candidates = vec![PathBuf::from("themes")];
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("themes"));
        candidates.push(dir.join("../../themes"));
    }
    candidates.into_iter().find(|path| path.is_dir())
}

fn load_theme(dir: &Path, manifest: &Path) -> Result<Theme, String> {
    let text = std::fs::read_to_string(manifest).map_err(|err| err.to_string())?;
    let manifest: Manifest =
        toml::from_str(&text).map_err(|err| format!("toml 解析失败: {err}"))?;

    if manifest.schema > 1 {
        eprintln!(
            "[theme] {} schema={} 高于程序支持版本 1，按只读模式加载",
            dir.display(),
            manifest.schema
        );
    }

    let id = dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("theme")
        .to_owned();

    let base = Palette::base(0xF03D52, 0x3D1F33);
    let palette = Palette {
        bg_window: pick(&manifest.color, "bg_window", base.bg_window),
        bg_panel: pick(&manifest.color, "bg_panel", base.bg_panel),
        bg_hover: pick(&manifest.color, "bg_hover", base.bg_hover),
        border: pick(&manifest.color, "border", base.border),
        text_normal: pick(&manifest.color, "text_normal", base.text_normal),
        text_sub: pick(&manifest.color, "text_sub", base.text_sub),
        accent: pick(&manifest.color, "accent", base.accent),
        accent_soft: pick(&manifest.color, "accent_soft", base.accent_soft),
        success: pick(&manifest.color, "success", base.success),
        warning: pick(&manifest.color, "warning", base.warning),
        error: pick(&manifest.color, "error", base.error),
        progress_bg: pick(&manifest.color, "progress_bg", base.progress_bg),
    };

    let root = std::fs::canonicalize(dir).map_err(|err| err.to_string())?;
    // 只解析路径，纹理在切换到该主题时由 ThemeStore 懒加载。
    let mut paths = std::collections::HashMap::new();

    for (key_name, rel_path) in &manifest.resource {
        let Some(key) = ResKey::from_name(key_name) else {
            eprintln!("[theme] {id}: 未知资源 key `{key_name}`，已忽略");
            continue;
        };
        let Some(path) = resolve(&root, rel_path) else {
            eprintln!("[theme] {id}: 资源路径越界或不存在 `{rel_path}`");
            continue;
        };
        paths.insert(key, path);
    }

    let mut nine_patch = std::collections::HashMap::new();
    for (key_name, slice) in &manifest.resource_nine_patch {
        let Some(key) = ResKey::from_name(key_name) else {
            eprintln!("[theme] {id}: 未知九宫格 key `{key_name}`，已忽略");
            continue;
        };
        if paths.contains_key(&key) {
            nine_patch.insert(key, *slice);
        } else {
            eprintln!("[theme] {id}: 九宫格 `{key_name}` 没有对应贴图，已忽略");
        }
    }

    Ok(Theme {
        id,
        name: manifest.meta.name,
        author: manifest.meta.author,
        homepage: manifest.meta.homepage,
        version: manifest.meta.version,
        license: manifest.meta.license,
        description: manifest.meta.description,
        sampling_pixel: manifest.meta.sampling_pixel,
        scale: manifest.meta.scale.max(1),
        palette,
        text_map: manifest.text_map,
        resources: std::collections::HashMap::new(),
        nine_patch,
        particles: manifest.effects.particles,
        paths,
    })
}

fn resolve(root: &Path, rel: &str) -> Option<PathBuf> {
    let rel = Path::new(rel);
    if rel.is_absolute() {
        return None;
    }
    if rel
        .components()
        .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    let canonical = std::fs::canonicalize(root.join(rel)).ok()?;
    canonical.starts_with(root).then_some(canonical)
}

pub(crate) fn load_texture(
    ctx: &egui::Context,
    path: &Path,
    options: egui::TextureOptions,
    name: &str,
) -> Result<egui::TextureHandle, String> {
    let is_svg = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"));
    let color = if is_svg {
        load_svg(path)?
    } else {
        let image = image::open(path)
            .map_err(|err| format!("{}: {err}", path.display()))?
            .to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw())
    };
    Ok(ctx.load_texture(name.to_owned(), color, options))
}

/// SVG 光栅化：不加载外部资源，单文件上限 8 MiB，最长边上限 2048。
fn load_svg(path: &Path) -> Result<egui::ColorImage, String> {
    const MAX_BYTES: u64 = 8 * 1024 * 1024;
    const MAX_DIM: u32 = 2048;

    let meta = std::fs::metadata(path).map_err(|err| format!("{}: {err}", path.display()))?;
    if meta.len() > MAX_BYTES {
        return Err(format!("{}: SVG 超过 8 MiB", path.display()));
    }
    let data = std::fs::read(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default())
        .map_err(|err| format!("{}: SVG 解析失败: {err}", path.display()))?;

    let size = tree.size().to_int_size();
    let width = size.width().max(1);
    let height = size.height().max(1);
    let scale = (MAX_DIM as f32 / width.max(height) as f32).min(1.0);
    let width = ((width as f32 * scale).round() as u32).max(1);
    let height = ((height as f32 * scale).round() as u32).max(1);

    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| format!("{}: SVG 尺寸无效", path.display()))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        pixmap.data(),
    ))
}

fn pick(map: &BTreeMap<String, String>, key: &str, fallback: Color32) -> Color32 {
    map.get(key)
        .and_then(|raw| parse_hex(raw))
        .unwrap_or(fallback)
}

fn parse_hex(raw: &str) -> Option<Color32> {
    let raw = raw.trim().trim_start_matches('#');
    let value = u32::from_str_radix(raw, 16).ok()?;
    match raw.len() {
        6 => Some(Color32::from_rgb(
            ((value >> 16) & 0xFF) as u8,
            ((value >> 8) & 0xFF) as u8,
            (value & 0xFF) as u8,
        )),
        8 => Some(Color32::from_rgba_unmultiplied(
            ((value >> 24) & 0xFF) as u8,
            ((value >> 16) & 0xFF) as u8,
            ((value >> 8) & 0xFF) as u8,
            (value & 0xFF) as u8,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Manifest, load_texture, parse_hex, resolve};
    use crate::theme::ResKey;
    use eframe::egui::Color32;
    use std::path::Path;

    #[test]
    fn rasterizes_svg_icons() {
        let path = std::env::temp_dir().join(format!(
            "thzip-spellarchive-icon-{}.svg",
            std::process::id()
        ));
        std::fs::write(
            &path,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#f03d52"/></svg>"##,
        )
        .unwrap();
        let ctx = eframe::egui::Context::default();
        let texture = load_texture(&ctx, &path, eframe::egui::TextureOptions::NEAREST, "test");
        assert!(texture.is_ok(), "{:?}", texture.err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn parses_rgb_hex() {
        assert_eq!(
            parse_hex("#F03D52"),
            Some(Color32::from_rgb(0xF0, 0x3D, 0x52))
        );
        assert_eq!(
            parse_hex("f5b840"),
            Some(Color32::from_rgb(0xF5, 0xB8, 0x40))
        );
        assert_eq!(parse_hex("#fff"), None);
        assert_eq!(parse_hex("#zzzzzz"), None);
    }

    #[test]
    fn rejects_path_escape() {
        let root = Path::new("C:/themes/reimu");
        assert!(resolve(root, "../marisa/png/a.png").is_none());
        assert!(resolve(root, "C:/Windows/System32/x.png").is_none());
    }

    #[test]
    fn res_keys_are_unique_and_roundtrip() {
        let mut seen = std::collections::HashSet::new();
        for &key in ResKey::ALL {
            assert!(seen.insert(key.name()), "duplicate key {}", key.name());
            assert_eq!(ResKey::from_name(key.name()), Some(key));
        }
    }

    #[test]
    fn repository_themes_reference_known_and_existing_resources() {
        let themes = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes");
        let mut checked = 0;
        for entry in std::fs::read_dir(&themes).unwrap() {
            let dir = std::fs::canonicalize(entry.unwrap().path()).unwrap();
            let manifest = dir.join("theme.toml");
            if !manifest.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&manifest).unwrap();
            let parsed: Manifest =
                toml::from_str(&text).unwrap_or_else(|err| panic!("{}: {err}", manifest.display()));
            for (key_name, rel_path) in &parsed.resource {
                assert!(
                    ResKey::from_name(key_name).is_some(),
                    "{}: unknown resource key {key_name}",
                    manifest.display()
                );
                assert!(
                    resolve(&dir, rel_path).is_some(),
                    "{}: missing resource {rel_path}",
                    manifest.display()
                );
            }
            for key_name in parsed.resource_nine_patch.keys() {
                assert!(
                    ResKey::from_name(key_name).is_some(),
                    "{}: unknown nine-patch key {key_name}",
                    manifest.display()
                );
            }
            checked += 1;
        }
        assert!(
            checked > 0,
            "no theme manifests found under {}",
            themes.display()
        );
    }

    /// 主题包是用户从外部拿来的，`theme.toml` 属于不可信输入。
    fn resolve_fixture() -> &'static Path {
        static ROOT: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        ROOT.get_or_init(|| {
            let base = std::env::temp_dir().join(format!("thzip-resolve-{}", std::process::id()));
            let root = base.join("theme");
            std::fs::create_dir_all(root.join("sub")).unwrap();
            std::fs::write(root.join("ok.png"), b"x").unwrap();
            std::fs::write(root.join("sub/file.png"), b"x").unwrap();
            std::fs::write(base.join("outside.png"), b"x").unwrap();
            std::fs::canonicalize(root).unwrap()
        })
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig { cases: 256, ..Default::default() })]

        /// 任意文本解析只能返回 `Err`，不能 panic：`toml` 里的深层嵌套、超长数字、
        /// 畸形转义都不能把主题加载拖崩。
        #[test]
        fn manifest_parsing_never_panics(text in ".{0,4096}") {
            let _: Result<Manifest, _> = toml::from_str(&text);
        }

        /// 资源路径来自不可信主题包：`resolve` 要么拒绝，要么给出主题目录**之内**的真实路径。
        #[test]
        fn resolve_never_escapes_root(segments in proptest::collection::vec(
            proptest::sample::select(vec![
                "ok.png", "sub", "nested", "file.png", "..", ".", "", "C:", "//server/share",
            ]),
            1..5,
        )) {
            let root = resolve_fixture();
            let rel = segments.join("/");
            if let Some(path) = resolve(root, &rel) {
                proptest::prop_assert!(
                    path.starts_with(root),
                    "资源路径逃出主题目录: {} -> {}",
                    rel,
                    path.display()
                );
            }
        }
    }
}
