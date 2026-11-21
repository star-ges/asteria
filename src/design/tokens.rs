//! Design tokens: every colour, size, spacing and duration the kit draws with, named once,
//! the way StarShell's `Theme.qml` does it. Values are StarShell's, so the family resemblance
//! is exact; tune them here and nowhere else.
//!
//! Light and dark are two palettes with the same names. `Look` follows the desktop's colour
//! scheme and crossfades every colour between them over `DARK_FADE`.

use egui::Color32;

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// One palette of the StarShell design language, plus Asteria's clarity hues (an approved
/// exception: text colour and small marks only, never fills, washes, borders or state).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    /// Every surface.
    pub background: Color32,
    /// Hover, focus, active, selected. Nothing else.
    pub accent: Color32,
    /// Headings and primary text.
    pub text_strong: Color32,
    /// Secondary text, captions, inactive items, icons at rest.
    pub text_muted: Color32,
    /// Placeholder text in an empty field: below the muted text, so a hint never reads as content
    /// or a title (about 3.3:1: under the 4.5:1 for text, over the 3:1 floor for placeholders).
    pub text_hint: Color32,
    /// Lines that frame sections.
    pub line_strong: Color32,
    /// Lines that divide rows inside a section.
    pub line: Color32,
    /// Errors (redder and deeper than the accent, so an error never looks hovered).
    pub clay: Color32,
    pub sienna: Color32,
    /// Warnings.
    pub ochre: Color32,
    /// Running, installed, success.
    pub olive: Color32,
    pub teal: Color32,
    pub slate: Color32,
    pub plum: Color32,
    /// Syntax colours for the config editor's text mode (StarShell's code palette: the
    /// language's own exception, for readability). Names stay `text_strong`, punctuation
    /// `text_muted`; the accent stays off code.
    pub code_keyword: Color32,
    pub code_string: Color32,
    pub code_constant: Color32,
    pub code_function: Color32,
    pub code_comment: Color32,
}

/// Cream. The accent is 4.7:1 on the background.
pub const LIGHT: Palette = Palette {
    background: hex(0xfef0d5),
    accent: hex(0xad5034),
    text_strong: hex(0x202020),
    text_muted: hex(0x4c4c4c),
    text_hint: hex(0x8a857c),
    line_strong: hex(0x202020),
    // textMuted at 45% on the background, made opaque so lines that cross don't darken.
    line: hex(0xaea697),
    clay: hex(0xa33b30),
    sienna: hex(0x94541f),
    ochre: hex(0x8a6516),
    olive: hex(0x566a2a),
    teal: hex(0x2e6862),
    slate: hex(0x35596f),
    plum: hex(0x7d4a63),
    // On cream the earth tones sank into the background, so light has its own clearer set.
    code_keyword: hex(0xa3306e),
    code_string: hex(0x2d7a3a),
    code_constant: hex(0x7046ad),
    code_function: hex(0x1d5f9c),
    code_comment: hex(0x756d60),
};

/// A warm near-black, its own palette rather than an inversion. The accent is 5.6:1.
pub const DARK: Palette = Palette {
    background: hex(0x1c1a17),
    accent: hex(0xf2683a),
    text_strong: hex(0xe2d6bf),
    text_muted: hex(0xa99f8d),
    text_hint: hex(0x716a5e),
    line_strong: hex(0x4e4941),
    line: hex(0x3a3630),
    clay: hex(0xee7266),
    sienna: hex(0xd68a4c),
    ochre: hex(0xaa843b),
    olive: hex(0x7c9251),
    teal: hex(0x5a948d),
    slate: hex(0x698fa7),
    plum: hex(0xb17a94),
    // Dark uses the terminal's earth tones (the clarity hues are StarShell's terminal colours).
    code_keyword: hex(0xb17a94),
    code_string: hex(0x7c9251),
    code_constant: hex(0xaa843b),
    code_function: hex(0x698fa7),
    code_comment: hex(0xa99f8d),
};

