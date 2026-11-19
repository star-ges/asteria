//! Text fields: a 1px frame (accent while focused or hovered), no well, a 2px accent caret,
//! an optional caption inside on the left. And the large query line a filter or search
//! takes over the head of a list with.

use egui::{Color32, Margin, Response, RichText, Stroke, TextEdit, Ui, vec2};

use super::{rule, text};
use crate::design::fonts;
use crate::design::motion;
use crate::design::tokens::{self as tk, Tokens};

/// The caret and selection the kit's text edits use (a scoped style; the old screens keep theirs).
fn edit_style(ui: &mut Ui, t: &Tokens) {
    let v = ui.visuals_mut();
    v.text_cursor.stroke = Stroke::new(tk::CARET_WIDTH, t.accent);
    v.selection.bg_fill = t.wash(0.16);
    v.selection.stroke = Stroke::new(1.0, t.text_strong);
    v.extreme_bg_color = Color32::TRANSPARENT;
    v.text_edit_bg_color = Some(Color32::TRANSPARENT);
}

pub struct Field<'a> {
    value: &'a mut String,
    label: &'a str,
    hint: &'a str,
    width: Option<f32>,
    password: bool,
    focus: bool,
}

impl<'a> Field<'a> {
    pub fn new(value: &'a mut String) -> Self {
        Self { value, label: "", hint: "", width: None, password: false, focus: false }
    }

    /// A caption inside the frame, before the text.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = hint;
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    pub fn password(mut self, on: bool) -> Self {
        self.password = on;
        self
    }

    /// Take keyboard focus this frame.
    pub fn focus(mut self, on: bool) -> Self {
        self.focus = on;
        self
    }

    pub fn show(self, ui: &mut Ui, t: &Tokens) -> Response {
        let width = self.width.unwrap_or_else(|| ui.available_width());
        let (rect, _) = ui.allocate_exact_size(vec2(width, tk::FIELD_HEIGHT), egui::Sense::hover());
        let label_w = if self.label.is_empty() { 0.0 } else { text::caption_galley(ui, self.label, t.text_muted).size().x + tk::GAP };
        let inner = egui::Rect::from_min_max(egui::pos2(rect.left() + tk::GAP + label_w, rect.top()), egui::pos2(rect.right() - tk::GAP, rect.bottom()));
        // A child that doesn't move the cursor: the frame's room is allocated already.
        let resp = ui
            .new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)))
            .scope(|ui| {
                edit_style(ui, t);
                let hint = RichText::new(self.hint).color(t.text_muted).font(fonts::text(tk::SIZE_BODY));
                ui.add(
                    TextEdit::singleline(self.value)
                        .frame(egui::Frame::NONE)
                        .margin(Margin::ZERO)
                        .font(fonts::text(tk::SIZE_BODY))
                        .text_color(t.text_strong)
                        .hint_text(hint)
                        .password(self.password)
                        .desired_width(inner.width())
                        .vertical_align(egui::Align::Center),
                )
            })
            .inner;
        if self.focus {
            resp.request_focus();
        }
        let hot = resp.has_focus() || ui.rect_contains_pointer(rect);
        let p = ui.painter();
        rule::frame(p, rect, motion::color(ui.ctx(), resp.id.with("frame"), hot, t.line_strong, t.accent));
        if !self.label.is_empty() {
            let c = motion::color(ui.ctx(), resp.id.with("label"), resp.has_focus(), t.text_muted, t.accent);
            text::paint_caption(p, egui::pos2(rect.left() + tk::GAP, rect.center().y), self.label, c);
        }
        resp
    }
}

/// A notes area: a 1px frame, no well, Switzer 16 at a reading line height, `rows` tall.
pub fn notes(ui: &mut Ui, t: &Tokens, value: &mut String, hint: &str, rows: usize) -> Response {
    notes_sized(ui, t, value, hint, rows, 0.0)
}

/// A notes area at least `min_height` tall (one that fills its column down to the page's foot).
pub fn notes_sized(ui: &mut Ui, t: &Tokens, value: &mut String, hint: &str, rows: usize, min_height: f32) -> Response {
    let size = tk::SIZE_READING;
    let line = size * tk::READING_LINE_HEIGHT;
    let color = t.text_strong;
    let mut layouter = move |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
        let mut job = egui::text::LayoutJob::default();
        job.append(buf.as_str(), 0.0, egui::TextFormat { font_id: fonts::text(size), color, line_height: Some(line), ..Default::default() });
        job.wrap.max_width = wrap;
        ui.fonts_mut(|f| f.layout_job(job))
    };
    let resp = ui
        .scope(|ui| {
            edit_style(ui, t);
            ui.add(
                TextEdit::multiline(value)
                    // A custom frame carries the margin (TextEdit then ignores `margin` for drawing).
                    .frame(egui::Frame::NONE.inner_margin(Margin::same(tk::GAP as i8)))
                    .margin(Margin::same(tk::GAP as i8))
                    .hint_text(RichText::new(hint).font(fonts::text(size)).color(t.text_muted))
                    .desired_width(ui.available_width())
                    .desired_rows(rows)
                    .min_size(vec2(0.0, min_height))
                    .layouter(&mut layouter),
            )
        })
        .inner;
    let hot = resp.has_focus() || resp.hovered();
    rule::frame(ui.painter(), resp.rect, motion::color(ui.ctx(), resp.id.with("frame"), hot, t.line_strong, t.accent));
    resp
}

/// The large query line (Switzer ExtraBold at `size`, 44 for a list filter, 28 for a search
/// pane), with a hairline under it that turns accent while focused.
pub fn query(ui: &mut Ui, t: &Tokens, value: &mut String, hint: &str, size: f32, focus: bool) -> Response {
    let resp = ui
        .scope(|ui| {
            edit_style(ui, t);
            let font = fonts::heading(size);
            ui.add(
                TextEdit::singleline(value)
                    .frame(egui::Frame::NONE.inner_margin(Margin::symmetric(0, tk::UNIT as i8)))
                    .margin(Margin::symmetric(0, tk::UNIT as i8))
                    .font(font.clone())
                    .text_color(t.text_strong)
                    .hint_text(RichText::new(hint).font(font).color(t.text_muted))
                    .desired_width(ui.available_width()),
            )
        })
        .inner;
    if focus {
        resp.request_focus();
    }
    let c = motion::color(ui.ctx(), resp.id.with("rule"), resp.has_focus(), t.line_strong, t.accent);
    rule::hline(ui.painter(), resp.rect.x_range(), resp.rect.bottom(), c);
    resp
}
