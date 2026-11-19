//! Slider and meter: an outlined track filled to the value, a ruler of ticks under it (longer
//! at the ends and the middle), and marks for a second value on the same scale (a default:
//! a short muted tick above the track, so it never reads as the slider's own end). A slider's
//! value has a handle: a bar across the track at the fill's end, moving with it.
//!
//! The slider never changes the value by being drawn: a value outside the range is shown
//! pinned to the end but left as it is, and nothing rounds it. Only a press, drag, wheel or
//! arrow key writes, and then it's clamped and stepped.

use egui::{Key, Rect, Response, Sense, Ui, vec2};

use super::{rule, text};
use crate::design::fonts;
use crate::design::motion;
use crate::design::tokens::{self as tk, Tokens};

/// What the meter shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fill {
    /// 0–1.
    Part(f32),
    /// Real activity of unknown length: a block that moves (so it repaints while shown).
    Unknown,
}

/// Paints a meter in `rect` (track at the top, ticks under it) with `fill_color`.
pub fn paint_meter(ui: &Ui, t: &Tokens, rect: Rect, fill: Fill, fill_color: egui::Color32, marks: &[f32]) {
    let p = ui.painter();
    let track = Rect::from_min_size(rect.min, vec2(rect.width(), tk::SLIDER_HEIGHT));
    match fill {
        Fill::Part(f) => {
            let w = (track.width() * f.clamp(0.0, 1.0)).round();
            if w > 0.0 {
                p.rect_filled(Rect::from_min_size(track.min, vec2(w, track.height())), 0.0, fill_color);
            }
        }
        Fill::Unknown => {
            let time = ui.input(|i| i.time) as f32;
            let w = track.width() * 0.25;
            let x = track.left() + (time * 0.8).fract() * (track.width() + w) - w;
            let r = Rect::from_x_y_ranges(x.max(track.left())..=(x + w).min(track.right()), track.y_range());
            p.rect_filled(r, 0.0, fill_color);
            ui.ctx().request_repaint();
        }
    }
    rule::frame(p, track, t.line_strong);
    for m in marks {
        let x = (track.left() + track.width() * m.clamp(0.0, 1.0)).min(track.right() - tk::METER_MARK_WIDTH).round();
        let r = Rect::from_min_size(egui::pos2(x, track.top() - tk::SLIDER_TICK_HEIGHT - tk::UNIT / 2.0), vec2(tk::METER_MARK_WIDTH, tk::SLIDER_TICK_HEIGHT));
        p.rect_filled(r, 0.0, t.text_muted);
    }
    let n = tk::SLIDER_TICKS;
    for i in 0..=n {
        let major = i == 0 || i == n || i * 2 == n;
        let x = (track.left() + track.width() * i as f32 / n as f32).min(track.right() - tk::LINE);
        let h = if major { tk::SLIDER_TICK_HEIGHT * 2.0 } else { tk::SLIDER_TICK_HEIGHT };
        rule::vline(p, x + 0.5, track.bottom()..=track.bottom() + h, if major { t.line_strong } else { t.line });
    }
}

/// The height a meter takes: track plus ruler.
pub const METER_TOTAL_HEIGHT: f32 = tk::SLIDER_HEIGHT + tk::SLIDER_TICK_HEIGHT * 2.0;

/// A read-only meter (progress, disk use), `width` wide.
pub fn meter(ui: &mut Ui, t: &Tokens, fill: Fill, width: f32, marks: &[f32]) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, METER_TOTAL_HEIGHT), Sense::hover());
    paint_meter(ui, t, rect, fill, t.text_strong, marks);
    let v = match fill {
        Fill::Part(f) => f as f64,
        Fill::Unknown => 0.0,
    };
    resp.widget_info(|| egui::WidgetInfo::slider(false, v, "progress"));
    resp
}

pub struct Slider<'a> {
    value: &'a mut f64,
    min: f64,
    max: f64,
    step: f64,
    label: &'a str,
    value_text: Option<String>,
    marks: Vec<f64>,
    hot: bool,
    width: f32,
}

impl<'a> Slider<'a> {
    pub fn new(value: &'a mut f64, min: f64, max: f64) -> Self {
        Self { value, min, max, step: (max - min) / 20.0, label: "", value_text: None, marks: Vec::new(), hot: false, width: 240.0 }
    }

