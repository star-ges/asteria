//! egui's own style, made from the tokens, so the stock widgets that remain (scroll bars,
//! text edits, context menus, tooltips, drag values) look like the kit: square, no shadows,
//! the one background, hairline frames, selection as a wash of the text colour, the accent
//! for hover and focus only. Applied at start and on every frame of the light ⇄ dark fade.

use egui::{CornerRadius, Shadow, Stroke, TextStyle};

use super::fonts;
use super::tokens::{self as tk, Tokens};

pub fn apply(ctx: &egui::Context, t: &Tokens) {
    ctx.set_visuals(if t.dark { egui::Visuals::dark() } else { egui::Visuals::light() });
    let square = CornerRadius::ZERO;
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, fonts::text(tk::SIZE_CAPTION)),
            (TextStyle::Body, fonts::text(tk::SIZE_BODY)),
            (TextStyle::Button, fonts::text(tk::SIZE_BODY)),
            (TextStyle::Monospace, fonts::raw(tk::SIZE_MONO)),
            (TextStyle::Heading, fonts::heading(tk::SIZE_HEADING)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(tk::UNIT, tk::UNIT * 0.75);
        style.spacing.button_padding = egui::vec2(tk::GAP, tk::UNIT * 0.75);
        style.spacing.interact_size.y = tk::SIZE_BODY + tk::GAP;
        style.spacing.menu_margin = egui::Margin::same(tk::UNIT as i8);
        style.spacing.scroll.floating = true;
        // Narrow windows clip text at the edge rather than reflowing it; paragraphs opt back
        // in to wrapping (the kit's body text).
        style.wrap_mode = Some(egui::TextWrapMode::Extend);

        let v = &mut style.visuals;
        v.dark_mode = t.dark;
        v.override_text_color = None;
        v.panel_fill = t.background;
        v.window_fill = t.background;
        v.window_stroke = Stroke::new(tk::LINE, t.line_strong);
        v.window_corner_radius = square;
        v.menu_corner_radius = square;
        v.window_shadow = Shadow::NONE;
        v.popup_shadow = Shadow::NONE;
        v.extreme_bg_color = t.background;
        v.text_edit_bg_color = Some(t.background);
        v.code_bg_color = t.wash(0.05);
        v.faint_bg_color = t.wash(0.06);
        v.hyperlink_color = t.text_strong;
        v.warn_fg_color = t.warning();
        v.error_fg_color = t.error();
        v.weak_text_color = Some(t.text_muted);
        // Selections are a wash of the text colour, never an accent tint.
        v.selection.bg_fill = t.wash(0.16);
        v.selection.stroke = Stroke::new(1.0, t.text_strong);
        v.text_cursor.stroke = Stroke::new(tk::CARET_WIDTH, t.accent);
        v.striped = false;
        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.5 };
        v.indent_has_left_vline = false;

        let w = &mut v.widgets;
        w.noninteractive.bg_fill = t.background;
        w.noninteractive.weak_bg_fill = t.background;
        w.noninteractive.bg_stroke = Stroke::new(tk::LINE, t.line);
        w.noninteractive.fg_stroke = Stroke::new(1.0, t.text_strong);

        w.inactive.bg_fill = t.wash(0.14);
        w.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        w.inactive.bg_stroke = Stroke::new(tk::LINE, t.line_strong);
        w.inactive.fg_stroke = Stroke::new(1.0, t.text_strong);

        for (state, fill) in [(&mut w.hovered, 0.06), (&mut w.active, 0.16), (&mut w.open, 0.12)] {
            state.bg_fill = t.wash(fill);
            state.weak_bg_fill = t.wash(fill);
            state.bg_stroke = Stroke::new(tk::LINE, t.accent);
            state.fg_stroke = Stroke::new(1.0, t.accent);
        }
        for wv in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
            wv.corner_radius = square;
            wv.expansion = 0.0;
        }
    });
}