impl Palette {
    /// `a` at `t = 0`, `b` at `t = 1`, every colour mixed the same way (`Theme.shade()`).
    pub fn mix(a: &Palette, b: &Palette, t: f32) -> Palette {
        let m = |x: Color32, y: Color32| x.lerp_to_gamma(y, t.clamp(0.0, 1.0));
        Palette {
            background: m(a.background, b.background),
            accent: m(a.accent, b.accent),
            text_strong: m(a.text_strong, b.text_strong),
            text_muted: m(a.text_muted, b.text_muted),
            text_hint: m(a.text_hint, b.text_hint),
            line_strong: m(a.line_strong, b.line_strong),
            line: m(a.line, b.line),
            clay: m(a.clay, b.clay),
            sienna: m(a.sienna, b.sienna),
            ochre: m(a.ochre, b.ochre),
            olive: m(a.olive, b.olive),
            teal: m(a.teal, b.teal),
            slate: m(a.slate, b.slate),
            plum: m(a.plum, b.plum),
            code_keyword: m(a.code_keyword, b.code_keyword),
            code_string: m(a.code_string, b.code_string),
            code_constant: m(a.code_constant, b.code_constant),
            code_function: m(a.code_function, b.code_function),
            code_comment: m(a.code_comment, b.code_comment),
        }
    }

    /// Selection and highlight: a wash of the text colour, never an accent tint.
    pub fn wash(&self, alpha: f32) -> Color32 {
        alpha_of(self.text_strong, alpha)
    }

    /// Text on a filled (`text_strong` or accent) surface.
    pub fn text_on_fill(&self) -> Color32 {
        self.background
    }

    pub fn error(&self) -> Color32 {
        self.clay
    }

    pub fn warning(&self) -> Color32 {
        self.ochre
    }

    pub fn ok(&self) -> Color32 {
        self.olive
    }

    /// A clarity hue by the name `groups.toml` stores: red → clay, orange → sienna,
    /// yellow → ochre, green → olive, cyan → teal, blue → slate, magenta → plum.
    pub fn named(&self, name: &str) -> Option<Color32> {
        Some(match name {
            "red" => self.clay,
            "orange" => self.sienna,
            "yellow" => self.ochre,
            "green" => self.olive,
            "cyan" => self.teal,
            "blue" => self.slate,
            "magenta" => self.plum,
            _ => return None,
        })
    }
}