    pub fn step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }

    pub fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// The value as shown above the track's right end (default: the number).
    pub fn value_text(mut self, text: impl Into<String>) -> Self {
        self.value_text = Some(text.into());
        self
    }

    /// Marks for a second value on the same scale (a default).
    pub fn mark(mut self, v: f64) -> Self {
        self.marks.push(v);
        self
    }

    /// Keyboard-current, from the pane's `Nav`.
    pub fn hot(mut self, on: bool) -> Self {
        self.hot = on;
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    fn part(&self, v: f64) -> f32 {
        if self.max <= self.min { 0.0 } else { ((v - self.min) / (self.max - self.min)).clamp(0.0, 1.0) as f32 }
    }

    /// The value a user's move asks for: clamped to the range and stepped from `min`.
    fn from_user(&self, v: f64) -> f64 {
        let v = v.clamp(self.min, self.max);
        if self.step > 0.0 { (self.min + ((v - self.min) / self.step).round() * self.step).clamp(self.min, self.max) } else { v }
    }

    /// `changed()` is set only when the user moved it.
    pub fn show(self, ui: &mut Ui, t: &Tokens) -> Response {
        let header = text::caption_galley(ui, "0", t.text_muted).size().y;
        let h = header + tk::UNIT + METER_TOTAL_HEIGHT;
        let (rect, mut resp) = ui.allocate_exact_size(vec2(self.width, h), Sense::click_and_drag());
        let track = Rect::from_min_size(egui::pos2(rect.left(), rect.top() + header + tk::UNIT), vec2(rect.width(), METER_TOTAL_HEIGHT));
        let mut new = None;
        if let Some(pos) = resp.interact_pointer_pos()
            && (resp.is_pointer_button_down_on() || resp.clicked())
        {
            let f = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
            new = Some(self.from_user(self.min + f * (self.max - self.min)));
            resp.request_focus();
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                new = Some(self.from_user(*self.value + scroll.signum() as f64 * self.step));
            }
        }
        if resp.has_focus() {
            let (dec, inc) = ui.input(|i| (i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::ArrowDown), i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::ArrowUp)));
            if dec {
                new = Some(self.from_user(*self.value - self.step));
            } else if inc {
                new = Some(self.from_user(*self.value + self.step));
            }
            // The arrows move the slider, not focus.
            ui.memory_mut(|m| m.set_focus_lock_filter(resp.id, egui::EventFilter { horizontal_arrows: true, vertical_arrows: true, ..Default::default() }));
        }
        if let Some(v) = new
            && v != *self.value
        {
            *self.value = v;
            resp.mark_changed();
        }
        let hot = resp.hovered() || resp.dragged() || resp.has_focus() || self.hot;
        let ctx = ui.ctx();
        let p = ui.painter();
        let label_c = motion::color(ctx, resp.id.with("label"), hot, t.text_muted, t.accent);
        text::paint_caption(p, egui::pos2(rect.left(), rect.top() + header / 2.0), self.label, label_c);
        let shown = self.value_text.clone().unwrap_or_else(|| format!("{}", *self.value));
        let value_c = motion::color(ctx, resp.id.with("value"), hot, t.text_strong, t.accent);
        // Tabular, so the number doesn't shimmer while it's dragged.
        let font = fonts::text(tk::SIZE_CAPTION);
        let vw = text::tabular_width(ui, &text::caps(&shown), font.clone());
        text::tabular(p, egui::pos2(rect.right() - vw, rect.top() + header / 2.0), &text::caps(&shown), font, value_c);
        let fill_c = motion::color(ctx, resp.id.with("fill"), hot, t.text_strong, t.accent);
        let marks: Vec<f32> = self.marks.iter().map(|m| self.part(*m)).collect();
        paint_meter(ui, t, track, Fill::Part(self.part(*self.value)), fill_c, &marks);
        // The handle: across the track at the value, moving with it.
        let x = (track.left() + track.width() * self.part(*self.value) - tk::METER_MARK_WIDTH / 2.0).clamp(track.left(), track.right() - tk::METER_MARK_WIDTH).round();
        let h = Rect::from_min_size(egui::pos2(x, track.top() - tk::SLIDER_TICK_HEIGHT / 2.0), vec2(tk::METER_MARK_WIDTH, tk::SLIDER_HEIGHT + tk::SLIDER_TICK_HEIGHT));
        ui.painter().rect_filled(h, 0.0, fill_c);
        resp.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
        let (v, label) = (*self.value, self.label.to_string());
        resp.widget_info(|| egui::WidgetInfo::slider(true, v, &label));
        resp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drawing a slider with a value outside its range, or between steps, leaves it alone.
    #[test]
    fn drawing_never_changes_the_value() {
        let ctx = egui::Context::default();
        for start in [5000.0, -3.0, 1.2345] {
            let mut v = start;
            for _ in 0..3 {
                crate::design::test_frame(&ctx, 0.0, |ui| {
                    let t = Tokens::new(false);
                    let r = Slider::new(&mut v, 0.0, 10.0).step(1.0).show(ui, &t);
                    assert!(!r.changed());
                });
            }
            assert_eq!(v, start);
        }
    }

    #[test]
    fn user_moves_are_clamped_and_stepped() {
        let mut v = 0.0;
        let s = Slider::new(&mut v, 1024.0, 32768.0).step(512.0);
        assert_eq!(s.from_user(99999.0), 32768.0);
        assert_eq!(s.from_user(0.0), 1024.0);
        assert_eq!(s.from_user(1300.0), 1536.0);
    }
}
