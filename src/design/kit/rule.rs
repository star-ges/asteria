//! Hairlines and numbered section labels. Lines are snapped to pixel centres, so lines that
//! meet do so exactly.

use egui::{Color32, Painter, Rangef, Rect, Response, Sense, Stroke, Ui, vec2};

use super::text;
use crate::design::motion;
use crate::design::tokens::{self as tk, Tokens};

pub fn hline(painter: &Painter, x: impl Into<Rangef>, y: f32, color: Color32) {
    let y = painter.round_to_pixel_center(y);
    painter.hline(x, y, Stroke::new(tk::LINE, color));
}

pub fn vline(painter: &Painter, x: f32, y: impl Into<Rangef>, color: Color32) {
    let x = painter.round_to_pixel_center(x);
    painter.vline(x, y, Stroke::new(tk::LINE, color));
}

/// A 1px frame drawn inside `rect`, sharing its edges with neighbours' frames.
pub fn frame(painter: &Painter, rect: Rect, color: Color32) {
    painter.rect_stroke(rect.shrink(0.5), egui::CornerRadius::ZERO, Stroke::new(tk::LINE, color), egui::StrokeKind::Middle);
}

/// A full-width hairline taking 1px of layout: `line` divides rows, `strong` frames sections.
pub fn rule(ui: &mut Ui, t: &Tokens, strong: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), tk::LINE), Sense::hover());
    hline(ui.painter(), rect.x_range(), rect.center().y, if strong { t.line_strong } else { t.line });
    resp
}

/// `01 / MODS ────────── 2`: a numbered section label over a strong rule.
pub struct SectionLabel<'a> {
    number: usize,
    label: &'a str,
    aside: &'a str,
    /// A clarity hue (a group's colour): the label and a small square take it.
    hue: Option<Color32>,
    /// While something is dragged over this section, its rule turns accent.
    drop_target: bool,
    sense: Sense,
}

impl<'a> SectionLabel<'a> {
    pub fn new(number: usize, label: &'a str) -> Self {
        Self { number, label, aside: "", hue: None, drop_target: false, sense: Sense::hover() }
    }

    pub fn aside(mut self, aside: &'a str) -> Self {
        self.aside = aside;
        self
    }

    pub fn hue(mut self, hue: Option<Color32>) -> Self {
        self.hue = hue;
        self
    }

    pub fn drop_target(mut self, on: bool) -> Self {
        self.drop_target = on;
        self
    }

    /// Clickable (folding a group): the label turns accent when hot.
    pub fn clickable(mut self) -> Self {
        self.sense = Sense::click();
        self
    }

    pub fn show(self, ui: &mut Ui, t: &Tokens) -> Response {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), tk::SECTION_LABEL_HEIGHT), self.sense);
        let p = ui.painter();
        let hot = self.sense.senses_click() && (resp.hovered() || resp.has_focus());
        let ink = self.hue.unwrap_or(t.text_muted);
        let ink = motion::color(ui.ctx(), resp.id, hot, ink, t.accent);
        let mut x = rect.left() + tk::GUTTER;
        let cy = rect.center().y;
        if let Some(hue) = self.hue {
            let sq = egui::Rect::from_center_size(egui::pos2(x + tk::MARK_SQUARE / 2.0, cy), vec2(tk::MARK_SQUARE, tk::MARK_SQUARE));
            p.rect_filled(sq, 0.0, hue);
            x += tk::MARK_SQUARE + tk::UNIT;
        }
        let label = format!("{} / {}", text::number(self.number), self.label);
        let r = text::paint_caption(p, egui::pos2(x, cy), &label, ink);
        let mut end = rect.right() - tk::GUTTER;
        if !self.aside.is_empty() {
            // The aside gives way to the label: elided to the room left, gone if there's none.
            let room = end - r.right() - tk::GAP * 2.0;
            if room > tk::GUTTER * 2.0 {
                let mut job = text::caption_job(self.aside, t.text_muted);
                job.wrap.max_width = room;
                job.wrap.max_rows = 1;
                let g = p.layout_job(job);
                end -= g.size().x;
                p.galley(egui::pos2(end, cy - g.size().y / 2.0), g, t.text_muted);
                end -= tk::GAP;
            }
        }
        // The line runs from the label to the aside, then the strong rule under the whole row.
        if end > r.right() + tk::GAP {
            hline(p, (r.right() + tk::GAP)..=end, cy, t.line);
        }
        let rule = motion::color(ui.ctx(), resp.id.with("drop"), self.drop_target, t.line_strong, t.accent);
        hline(p, rect.x_range(), rect.bottom() - 0.5, rule);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, self.label));
        resp
    }
}
