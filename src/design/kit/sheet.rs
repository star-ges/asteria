//! Transient things take over a space that already exists instead of opening windows:
//!
//! - a **sheet** takes over the content area under the bar (its content rises 24px into place);
//! - a **strip** takes over the foot of a pane for a confirmation;
//! - a **notice** takes over the bottom bar's hint cell (info for 5s; errors until dismissed).
//!
//! And the small blocks the plan's new patterns are built from: the bracketed paragraph, the
//! spec strip, empty and loading states.

use egui::{Color32, Rect, Response, Sense, Ui, UiBuilder, vec2};

use super::button::Button;
use super::{rule, text};
use crate::design::fonts;
use crate::design::motion::{self, Motion};
use crate::design::tokens::{self as tk, Tokens};

/// 0 → 1 the first frames a transient thing is shown (a sheet rising, a notice fading in).
/// `id` should be new each time it opens.
pub fn entrance(ui: &Ui, id: egui::Id) -> f32 {
    entrance_with(ui, id, Motion::Medium)
}

/// `entrance` over another motion (a picture crossfading in: `Motion::ImageFade`).
pub fn entrance_with(ui: &Ui, id: egui::Id, motion: Motion) -> f32 {
    let ctx = ui.ctx();
    let seen = id.with("seen");
    if ctx.data(|d| d.get_temp::<bool>(seen)).is_none() {
        ctx.data_mut(|d| d.insert_temp(seen, true));
        motion::animate(ctx, id, 0.0, motion);
    }
    motion::animate(ctx, id, 1.0, motion)
}

/// A sheet over `rect` (the content area): it covers what's under it with the background,
/// shows a numbered caption and a heading (either may be empty), then `body`. Returns what
/// `body` returns.
pub fn sheet<R>(ui: &mut Ui, t: &Tokens, rect: Rect, id: egui::Id, caption: &str, title: &str, body: impl FnOnce(&mut Ui) -> R) -> R {
    let rise = entrance(ui, id);
    // Block the pointer from the panes under it.
    let _ = ui.interact(rect, id.with("cover"), Sense::click_and_drag());
    ui.painter().rect_filled(rect, 0.0, t.background);
    let inner = rect.shrink2(vec2(tk::GUTTER, 0.0)).translate(vec2(0.0, (1.0 - rise) * tk::PANEL_RISE));
    let out = ui.scope_builder(UiBuilder::new().max_rect(inner).id_salt(id), |ui| {
        ui.multiply_opacity(rise);
        ui.add_space(tk::GUTTER);
        if !caption.is_empty() {
            text::caption(ui, t, caption);
            ui.add_space(tk::UNIT);
        }
        if !title.is_empty() {
            text::heading(ui, t, title);
            ui.add_space(tk::GAP);
        }
        // A Ui of its own: a body that sets its height (`set_height`) would otherwise move
        // the cursor back over the caption.
        ui.scope(body).inner
    });
    out.inner
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripAnswer {
    Confirm,
    Cancel,
}

/// A confirmation at the foot of a pane: a caption (what's being done) over a sentence in
/// `text_strong`, wrapping beside the buttons, then the action (filled with the error colour)
/// and cancel. Enter and Esc go through the caller's `modal_nav`, like the buttons.
pub fn strip(ui: &mut Ui, t: &Tokens, id: egui::Id, caption: &str, sentence: &str, action: &str) -> Option<StripAnswer> {
    let rise = entrance(ui, id);
    let width = ui.available_width();
    let buttons = Button::new("cancel").width(ui, t) + Button::new(action).width(ui, t) + tk::GAP * 2.0;
    let text_w = (width - tk::GUTTER * 2.0 - buttons).max(120.0);
    let mut job = egui::text::LayoutJob::simple(sentence.to_string(), fonts::text(tk::SIZE_BODY), t.text_strong, text_w);
    job.wrap.max_rows = 4;
    let g = ui.painter().layout_job(job);
    let cap = (!caption.is_empty()).then(|| text::caption_galley(ui, caption, t.text_muted));
    let cap_h = cap.as_ref().map_or(0.0, |c| c.size().y + tk::UNIT / 2.0);
    let h = (cap_h + g.size().y).max(tk::BUTTON_HEIGHT) + tk::GAP * 2.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
    let mut answer = None;
    let inner = rect.shrink2(vec2(tk::GUTTER, 0.0));
    ui.scope_builder(UiBuilder::new().max_rect(inner).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
        ui.multiply_opacity(rise);
        ui.spacing_mut().item_spacing.x = tk::GAP;
        if Button::new("cancel").show(ui, t).clicked() {
            answer = Some(StripAnswer::Cancel);
        }
        if Button::new(action).danger().show(ui, t).clicked() {
            answer = Some(StripAnswer::Confirm);
        }
    });
    let p = ui.painter();
    let top = rect.center().y - (cap_h + g.size().y) / 2.0;
    let a = |c| tk::alpha_of(c, rise);
    if let Some(c) = cap {
        p.galley(egui::pos2(inner.left(), top), c, a(t.text_muted));
    }
    p.galley(egui::pos2(inner.left(), top + cap_h), g, a(t.text_strong));
    rule::hline(p, rect.x_range(), rect.top() + 0.5, t.line_strong);
    let label = format!("{caption}: {sentence}");
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &label));
    answer
}