/// `c` at `alpha` opacity.
pub fn alpha_of(c: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// The palette in use this frame (already crossfaded) and the system's motion setting. Kit
/// functions take it as `t`; `t.accent` etc. read the palette.
#[derive(Debug, Clone, PartialEq)]
pub struct Tokens {
    pub p: Palette,
    /// The scheme being moved towards (the crossfade may still be on its way).
    pub dark: bool,
    /// The desktop asked for no animations: every duration collapses to 0 and stars don't appear.
    pub reduced_motion: bool,
}

impl std::ops::Deref for Tokens {
    type Target = Palette;
    fn deref(&self) -> &Palette {
        &self.p
    }
}

impl Tokens {
    pub fn new(dark: bool) -> Self {
        Self { p: if dark { DARK } else { LIGHT }, dark, reduced_motion: false }
    }
}

// ---------------------------------------------------------------------------
// Type. Melodrama Medium for display only; Switzer ExtraBold for headings; Switzer SemiBold
// for everything else; JetBrains Mono for raw text only (logs, crash quotes, config text mode).

/// Hover-list titles, the instance name.
pub const SIZE_DISPLAY: f32 = 64.0;
/// One step down, for narrow columns and the filter query.
pub const SIZE_DISPLAY_SMALL: f32 = 44.0;
/// The index wordmark and big figures (line height `DISPLAY_LINE_HEIGHT`).
pub const SIZE_DISPLAY_LARGE: f32 = 144.0;
/// Melodrama's line box is tall: very large sizes sit at 0.8 of it.
pub const DISPLAY_LINE_HEIGHT: f32 = 0.8;
/// Page and sheet titles.
pub const SIZE_HEADING: f32 = 28.0;
/// Tile titles, compact-list titles in dense places.
pub const SIZE_HEADING_SMALL: f32 = 18.0;
/// Compact hover-list titles.
pub const SIZE_LIST_COMPACT: f32 = 20.0;
pub const SIZE_BODY: f32 = 14.0;
/// Uppercase tracked labels.
pub const SIZE_CAPTION: f32 = 11.0;
/// Letter-spacing for captions, px.
pub const CAPTION_TRACKING: f32 = 2.0;
/// A search or filter query, Switzer ExtraBold.
pub const SIZE_SEARCH: f32 = 44.0;
/// Raw text (JetBrains Mono).
pub const SIZE_MONO: f32 = 13.0;
/// Body paragraphs that are read at length (notes): line height as a factor of the size.
pub const READING_LINE_HEIGHT: f32 = 1.5;
/// Text read at length (notes).
pub const SIZE_READING: f32 = 16.0;

// ---------------------------------------------------------------------------
// Icons: Phosphor, Regular in `text_muted` at rest, accent when hot; Bold in the ink colour
// only where the icon is the label (play, stop).

/// Beside body text and headings.
pub const ICON_SIZE: f32 = 18.0;
/// Beside captions.
pub const ICON_SIZE_SMALL: f32 = 14.0;
/// Icon → its label.
pub const ICON_GAP: f32 = 6.0;

// ---------------------------------------------------------------------------
// Space and lines.

pub const UNIT: f32 = 8.0;
/// Outer padding of sections and panes.
pub const GUTTER: f32 = 24.0;
/// Between related elements.
pub const GAP: f32 = 12.0;
pub const LINE: f32 = 1.0;
/// Accent underline at the foot of a hovered or open cell, and the left tick of an unsaved row.
pub const MARKER: f32 = 3.0;
/// The small square that marks "unread" or "needs attention" (and a group's hue).
pub const MARK_SQUARE: f32 = 6.0;

// ---------------------------------------------------------------------------
// Lists (the hover list and its compact and quiet variants).

pub const LIST_ROW_HEIGHT: f32 = 104.0;
/// Left inset of titles at rest.
pub const LIST_INDENT: f32 = 120.0;
/// Title → description.
pub const LIST_DESCRIPTION_GAP: f32 = 28.0;
pub const LIST_DESCRIPTION_WIDTH: f32 = 300.0;
pub const LIST_DESCRIPTION_LINES: usize = 3;
/// Below this much room for a compact title beside its description, the description goes under it.
pub const LIST_TITLE_MIN: f32 = 120.0;
/// Between a title and the description under it.
pub const LIST_STACK_GAP: f32 = 2.0;
pub const LIST_ROW_HEIGHT_COMPACT: f32 = 56.0;
pub const LIST_INDENT_COMPACT: f32 = 64.0;
pub const LIST_DESCRIPTION_GAP_COMPACT: f32 = 20.0;
pub const LIST_DESCRIPTION_WIDTH_COMPACT: f32 = 170.0;
/// Quiet rows (long navigators like the configs tree): no shift, no stars.
pub const LIST_ROW_HEIGHT_QUIET: f32 = 32.0;
pub const LIST_INDENT_QUIET: f32 = 24.0;
/// The instance list beside an instance's page: about a quarter of the window, within these.
pub const LIST_RAIL_MIN: f32 = 240.0;
pub const LIST_RAIL_MAX: f32 = 360.0;
pub const LIST_RAIL_SHARE: f32 = 0.26;
/// The overview's left column (components) at most; the notes take the rest.
pub const OVERVIEW_COLUMN_MAX: f32 = 520.0;
/// Below this width the overview's columns stack.
pub const OVERVIEW_TWO_COLUMNS: f32 = 720.0;
/// The components table's version column.
pub const COMPONENT_VERSION_WIDTH: f32 = 140.0;
/// A sheet's reading column (headings, paragraphs, a table to choose from) at most.
pub const SHEET_WIDTH: f32 = 760.0;
/// A sheet with two panes side by side (mod sets: the sets, then a set's mods) at most.
pub const SHEET_WIDTH_WIDE: f32 = 1080.0;
/// The mod sets sheet's left pane (the sets).
pub const SETS_PANE_WIDTH: f32 = 320.0;
/// The configs tab's file list: its share of the tab's width, within these bounds.
pub const CONFIG_FILES_SHARE: f32 = 0.28;
pub const CONFIG_FILES_MIN: f32 = 220.0;
pub const CONFIG_FILES_MAX: f32 = 360.0;
/// A config list's rows before it starts folded.
pub const CONFIG_LIST_FOLD: usize = 6;
/// Below this width the content tab shows installed and Modrinth one at a time.
pub const CONTENT_TWO_PANES: f32 = 760.0;
/// The version columns of the found-updates table.
pub const UPDATE_VERSION_WIDTH: f32 = 120.0;
/// The "taken" column of a backups table.
pub const BACKUP_WHEN_WIDTH: f32 = 200.0;
/// The "kind" column of a backups table (`AUTO`, `KEPT`).
pub const BACKUP_KIND_WIDTH: f32 = 80.0;
/// The worlds tab's worlds pane: its share of the tab (the backups table takes the rest).
pub const WORLDS_SHARE: f32 = 0.55;
/// The log history's file list: its share of the tab, within these bounds.
pub const LOG_FILES_SHARE: f32 = 0.26;
pub const LOG_FILES_MIN: f32 = 220.0;
pub const LOG_FILES_MAX: f32 = 340.0;
/// A log line's level column (`WARN`, `ERROR`, `ASTERIA`).
pub const LOG_LEVEL_WIDTH: f32 = 76.0;
/// Log lines: a little air above and below the raw text.
pub const LOG_LINE_HEIGHT: f32 = SIZE_MONO + 7.0;
/// The crash analysis gets at most this share of the log's pane.
pub const CRASH_PANEL_SHARE: f32 = 0.5;
/// A crash analysis' suspects shown before "n more".
pub const CRASH_SUSPECTS_SHOWN: usize = 4;
/// Its confidence column.
pub const CRASH_CONFIDENCE_WIDTH: f32 = 100.0;
/// A server's name and address fields go side by side while each gets at least this.
pub const SERVER_FIELD_MIN: f32 = 180.0;
/// Screenshot tiles: at least this wide (as many columns as fit).
pub const SHOT_TILE_MIN: f32 = 240.0;
/// Leftward shift of the hot title.
pub const HOVER_SHIFT: f32 = 32.0;
pub const HOVER_SHIFT_COMPACT: f32 = 20.0;
/// The description slides in from this far right.
pub const DESCRIPTION_OFFSET: f32 = 12.0;
/// Inline picture at the row's right end.
pub const LIST_PREVIEW_HEIGHT: f32 = 72.0;
pub const LIST_PREVIEW_HEIGHT_COMPACT: f32 = 40.0;
/// Numbered section labels (`01 / MODS ──── 2`).
pub const SECTION_LABEL_HEIGHT: f32 = 40.0;

pub const STAR_COUNT: usize = 5;
pub const STAR_COUNT_COMPACT: usize = 3;
pub const STAR_SIZE_MIN: f32 = 14.0;
pub const STAR_SIZE_MAX: f32 = 46.0;
pub const STAR_SIZE_MIN_COMPACT: f32 = 10.0;
pub const STAR_SIZE_MAX_COMPACT: f32 = 26.0;
/// Inner / outer radius of a star: lower is spikier.
pub const STAR_INNER_RATIO: f32 = 0.22;

// ---------------------------------------------------------------------------
// Motion, in seconds. Ease-out curves, no bounce.

/// Colour and controls: OutCubic.
pub const FAST: f32 = 0.18;
/// Movement (shifts, reveals, panels): OutExpo.
pub const MEDIUM: f32 = 0.28;
/// Stars and inline pictures: OutExpo, staggered, drifting in from a smaller scale.
pub const IMAGERY: f32 = 0.35;
pub const IMAGERY_STAGGER: f32 = 0.05;
pub const IMAGERY_DRIFT: f32 = 18.0;
pub const IMAGERY_START_SCALE: f32 = 0.92;
/// The description waits this long before it fades in.
pub const DESCRIPTION_DELAY: f32 = 0.06;
/// A sheet's content rises this far into place.
pub const PANEL_RISE: f32 = 24.0;
/// A big picture changing: a crossfade, no blur or zoom.
pub const IMAGE_FADE: f32 = 0.7;
/// Light ⇄ dark.
pub const DARK_FADE: f32 = 0.6;
/// How long an info notice holds the bottom bar (errors stay until dismissed).
pub const NOTICE_SECONDS: f32 = 5.0;

// ---------------------------------------------------------------------------
// Controls.

pub const BUTTON_HEIGHT: f32 = 36.0;
pub const BUTTON_PADDING: f32 = 18.0;
pub const SWITCH_WIDTH: f32 = 36.0;
pub const SWITCH_HEIGHT: f32 = 20.0;
/// Track edge → knob.
pub const SWITCH_INSET: f32 = 4.0;
pub const CHECKBOX_SIZE: f32 = 16.0;
pub const FIELD_HEIGHT: f32 = 44.0;
pub const CARET_WIDTH: f32 = 2.0;
pub const SLIDER_HEIGHT: f32 = 10.0;
pub const SLIDER_TICK_HEIGHT: f32 = 5.0;
/// Intervals in a slider's or meter's ruler.
pub const SLIDER_TICKS: usize = 10;
/// A mark for a second value (a default) on a slider or meter.
pub const METER_MARK_WIDTH: f32 = 2.0;

pub const BAR_HEIGHT: f32 = 48.0;
/// Horizontal padding inside each bar cell.
pub const BAR_CELL_PADDING: f32 = 20.0;
pub const BAR_MARK_SIZE: f32 = 22.0;

/// Number + large title.
pub const SETTING_GROUP_HEADER_HEIGHT: f32 = 72.0;
/// Row labels sit this far in, past the group's bracket.
pub const SETTING_INDENT: f32 = 56.0;
/// At least; rows with a description grow.
pub const SETTING_ROW_HEIGHT: f32 = 72.0;
/// The shared column every row's control sits on (its width).
pub const SETTING_CONTROL_WIDTH: f32 = 300.0;
pub const SETTING_DESCRIPTION_WIDTH: f32 = 560.0;

pub const TABLE_ROW_HEIGHT: f32 = 32.0;
pub const TABLE_HEADER_HEIGHT: f32 = 32.0;

pub const TILE_CAPTION_HEIGHT: f32 = 44.0;

/// A cell of the spec strip: caption over value.
pub const SPEC_CELL_HEIGHT: f32 = 64.0;

#[cfg(test)]
pub mod tests {
    use super::*;

    fn luminance(c: Color32) -> f64 {
        let f = |v: u8| {
            let c = v as f64 / 255.0;
            if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
    }

    pub fn contrast(a: Color32, b: Color32) -> f64 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// Text, the accent and every clarity hue stay readable on their own background, and
    /// text on a filled button stays readable on both fills.
    #[test]
    fn every_text_colour_is_at_least_4_5_to_1() {
        for p in [&LIGHT, &DARK] {
            for c in [p.accent, p.text_strong, p.text_muted, p.clay, p.sienna, p.ochre, p.olive, p.teal, p.slate, p.plum, p.code_keyword, p.code_string, p.code_constant, p.code_function, p.code_comment] {
                assert!(contrast(c, p.background) >= 4.5, "{c:?} on {:?} is {:.2}:1", p.background, contrast(c, p.background));
            }
            for fill in [p.text_strong, p.accent, p.clay, p.clay.lerp_to_gamma(p.text_strong, 0.25)] {
                assert!(contrast(p.text_on_fill(), fill) >= 4.5, "text on {fill:?} is {:.2}:1", contrast(p.text_on_fill(), fill));
            }
        }
    }

    /// Placeholders sit below the muted text but stay legible: at least 3:1, and fainter than `text_muted`.
    #[test]
    fn hints_are_fainter_than_text_but_legible() {
        for p in [&LIGHT, &DARK] {
            let c = contrast(p.text_hint, p.background);
            assert!(c >= 3.0, "hint {:?} is {c:.2}:1", p.text_hint);
            assert!(c < contrast(p.text_muted, p.background));
        }
    }

    #[test]
    fn stored_group_colours_map_onto_clarity_hues() {
        for name in crate::groups::COLORS {
            assert!(LIGHT.named(name).is_some(), "{name}");
        }
        assert_eq!(LIGHT.named("red"), Some(LIGHT.clay));
        assert_eq!(DARK.named("blue"), Some(DARK.slate));
    }

    #[test]
    fn the_crossfade_starts_and_ends_on_the_palettes() {
        assert_eq!(Palette::mix(&LIGHT, &DARK, 0.0), LIGHT);
        assert_eq!(Palette::mix(&LIGHT, &DARK, 1.0), DARK);
        let mid = Palette::mix(&LIGHT, &DARK, 0.5);
        assert_ne!(mid.background, LIGHT.background);
        assert_ne!(mid.background, DARK.background);
    }
}
