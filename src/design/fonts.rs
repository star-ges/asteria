//! Fonts, found through fontconfig by family and style.
//!
//! - Switzer SemiBold (`text`), Switzer ExtraBold (`heading`), Melodrama Medium (`display`).
//!   Fontshare's licence forbids redistribution, so they are never bundled: when one is
//!   missing, fontconfig's `sans-serif` stands in and Settings says so (`FontSet::missing`).
//! - Phosphor Regular and Bold (`icons`, `icons-bold`), bundled (MIT), always the bundled
//!   copy so the codepoints in `glyphs.rs` match.
//! - JetBrains Mono (`raw`) for raw text only: the system's if it has the plain family, else
//!   the bundled Regular (OFL 1.1, licence beside it in `assets/fonts`).
//!
//! egui's own `Proportional` and `Monospace` families (stock widgets: context menus, tooltips)
//! are the text face and JetBrains Mono.

use std::path::PathBuf;
use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontId};

/// Phosphor 2.1.2 (MIT), from `@phosphor-icons/web`.
static PHOSPHOR: &[u8] = include_bytes!("../../assets/fonts/Phosphor.ttf");
static PHOSPHOR_BOLD: &[u8] = include_bytes!("../../assets/fonts/Phosphor-Bold.ttf");
/// JetBrains Mono 2.304 Regular (OFL 1.1).
static JETBRAINS_MONO: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");

const TEXT: &str = "text";
const HEADING: &str = "heading";
const DISPLAY: &str = "display";
const ICONS: &str = "icons";
const ICONS_BOLD: &str = "icons-bold";
const RAW: &str = "raw";

pub fn text(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(TEXT.into()))
}

pub fn heading(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(HEADING.into()))
}

pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(DISPLAY.into()))
}

pub fn icons(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(ICONS.into()))
}

pub fn icons_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(ICONS_BOLD.into()))
}

/// JetBrains Mono, for raw text only (an approved exception).
pub fn raw(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(RAW.into()))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FontSet {
    pub text: Option<PathBuf>,
    pub heading: Option<PathBuf>,
    pub display: Option<PathBuf>,
    /// The system's JetBrains Mono, if it has the plain family (else the bundled one).
    pub raw: Option<PathBuf>,
    /// The families that weren't found and were replaced by `sans-serif` ("Switzer", "Melodrama").
    pub missing: Vec<&'static str>,
}

/// fontconfig's best match for `pattern`: (family, file). egui reads only TTF and OTF files.
fn fc_match(pattern: &str) -> Option<(String, PathBuf)> {
    let out = std::process::Command::new("fc-match").args(["-f", "%{family[0]}\n%{file}", pattern]).output().ok()?;
    let out = String::from_utf8_lossy(&out.stdout);
    let (family, file) = out.split_once('\n')?;
    let p = PathBuf::from(file.trim());
    let ok = p.extension().is_some_and(|e| {
        let e = e.to_string_lossy().to_lowercase();
        e == "ttf" || e == "otf"
    });
    ok.then(|| (family.trim().to_string(), p))
}

/// The file for `family` in `style`, only if fontconfig really has that family (it always
/// returns *something*).
fn exact(family: &str, style: &str) -> Option<PathBuf> {
    fc_match(&format!("{family}:style={style}")).filter(|(f, _)| f.eq_ignore_ascii_case(family)).map(|(_, p)| p)
}