/// A state that holds a page for a while (a search in progress), across the page under its
/// head: a caption (in a clarity hue for a status, else muted) over a sentence in `text_strong`,
/// wrapping beside `buttons` at the right, which keep their order. Draws only its foot line, so
/// it shares the line above it. Returns the button clicked.
pub fn banner(ui: &mut Ui, t: &Tokens, caption: &str, caption_color: Option<Color32>, sentence: &str, buttons: &[Button]) -> Option<usize> {
    let width = ui.available_width();
    let buttons_w: f32 = buttons.iter().map(|b| b.width(ui, t) + tk::GAP).sum();
    let text_w = (width - tk::GUTTER * 2.0 - buttons_w - tk::GAP).max(160.0);
    let mut job = egui::text::LayoutJob::simple(sentence.to_string(), fonts::text(tk::SIZE_BODY), Color32::PLACEHOLDER, text_w);
    job.wrap.max_rows = 4;
    let g = ui.painter().layout_job(job);
    let cap_c = caption_color.unwrap_or(t.text_muted);
    let cap = text::caption_galley(ui, caption, Color32::PLACEHOLDER);
    let cap_h = cap.size().y + tk::UNIT / 2.0;
    let h = (cap_h + g.size().y).max(tk::BUTTON_HEIGHT) + tk::GAP * 2.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
    let inner = rect.shrink2(vec2(tk::GUTTER, 0.0));
    let mut clicked = None;
    // A child that doesn't move the cursor: the row is allocated already.
    let mut row = ui.new_child(UiBuilder::new().max_rect(inner).layout(egui::Layout::right_to_left(egui::Align::Center)));
    row.spacing_mut().item_spacing.x = tk::GAP;
    for (i, b) in buttons.iter().enumerate().rev() {
        if b.clone().show(&mut row, t).clicked() {
            clicked = Some(i);
        }
    }
    let p = ui.painter();
    let top = rect.center().y - (cap_h + g.size().y) / 2.0;
    if caption_color.is_some() {
        p.rect_filled(Rect::from_center_size(egui::pos2(inner.left() + tk::MARK_SQUARE / 2.0, top + cap.size().y / 2.0), vec2(tk::MARK_SQUARE, tk::MARK_SQUARE)), 0.0, cap_c);
        p.galley(egui::pos2(inner.left() + tk::MARK_SQUARE + tk::UNIT, top), cap, cap_c);
    } else {
        p.galley(egui::pos2(inner.left(), top), cap, cap_c);
    }
    p.galley(egui::pos2(inner.left(), top + cap_h), g, t.text_strong);
    rule::hline(p, rect.x_range(), rect.bottom() - 0.5, t.line_strong);
    let label = format!("{caption}: {sentence}");
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &label));
    clicked
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Error,
}

