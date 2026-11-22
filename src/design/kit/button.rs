//! Buttons: outlined squares with uppercase tracked labels. `filled` is the one primary
//! button on a screen (`text_strong`, accent when hot); `checked` fills with the accent (a
//! toggle, the chosen segment); `danger` is a destructive action's confirming button, filled
//! with the error colour (clay; user-approved, the one fill a clarity hue gets). Joined rows
//! share their borders.

use egui::{CursorIcon, Rect, Response, Sense, Ui, vec2};

use super::{rule, text};
use crate::design::fonts;
use crate::design::motion;
use crate::design::tokens::{self as tk, Tokens};

#[derive(Clone)]
pub struct Button<'a> {
    label: &'a str,
    icon: Option<&'a str>,
    filled: bool,
    danger: bool,
    checked: bool,
    enabled: bool,
    /// Keyboard-current, from the pane's `Nav` (the same response as hover).
    hot: bool,
    min_width: f32,
    /// The label keeps its case (a key).
    key: bool,
    /// What a screen reader calls an icon-only button (it has no label to read).
    name: &'a str,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str) -> Self {
        Self { label, icon: None, filled: false, danger: false, checked: false, enabled: true, hot: false, min_width: 0.0, key: false, name: label }
    }

    /// An icon before the label, or alone in a square button with an empty label (then it's
    /// the label: play, stop, arrows).
    /// An icon-only button (`Button::new("").icon(..)`) with `name` for screen readers.
    pub fn named(mut self, name: &'a str) -> Self {
        self.name = name;
        self
    }

    pub fn icon(mut self, glyph: &'a str) -> Self {
        self.icon = Some(glyph);
        self
    }

    pub fn filled(mut self) -> Self {
        self.filled = true;
        self
    }

    /// Confirms a destructive action: filled with the error colour, a shade towards the ink when hot.
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn checked(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }

    pub fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }

    pub fn hot(mut self, on: bool) -> Self {
        self.hot = on;
        self
    }

    /// The label is a key and keeps its case: `e` and `E` are different keys.
    pub fn key(mut self) -> Self {
        self.key = true;
        self
    }

    fn label_galley(&self, ui: &Ui, color: egui::Color32) -> std::sync::Arc<egui::Galley> {
        if self.key { ui.painter().layout_job(text::key_job(self.label, color)) } else { text::caption_galley(ui, self.label, color) }
    }

    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = w;
        self
    }

    fn content_width(&self, ui: &Ui, t: &Tokens) -> f32 {
        let label = if self.label.is_empty() { 0.0 } else { self.label_galley(ui, t.text_strong).size().x };
        let icon = if self.icon.is_some() { tk::ICON_SIZE_SMALL + if self.label.is_empty() { 0.0 } else { tk::ICON_GAP } } else { 0.0 };
        label + icon
    }

    pub fn width(&self, ui: &Ui, t: &Tokens) -> f32 {
        if self.label.is_empty() && self.icon.is_some() {
            return tk::BUTTON_HEIGHT;
        }
        (self.content_width(ui, t) + tk::BUTTON_PADDING * 2.0).max(self.min_width).round()
    }

    pub fn show(self, ui: &mut Ui, t: &Tokens) -> Response {
        let size = vec2(self.width(ui, t), tk::BUTTON_HEIGHT);
        let sense = if self.enabled { Sense::click() } else { Sense::hover() };
        let (rect, resp) = ui.allocate_exact_size(size, sense);
        self.paint(ui, t, rect, &resp, true);
        resp
    }

    /// Paints into `rect`; `own_frame` false leaves the frame to a joined row.
    fn paint(&self, ui: &Ui, t: &Tokens, rect: Rect, resp: &Response, own_frame: bool) {
        let hot = self.enabled && (resp.hovered() || resp.has_focus() || self.hot);
        // A disabled button is never filled: it would read as the one to press.
        let solid = self.enabled && (self.checked || self.filled || self.danger);
        let ctx = ui.ctx();
        let p = ui.painter();
        let (rest, on) = if self.danger {
            (t.error(), t.error().lerp_to_gamma(t.text_strong, 0.25))
        } else if self.checked {
            (t.accent, t.accent)
        } else {
            (t.text_strong, t.accent)
        };
        let fill = motion::color(ctx, resp.id.with("fill"), hot, rest, on);
        if solid {
            p.rect_filled(rect, 0.0, fill);
        } else if resp.is_pointer_button_down_on() {
            p.rect_filled(rect, 0.0, t.wash(0.12));
        }
        if own_frame || solid || hot {
            let border = if solid { fill } else { motion::color(ctx, resp.id.with("border"), hot, t.line_strong, t.accent) };
            rule::frame(p, rect, border);
        }
        let ink = if !self.enabled {
            t.text_muted
        } else if solid {
            t.text_on_fill()
        } else {
            motion::color(ctx, resp.id.with("ink"), hot, t.text_strong, t.accent)
        };
        let icon_only = self.label.is_empty();
        let mut x = rect.center().x - self.content_width(ui, t) / 2.0;
        if let Some(g) = self.icon {
            let c = if icon_only || solid || hot { ink } else { t.text_muted };
            let font = if icon_only { fonts::icons_bold(tk::ICON_SIZE_SMALL) } else { fonts::icons(tk::ICON_SIZE_SMALL) };
            text::paint_glyph(p, egui::pos2(x + tk::ICON_SIZE_SMALL / 2.0, rect.center().y), g, font, c);
            x += tk::ICON_SIZE_SMALL + tk::ICON_GAP;
        }
        if !icon_only {
            let g = self.label_galley(ui, ink);
            p.galley(egui::pos2(x, rect.center().y - g.size().y / 2.0), g, ink);
        }
        if self.enabled {
            resp.clone().on_hover_cursor(CursorIcon::PointingHand);
        }
        let label = if self.label.is_empty() { self.name } else { self.label }.to_string();
        let (enabled, checked) = (self.enabled, self.checked);
        resp.widget_info(|| {
            if checked { egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, true, &label) } else { egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &label) }
        });
    }
}

