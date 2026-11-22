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
    SettingGroup::new(number, title).aside(aside).show(ui, t, rows).1.expect("a group that doesn't fold is always open")
}

/// A settings group: the head (a number on the title's baseline, the title, an aside at the
/// right), then its rows on the bracket. A foldable group's head is clickable (its title turns
/// accent when hot, like any row you reach for) and, folded, it's the head and its rule alone.
pub struct SettingGroup<'a> {
    number: usize,
    title: &'a str,
    aside: &'a str,
    /// `Some(open)`: the head folds and opens the group.
    fold: Option<bool>,
    hot: bool,
    id: Option<egui::Id>,
}

impl<'a> SettingGroup<'a> {
    pub fn new(number: usize, title: &'a str) -> Self {
        Self { number, title, aside: "", fold: None, hot: false, id: None }
    }

    pub fn aside(mut self, aside: &'a str) -> Self {
        self.aside = aside;
        self
    }

    /// The head folds the group; `open` is whether it's open now.
    pub fn foldable(mut self, open: bool) -> Self {
        self.fold = Some(open);
        self
    }

    /// Keyboard-current, from the pane's `Nav`.
    pub fn hot(mut self, on: bool) -> Self {
        self.hot = on;
        self
    }

    /// The head's id, where titles may repeat.
    pub fn id_salt(mut self, id: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id = Some(egui::Id::new(id));
        self
    }

    /// The head's response (`clicked()` on a foldable one means "fold or open") and what `rows`
    /// returns, if the group is open.
    pub fn show<R>(self, ui: &mut Ui, t: &Tokens, rows: impl FnOnce(&mut Ui) -> R) -> (egui::Response, Option<R>) {
        let sense = if self.fold.is_some() { Sense::click() } else { Sense::hover() };
        // With an id of its own, the head's one interactive widget is the one made from that id.
        let (head, resp) = ui.allocate_exact_size(vec2(ui.available_width(), tk::SETTING_GROUP_HEADER_HEIGHT), if self.id.is_some() { Sense::hover() } else { sense });
        let resp = match self.id {
            Some(id) => ui.interact(head, ui.id().with(id), sense),
            None => resp,
        };
        let hot = self.fold.is_some() && (self.hot || resp.hovered() || resp.has_focus());
        let x = head.left() + tk::GUTTER;
        let num = text::caption_galley(ui, &format!("{} /", text::number(self.number)), t.text_muted);
        let title_x = x + num.size().x + tk::GAP;
        // The aside gives way to the title, the title to the head's width: both elided.
        let p = ui.painter();
        let aside_g = (!self.aside.is_empty()).then(|| {
            let mut job = text::caption_job(self.aside, t.text_muted);
            job.wrap.max_width = ((head.right() - tk::GUTTER - title_x) * 0.45).max(tk::GUTTER * 2.0);
            job.wrap.max_rows = 1;
            p.layout_job(job)
        });
        let aside_w = aside_g.as_ref().map_or(0.0, |g| g.size().x + tk::GUTTER);
        let mut job = egui::text::LayoutJob::simple_singleline(self.title.to_string(), fonts::heading(tk::SIZE_HEADING), egui::Color32::PLACEHOLDER);
        job.wrap.max_width = (head.right() - tk::GUTTER - aside_w - title_x).max(40.0);
        job.wrap.max_rows = 1;
        let title_g = p.layout_job(job);
        // The number sits on the title's baseline.
        let title_top = head.center().y - title_g.size().y / 2.0;
        let baseline = title_top + text::baseline(&title_g);
        let ctx = ui.ctx();
        let num_c = motion::color(ctx, resp.id.with("number"), hot, t.text_muted, t.accent);
        let title_c = motion::color(ctx, resp.id.with("title"), hot, t.text_strong, t.accent);
        p.galley(egui::pos2(x, baseline - text::baseline(&num)), num, num_c);
        p.galley(egui::pos2(title_x, title_top), title_g, title_c);
        if let Some(g) = aside_g {
            p.galley(egui::pos2(head.right() - tk::GUTTER - g.size().x, head.center().y - g.size().y / 2.0), g, t.text_muted);
        }
        if self.fold.is_some() {
            resp.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
        }
        let (title, fold) = (self.title.to_string(), self.fold);
        resp.widget_info(|| match fold {
            Some(open) => egui::WidgetInfo::selected(egui::WidgetType::CollapsingHeader, true, open, &title),
            None => egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &title),
        });
        let spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 0.0;
        let out = if self.fold == Some(false) {
            None
        } else {
            let top = ui.cursor().top();
            let out = ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                rows(ui)
            });
            let bottom = ui.cursor().top();
            rule::vline(ui.painter(), head.left() + tk::GUTTER, top..=bottom, t.line);
            Some(out.inner)
        };
        rule::rule(ui, t, true);
        ui.spacing_mut().item_spacing.y = spacing;
        (resp, out)
    }
}