impl NoticeKind {
    /// Whether a notice `age` seconds old still holds the bar: info for `NOTICE_SECONDS`,
    /// errors until dismissed.
    pub fn alive(self, age: f32) -> bool {
        self == NoticeKind::Error || age < tk::NOTICE_SECONDS
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoticeOut {
    pub dismissed: bool,
    /// `MORE` was clicked: show the whole message (in a sheet).
    pub more: bool,
}

/// A notice in `rect` (the bottom bar's hint cell): an error is marked by a clay square and an
/// `ERROR` caption and has a dismiss; a long message shows its first line and `MORE`.
pub fn notice(ui: &mut Ui, t: &Tokens, rect: Rect, id: egui::Id, kind: NoticeKind, message: &str) -> NoticeOut {
    let mut out = NoticeOut::default();
    let a = entrance(ui, id);
    let p = ui.painter().clone();
    let cy = rect.center().y;
    let mut x = rect.left() + tk::GUTTER;
    if kind == NoticeKind::Error {
        p.rect_filled(Rect::from_center_size(egui::pos2(x + tk::MARK_SQUARE / 2.0, cy), vec2(tk::MARK_SQUARE, tk::MARK_SQUARE)), 0.0, tk::alpha_of(t.error(), a));
        x += tk::MARK_SQUARE + tk::UNIT;
        x = text::paint_caption(&p, egui::pos2(x, cy), "error", tk::alpha_of(t.error(), a)).right() + tk::GAP;
    }
    let first = message.lines().next().unwrap_or_default();
    let long = message.lines().count() > 1 || first.chars().count() > 160;
    // Right end: dismiss (errors), then MORE.
    let mut right = rect.right() - tk::GUTTER;
    if kind == NoticeKind::Error {
        let g = text::caption_galley(ui, "dismiss", t.text_muted);
        let r = Rect::from_min_size(egui::pos2(right - g.size().x, cy - g.size().y / 2.0), g.size()).expand(4.0);
        let resp = ui.interact(r, id.with("dismiss"), Sense::click());
        let c = motion::color(ui.ctx(), resp.id, resp.hovered(), t.text_muted, t.accent);
        p.galley(egui::pos2(right - g.size().x, cy - g.size().y / 2.0), text::caption_galley(ui, "dismiss", c), c);
        out.dismissed = resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
        right -= g.size().x + tk::GAP * 2.0;
    }
    if long {
        let g = text::caption_galley(ui, "more", t.text_strong);
        let r = Rect::from_min_size(egui::pos2(right - g.size().x, cy - g.size().y / 2.0), g.size()).expand(4.0);
        let resp = ui.interact(r, id.with("more"), Sense::click());
        let c = motion::color(ui.ctx(), resp.id, resp.hovered(), t.text_strong, t.accent);
        p.galley(egui::pos2(right - g.size().x, cy - g.size().y / 2.0), text::caption_galley(ui, "more", c), c);
        out.more = resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
        right -= g.size().x + tk::GAP * 2.0;
    }
    let mut job = egui::text::LayoutJob::simple_singleline(first.to_string(), fonts::text(tk::SIZE_BODY), tk::alpha_of(t.text_strong, a));
    job.wrap.max_width = (right - x).max(20.0);
    job.wrap.max_rows = 1;
    let g = p.layout_job(job);
    p.galley(egui::pos2(x, cy - g.size().y / 2.0), g, t.text_strong);
    out
}

/// A paragraph set off by a hairline at its left, under a caption: warnings that matter
/// ("before you start") without a colour or a box.
pub fn bracketed<R>(ui: &mut Ui, t: &Tokens, caption: &str, body: impl FnOnce(&mut Ui) -> R) -> R {
    let top = ui.cursor().top();
    let out = ui.horizontal_top(|ui| {
        ui.add_space(tk::GAP + tk::LINE);
        ui.vertical(|ui| {
            text::caption_colored(ui, caption, t.text_strong);
            ui.add_space(tk::UNIT / 2.0);
            body(ui)
        })
        .inner
    });
    let left = out.response.rect.left();
    rule::vline(ui.painter(), left, top..=out.response.rect.bottom(), t.line_strong);
    out.inner
}

/// One cell of a spec strip: a caption over a value. `link` cells lead somewhere (accent and
/// the marker when hot); the others don't react.
pub struct Spec<'a> {
    pub caption: &'a str,
    pub value: &'a str,
    pub link: bool,
    /// A clarity hue for the value (a group's colour), else `text_strong`.
    pub hue: Option<Color32>,
}

impl<'a> Spec<'a> {
    pub fn new(caption: &'a str, value: &'a str) -> Self {
        Self { caption, value, link: false, hue: None }
    }

    pub fn link(mut self) -> Self {
        self.link = true;
        self
    }

