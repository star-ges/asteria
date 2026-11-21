//! The quiet row, for lists you work through rather than choose from (a set's checklist, the
//! mods a search is to test): body text on a hairline, a checkbox before it or a control at
//! its right end, a caption aside. Hot (hovered, or keyboard-current) turns the title accent and
//! nothing else moves; the keyboard's row has the wash. `attention` puts a small accent square
//! at the row's left edge (the kit's "unread" marker): something on this row wants a look.

use egui::{Align, Color32, Layout, Rect, Response, Sense, Ui, UiBuilder, vec2};

use super::{rule, switch, text};
use crate::design::fonts;
use crate::design::motion;
use crate::design::tokens::{self as tk, Tokens};

pub struct QuietRow<'a> {
    id: egui::Id,
    title: &'a str,
    aside: &'a str,
    aside_color: Option<Color32>,
    check: Option<bool>,
    hot: bool,
    selected: bool,
    attention: bool,
    tall: bool,
}

impl<'a> QuietRow<'a> {
    pub fn new(id: impl std::hash::Hash + std::fmt::Debug, title: &'a str) -> Self {
        Self { id: egui::Id::new(id), title, aside: "", aside_color: None, check: None, hot: false, selected: false, attention: false, tall: false }
    }

    /// A caption before the right end, muted or in a colour (a clarity hue).
    pub fn aside(mut self, aside: &'a str, color: Option<Color32>) -> Self {
        self.aside = aside;
        self.aside_color = color;
        self
    }

    /// A checkbox before the title; a click on the row means "toggle".
    pub fn check(mut self, on: bool) -> Self {
        self.check = Some(on);
        self
    }

    pub fn hot(mut self, on: bool) -> Self {
        self.hot = on;
        self
    }

    /// The keyboard's row: a faint wash.
    pub fn selected(mut self, on: bool) -> Self {
        self.selected = on;
        self
    }

    pub fn attention(mut self, on: bool) -> Self {
        self.attention = on;
        self
    }

    /// Tall enough for a button-high control at the right end.
    pub fn tall(mut self) -> Self {
        self.tall = true;
        self
    }

    /// Draws the row; `control` lays out its right end, right to left (it may add nothing).
    pub fn show<R>(self, ui: &mut Ui, t: &Tokens, control: impl FnOnce(&mut Ui) -> R) -> (Response, R) {
        let h = if self.tall { tk::BUTTON_HEIGHT + tk::GAP } else { tk::LIST_ROW_HEIGHT_QUIET };
        // Checklists move with their own keys (`Nav`), so rows stay out of Tab's order.
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::CLICK);
        let right = rect.right() - tk::GUTTER;
        // The control first, so the text takes the room it leaves.
        let ctrl = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(egui::pos2(rect.left() + tk::GUTTER, rect.top()), egui::pos2(right, rect.bottom()))).layout(Layout::right_to_left(Align::Center)).id_salt(self.id));
        let mut ctrl = ctrl;
        ctrl.spacing_mut().item_spacing.x = tk::GAP;
        let inner = control(&mut ctrl);
        let used = ctrl.min_rect().width();
        let mut end = right - if used > 0.5 { used + tk::GAP } else { 0.0 };
        let hot = self.hot || resp.hovered() || resp.has_focus() || ui.rect_contains_pointer(rect);
        let ctx = ui.ctx().clone();
        let p = ui.painter();
        if self.selected {
            p.rect_filled(rect, 0.0, t.wash(0.06));
        }
        let cy = rect.center().y;
        if self.attention {
            let sq = Rect::from_center_size(egui::pos2(rect.left() + tk::GUTTER / 2.0, cy), vec2(tk::MARK_SQUARE, tk::MARK_SQUARE));
            p.rect_filled(sq, 0.0, t.accent);
        }
        let mut x = rect.left() + tk::GUTTER;
        if let Some(on) = self.check {
            let b = Rect::from_min_size(egui::pos2(x, cy - tk::CHECKBOX_SIZE / 2.0), vec2(tk::CHECKBOX_SIZE, tk::CHECKBOX_SIZE));
            switch::paint_box(ui, t, resp.id, b, on, hot, true);
            x += tk::CHECKBOX_SIZE + tk::GAP;
        }
        let p = ui.painter();
        if !self.aside.is_empty() {
            let room = end - x - tk::LIST_TITLE_MIN;
            if room > tk::GUTTER * 2.0 {
                let mut job = text::caption_job(self.aside, Color32::PLACEHOLDER);
                job.wrap.max_width = room;
                job.wrap.max_rows = 1;
                let g = p.layout_job(job);
                let c = self.aside_color.unwrap_or(t.text_muted);
                p.galley(egui::pos2(end - g.size().x, cy - g.size().y / 2.0), g.clone(), c);
                end -= g.size().x + tk::GAP;
            }
        }
        let title_c = motion::color(&ctx, resp.id.with("title"), hot, t.text_strong, t.accent);
        let mut job = egui::text::LayoutJob::simple_singleline(self.title.to_string(), fonts::text(tk::SIZE_BODY), Color32::PLACEHOLDER);
        job.wrap.max_width = (end - x).max(40.0);
        job.wrap.max_rows = 1;
        let g = p.layout_job(job);
        p.galley(egui::pos2(x, cy - g.size().y / 2.0), g, title_c);
        rule::hline(p, rect.x_range(), rect.bottom() - 0.5, t.line);
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        let (title, check, selected) = (self.title.to_string(), self.check, self.selected);
        resp.widget_info(|| match check {
            Some(on) => egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, &title),
            None => egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &title),
        });
        (resp, inner)
    }
}
