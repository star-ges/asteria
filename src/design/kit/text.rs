//! Type: captions (uppercase, tracked), headings, display sizes, body paragraphs, tabular
//! figures and icons.

use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontId, Galley, Painter, Pos2, Rect, Response, Sense, Ui, vec2};

use crate::design::fonts;
use crate::design::motion;
use crate::design::tokens::{self as tk, Tokens};

/// Captions, labels, tabs and button labels are written in sentence case in the source and
/// rendered uppercase here, in one place.
pub fn caps(text: &str) -> String {
    text.to_uppercase()
}

/// A caption as a layout job: Switzer SemiBold 11, uppercase, tracked.
pub fn caption_job(text: &str, color: Color32) -> LayoutJob {
    key_job(&caps(text), color)
}

/// A caption in its own case (a key: `e` and `E` are different keys), tracked.
pub fn key_job(text: &str, color: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(text, 0.0, TextFormat { font_id: fonts::text(tk::SIZE_CAPTION), color, extra_letter_spacing: tk::CAPTION_TRACKING, ..Default::default() });
    job
}

/// A caption that wraps at `width` (descriptions), at most `lines` lines.
pub fn caption_wrapped(text: &str, color: Color32, width: f32, lines: usize) -> LayoutJob {
    let mut job = caption_job(text, color);
    job.wrap.max_width = width;
    job.wrap.max_rows = lines;
    job.wrap.break_anywhere = false;
    job
}

pub fn caption_galley(ui: &Ui, text: &str, color: Color32) -> Arc<Galley> {
    ui.painter().layout_job(caption_job(text, color))
}

fn label(ui: &mut Ui, job: LayoutJob) -> Response {
    ui.add(egui::Label::new(job).selectable(false))
}

/// A muted caption. Like every caption laid out by itself, it wraps at the room it has
/// (a single-line label would widen its scroll area or pane instead).
pub fn caption(ui: &mut Ui, t: &Tokens, text: &str) -> Response {
    caption_colored(ui, text, t.text_muted)
}

pub fn caption_colored(ui: &mut Ui, text: &str, color: Color32) -> Response {
    // The label's own wrap mode wins over the job's, so it's asked of the label.
    ui.add(egui::Label::new(caption_job(text, color)).selectable(false).wrap())
}

/// Page and sheet titles: Switzer ExtraBold 28.
pub fn heading(ui: &mut Ui, t: &Tokens, text: &str) -> Response {
    ui.add(egui::Label::new(egui::RichText::new(text).font(fonts::heading(tk::SIZE_HEADING)).color(t.text_strong)).selectable(false).truncate())
}

/// Small headings: Switzer ExtraBold 18.
pub fn heading_small(ui: &mut Ui, t: &Tokens, text: &str) -> Response {
    ui.add(egui::Label::new(egui::RichText::new(text).font(fonts::heading(tk::SIZE_HEADING_SMALL)).color(t.text_strong)).selectable(false).truncate())
}

/// Melodrama Medium at `size`, on one line (elided).
pub fn display(ui: &mut Ui, t: &Tokens, text: &str, size: f32) -> Response {
    ui.add(egui::Label::new(egui::RichText::new(text).font(fonts::display(size)).color(t.text_strong)).selectable(false).truncate())
}

/// Melodrama very large (the index wordmark, big figures). Its line box is tall above the
/// letters, so the box is closed up to `DISPLAY_LINE_HEIGHT` by trimming the top.
pub fn display_large(ui: &mut Ui, t: &Tokens, text: &str, size: f32) -> Response {
    let mut job = LayoutJob::simple_singleline(text.to_string(), fonts::display(size), t.text_strong);
    job.wrap.max_rows = 1;
    job.wrap.max_width = ui.available_width();
    let g = ui.painter().layout_job(job);
    let h = g.size().y * tk::DISPLAY_LINE_HEIGHT;
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x, h), Sense::hover());
    ui.painter().galley(egui::pos2(rect.left(), rect.bottom() - g.size().y), g, t.text_strong);
    let text = text.to_string();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &text));
    resp
}

/// A body paragraph that wraps (labels clip by default), never justified.
pub fn body(ui: &mut Ui, t: &Tokens, text: &str) -> Response {
    body_colored(ui, text, t.text_strong)
}

pub fn body_muted(ui: &mut Ui, t: &Tokens, text: &str) -> Response {
    body_colored(ui, text, t.text_muted)
}

pub fn body_colored(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let rt = egui::RichText::new(text).font(fonts::text(tk::SIZE_BODY)).color(color);
    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| ui.add(egui::Label::new(rt).wrap())).inner
}

/// Raw text (log lines, crash quotes): JetBrains Mono, an approved exception.
pub fn raw(ui: &mut Ui, text: &str, color: Color32) -> Response {
    ui.add(egui::Label::new(egui::RichText::new(text).font(fonts::raw(tk::SIZE_MONO)).color(color)).truncate())
}

