//! Switch and checkbox: square track and knob, square box. Neither changes its value by
//! being drawn; they report a click and the owner flips the value.

use egui::{CursorIcon, Rect, Response, Sense, Ui, vec2};

use super::{rule, text};
use crate::design::fonts;
use crate::design::glyphs;
use crate::design::motion::{self, Motion};
use crate::design::tokens::{self as tk, Tokens};

/// An outlined square track with a square knob: on = knob right, filled with the accent.
/// `hot` is keyboard-current. Returns the response; `clicked()` means "toggle".
pub fn switch(ui: &mut Ui, t: &Tokens, on: bool, enabled: bool, hot: bool, label: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(tk::SWITCH_WIDTH, tk::SWITCH_HEIGHT), if enabled { Sense::click() } else { Sense::hover() });
    let hot = enabled && (resp.hovered() || resp.has_focus() || hot);
    let ctx = ui.ctx();
    let fade = |c: egui::Color32| if enabled { c } else { tk::alpha_of(c, 0.5) };
    let p = ui.painter();
    rule::frame(p, rect, fade(motion::color(ctx, resp.id.with("track"), hot, t.line_strong, t.accent)));
    let size = tk::SWITCH_HEIGHT - tk::SWITCH_INSET * 2.0;
    let travel = tk::SWITCH_WIDTH - tk::SWITCH_INSET * 2.0 - size;
    let k = motion::toggle(ctx, resp.id.with("knob"), on, Motion::Medium);
    let knob = Rect::from_min_size(rect.min + vec2(tk::SWITCH_INSET + travel * k, tk::SWITCH_INSET), vec2(size, size));
    let fill = motion::toggle(ctx, resp.id.with("fill"), on, Motion::Medium);
    if fill > 0.0 {
        p.rect_filled(knob, 0.0, fade(tk::alpha_of(t.accent, fill)));
    }
    let border = if on || hot { t.accent } else { t.text_muted };
    rule::frame(p, knob, fade(border));
    if enabled {
        resp.clone().on_hover_cursor(CursorIcon::PointingHand);
    }
    let label = label.to_string();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, on, &label));
    resp
}

/// A square checkbox with an optional label after it (body text). The whole row is the hit
/// area. `clicked()` means "toggle".
pub fn checkbox(ui: &mut Ui, t: &Tokens, on: bool, enabled: bool, hot: bool, label: &str) -> Response {
    let font = fonts::text(tk::SIZE_BODY);
    let g = (!label.is_empty()).then(|| ui.painter().layout_no_wrap(label.to_string(), font, egui::Color32::PLACEHOLDER));
    let w = tk::CHECKBOX_SIZE + g.as_ref().map_or(0.0, |g| tk::UNIT + g.size().x);
    let h = tk::CHECKBOX_SIZE.max(g.as_ref().map_or(0.0, |g| g.size().y));
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), if enabled { Sense::click() } else { Sense::hover() });
    let hot = enabled && (resp.hovered() || resp.has_focus() || hot);
    let ctx = ui.ctx();
    let p = ui.painter();
    let fade = |c: egui::Color32| if enabled { c } else { tk::alpha_of(c, 0.5) };
    let b = Rect::from_min_size(egui::pos2(rect.left(), rect.center().y - tk::CHECKBOX_SIZE / 2.0), vec2(tk::CHECKBOX_SIZE, tk::CHECKBOX_SIZE));
    let border = motion::color(ctx, resp.id.with("box"), hot || on, t.line_strong, t.accent);
    let fill = motion::toggle(ctx, resp.id.with("fill"), on, Motion::Fast);
    if fill > 0.0 {
        p.rect_filled(b, 0.0, fade(tk::alpha_of(t.accent, fill)));
        text::paint_glyph(p, b.center(), glyphs::CHECK, fonts::icons_bold(tk::CHECKBOX_SIZE - 4.0), tk::alpha_of(t.text_on_fill(), fill));
    }
    rule::frame(p, b, fade(border));
    if let Some(g) = g {
        let c = motion::color(ctx, resp.id.with("label"), hot, if enabled { t.text_strong } else { t.text_muted }, t.accent);
        p.galley(egui::pos2(b.right() + tk::UNIT, rect.center().y - g.size().y / 2.0), g, c);
    }
    if enabled {
        resp.clone().on_hover_cursor(CursorIcon::PointingHand);
    }
    let label = label.to_string();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, on, &label));
    resp
}
