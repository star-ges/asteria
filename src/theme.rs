//! Colours and fonts.
//!
//! Asteria is drawn in the StarShell design language: one surface colour, text in two
//! neutrals, one accent for hover/focus/active/selected, plus a small set of earthy
//! "clarity hues" used only for readability (groups, errors, warnings, statuses; never
//! fills or state). Light and dark are two palettes with the same names. Which one is in
//! use follows the desktop's colour scheme, read from the XDG desktop portal (the
//! `org.freedesktop.appearance color-scheme` setting), so it works on any desktop with a
//! portal and doesn't depend on StarShell itself.
//!
//! This is the Phase 1 stop-gap of the redesign (see `docs/asteria/ASTERIA_REDESIGN_PLAN.md`):
//! the old widget kit reads these fields; Phase 2 replaces them with design tokens.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Shadow, Stroke};

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub dark: bool,

    pub accent: Color32,
    pub selection: Color32,
    pub muted: Color32,

    pub bg: Color32,
    pub bg_dark: Color32,
    pub bg_darker: Color32,
    pub bg_light: Color32,

    pub fg: Color32,
    pub fg_dark: Color32,
    pub fg_light: Color32,

    // Clarity hues, under the names groups store (`groups.toml`) so existing data maps on:
    // red → clay, orange → sienna, yellow → ochre, green → olive, cyan → teal,
    // blue → slate, magenta → plum. Each is at least 4.5:1 on its own background.
    pub red: Color32,
    pub yellow: Color32,
    pub orange: Color32,
    pub green: Color32,
    pub cyan: Color32,
    pub blue: Color32,
    pub magenta: Color32,

    /// Frames of dialogs and notices: the strong hairline colour.
    pub border_active: Color32,

    pub normal_fill_alpha: f32,
    pub normal_border_alpha: f32,
    pub hover_fill_alpha: f32,
    pub hover_border_alpha: f32,
    pub selected_fill_alpha: f32,
    pub pressed_fill_alpha: f32,
    pub selection_fill_alpha: f32,

    pub menu_selected_text: Color32,
    pub menu_selected_bg_alpha: f32,
    pub menu_selected_border_alpha: f32,
    pub scrim_alpha: f32,

    pub font_size: f32,
}

/// One palette of the StarShell design language (`Theme.qml` in StarShell).
struct Palette {
    background: Color32,
    accent: Color32,
    text_strong: Color32,
    text_muted: Color32,
    line_strong: Color32,
    clay: Color32,
    sienna: Color32,
    ochre: Color32,
    olive: Color32,
    teal: Color32,
    slate: Color32,
    plum: Color32,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Cream. The accent is 4.7:1 on the background.
const LIGHT: Palette = Palette {
    background: hex(0xfef0d5),
    accent: hex(0xad5034),
    text_strong: hex(0x202020),
    text_muted: hex(0x4c4c4c),
    line_strong: hex(0x202020),
    clay: hex(0xa33b30),
    sienna: hex(0x94541f),
    ochre: hex(0x8a6516),
    olive: hex(0x566a2a),
    teal: hex(0x2e6862),
    slate: hex(0x35596f),
    plum: hex(0x7d4a63),
};

/// "Inverted cream": a warm near-black. The accent is 5.6:1 on the background.
const DARK: Palette = Palette {
    background: hex(0x1c1a17),
    accent: hex(0xf2683a),
    text_strong: hex(0xe2d6bf),
    text_muted: hex(0xa99f8d),
    line_strong: hex(0x4e4941),
    clay: hex(0xee7266),
    sienna: hex(0xd68a4c),
    ochre: hex(0xaa843b),
    olive: hex(0x7c9251),
    teal: hex(0x5a948d),
    slate: hex(0x698fa7),
    plum: hex(0xb17a94),
};

impl Default for Theme {
    fn default() -> Self {
        Self::starshell(false)
    }
}

impl Theme {
    pub fn starshell(dark: bool) -> Self {
        let p = if dark { &DARK } else { &LIGHT };
        // Wells and raised surfaces are faint washes of the text colour on the one
        // background (they go in Phase 2; nothing gets a second surface colour).
        let wash = |a: f32| p.background.lerp_to_gamma(p.text_strong, a);
        Self {
            dark,
            accent: p.accent,
            selection: wash(0.12),
            muted: p.text_muted,
            bg: p.background,
            bg_dark: wash(0.05),
            bg_darker: wash(0.08),
            bg_light: wash(0.06),
            fg: p.text_strong,
            fg_dark: p.text_muted,
            fg_light: p.text_strong,
            red: p.clay,
            orange: p.sienna,
            yellow: p.ochre,
            green: p.olive,
            cyan: p.teal,
            blue: p.slate,
            magenta: p.plum,
            border_active: p.line_strong,
            normal_fill_alpha: 0.0,
            normal_border_alpha: 0.45,
            hover_fill_alpha: 0.06,
            hover_border_alpha: 0.25,
            selected_fill_alpha: 0.12,
            pressed_fill_alpha: 0.16,
            selection_fill_alpha: 0.16,
            menu_selected_text: p.accent,
            menu_selected_bg_alpha: 0.06,
            menu_selected_border_alpha: 0.0,
            scrim_alpha: 0.5,
            font_size: 12.0,
        }
    }