/// Text with tabular figures: egui has no OpenType features, so each digit is centred in a
/// cell as wide as `0` and numbers that change in place don't shimmer. Painted with its left
/// edge at `pos.x` and centred on `pos.y`; returns the rect it took.
pub fn tabular(painter: &Painter, pos: Pos2, text: &str, font: FontId, color: Color32) -> Rect {
    let cell = painter.layout_no_wrap("0".into(), font.clone(), color).size().x;
    let mut x = pos.x;
    let mut height: f32 = 0.0;
    for ch in text.chars() {
        let g = painter.layout_no_wrap(ch.to_string(), font.clone(), color);
        height = height.max(g.size().y);
        let w = if ch.is_ascii_digit() { cell } else { g.size().x };
        let gx = x + (w - g.size().x) / 2.0;
        painter.galley(egui::pos2(gx, pos.y - g.size().y / 2.0), g, color);
        x += w;
    }
    Rect::from_min_max(egui::pos2(pos.x, pos.y - height / 2.0), egui::pos2(x, pos.y + height / 2.0))
}

/// How wide `tabular` lays `text` out.
pub fn tabular_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    let p = ui.painter();
    let cell = p.layout_no_wrap("0".into(), font.clone(), Color32::WHITE).size().x;
    text.chars().map(|ch| if ch.is_ascii_digit() { cell } else { p.layout_no_wrap(ch.to_string(), font.clone(), Color32::WHITE).size().x }).sum()
}

/// A tabular number as a widget.
pub fn figure(ui: &mut Ui, text: &str, font: FontId, color: Color32) -> Response {
    let w = tabular_width(ui, text, font.clone());
    let h = ui.painter().layout_no_wrap("0".into(), font.clone(), color).size().y;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    tabular(ui.painter(), egui::pos2(rect.left(), rect.center().y), text, font, color);
    resp
}

/// Paints a glyph with its drawn shape centred on `center` (icon glyphs overhang their advance box).
pub fn paint_glyph(painter: &Painter, center: Pos2, glyph: &str, font: FontId, color: Color32) -> Rect {
    let g = painter.layout_no_wrap(glyph.to_string(), font, color);
    let drawn = g.rows.first().and_then(|r| r.glyphs.first().map(|x| Rect::from_min_size(r.pos + x.pos.to_vec2() + x.uv_rect.offset, x.uv_rect.size)));
    let mid = drawn.filter(|d| d.width() > 0.0).map_or(g.size() / 2.0, |d| d.center().to_vec2());
    let size = g.size();
    painter.galley(center - mid, g, color);
    Rect::from_center_size(center, size)
}

/// A Phosphor icon: Regular in `text_muted` at rest, accent when `hot`. It supports a label;
/// it never replaces one (except play and stop, which use `icon_label`).
pub fn icon(ui: &mut Ui, t: &Tokens, glyph: &str, size: f32, hot: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let c = motion::color(ui.ctx(), resp.id, hot, t.text_muted, t.accent);
    paint_glyph(ui.painter(), rect.center(), glyph, fonts::icons(size), c);
    resp
}

/// An icon that *is* the label (play, stop): Phosphor Bold in `color`.
pub fn icon_label(painter: &Painter, center: Pos2, glyph: &str, size: f32, color: Color32) -> Rect {
    paint_glyph(painter, center, glyph, fonts::icons_bold(size), color)
}

/// A bottom-bar key hint: the key in `text_strong` (in its own case), the label muted, no boxes.
pub fn key_hint(ui: &mut Ui, t: &Tokens, key: &str, label_text: &str) -> Response {
    // The key keeps its own case: `e` and `E` are different keys.
    let mut job = LayoutJob::default();
    job.append(key, 0.0, TextFormat { font_id: fonts::text(tk::SIZE_CAPTION), color: t.text_strong, extra_letter_spacing: tk::CAPTION_TRACKING, ..Default::default() });
    job.append("  ", 0.0, TextFormat { font_id: fonts::text(tk::SIZE_CAPTION), ..Default::default() });
    job.append(&caps(label_text), 0.0, TextFormat { font_id: fonts::text(tk::SIZE_CAPTION), color: t.text_muted, extra_letter_spacing: tk::CAPTION_TRACKING, ..Default::default() });
    label(ui, job)
}

/// Paints a caption with its left edge at `pos.x`, centred on `pos.y`. Returns its rect.
pub fn paint_caption(painter: &Painter, pos: Pos2, text: &str, color: Color32) -> Rect {
    let g = painter.layout_job(caption_job(text, color));
    let r = Align2::LEFT_CENTER.anchor_size(pos, g.size());
    painter.galley(r.min, g, color);
    r
}

/// Two-digit section numbers: `01`.
pub fn number(n: usize) -> String {
    format!("{n:02}")
}

/// The first line's baseline, from the galley's top.
pub fn baseline(g: &Galley) -> f32 {
    g.rows.first().map_or(g.size().y, |r| r.pos.y + r.row.glyphs.first().map_or(r.row.size.y, |gl| gl.pos.y))
}

/// A size in bytes, as people read it: `46.6 GB`, `812 kB`.
pub fn size_label(n: u64) -> String {
    let n = n as f64;
    match n {
        n if n >= 1e9 => format!("{:.1} GB", n / 1e9),
        n if n >= 1e6 => format!("{:.1} MB", n / 1e6),
        n if n >= 1e3 => format!("{:.0} kB", n / 1e3),
        n => format!("{n} B"),
    }
}

/// How long ago, in one short word: `just now`, `12m ago`, `3d ago`, `never`.
pub fn ago(when: Option<chrono::DateTime<chrono::Utc>>) -> String {
    let Some(w) = when else { return "never".into() };
    let s = (chrono::Utc::now() - w).num_seconds().max(0);
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{}m ago", s / 60),
        3600..=86399 => format!("{}h ago", s / 3600),
        _ => format!("{}d ago", s / 86400),
    }
}