/// Outlined buttons joined into one row, sharing borders. Returns the index clicked.
pub fn button_row(ui: &mut Ui, t: &Tokens, buttons: &[Button]) -> Option<usize> {
    let widths: Vec<f32> = buttons.iter().map(|b| b.width(ui, t)).collect();
    // Shared borders: each button overlaps the previous one's right edge.
    let total = widths.iter().sum::<f32>() - (buttons.len().saturating_sub(1)) as f32 * tk::LINE;
    let (rect, _) = ui.allocate_exact_size(vec2(total, tk::BUTTON_HEIGHT), Sense::hover());
    let mut rects = Vec::with_capacity(buttons.len());
    let mut x = rect.left();
    for w in &widths {
        rects.push(Rect::from_min_size(egui::pos2(x, rect.top()), vec2(*w, tk::BUTTON_HEIGHT)));
        x += w - tk::LINE;
    }
    // One frame round the row and one line between neighbours; hot and filled buttons
    // then draw their own frame over them.
    let p = ui.painter();
    rule::frame(p, rect, t.line_strong);
    for r in &rects[..rects.len().saturating_sub(1)] {
        rule::vline(p, r.right() - 0.5, rect.y_range(), t.line_strong);
    }
    let mut clicked = None;
    for (i, (b, r)) in buttons.iter().zip(rects).enumerate() {
        let id = ui.id().with(("button-row", rect.min.x as i32, rect.min.y as i32, i));
        let resp = ui.interact(r, id, if b.enabled { Sense::click() } else { Sense::hover() });
        b.paint(ui, t, r, &resp, false);
        if resp.clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// A choice between a few options as joined buttons; the chosen one is filled with the accent.
/// Never changes `value` by being drawn; returns true when the user picked another.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, t: &Tokens, value: &mut T, options: &[(T, &str)]) -> bool {
    let buttons: Vec<Button> = options.iter().map(|(v, label)| Button::new(label).checked(*v == *value)).collect();
    match button_row(ui, t, &buttons) {
        Some(i) if options[i].0 != *value => {
            *value = options[i].0;
            true
        }
        _ => false,
    }
}

/// A choice from more options than a joined row holds: an outlined button with the current
/// choice and a caret, opening a menu of the options (Switzer, the current one marked). Never
/// changes anything by being drawn; returns the option picked, if it's another.
pub fn menu(ui: &mut Ui, t: &Tokens, id: egui::Id, current: Option<usize>, label: &str, options: &[&str], min_width: f32, hot: bool) -> Option<usize> {
    let resp = Button::new(label).icon(crate::design::glyphs::CARET_DOWN).min_width(min_width).hot(hot).enabled(!options.is_empty()).show(ui, t);
    let mut pick = None;
    egui::Popup::menu(&resp).id(id.with("menu")).width(resp.rect.width().max(min_width)).show(|ui| {
        ui.style_mut().override_font_id = Some(fonts::text(tk::SIZE_BODY));
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for (i, o) in options.iter().enumerate() {
                if ui.selectable_label(current == Some(i), *o).clicked() && current != Some(i) {
                    pick = Some(i);
                }
            }
        });
    });
    pick
}