    pub fn alpha(c: Color32, a: f32) -> Color32 {
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
    }

    pub fn fg_a(&self, a: f32) -> Color32 {
        Self::alpha(self.fg, a)
    }

    /// Body text size.
    pub fn body_size(&self) -> f32 {
        (self.font_size * 1.1).round()
    }

    /// Clarity hue by its stored name (group colours are kept by name so they re-theme).
    pub fn named(&self, name: &str) -> Option<Color32> {
        Some(match name {
            "red" => self.red,
            "orange" => self.orange,
            "yellow" => self.yellow,
            "green" => self.green,
            "cyan" => self.cyan,
            "blue" => self.blue,
            "magenta" => self.magenta,
            _ => return None,
        })
    }

    pub fn apply(&self, ctx: &egui::Context) {
        ctx.set_visuals(if self.dark { egui::Visuals::dark() } else { egui::Visuals::light() });
        let size = self.body_size();
        let square = CornerRadius::ZERO;
        ctx.all_styles_mut(|style| {
            use egui::{FontId, TextStyle};
            style.text_styles = [
                (TextStyle::Small, FontId::monospace(size - 2.0)),
                (TextStyle::Body, FontId::monospace(size)),
                (TextStyle::Button, FontId::monospace(size)),
                (TextStyle::Monospace, FontId::monospace(size)),
                (TextStyle::Heading, FontId::monospace(size * 1.6)),
            ]
            .into();
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(10.0, 5.0);
            style.spacing.interact_size.y = size + 12.0;
            style.spacing.scroll.floating = true;
            // Narrow windows clip text at the edge rather than reflowing it;
            // paragraphs opt back in to wrapping (see widgets::para).
            style.wrap_mode = Some(egui::TextWrapMode::Extend);

            let v = &mut style.visuals;
            v.dark_mode = self.dark;
            v.override_text_color = None;
            v.panel_fill = self.bg;
            v.window_fill = self.bg;
            v.window_stroke = Stroke::new(1.0, self.border_active);
            v.window_corner_radius = square;
            v.menu_corner_radius = square;
            v.window_shadow = Shadow::NONE;
            v.popup_shadow = Shadow::NONE;
            v.extreme_bg_color = self.bg_dark;
            v.text_edit_bg_color = Some(self.bg);
            v.code_bg_color = self.bg_dark;
            v.faint_bg_color = self.bg_light;
            v.hyperlink_color = self.fg;
            v.warn_fg_color = self.yellow;
            v.error_fg_color = self.red;
            v.weak_text_color = Some(self.muted);
            // Selections are a wash of the text colour, never an accent tint.
            v.selection.bg_fill = self.fg_a(self.selection_fill_alpha);
            v.selection.stroke = Stroke::new(1.0, self.fg);
            v.text_cursor.stroke = Stroke::new(2.0, self.accent);
            v.striped = false;
            v.slider_trailing_fill = true;
            v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.5 };
            v.indent_has_left_vline = false;

            let w = &mut v.widgets;
            w.noninteractive.bg_fill = self.bg;
            w.noninteractive.weak_bg_fill = self.bg;
            w.noninteractive.bg_stroke = Stroke::new(1.0, self.fg_a(0.2));
            w.noninteractive.fg_stroke = Stroke::new(1.0, self.fg);

            let fill = |a| self.fg_a(a);
            w.inactive.bg_fill = self.fg_a(0.14);
            w.inactive.weak_bg_fill = fill(self.normal_fill_alpha);
            w.inactive.bg_stroke = Stroke::new(1.0, self.fg_a(self.normal_border_alpha));
            w.inactive.fg_stroke = Stroke::new(1.0, self.fg);

            w.hovered.bg_fill = fill(self.hover_fill_alpha);
            w.hovered.weak_bg_fill = fill(self.hover_fill_alpha);
            w.hovered.bg_stroke = Stroke::new(1.0, self.accent);
            w.hovered.fg_stroke = Stroke::new(1.0, self.accent);

            w.active.bg_fill = fill(self.pressed_fill_alpha);
            w.active.weak_bg_fill = fill(self.pressed_fill_alpha);
            w.active.bg_stroke = Stroke::new(1.0, self.accent);
            w.active.fg_stroke = Stroke::new(1.0, self.accent);

            w.open.bg_fill = fill(self.selected_fill_alpha);
            w.open.weak_bg_fill = fill(self.selected_fill_alpha);
            w.open.bg_stroke = Stroke::new(1.0, self.accent);
            w.open.fg_stroke = Stroke::new(1.0, self.accent);

            for wv in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
                wv.corner_radius = square;
                wv.expansion = 0.0;
            }
        });
    }
}