impl FontSet {
    pub fn resolve() -> Self {
        let mut missing = Vec::new();
        let mut pick = |family: &'static str, style: &str, fallback: &str| {
            exact(family, style).or_else(|| {
                if !missing.contains(&family) {
                    missing.push(family);
                }
                fc_match(fallback).map(|(_, p)| p)
            })
        };
        let text = pick("Switzer", "Semibold", "sans-serif:medium");
        let heading = pick("Switzer", "Extrabold", "sans-serif:bold");
        let display = pick("Melodrama", "Medium", "sans-serif:medium");
        Self {
            text,
            heading,
            display,
            raw: exact("JetBrains Mono", "Regular"),
            missing,
        }
    }

    pub fn apply(&self, ctx: &egui::Context) {
        ctx.set_fonts(self.definitions());
    }

    pub fn definitions(&self) -> FontDefinitions {
        let mut defs = FontDefinitions::default();
        // egui's own fonts (Ubuntu, emoji) stay at the end of every chain as a last resort.
        let builtin: Vec<String> = defs.families[&FontFamily::Proportional].clone();
        let mut load = |name: &str, path: &Option<PathBuf>| -> bool {
            let Some(bytes) = path.as_ref().and_then(|p| std::fs::read(p).ok()) else {
                return false;
            };
            defs.font_data.insert(name.into(), Arc::new(FontData::from_owned(bytes)));
            true
        };

        let has_text = load(TEXT, &self.text);
        let has_heading = load(HEADING, &self.heading);
        let has_display = load(DISPLAY, &self.display);
        let has_raw = load(RAW, &self.raw);

        // The kit.
        defs.font_data.insert(ICONS.into(), Arc::new(FontData::from_static(PHOSPHOR)));
        defs.font_data.insert(ICONS_BOLD.into(), Arc::new(FontData::from_static(PHOSPHOR_BOLD)));
        if !has_raw {
            defs.font_data.insert(RAW.into(), Arc::new(FontData::from_static(JETBRAINS_MONO)));
        }
        // Each type face, then Phosphor (so a label can carry an icon inline), then egui's.
        let chain = |first: Option<&str>| -> Vec<String> {
            first.into_iter().map(String::from).chain([ICONS.to_string()]).chain(builtin.iter().cloned()).collect()
        };
        defs.families.insert(FontFamily::Name(TEXT.into()), chain(has_text.then_some(TEXT)));
        defs.families.insert(FontFamily::Name(HEADING.into()), chain(has_heading.then_some(HEADING).or(has_text.then_some(TEXT))));
        defs.families.insert(FontFamily::Name(DISPLAY.into()), chain(has_display.then_some(DISPLAY).or(has_text.then_some(TEXT))));
        defs.families.insert(FontFamily::Name(RAW.into()), chain(Some(RAW)));
        defs.families.insert(FontFamily::Name(ICONS.into()), vec![ICONS.into()]);
        defs.families.insert(FontFamily::Name(ICONS_BOLD.into()), vec![ICONS_BOLD.into()]);
        // egui's stock widgets: the text face and the raw face.
        defs.families.insert(FontFamily::Proportional, chain(has_text.then_some(TEXT)));
        defs.families.insert(FontFamily::Monospace, chain(Some(RAW)));
        defs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no fonts on the system at all, every family still has a face (bundled or egui's),
    /// so the kit draws something readable.
    #[test]
    fn every_family_has_a_face_without_system_fonts() {
        let defs = FontSet::default().definitions();
        for family in [TEXT, HEADING, DISPLAY, ICONS, ICONS_BOLD, RAW].map(|n| FontFamily::Name(n.into())).into_iter().chain([FontFamily::Proportional, FontFamily::Monospace]) {
            let name = format!("{family:?}");
            let fam = &defs.families[&family];
            assert!(fam.iter().any(|f| defs.font_data.contains_key(f)), "{name} has no face");
        }
    }

    /// Every icon in the glyph table is in the bundled Phosphor, in both weights.
    #[test]
    fn every_glyph_is_in_phosphor() {
        let ctx = egui::Context::default();
        crate::design::test_frame(&ctx, 0.0, |_| {});
        ctx.fonts_mut(|f| {
            for (name, g) in crate::design::glyphs::ALL {
                let c = g.chars().next().unwrap();
                for font in [icons(18.0), icons_bold(18.0)] {
                    assert!(f.has_glyph(&font, c), "{name} ({:x}) missing from {font:?}", c as u32);
                }
            }
        });
    }
}
