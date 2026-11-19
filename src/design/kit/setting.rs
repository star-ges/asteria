//! Settings groups and rows (StarShell's `SettingGroup` / `SettingRow`): a group opens with a
//! big number and title; a vertical hairline runs down the left of its rows like a bracket,
//! and the rows' own hairlines start from it, so only the strong rule under the group runs
//! edge to edge (full-width line = new group, inset line = same group). Each row: label left,
//! its control right on a shared column, so every switch and field lines up exactly.

use egui::{Align, Layout, Rect, Sense, Ui, UiBuilder, vec2};

use super::{rule, text};
use crate::design::fonts;
use crate::design::motion;
use crate::design::tokens::{self as tk, Tokens};

/// A numbered group; `rows` adds its `SettingRow`s.
pub fn setting_group<R>(ui: &mut Ui, t: &Tokens, number: usize, title: &str, aside: &str, rows: impl FnOnce(&mut Ui) -> R) -> R {
    let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), tk::SETTING_GROUP_HEADER_HEIGHT), Sense::hover());
    let p = ui.painter();
    let title_g = p.layout_no_wrap(title.to_string(), fonts::heading(tk::SIZE_HEADING), t.text_strong);
    let num = text::caption_galley(ui, &format!("{} /", text::number(number)), t.text_muted);
    let x = head.left() + tk::GUTTER;
    // The number sits on the title's baseline.
    let title_top = head.center().y - title_g.size().y / 2.0;
    let baseline = title_top + text::baseline(&title_g);
    let num_base = text::baseline(&num);
    let title_x = x + num.size().x + tk::GAP;
    p.galley(egui::pos2(x, baseline - num_base), num, t.text_muted);
    p.galley(egui::pos2(title_x, title_top), title_g, t.text_strong);
    let title_right = title_x + p.layout_no_wrap(title.to_string(), fonts::heading(tk::SIZE_HEADING), t.text_strong).size().x;
    let room = head.right() - tk::GUTTER - title_right - tk::GUTTER;
    if !aside.is_empty() && room > tk::GUTTER * 2.0 {
        // The aside gives way to the title: elided to the room left.
        let mut job = text::caption_job(aside, t.text_muted);
        job.wrap.max_width = room;
        job.wrap.max_rows = 1;
        let g = p.layout_job(job);
        p.galley(egui::pos2(head.right() - tk::GUTTER - g.size().x, head.center().y - g.size().y / 2.0), g, t.text_muted);
    }
    let top = ui.cursor().top();
    let out = ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        rows(ui)
    });
    let bottom = ui.cursor().top();
    let p = ui.painter();
    rule::vline(p, head.left() + tk::GUTTER, top..=bottom, t.line);
    ui.spacing_mut().item_spacing.y = 0.0;
    rule::rule(ui, t, true);
    ui.spacing_mut().item_spacing.y = ui.style().spacing.item_spacing.y;
    out.inner
}

pub struct SettingRow<'a> {
    label: &'a str,
    caption: &'a str,
    description: &'a str,
    dense: bool,
    hot: bool,
    unsaved: bool,
}

impl<'a> SettingRow<'a> {
    pub fn new(label: &'a str) -> Self {
        Self { label, caption: "", description: "", dense: false, hot: false, unsaved: false }
    }

    /// An uppercase caption under the label (short facts: a unit, a default).
    pub fn caption(mut self, caption: &'a str) -> Self {
        self.caption = caption;
        self
    }

    /// A muted paragraph under the label (a config comment), wrapping at 560px.
    pub fn description(mut self, description: &'a str) -> Self {
        self.description = description;
        self
    }

    /// Switzer SemiBold 14 labels, for long field lists (configs).
    pub fn dense(mut self) -> Self {
        self.dense = true;
        self
    }

    /// Keyboard-current, from the pane's `Nav`.
    pub fn hot(mut self, on: bool) -> Self {
        self.hot = on;
        self
    }