// ---------------------------------------------------------------------------
// The desktop's colour scheme, from the XDG desktop portal.

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";
const COLOR_SCHEME: &str = "color-scheme";

/// The portal's `color-scheme`: 1 is "prefer dark"; 0 (no preference) and 2 (prefer
/// light) are light, the original palette.
fn is_dark(value: &zbus::zvariant::Value) -> Option<bool> {
    match value {
        zbus::zvariant::Value::U32(n) => Some(*n == 1),
        // `Read` (older portals) wraps the value in one more variant.
        zbus::zvariant::Value::Value(inner) => is_dark(inner),
        _ => None,
    }
}

fn settings_proxy(conn: &zbus::blocking::Connection) -> zbus::Result<zbus::blocking::Proxy<'static>> {
    zbus::blocking::Proxy::new(conn, PORTAL, PORTAL_PATH, SETTINGS)
}

fn read_scheme(proxy: &zbus::blocking::Proxy) -> Option<bool> {
    let read = |method: &str| -> Option<bool> {
        let v: zbus::zvariant::OwnedValue = proxy.call(method, &(APPEARANCE, COLOR_SCHEME)).ok()?;
        is_dark(&v)
    };
    read("ReadOne").or_else(|| read("Read"))
}

/// Whether the desktop prefers dark, read once (used at start, before the first frame).
pub fn system_prefers_dark() -> bool {
    zbus::blocking::Connection::session()
        .ok()
        .and_then(|c| settings_proxy(&c).ok().and_then(|p| read_scheme(&p)))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Fonts: fontconfig's `monospace` until the Phase 2 type system (Switzer, Melodrama,
// Phosphor) replaces it. A Nerd Font is chained in after it for icon glyphs.

#[derive(Debug, Clone, PartialEq)]
pub struct FontSet {
    pub regular: Option<PathBuf>,
    pub bold: Option<PathBuf>,
    pub icons: Option<PathBuf>,
}

fn fc_match(pattern: &str) -> Option<PathBuf> {
    let out = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", pattern])
        .output()
        .ok()?;
    let p = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    // egui can't read TrueType collections or variable-only WOFF.
    let ok = p.extension().is_some_and(|e| {
        let e = e.to_string_lossy().to_lowercase();
        e == "ttf" || e == "otf"
    });
    ok.then_some(p)
}

impl FontSet {
    pub fn resolve() -> Self {
        Self {
            regular: fc_match("monospace"),
            bold: fc_match("monospace:bold"),
            // fontconfig always returns *something*; only take real Nerd Fonts.
            icons: ["Symbols Nerd Font Mono", "JetBrainsMono Nerd Font"]
                .iter()
                .filter_map(|f| fc_match(f))
                .find(|p| p.to_string_lossy().contains("Nerd")),
        }
    }

    pub fn apply(&self, ctx: &egui::Context) {
        let mut defs = FontDefinitions::default();
        let mut mono: Vec<String> = Vec::new();
        let mut load = |name: &str, path: &Option<PathBuf>| -> bool {
            let Some(bytes) = path.as_ref().and_then(|p| std::fs::read(p).ok()) else {
                return false;
            };
            defs.font_data.insert(name.into(), Arc::new(FontData::from_owned(bytes)));
            true
        };
        if load("mono", &self.regular) {
            mono.push("mono".into());
        }
        let bold = load("mono-bold", &self.bold);
        if load("icons", &self.icons) {
            mono.push("icons".into());
        }
        // Bundled last, so icons render even without a Nerd Font installed;
        // the system's own Nerd Font still wins when there is one.
        defs.font_data.insert("bundled-icons".into(), Arc::new(FontData::from_static(BUNDLED_ICONS)));
        mono.push("bundled-icons".into());
        let fam = defs.families.entry(FontFamily::Monospace).or_default();
        for (i, name) in mono.iter().enumerate() {
            fam.insert(i, name.clone());
        }
        let prop = defs.families.entry(FontFamily::Proportional).or_default();
        for (i, name) in mono.iter().enumerate() {
            prop.insert(i, name.clone());
        }
        if bold {
            let mut fam = vec!["mono-bold".to_string()];
            fam.extend(mono.iter().skip(1).cloned());
            fam.extend(defs.families[&FontFamily::Monospace].iter().cloned());
            defs.families.insert(FontFamily::Name("bold".into()), fam);
        } else {
            let fam = defs.families[&FontFamily::Monospace].clone();
            defs.families.insert(FontFamily::Name("bold".into()), fam);
        }
        ctx.set_fonts(defs);
    }
}

/// Symbols Nerd Font Mono v3.5.1 (MIT), see assets/fonts.
static BUNDLED_ICONS: &[u8] = include_bytes!("../assets/fonts/SymbolsNerdFontMono-Regular.ttf");

pub fn bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, FontFamily::Name("bold".into()))
}

// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct Pending {
    pub theme: Option<Theme>,
    pub fonts: Option<FontSet>,
}

/// Watches the portal's colour scheme (a D-Bus signal) and fontconfig (polled), and
/// leaves what changed in `Pending` for the next frame.
pub fn spawn_watcher(ctx: egui::Context) -> Arc<Mutex<Pending>> {
    let pending = Arc::new(Mutex::new(Pending::default()));

    let out = pending.clone();
    let repaint = ctx.clone();
    std::thread::spawn(move || {
        loop {
            // Each pass follows the portal until the connection ends, then reconnects
            // (the portal may start after Asteria, or restart).
            let _ = (|| -> zbus::Result<()> {
                let conn = zbus::blocking::Connection::session()?;
                let proxy = settings_proxy(&conn)?;
                let signals = proxy.receive_signal("SettingChanged")?;
                if let Some(dark) = read_scheme(&proxy) {
                    out.lock().unwrap().theme = Some(Theme::starshell(dark));
                    repaint.request_repaint();
                }
                for msg in signals {
                    let Ok((ns, key, value)) = msg.body().deserialize::<(String, String, zbus::zvariant::OwnedValue)>() else {
                        continue;
                    };
                    if ns == APPEARANCE && key == COLOR_SCHEME
                        && let Some(dark) = is_dark(&value)
                    {
                        out.lock().unwrap().theme = Some(Theme::starshell(dark));
                        repaint.request_repaint();
                    }
                }
                Ok(())
            })();
            std::thread::sleep(Duration::from_secs(30));
        }
    });

    let out = pending.clone();
    std::thread::spawn(move || {
        let fonts_conf = dirs::config_dir().unwrap_or_default().join("fontconfig/fonts.conf");
        let mtime = |p: &PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let mut last: Option<SystemTime> = mtime(&fonts_conf);
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let now = mtime(&fonts_conf);
            if now != last {
                last = now;
                out.lock().unwrap().fonts = Some(FontSet::resolve());
                ctx.request_repaint();
            }
        }
    });
    pending
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: Color32) -> f64 {
        let f = |v: u8| {
            let c = v as f64 / 255.0;
            if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f64 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// Text, the accent and every clarity hue stay readable on their own background.
    #[test]
    fn every_text_colour_is_at_least_4_5_to_1() {
        for p in [&LIGHT, &DARK] {
            for c in [p.accent, p.text_strong, p.text_muted, p.clay, p.sienna, p.ochre, p.olive, p.teal, p.slate, p.plum] {
                assert!(contrast(c, p.background) >= 4.5, "{c:?} on {:?} is {:.2}:1", p.background, contrast(c, p.background));
            }
        }
    }

    #[test]
    fn stored_group_colours_map_onto_clarity_hues() {
        let t = Theme::starshell(false);
        for name in crate::groups::COLORS {
            assert!(t.named(name).is_some(), "{name}");
        }
        assert_eq!(t.named("red"), Some(LIGHT.clay));
    }

    #[test]
    fn the_portal_scheme_reads_either_shape() {
        use zbus::zvariant::Value;
        assert_eq!(is_dark(&Value::U32(1)), Some(true));
        assert_eq!(is_dark(&Value::U32(2)), Some(false));
        assert_eq!(is_dark(&Value::U32(0)), Some(false));
        assert_eq!(is_dark(&Value::Value(Box::new(Value::U32(1)))), Some(true));
    }
}
