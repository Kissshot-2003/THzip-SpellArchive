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

pub fn load_all(ctx: &egui::Context) -> Vec<Theme> {
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
        match load_theme(ctx, &dir, &manifest) {
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

fn load_theme(ctx: &egui::Context, dir: &Path, manifest: &Path) -> Result<Theme, String> {
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

    let options = if manifest.meta.sampling_pixel {
        egui::TextureOptions::NEAREST
    } else {
        egui::TextureOptions::LINEAR
    };

    let root = std::fs::canonicalize(dir).map_err(|err| err.to_string())?;
    let mut resources = std::collections::HashMap::new();

    for (key_name, rel_path) in &manifest.resource {
        let Some(key) = ResKey::from_name(key_name) else {
            eprintln!("[theme] {id}: 未知资源 key `{key_name}`，已忽略");
            continue;
        };
        let Some(path) = resolve(&root, rel_path) else {
            eprintln!("[theme] {id}: 资源路径越界或不存在 `{rel_path}`");
            continue;
        };
        match load_texture(ctx, &path, options, &format!("{id}/{key_name}")) {
            Ok(texture) => {
                resources.insert(key, texture);
            }
            Err(err) => eprintln!("[theme] {id}: {key_name} 载入失败: {err}"),
        }
    }

    Ok(Theme {
        id,
        name: manifest.meta.name,
        author: manifest.meta.author,
        version: manifest.meta.version,
        license: manifest.meta.license,
        description: manifest.meta.description,
        sampling_pixel: manifest.meta.sampling_pixel,
        scale: manifest.meta.scale.max(1),
        palette,
        text_map: manifest.text_map,
        resources,
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

fn load_texture(
    ctx: &egui::Context,
    path: &Path,
    options: egui::TextureOptions,
    name: &str,
) -> Result<egui::TextureHandle, String> {
    let image = image::open(path)
        .map_err(|err| format!("{}: {err}", path.display()))?
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Ok(ctx.load_texture(name.to_owned(), color, options))
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
    use super::{parse_hex, resolve};
    use eframe::egui::Color32;
    use std::path::Path;

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
}
