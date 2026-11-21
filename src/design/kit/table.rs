//! The quiet table, for things you read (backups, components, mod metadata, crash evidence):
//! hairline rows, uppercase tracked headers, tabular figures, a wash for the selection, and
//! no hover. Header and rows are separate calls so long tables can use `ScrollArea::show_rows`.

use egui::{Color32, Rect, Response, Sense, Ui, vec2};

use super::{rule, text};
use crate::design::fonts;
use crate::design::tokens::{self as tk, Tokens};

#[derive(Debug, Clone, Copy)]
pub enum Width {
    Fixed(f32),
    /// Takes what the fixed columns leave (shared equally between fill columns).
    Fill,
}

#[derive(Debug, Clone, Copy)]
pub struct Column<'a> {
    pub title: &'a str,
    pub width: Width,
    /// Figures align right.
    pub right: bool,
}

impl<'a> Column<'a> {
    pub fn fill(title: &'a str) -> Self {
        Self { title, width: Width::Fill, right: false }
    }

    pub fn fixed(title: &'a str, w: f32) -> Self {
        Self { title, width: Width::Fixed(w), right: false }
    }

    pub fn right(mut self) -> Self {
        self.right = true;
        self
    }
}

#[derive(Debug, Clone)]
pub enum Cell {
    /// Body text, `text_strong`.
    Text(String),
    /// Body text, muted.
    Muted(String),
    /// A number, in tabular figures.
    Figure(String),
    /// An uppercase caption, in a colour (a clarity hue for a status, or muted).
    Caption(String, Option<Color32>),
    /// A small picture (content: a loader's logo) before body text, `text_strong`.
    Lead(egui::ImageSource<'static>, String),
}

fn widths(columns: &[Column], total: f32) -> Vec<f32> {
    let pad = tk::GUTTER * 2.0;
    let fixed: f32 = columns.iter().map(|c| if let Width::Fixed(w) = c.width { w } else { 0.0 }).sum();
    let fills = columns.iter().filter(|c| matches!(c.width, Width::Fill)).count().max(1) as f32;
    let fill = ((total - pad - fixed) / fills).max(40.0);
    columns.iter().map(|c| if let Width::Fixed(w) = c.width { w } else { fill }).collect()
}

/// The header row: captions over a strong rule.
pub fn header(ui: &mut Ui, t: &Tokens, columns: &[Column]) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), tk::TABLE_HEADER_HEIGHT), Sense::hover());
    let ws = widths(columns, rect.width());
    let p = ui.painter();
    let mut x = rect.left() + tk::GUTTER;
    for (c, w) in columns.iter().zip(ws) {
        let g = text::caption_galley(ui, c.title, t.text_muted);
        let gx = if c.right { x + w - g.size().x } else { x };
        p.galley(egui::pos2(gx, rect.center().y - g.size().y / 2.0), g, t.text_muted);
        x += w;
    }
    rule::hline(p, rect.x_range(), rect.bottom() - 0.5, t.line_strong);
    resp
}

/// One row. A click selects it (the caller keeps the selection); there is no hover state.
pub fn row(ui: &mut Ui, t: &Tokens, columns: &[Column], cells: &[Cell], selected: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), tk::TABLE_ROW_HEIGHT), Sense::click());
    let ws = widths(columns, rect.width());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 0.0, t.wash(0.08));
    }
    let mut x = rect.left() + tk::GUTTER;
    let cy = rect.center().y;
    for ((c, w), cell) in columns.iter().zip(ws).zip(cells) {
        // Left-aligned cells stop a gap short of the next column; right-aligned ones end on it.
        let end = if c.right { x + w } else { x + w - tk::GAP };
        let clip = Rect::from_min_max(egui::pos2(x, rect.top()), egui::pos2(end, rect.bottom()));
        let p = p.with_clip_rect(clip.intersect(p.clip_rect()));
        match cell {
            Cell::Figure(s) => {
                let font = fonts::text(tk::SIZE_BODY);
                let fw = text::tabular_width(ui, s, font.clone());
                let fx = if c.right { x + w - fw } else { x };
                text::tabular(&p, egui::pos2(fx, cy), s, font, t.text_strong);
            }
            Cell::Caption(s, color) => {
                let color = color.unwrap_or(t.text_muted);
                let g = text::caption_galley(ui, s, color);
                let gx = if c.right { x + w - g.size().x } else { x };
                p.galley(egui::pos2(gx, cy - g.size().y / 2.0), g, color);
            }
            Cell::Lead(src, s) => {
                let size = tk::ICON_SIZE;
                let r = Rect::from_min_size(egui::pos2(x, cy - size / 2.0), vec2(size, size));
                egui::Image::new(src.clone()).corner_radius(0).paint_at(ui, r.intersect(clip));
                let g = p.layout_no_wrap(s.clone(), fonts::text(tk::SIZE_BODY), t.text_strong);
                p.galley(egui::pos2(x + size + tk::UNIT, cy - g.size().y / 2.0), g, t.text_strong);
            }
            Cell::Text(s) | Cell::Muted(s) => {
                let color = if matches!(cell, Cell::Text(_)) { t.text_strong } else { t.text_muted };
                let g = p.layout_no_wrap(s.clone(), fonts::text(tk::SIZE_BODY), color);
                let gx = if c.right { x + w - g.size().x } else { x };
                p.galley(egui::pos2(gx, cy - g.size().y / 2.0), g, color);
            }
        }
        x += w;
    }
    rule::hline(ui.painter(), rect.x_range(), rect.bottom() - 0.5, t.line);
    let label = cells.iter().map(|c| match c {
        Cell::Text(s) | Cell::Muted(s) | Cell::Figure(s) | Cell::Caption(s, _) | Cell::Lead(_, s) => s.as_str(),
    }).collect::<Vec<_>>().join(", ");
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &label));
    resp
}

/// The keyboard is on this row (its pane has the keys): the accent marker at its left edge.
pub fn mark(ui: &Ui, t: &Tokens, rect: Rect) {
    ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(tk::MARKER, rect.height())), 0.0, t.accent);
}