    pub fn hue(mut self, hue: Option<Color32>) -> Self {
        self.hue = hue;
        self
    }
}

/// A row of hairline cells (the strong-lines grid): each a caption over a value, sharing
/// their lines. Returns the link cell clicked.
pub fn spec_strip(ui: &mut Ui, t: &Tokens, cells: &[Spec]) -> Option<usize> {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), tk::SPEC_CELL_HEIGHT), Sense::hover());
    let n = cells.len().max(1);
    let w = (rect.width() / n as f32).floor();
    let mut clicked = None;
    let p = ui.painter();
    for (i, c) in cells.iter().enumerate() {
        let r = Rect::from_min_size(rect.min + vec2(i as f32 * w, 0.0), vec2(if i + 1 == n { rect.width() - w * i as f32 } else { w }, rect.height()));
        let resp = ui.interact(r, ui.id().with(("spec", i)), if c.link { Sense::click() } else { Sense::hover() });
        let hot = c.link && resp.hovered();
        let cap_c = motion::color(ui.ctx(), resp.id.with("cap"), hot, t.text_muted, t.accent);
        let val_c = motion::color(ui.ctx(), resp.id.with("val"), hot, c.hue.unwrap_or(t.text_strong), t.accent);
        // The first cell's text sits on the page's gutter, so it lines up with what's above.
        let inset = if i == 0 { tk::GUTTER } else { tk::GAP };
        let x = r.left() + inset;
        let cap = text::caption_galley(ui, c.caption, cap_c);
        let mut job = egui::text::LayoutJob::simple_singleline(c.value.to_string(), fonts::text(tk::SIZE_BODY), val_c);
        job.wrap.max_width = r.width() - inset - tk::GAP;
        job.wrap.max_rows = 1;
        let val = p.layout_job(job);
        let total = cap.size().y + tk::UNIT / 2.0 + val.size().y;
        let top = r.center().y - total / 2.0;
        p.galley(egui::pos2(x, top), cap.clone(), cap_c);
        p.galley(egui::pos2(x, top + cap.size().y + tk::UNIT / 2.0), val, val_c);
        if i > 0 {
            rule::vline(p, r.left(), r.y_range(), t.line_strong);
        }
        if c.link {
            let m = motion::toggle(ui.ctx(), resp.id.with("marker"), hot, Motion::Fast);
            if m > 0.0 {
                p.rect_filled(Rect::from_min_max(egui::pos2(r.left(), r.bottom() - tk::MARKER), egui::pos2(r.right(), r.bottom())), 0.0, tk::alpha_of(t.accent, m));
            }
            if resp.clone().on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                clicked = Some(i);
            }
        }
        let label = format!("{}: {}", c.caption, c.value);
        let link = c.link;
        resp.widget_info(|| egui::WidgetInfo::labeled(if link { egui::WidgetType::Button } else { egui::WidgetType::Label }, true, &label));
    }
    rule::hline(p, rect.x_range(), rect.top() + 0.5, t.line_strong);
    rule::hline(p, rect.x_range(), rect.bottom() - 0.5, t.line_strong);
    clicked
}

/// An empty state, left-aligned to the grid: the title large, the hint as body, an action
/// as an outlined button. Returns the action's response.
pub fn empty_state(ui: &mut Ui, t: &Tokens, title: &str, hint: &str, action: Option<&str>) -> Option<Response> {
    ui.add_space(tk::GUTTER);
    text::heading(ui, t, title);
    if !hint.is_empty() {
        ui.add_space(tk::UNIT);
        ui.scope(|ui| {
            ui.set_max_width(tk::SETTING_DESCRIPTION_WIDTH);
            text::body_muted(ui, t, hint);
        });
    }
    action.map(|a| {
        ui.add_space(tk::GAP);
        Button::new(a).show(ui, t)
    })
}

/// Nothing to show yet: a muted caption in the place the list will occupy. No spinner.
pub fn loading(ui: &mut Ui, t: &Tokens, what: &str) -> Response {
    ui.add_space(tk::GUTTER);
    text::caption(ui, t, what)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Old dialogs size themselves (`set_height`) inside a sheet; egui moves a Ui's cursor back
    /// to its top when its height is set, so the body must have a Ui of its own or it's drawn
    /// over the sheet's caption.
    #[test]
    fn a_sheet_body_that_sets_its_height_stays_under_the_caption() {
        let ctx = egui::Context::default();
        for _ in 0..2 {
            crate::design::test_frame(&ctx, 0.0, |ui| {
                let t = Tokens::new(false);
                let rect = Rect::from_min_size(egui::pos2(0.0, 100.0), vec2(800.0, 600.0));
                let (before, after) = sheet(ui, &t, rect, egui::Id::new("test-sheet"), "01 / instances · edit", "", |ui| {
                    let before = ui.cursor().top();
                    ui.set_height(300.0);
                    (before, ui.cursor().top())
                });
                // The body goes on where the caption left off, never back up over it.
                assert!(after >= before, "the body's cursor moved up from {before} to {after}");
            });
        }
    }
}
