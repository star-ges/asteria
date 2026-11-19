//! The old screens' colours, made from the design tokens' palette so they follow light and
//! dark (and the crossfade) with everything else. Screens move off it onto `Tokens` and the
//! kit phase by phase (see `docs/asteria/ASTERIA_REDESIGN_PLAN.md`); it goes with the last one.

use egui::{Color32, CornerRadius, Shadow, Stroke};

use super::tokens::{DARK, LIGHT, Palette};

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

impl Default for Theme {
    fn default() -> Self {
        Self::starshell(false)
    }
}

impl Theme {
    pub fn starshell(dark: bool) -> Self {
        Self::from_palette(if dark { &DARK } else { &LIGHT }, dark)
    }

    /// The old screens' colours, made from a (possibly mid-crossfade) palette.
    pub fn from_palette(p: &Palette, dark: bool) -> Self {
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
