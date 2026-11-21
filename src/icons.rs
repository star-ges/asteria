//! Instance icons: Prism's built-in set plus the Asteria logo, and images
//! the user adds (kept in the data dir, referenced as `custom:<file>`).

use std::path::{Path, PathBuf};

use anyhow::bail;
use egui::ImageSource;
use egui::load::Bytes;

use crate::paths;

pub const DEFAULT: &str = "asteria";

/// Omacraft's logo key, still in instances (and Prism exports) made before Asteria:
/// it means the default icon.
pub const LEGACY_DEFAULT: &str = "omacraft";

/// `key`, with Omacraft's logo key read as Asteria's.
pub fn canonical(key: &str) -> &str {
    if key == LEGACY_DEFAULT { DEFAULT } else { key }
}

macro_rules! builtin {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_bytes!(concat!("../assets/icons/", $name, ".svg")) as &[u8])),*]
    };
}

/// Ordered for the picker: logo, blocks, items, mobs, platforms.
pub const BUILTIN: &[(&str, &[u8])] = builtin!(
    "asteria", "grass", "dirt", "stone", "planks", "brick", "iron", "gold", "diamond", "tnt", "netherstar", "enderpearl",
    "meat", "flame", "gear", "magitech", "creeper", "squarecreeper", "skeleton", "enderman", "chicken", "bee", "fox",
    "steve", "herobrine", "fabricmc", "quiltmc", "neoforged", "modrinth",
);

pub fn dir() -> PathBuf {
    paths::data_dir().join("icons")
}

pub fn is_builtin(key: &str) -> bool {
    let key = canonical(key);
    BUILTIN.iter().any(|(n, _)| *n == key)
}

pub fn source(key: &str) -> ImageSource<'static> {
    if let Some(file) = key.strip_prefix("custom:") {
        let p = dir().join(file);
        if p.exists() {
            return ImageSource::Uri(format!("file://{}", p.display()).into());
        }
    }
    let key = canonical(key);
    let (name, bytes) = BUILTIN.iter().find(|(n, _)| *n == key).unwrap_or(&BUILTIN[0]);
    ImageSource::Bytes { uri: format!("bytes://icon/{name}.svg").into(), bytes: Bytes::Static(bytes) }
}

/// Keys of the user's own icons.
pub fn custom() -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| is_image(Path::new(n)))
        .map(|n| format!("custom:{n}"))
        .collect();
    out.sort();
    out
}

fn is_image(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("png" | "jpg" | "jpeg" | "svg" | "webp")
    )
}

/// Copy an image into the icons dir; returns its key.
pub fn add_custom(path: &Path) -> anyhow::Result<String> {
    if !is_image(path) {
        bail!("icons must be PNG, JPEG or SVG");
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().unwrap().to_string_lossy().to_ascii_lowercase();
    add_bytes(&stem, &ext, &std::fs::read(path)?)
}

/// Save image bytes (a PNG, JPEG, SVG or WebP, by `ext`) as an icon named after `stem`, never
/// over another; returns its key.
pub fn add_bytes(stem: &str, ext: &str, bytes: &[u8]) -> anyhow::Result<String> {
    if !is_image(Path::new(&format!("icon.{ext}"))) {
        bail!("icons must be PNG, JPEG, SVG or WebP");
    }
    std::fs::create_dir_all(dir())?;
    let stem: String = stem.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let stem = if stem.is_empty() { "icon".to_string() } else { stem };
    let mut name = format!("{stem}.{ext}");
    let mut n = 2;
    while dir().join(&name).exists() {
        name = format!("{stem}-{n}.{ext}");
        n += 1;
    }
    crate::fsutil::write_atomic(&dir().join(&name), bytes, 0o644)?;
    Ok(format!("custom:{name}"))
}

pub fn remove_custom(key: &str) {
    if let Some(file) = key.strip_prefix("custom:") {
        let _ = std::fs::remove_file(dir().join(file));
    }
}