    /// Edited but not saved: a short accent tick at the row's left edge, on the bracket.
    pub fn unsaved(mut self, on: bool) -> Self {
        self.unsaved = on;
        self
    }

    /// `control` lays out the row's controls, right to left from the shared column's right edge.
    pub fn show<R>(self, ui: &mut Ui, t: &Tokens, control: impl FnOnce(&mut Ui) -> R) -> R {
        let full = ui.available_rect_before_wrap();
        let control_right = full.right() - tk::GUTTER;
        // The shared control column, narrower when the row is (labels keep at least half), or
        // wider when the control needs it: its width is remembered from the last frame (a frame
        // whose width changed is redone before it's shown).
        let row_id = ui.id().with(("setting-row", self.label));
        let column = tk::SETTING_CONTROL_WIDTH.min(((control_right - full.left() - tk::SETTING_INDENT - tk::GUTTER) / 2.0).max(0.0));
        let used = ui.ctx().data(|d| d.get_temp::<f32>(row_id));
        let control_left = control_right - column.max(used.unwrap_or(0.0));
        let label_x = full.left() + tk::SETTING_INDENT;
        let label_w = (control_left - tk::GUTTER - label_x).max(40.0);
        let p = ui.painter();
        let label_font = if self.dense { fonts::text(tk::SIZE_BODY) } else { fonts::heading(tk::SIZE_HEADING_SMALL) };
        // Laid out in the placeholder colour, so painting it can take the hot colour.
        let mut label_job = egui::text::LayoutJob::simple_singleline(self.label.to_string(), label_font, egui::Color32::PLACEHOLDER);
        label_job.wrap.max_width = label_w;
        label_job.wrap.max_rows = 1;
        let label_g = p.layout_job(label_job);
        let under = if !self.description.is_empty() {
            let mut job = egui::text::LayoutJob::simple(self.description.to_string(), fonts::text(tk::SIZE_BODY), t.text_muted, label_w.min(tk::SETTING_DESCRIPTION_WIDTH));
            job.wrap.max_rows = 6;
            Some(p.layout_job(job))
        } else if !self.caption.is_empty() {
            let mut job = text::caption_job(self.caption, t.text_muted);
            job.wrap.max_width = label_w;
            job.wrap.max_rows = 1;
            Some(p.layout_job(job))
        } else {
            None
        };
        let labels_h = label_g.size().y + under.as_ref().map_or(0.0, |g| tk::UNIT / 2.0 + g.size().y);
        let min_h = if self.dense { tk::SETTING_ROW_HEIGHT - tk::GAP * 2.0 } else { tk::SETTING_ROW_HEIGHT };
        let h = min_h.max(labels_h + tk::GAP * 2.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(full.width(), h), Sense::hover());
        let control_rect = Rect::from_min_max(egui::pos2(control_left, rect.top()), egui::pos2(control_right, rect.bottom()));
        let out = ui.scope_builder(UiBuilder::new().max_rect(control_rect).layout(Layout::right_to_left(Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = tk::GAP;
            control(ui)
        });
        let width = out.response.rect.width().ceil();
        if used.is_none_or(|u| (u - width).abs() > 0.5) {
            ui.ctx().data_mut(|d| d.insert_temp(row_id, width));
            ui.ctx().request_discard("a setting row's control changed width");
        }
        let hot = self.hot || resp.hovered() || ui.rect_contains_pointer(rect);
        let p = ui.painter();
        rule::hline(p, (rect.left() + tk::GUTTER)..=rect.right(), rect.top() + 0.5, t.line);
        let label_c = motion::color(ui.ctx(), resp.id.with("label"), hot, t.text_strong, t.accent);
        let top = rect.center().y - labels_h / 2.0;
        p.galley(egui::pos2(label_x, top), label_g.clone(), label_c);
        if let Some(g) = under {
            p.galley(egui::pos2(label_x, top + label_g.size().y + tk::UNIT / 2.0), g, t.text_muted);
        }
        let tick = motion::toggle(ui.ctx(), resp.id.with("unsaved"), self.unsaved, motion::Motion::Fast);
        if tick > 0.0 {
            let x = rect.left() + tk::GUTTER - tk::MARKER / 2.0 + 0.5;
            let r = Rect::from_center_size(egui::pos2(x, rect.center().y), vec2(tk::MARKER, (labels_h).max(tk::GAP * 2.0)));
            p.rect_filled(r, 0.0, tk::alpha_of(t.accent, tick));
        }
        let label = self.label.to_string();
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &label));
        out.inner
    }
}

