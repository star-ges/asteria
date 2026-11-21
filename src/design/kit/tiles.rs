//! A grid of tiles sharing their hairlines (screenshots, icons): the picture edge to edge in
//! its cell, a caption line under it. Hot: the caption turns accent and the 3px marker shows
//! at the cell's foot. Selected: a wash over the caption line.

use egui::{Rect, Sense, Ui, vec2};

use super::{rule, text};
use crate::design::motion;
use crate::design::tokens::{self as tk, Tokens};

pub struct Tiles {
    pub count: usize,
    pub columns: usize,
    /// Picture width / height.
    pub aspect: f32,
    pub selected: Option<usize>,
    /// Keyboard-current, from the pane's `Nav`.
    pub hot: Option<usize>,
}

/// A big picture changing: `current` crossfades in over `previous` (700ms, no blur or zoom).
/// `id` must be new for each change.
pub fn crossfade(ui: &Ui, rect: Rect, id: egui::Id, previous: Option<egui::ImageSource>, current: egui::ImageSource) {
    let a = super::sheet::entrance_with(ui, id, motion::Motion::ImageFade);
    if let Some(prev) = previous
        && a < 1.0
    {
        egui::Image::new(prev).tint(egui::Color32::from_white_alpha(((1.0 - a) * 255.0) as u8)).paint_at(ui, rect);
    }
    egui::Image::new(current).tint(egui::Color32::from_white_alpha((a * 255.0) as u8)).paint_at(ui, rect);
}

pub struct TilesOut {
    pub clicked: Option<usize>,
    pub double_clicked: Option<usize>,
    /// Each tile's rect, to scroll a keyboard move into view.
    pub rects: Vec<Rect>,
}

impl Tiles {
    /// `paint(ui, index, picture_rect)` draws the picture and returns the caption.
    pub fn show(self, ui: &mut Ui, t: &Tokens, mut paint: impl FnMut(&Ui, usize, Rect) -> String) -> TilesOut {
        let cols = self.columns.max(1);
        let total_w = ui.available_width();
        let cell_w = (total_w / cols as f32).floor();
        let pic_h = (cell_w / self.aspect).round();
        let cell_h = pic_h + tk::TILE_CAPTION_HEIGHT;
        let rows = self.count.div_ceil(cols);
        let (area, _) = ui.allocate_exact_size(vec2(cell_w * cols as f32, cell_h * rows as f32), Sense::hover());
        let mut out = TilesOut { clicked: None, double_clicked: None, rects: Vec::with_capacity(self.count) };
        for i in 0..self.count {
            let (r, c) = (i / cols, i % cols);
            let cell = Rect::from_min_size(area.min + vec2(c as f32 * cell_w, r as f32 * cell_h), vec2(cell_w, cell_h));
            out.rects.push(cell);
            if !ui.is_rect_visible(cell) {
                continue;
            }
            // Tiles move with their own keys (`Nav`), so they stay out of Tab's order.
            let resp = ui.interact(cell, ui.id().with(("tile", i)), Sense::CLICK);
            let pic = Rect::from_min_size(cell.min, vec2(cell_w, pic_h)).shrink(tk::LINE);
            let caption = paint(ui, i, pic);
            let hot = resp.hovered() || self.hot == Some(i);
            let p = ui.painter();
            let cap_rect = Rect::from_min_max(egui::pos2(cell.left(), cell.top() + pic_h), cell.max);
            if self.selected == Some(i) {
                p.rect_filled(cap_rect, 0.0, t.wash(tk::ROW_WASH));
            }
            let color = motion::color(ui.ctx(), resp.id, hot, t.text_muted, t.accent);
            let mut job = text::caption_job(&caption, color);
            job.wrap.max_width = cell_w - tk::GAP * 2.0;
            job.wrap.max_rows = 1;
            let g = p.layout_job(job);
            p.galley(egui::pos2(cap_rect.left() + tk::GAP, cap_rect.center().y - g.size().y / 2.0), g, color);
            let m = motion::toggle(ui.ctx(), resp.id.with("marker"), hot, motion::Motion::Fast);
            if m > 0.0 {
                let r = Rect::from_min_max(egui::pos2(cell.left(), cell.bottom() - tk::MARKER), egui::pos2(cell.left() + cell_w * m, cell.bottom()));
                p.rect_filled(r, 0.0, t.accent);
            }
            if resp.clicked() {
                out.clicked = Some(i);
            }
            if resp.double_clicked() {
                out.double_clicked = Some(i);
            }
            resp.on_hover_cursor(egui::CursorIcon::PointingHand).widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, self.selected == Some(i), &caption));
        }
        // Shared lines: the frame, one line between columns, and under each picture and row.
        let p = ui.painter();
        rule::frame(p, area, t.line_strong);
        for c in 1..cols {
            rule::vline(p, area.left() + c as f32 * cell_w, area.y_range(), t.line_strong);
        }
        for r in 0..rows {
            let top = area.top() + r as f32 * cell_h;
            rule::hline(p, area.x_range(), top + pic_h, t.line);
            if r > 0 {
                rule::hline(p, area.x_range(), top, t.line_strong);
            }
        }
        out
    }
}