/// Several controls `width` wide in a row's right-to-left control column, laid out left to right
/// inside, so Tab visits them in reading order (a field before its menu, width before height).
pub fn in_order<R>(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.allocate_ui_with_layout(vec2(width, ui.available_height()), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = tk::GAP;
        add(ui)
    })
    .inner
}

/// What a [`SettingRow`] reports besides its control's result.
pub struct RowOut {
    /// The whole row, by hover (`hovered()`), and whether it was pressed anywhere (also on its control).
    pub response: egui::Response,
    pub pressed: bool,
    /// The facts line's link was clicked.
    pub action: bool,
}

pub struct SettingRow<'a> {
    label: &'a str,
    caption: &'a str,
    description: &'a str,
    facts: &'a str,
    facts_color: Option<egui::Color32>,
    action: &'a str,
    dense: bool,
    hot: bool,
    unsaved: bool,
    id: Option<egui::Id>,
}

impl<'a> SettingRow<'a> {
    pub fn new(label: &'a str) -> Self {
        Self { label, caption: "", description: "", facts: "", facts_color: None, action: "", dense: false, hot: false, unsaved: false, id: None }
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

    /// A caption line under the description (a range, a default), muted or in a clarity hue.
    pub fn facts(mut self, facts: &'a str, color: Option<egui::Color32>) -> Self {
        self.facts = facts;
        self.facts_color = color;
        self
    }

    /// A caption link at the end of the facts line ("use default"); [`RowOut::action`] says it was clicked.
    pub fn action(mut self, action: &'a str) -> Self {
        self.action = action;
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

    /// The row's id, where labels may repeat (keys in different sections).
    pub fn id_salt(mut self, id: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id = Some(egui::Id::new(id));
        self
    }

    /// `control` lays out the row's controls, right to left from the shared column's right edge.
    /// Several controls that Tab should visit in reading order go in [`in_order`].
    pub fn show<R>(self, ui: &mut Ui, t: &Tokens, control: impl FnOnce(&mut Ui) -> R) -> R {
        self.show_row(ui, t, control).1
    }

    /// Like [`show`](Self::show), with the row's own response and its link.
    pub fn show_row<R>(self, ui: &mut Ui, t: &Tokens, control: impl FnOnce(&mut Ui) -> R) -> (RowOut, R) {
        let full = ui.available_rect_before_wrap();
        let control_right = full.right() - tk::GUTTER;
        // The shared control column, narrower when the row is (labels keep at least half), or
        // wider when the control needs it; a control taller than the labels makes the row
        // taller. Its size is remembered from the last frame (a frame whose size changed is
        // redone before it's shown).
        let row_id = ui.id().with(("setting-row", self.id.unwrap_or_else(|| egui::Id::new(self.label))));
        let column = tk::SETTING_CONTROL_WIDTH.min(((control_right - full.left() - tk::SETTING_INDENT - tk::GUTTER) / 2.0).max(0.0));
        let used = ui.ctx().data(|d| d.get_temp::<egui::Vec2>(row_id));
        let control_left = control_right - column.max(used.map_or(0.0, |u| u.x));
        let label_x = full.left() + tk::SETTING_INDENT;
        let label_font = if self.dense { fonts::text(tk::SIZE_BODY) } else { fonts::heading(tk::SIZE_HEADING_SMALL) };
        // A row too narrow for its label beside the control puts the control under it (dense
        // rows, the configs editor's many, keep to one line and truncate).
        let label_natural = ui.painter().layout_no_wrap(self.label.to_string(), label_font.clone(), egui::Color32::PLACEHOLDER).size().x;
        let stacked = !self.dense && label_natural > control_left - tk::GUTTER - label_x;
        let control_left = if stacked { label_x } else { control_left };
        let label_w = if stacked { control_right - label_x } else { (control_left - tk::GUTTER - label_x).max(40.0) };
        let p = ui.painter();
        // Laid out in the placeholder colour, so painting it can take the hot colour.
        let mut label_job = egui::text::LayoutJob::simple_singleline(self.label.to_string(), label_font, egui::Color32::PLACEHOLDER);
        label_job.wrap.max_width = label_w;
        label_job.wrap.max_rows = 1;
        let label_g = p.layout_job(label_job);
        let mut under = Vec::new();
        if !self.description.is_empty() {
            let mut job = egui::text::LayoutJob::simple(self.description.to_string(), fonts::text(tk::SIZE_BODY), t.text_muted, label_w.min(tk::SETTING_DESCRIPTION_WIDTH));
            job.wrap.max_rows = 6;
            under.push(p.layout_job(job));
        } else if !self.caption.is_empty() {
            let mut job = text::caption_job(self.caption, t.text_muted);
            job.wrap.max_width = label_w;
            job.wrap.max_rows = 1;
            under.push(p.layout_job(job));
        }
        let action_g = (!self.action.is_empty()).then(|| text::caption_galley(ui, self.action, egui::Color32::PLACEHOLDER));
        let action_w = action_g.as_ref().map_or(0.0, |g| g.size().x + tk::GAP);
        let facts_g = (!self.facts.is_empty()).then(|| {
            let mut job = text::caption_job(self.facts, egui::Color32::PLACEHOLDER);
            job.wrap.max_width = (label_w - action_w).max(40.0);
            job.wrap.max_rows = 1;
            p.layout_job(job)
        });
        let facts_h = facts_g.as_ref().map(|g| g.size().y).or(action_g.as_ref().map(|g| g.size().y));
        let under_h: f32 = under.iter().map(|g| tk::UNIT / 2.0 + g.size().y).sum::<f32>() + facts_h.map_or(0.0, |h| tk::UNIT / 2.0 + h);
        let labels_h = label_g.size().y + under_h;
        let min_h = if self.dense { tk::SETTING_ROW_HEIGHT - tk::GAP * 2.0 } else { tk::SETTING_ROW_HEIGHT };
        let control_h = used.map_or(0.0, |u| u.y);
        let h = if stacked {
            labels_h + tk::UNIT + control_h + tk::GAP * 2.0
        } else {
            min_h.max(labels_h + tk::GAP * 2.0).max(control_h + tk::GAP * 2.0)
        };
        let (rect, resp) = ui.allocate_exact_size(vec2(full.width(), h), Sense::hover());
        let resp = match self.id {
            Some(_) => ui.interact(rect, row_id.with("row"), Sense::hover()),
            None => resp,
        };
        let control_top = if stacked { rect.top() + tk::GAP + labels_h + tk::UNIT } else { rect.top() + tk::GAP };
        let control_rect = Rect::from_min_max(egui::pos2(control_left, control_top), egui::pos2(control_right, rect.bottom() - tk::GAP));
        let mut child = ui.new_child(UiBuilder::new().max_rect(control_rect).layout(Layout::right_to_left(Align::Center)).id_salt(row_id.with("control")));
        child.spacing_mut().item_spacing.x = tk::GAP;
        let out = control(&mut child);
        let size = child.min_rect().size().ceil();
        if used.is_none_or(|u| (u.x - size.x).abs() > 0.5 || (u.y - size.y).abs() > 0.5) {
            ui.ctx().data_mut(|d| d.insert_temp(row_id, size));
            ui.ctx().request_discard("a setting row's control changed size");
        }
        let hot = self.hot || ui.rect_contains_pointer(rect);
        let pressed = ui.rect_contains_pointer(rect) && ui.input(|i| i.pointer.primary_pressed());
        let p = ui.painter();
        rule::hline(p, (rect.left() + tk::GUTTER)..=rect.right(), rect.top() + 0.5, t.line);
        let label_c = motion::color(ui.ctx(), row_id.with("label"), hot, t.text_strong, t.accent);
        let top = rect.top() + tk::GAP.max((h - labels_h) / 2.0).min(h / 2.0);
        let top = if stacked || control_h > labels_h { rect.top() + tk::GAP } else { top };
        p.galley(egui::pos2(label_x, top), label_g.clone(), label_c);
        let mut y = top + label_g.size().y;
        for g in under {
            y += tk::UNIT / 2.0;
            let gh = g.size().y;
            p.galley(egui::pos2(label_x, y), g, t.text_muted);
            y += gh;
        }
        let mut action = false;
        if let Some(fh) = facts_h {
            y += tk::UNIT / 2.0;
            let mut x = label_x;
            if let Some(g) = facts_g {
                x += g.size().x + tk::GAP;
                p.galley(egui::pos2(label_x, y), g, self.facts_color.unwrap_or(t.text_muted));
            }
            if let Some(g) = action_g {
                let r = Rect::from_min_size(egui::pos2(x, y), vec2(g.size().x, fh));
                let a = ui.interact(r.expand(tk::UNIT / 2.0), row_id.with("action"), Sense::click());
                let c = motion::color(ui.ctx(), a.id, a.hovered() || a.has_focus(), t.text_strong, t.accent);
                ui.painter().galley(r.min, g, c);
                action = a.clicked();
                let label = self.action.to_string();
                a.on_hover_cursor(egui::CursorIcon::PointingHand).widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, true, &label));
            }
        }
        let tick = motion::toggle(ui.ctx(), row_id.with("unsaved"), self.unsaved, motion::Motion::Fast);
        if tick > 0.0 {
            let x = rect.left() + tk::GUTTER - tk::MARKER / 2.0 + 0.5;
            let r = Rect::from_min_size(egui::pos2(x - tk::MARKER / 2.0, top), vec2(tk::MARKER, labels_h.max(tk::GAP * 2.0)));
            ui.painter().rect_filled(r, 0.0, tk::alpha_of(t.accent, tick));
        }
        let label = self.label.to_string();
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &label));
        (RowOut { response: resp, pressed, action }, out)
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