/// One of a few answers, chosen by clicking the row: laid out like a `SettingRow` (label,
/// description under it, hairline from the bracket), with a square on the control column
/// that fills with the accent when chosen. For quiet choices inside a group (a bisect step's
/// answers), where a hover list would be too loud. `clicked()` means "choose this".
pub fn choice_row(ui: &mut Ui, t: &Tokens, label: &str, description: &str, selected: bool, hot: bool) -> egui::Response {
    let full = ui.available_rect_before_wrap();
    let label_x = full.left() + tk::SETTING_INDENT;
    let mark_x = full.right() - tk::GUTTER - tk::CHECKBOX_SIZE;
    let w = (mark_x - tk::GUTTER - label_x).max(40.0);
    let p = ui.painter();
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_string(), fonts::heading(tk::SIZE_HEADING_SMALL), egui::Color32::PLACEHOLDER);
    job.wrap.max_width = w;
    job.wrap.max_rows = 1;
    let label_g = p.layout_job(job);
    let desc_g = (!description.is_empty()).then(|| {
        let mut job = egui::text::LayoutJob::simple(description.to_string(), fonts::text(tk::SIZE_BODY), t.text_muted, w.min(tk::SETTING_DESCRIPTION_WIDTH));
        job.wrap.max_rows = 3;
        p.layout_job(job)
    });
    let labels_h = label_g.size().y + desc_g.as_ref().map_or(0.0, |g| tk::UNIT / 2.0 + g.size().y);
    let h = (tk::SETTING_ROW_HEIGHT - tk::GAP * 2.0).max(labels_h + tk::GAP * 2.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(full.width(), h), Sense::click());
    let hot = hot || resp.hovered() || resp.has_focus();
    let ctx = ui.ctx();
    let p = ui.painter();
    if selected {
        p.rect_filled(Rect::from_min_max(egui::pos2(rect.left() + tk::GUTTER, rect.top()), rect.max), 0.0, t.wash(0.06));
    }
    rule::hline(p, (rect.left() + tk::GUTTER)..=rect.right(), rect.top() + 0.5, t.line);
    let top = rect.center().y - labels_h / 2.0;
    let label_c = motion::color(ctx, resp.id.with("label"), hot, t.text_strong, t.accent);
    p.galley(egui::pos2(label_x, top), label_g.clone(), label_c);
    if let Some(g) = desc_g {
        p.galley(egui::pos2(label_x, top + label_g.size().y + tk::UNIT / 2.0), g, t.text_muted);
    }
    let sq = Rect::from_min_size(egui::pos2(mark_x, rect.center().y - tk::CHECKBOX_SIZE / 2.0), vec2(tk::CHECKBOX_SIZE, tk::CHECKBOX_SIZE));
    let fill = motion::toggle(ctx, resp.id.with("fill"), selected, motion::Motion::Fast);
    if fill > 0.0 {
        p.rect_filled(sq.shrink(tk::SWITCH_INSET - 1.0), 0.0, tk::alpha_of(t.accent, fill));
    }
    rule::frame(p, sq, motion::color(ctx, resp.id.with("box"), hot || selected, t.line_strong, t.accent));
    let label = label.to_string();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, selected, &label));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
