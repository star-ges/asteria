//! The bars: a row of hairline cells (StarShell's bar). The open cell, and any hot one, gets
//! the 3px accent marker at its foot; a hot cell's label turns accent. And the mark: a
//! four-pointed star on its own; hot, it turns accent and turns 45°.

use egui::{Color32, Rect, Response, Sense, Shape, Ui, vec2};

use super::stars;
use super::{rule, text};
use crate::design::fonts;
use crate::design::motion::{self, Motion};
use crate::design::tokens::{self as tk, Tokens};

/// A bar `BAR_HEIGHT` tall across the available width, a hairline under it; `cells` lays its
/// cells out left to right (use a right-to-left layout inside for the right end).
pub fn bar<R>(ui: &mut Ui, t: &Tokens, cells: impl FnOnce(&mut Ui) -> R) -> R {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), tk::BAR_HEIGHT), Sense::hover());
    let out = ui.scope_builder(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        cells(ui)
    });
    rule::hline(ui.painter(), rect.x_range(), rect.bottom() - 0.5, t.line_strong);
    out.inner
}

pub struct BarCell<'a> {
    label: &'a str,
    number: Option<usize>,
    open: bool,
    hot: bool,
    /// A clarity-hue square and caption after the label (`RUNNING` in olive).
    status: Option<(&'a str, Color32)>,
    /// On the bar's right end: the cell's line is on its left.
    right: bool,
    sense: Sense,
    padding: f32,
}

impl<'a> BarCell<'a> {
    pub fn new(label: &'a str) -> Self {
        Self { label, number: None, open: false, hot: false, status: None, right: false, sense: Sense::click(), padding: tk::BAR_CELL_PADDING }
    }

    /// Less room either side of the label, where a row of cells must fit a narrow pane.
    pub fn padding(mut self, p: f32) -> Self {
        self.padding = p;
        self
    }

    /// How wide the cell will be.
    pub fn width(&self, ui: &Ui, t: &Tokens) -> f32 {
        let g = text::caption_galley(ui, &self.text(), t.text_strong);
        let status_w = self.status.map_or(0.0, |(s, c)| tk::GAP + tk::MARK_SQUARE + tk::UNIT + text::caption_galley(ui, s, c).size().x);
        (g.size().x + status_w + self.padding * 2.0).round()
    }

    fn text(&self) -> String {
        match self.number {
            // `01 / INSTANCES`, as every numbered label in the kit (and StarShell's own bar).
            Some(n) => format!("{} / {}", text::number(n), self.label),
            None => self.label.to_string(),
        }
    }

    pub fn number(mut self, n: usize) -> Self {
        self.number = Some(n);
        self
    }

    pub fn open(mut self, on: bool) -> Self {
        self.open = on;
        self
    }

    pub fn hot(mut self, on: bool) -> Self {
        self.hot = on;
        self
    }

    pub fn status(mut self, text: &'a str, color: Color32) -> Self {
        self.status = Some((text, color));
        self
    }

    pub fn right(mut self) -> Self {
        self.right = true;
        self
    }

    /// A cell that shows something but leads nowhere: no hover response (honest interactions).
    pub fn passive(mut self) -> Self {
        self.sense = Sense::hover();
        self
    }

    pub fn show(self, ui: &mut Ui, t: &Tokens) -> Response {
        let label = self.text();
        let status_g = self.status.map(|(s, c)| (text::caption_galley(ui, s, c), c));
        let w = self.width(ui, t);
        let (rect, resp) = ui.allocate_exact_size(vec2(w, tk::BAR_HEIGHT), self.sense);
        let active = self.sense.senses_click();
        let hot = active && (resp.hovered() || resp.has_focus() || self.hot);
        let ctx = ui.ctx();
        let p = ui.painter();
        let ink = motion::color(ctx, resp.id.with("ink"), hot, if self.open { t.text_strong } else { t.text_muted }, t.accent);
        let mut x = rect.left() + self.padding;
        let r = text::paint_caption(p, egui::pos2(x, rect.center().y), &label, ink);
        x = r.right();
        if let Some((g, c)) = status_g {
            x += tk::GAP;
            p.rect_filled(Rect::from_center_size(egui::pos2(x + tk::MARK_SQUARE / 2.0, rect.center().y), vec2(tk::MARK_SQUARE, tk::MARK_SQUARE)), 0.0, c);
            x += tk::MARK_SQUARE + tk::UNIT;
            p.galley(egui::pos2(x, rect.center().y - g.size().y / 2.0), g, c);
        }
        let line_x = if self.right { rect.left() } else { rect.right() - tk::LINE };
        rule::vline(p, line_x, rect.y_range(), t.line_strong);
        let m = motion::toggle(ctx, resp.id.with("marker"), hot || self.open, Motion::Fast);
        if m > 0.0 {
            let mr = Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - tk::MARKER), egui::pos2(rect.right() - tk::LINE, rect.bottom()));
            p.rect_filled(mr, 0.0, tk::alpha_of(t.accent, m));
        }
        let mut resp = resp;
        if active {
            resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
        let open = self.open;
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, active, open, &label));
        resp
    }
}

/// Paints the mark (a four-pointed star) in `rect`, turned by `turn` (0–1 of 45°).
pub fn paint_mark(ui: &Ui, rect: Rect, color: Color32, turn: f32) {
    let p = ui.painter();
    let mut shapes = Vec::new();
    stars::paint_star(&mut shapes, rect.center(), rect.width() * 0.5, 4, tk::STAR_INNER_RATIO, turn * std::f32::consts::FRAC_PI_4, color, false);
    p.add(Shape::Vec(shapes));
}

/// The bar's first cell: the mark and the wordmark (Switzer ExtraBold 18).
pub fn mark_cell(ui: &mut Ui, t: &Tokens, name: &str) -> Response {
    let g = ui.painter().layout_no_wrap(name.to_string(), fonts::heading(tk::SIZE_HEADING_SMALL), Color32::PLACEHOLDER);
    let w = (tk::BAR_CELL_PADDING * 2.0 + tk::BAR_MARK_SIZE + tk::GAP + g.size().x).round();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, tk::BAR_HEIGHT), Sense::click());
    let hot = resp.hovered() || resp.has_focus();
    let ctx = ui.ctx();
    let c = motion::color(ctx, resp.id, hot, t.text_strong, t.accent);
    let turn = motion::toggle(ctx, resp.id.with("turn"), hot, Motion::Medium);
    let m = Rect::from_min_size(egui::pos2(rect.left() + tk::BAR_CELL_PADDING, rect.center().y - tk::BAR_MARK_SIZE / 2.0), vec2(tk::BAR_MARK_SIZE, tk::BAR_MARK_SIZE));
    paint_mark(ui, m, c, turn);
    ui.painter().galley(egui::pos2(m.right() + tk::GAP, rect.center().y - g.size().y / 2.0), g, c);
    rule::vline(ui.painter(), rect.right() - tk::LINE, rect.y_range(), t.line_strong);
    let name = name.to_string();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
