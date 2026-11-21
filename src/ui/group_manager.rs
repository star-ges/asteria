//! Manage groups (g): create, rename, colour, reorder, delete, and choose which instances
//! belong to each. The groups as a compact hover list (each with its clarity hue's tick) beside
//! the selected one: its settings, then its instances as a checklist.

use egui::{Align, Layout, Sense, Ui, vec2};

use super::content::{padded, pane_head, sheet_foot, sheet_panes};
use super::nav::Nav;
use super::{App, ToastKind};
use crate::design::glyphs;
use crate::design::kit::{self, Button, HoverList, HoverRow, QuietRow, SettingRow, Variant};
use crate::design::motion;
use crate::design::tokens::{self as tk, Tokens};
use crate::groups::{COLORS, Group};
use crate::keymap::Action;

#[derive(Default)]
pub struct GroupManager {
    selected: Option<String>,
    new_name: String,
    rename: String,
    focus_new: bool,
    focus_rename: bool,
    /// Keyboard is in the member list (else the group list).
    in_members: bool,
    member: usize,
}

impl GroupManager {
    pub fn new(selected: Option<String>) -> Self {
        let rename = selected.clone().unwrap_or_default();
        Self { focus_new: selected.is_none(), selected, rename, new_name: String::new(), ..Default::default() }
    }

    fn select(&mut self, name: Option<String>) {
        self.rename = name.clone().unwrap_or_default();
        self.selected = name;
    }
}

impl App {
    fn save_groups(&mut self) {
        if let Err(e) = self.groups.save() {
            self.toast(ToastKind::Error, format!("Couldn't save groups: {e}"));
        }
    }

    /// Point every instance in `from` at `to` ("" for no group).
    fn regroup(&mut self, from: &str, to: &str) {
        for i in self.instances.iter_mut().filter(|i| i.group == from) {
            i.group = to.to_string();
            let _ = i.save();
        }
    }
}

/// Returns true when the dialog should close.
pub fn show(app: &mut App, s: &mut GroupManager, ui: &mut Ui) -> bool {
    let t = app.look.tokens.clone();
    if s.selected.as_ref().is_some_and(|n| app.groups.get(n).is_none()) {
        s.select(None);
    }
    ui.spacing_mut().item_spacing.y = 0.0;
    kit::heading(ui, &t, "Groups");
    ui.add_space(tk::UNIT);
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(tk::SETTING_DESCRIPTION_WIDTH));
        kit::body_muted(ui, &t, "Groups order the instance list. A group's colour marks its label and its instances.");
    });
    ui.add_space(tk::GUTTER);

    // Keyboard: j/k groups, l into members (space toggles), h back,
    // a new, r rename, c colour, J/K reorder, x delete.
    let kb = app.modal_nav();
    match kb {
        Some(Nav::Right) if s.selected.is_some() => s.in_members = true,
        Some(Nav::Left) => s.in_members = false,
        Some(Nav::Add) => s.focus_new = true,
        Some(Nav::Rename) => s.focus_rename = true,
        _ => {}
    }
    let moved = kb.filter(|n| n.is_move());
    if let Some(n) = moved {
        if s.in_members {
            s.member = n.step(app.instances.len(), Some(s.member)).unwrap_or(0);
        } else {
            let names = app.groups.names();
            let pos = s.selected.as_ref().and_then(|g| names.iter().position(|x| x == g));
            if let Some(p) = n.step(names.len(), pos) {
                s.select(Some(names[p].clone()));
            }
        }
    }
    let in_members = s.in_members;
    let foot = tk::LINE + tk::GAP + tk::BUTTON_HEIGHT + tk::GUTTER;
    let (mut left, mut right) = sheet_panes(ui, &t, 0.4, tk::LIST_RAIL_MIN, foot);

    // -- left: the groups, and a new one
    {
    let ui = &mut left;
    ui.spacing_mut().item_spacing.y = 0.0;
    let list = app.groups.list.clone();
    pane_head(ui, &t, 1, "groups", &list.len().to_string(), &[], 0.0);
    let list_h = (ui.available_height() - tk::FIELD_HEIGHT - tk::GAP * 2.0).max(tk::LIST_ROW_HEIGHT_COMPACT);
    egui::ScrollArea::vertical().id_salt("groups").max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if list.is_empty() {
            padded(ui, |ui| kit::loading(ui, &t, "none yet"));
        }
        let mut rows = HoverList::begin(ui, Variant::Compact).indent(tk::GUTTER + tk::HOVER_SHIFT_COMPACT);
        for g in &list {
            let count = app.instances.iter().filter(|i| i.group == g.name).count();
            let desc = if count == 1 { "1 instance".to_string() } else { format!("{count} instances") };
            let on = s.selected.as_deref() == Some(g.name.as_str());
            let hue = g.color.as_deref().and_then(|c| t.named(c));
            let r = rows.row(ui, &t, HoverRow::new(("group", &g.name), &g.name).description(&desc).hue(hue).selected(on).hot(on && !in_members));
            if on && !in_members && moved.is_some() {
                r.scroll_to_me(None);
            }
            if r.clicked() {
                s.select(Some(g.name.clone()));
                s.in_members = false;
            }
        }
        rows.end(ui);
    });
    ui.add_space(tk::GAP);
    padded(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = tk::GAP;
            let name = s.new_name.trim().to_string();
            let ok = !name.is_empty() && app.groups.get(&name).is_none();
            let add_w = Button::new("add").width(ui, &t);
            let r = kit::Field::new(&mut s.new_name).hint("New group").width(ui.available_width() - add_w - tk::GAP).focus(std::mem::take(&mut s.focus_new)).show(ui, &t);
            let add = Button::new("add").enabled(ok).show(ui, &t).clicked() || (ok && r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if add {
                app.groups.list.push(Group { name: name.clone(), color: None });
                app.save_groups();
                s.new_name.clear();
                s.select(Some(name));
            }
        });
    });

    }

    // -- right: the selected group
    {
    let ui = &mut right;
    ui.spacing_mut().item_spacing.y = 0.0;
    if let Some(name) = s.selected.clone() {
        group_page(app, s, ui, &t, &name, kb);
    } else {
        padded(ui, |ui| kit::empty_state(ui, &t, "Pick or make a group", "Groups can have a colour, and instances move between them here or by dragging them in the list.", None));
    }
    }

    let k = |a| app.keymap.label(a);
    let hints = [
        (format!("{}/{}", k(Action::FocusLeft), k(Action::FocusRight)), "groups/instances"),
        (k(Action::Add), "new"),
        (k(Action::Rename), "rename"),
        (k(Action::Cycle), "colour"),
        (k(Action::Delete), "delete"),
    ];
    sheet_foot(ui, &t, &hints, &[Button::new("close")]) == Some(0)
}

/// The selected group: its name, colour, place and deletion, then its instances.
fn group_page(app: &mut App, s: &mut GroupManager, ui: &mut Ui, t: &Tokens, name: &str, kb: Option<Nav>) {
    let key = |n: Nav| kb == Some(n);
    let in_members = s.in_members;
    let count = app.instances.iter().filter(|i| i.group == name).count();
    let aside = if count == 1 { "1 instance".to_string() } else { format!("{count} instances") };
    let mut deleted = false;
    pane_head(ui, t, 2, name, &aside, &[], 0.0);
    ui.scope(|ui| {
        SettingRow::new("Name").show(ui, t, |ui| {
            let new = s.rename.trim().to_string();
            let ok = !new.is_empty() && new != name;
            let go = Button::new("rename").enabled(ok).show(ui, t).clicked();
            let w = (ui.available_width().min(tk::SETTING_CONTROL_WIDTH) - tk::GAP).max(tk::GUTTER * 4.0);
            let r = kit::Field::new(&mut s.rename).width(w).focus(std::mem::take(&mut s.focus_rename)).show(ui, t);
            let go = go || (ok && r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if go {
                if app.groups.get(&new).is_some() {
                    app.toast(ToastKind::Error, format!("There's already a group called {new}"));
                } else {
                    if let Some(g) = app.groups.list.iter_mut().find(|g| g.name == name) {
                        g.name = new.clone();
                    }
                    app.regroup(name, &new);
                    for c in app.cfg.collapsed_groups.iter_mut().filter(|c| *c == name) {
                        *c = new.clone();
                    }
                    let _ = app.cfg.save();
                    app.save_groups();
                    s.select(Some(new));
                }
            }
        });
        let current = app.groups.color(name).map(String::from);
        let hue_name = current.clone().unwrap_or_else(|| "none".into());
        let mut pick: Option<Option<String>> = None;
        SettingRow::new("Colour").caption(&hue_name).show(ui, t, |ui| {
            if let Some(c) = swatches(ui, t, current.as_deref()) {
                pick = Some(c);
            }
        });
        if key(Nav::Cycle) {
            let i = current.as_deref().and_then(|c| COLORS.iter().position(|x| *x == c));
            pick = Some(match i {
                None => Some(COLORS[0].to_string()),
                Some(i) if i + 1 < COLORS.len() => Some(COLORS[i + 1].to_string()),
                Some(_) => None,
            });
        }
        if let Some(c) = pick {
            if let Some(g) = app.groups.list.iter_mut().find(|g| g.name == name) {
                g.color = c;
            }
            app.save_groups();
        }
        let i = app.groups.list.iter().position(|g| g.name == name).unwrap_or(0);
        let n = app.groups.list.len();
        SettingRow::new("Place").caption(&format!("{} of {n} in the list", i + 1)).show(ui, t, |ui| {
            let clicked = kit::button_row(ui, t, &[Button::new("").icon(glyphs::ARROW_UP).enabled(i > 0), Button::new("").icon(glyphs::ARROW_DOWN).enabled(i + 1 < n)]);
            if (clicked == Some(0) || key(Nav::MoveUp)) && i > 0 {
                app.groups.list.swap(i, i - 1);
                app.save_groups();
            }
            if (clicked == Some(1) || key(Nav::MoveDown)) && i + 1 < n {
                app.groups.list.swap(i, i + 1);
                app.save_groups();
            }
        });
        SettingRow::new("Delete").caption("its instances stay, ungrouped").show(ui, t, |ui| {
            if Button::new("delete group").icon(glyphs::TRASH).show(ui, t).clicked() || (!in_members && key(Nav::Delete)) {
                app.groups.list.retain(|g| g.name != name);
                app.regroup(name, "");
                app.cfg.collapsed_groups.retain(|c| c != name);
                let _ = app.cfg.save();
                app.save_groups();
                deleted = true;
            }
        });
    });
    kit::rule(ui, t, true);
    if deleted {
        s.select(None);
        return;
    }
    let name = s.selected.clone().unwrap_or_default();

    pane_head(ui, t, 3, "instances", &format!("{count} in {name}"), &[], 0.0);
    let mut changed = None;
    egui::ScrollArea::vertical().id_salt("members").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (n, inst) in app.instances.iter().enumerate() {
            let member = inst.group == name;
            let cursor = in_members && s.member == n;
            if cursor && key(Nav::Toggle) {
                changed = Some((n, !member));
            }
            let aside = if !member && !inst.group.is_empty() { format!("in {}", inst.group) } else { String::new() };
            let (r, _) = QuietRow::new(("member", &inst.id), &inst.name).check(member).aside(&aside, None).hot(cursor).selected(cursor).show(ui, t, |_| ());
            if cursor && kb.is_some_and(|n| n.is_move()) {
                r.scroll_to_me(None);
            }
            if r.clicked() {
                s.in_members = true;
                s.member = n;
                changed = Some((n, !member));
            }
        }
    });
    if let Some((n, member)) = changed {
        let i = &mut app.instances[n];
        i.group = if member { name.clone() } else { String::new() };
        let _ = i.save();
    }
}

/// No colour, then the seven clarity hues, as small squares (a hue is a mark, never a fill
/// behind text); the current one framed. Laid out right to left, in order. Returns a pick.
fn swatches(ui: &mut Ui, t: &Tokens, current: Option<&str>) -> Option<Option<String>> {
    let side = tk::SWATCH_SIZE;
    let all: Vec<Option<&str>> = std::iter::once(None).chain(COLORS.iter().copied().map(Some)).collect();
    let mut pick = None;
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = tk::UNIT / 2.0;
        for c in all.iter().rev() {
            let (rect, resp) = ui.allocate_exact_size(vec2(side, side), Sense::click());
            let on = *c == current;
            let hot = resp.hovered() || resp.has_focus();
            let p = ui.painter();
            let inner = rect.shrink(tk::SWITCH_INSET);
            match c.and_then(|c| t.named(c)) {
                Some(col) => {
                    p.rect_filled(inner, 0.0, col);
                }
                None => {
                    kit::rule::frame(p, inner, t.text_muted);
                    p.line_segment([inner.left_bottom(), inner.right_top()], egui::Stroke::new(tk::LINE, t.text_muted));
                }
            }
            // The current one framed in ink; the one you reach for in the accent.
            let rest = if on { t.text_strong } else { egui::Color32::TRANSPARENT };
            kit::rule::frame(p, rect, motion::color(ui.ctx(), resp.id, hot, rest, t.accent));
            let label = c.unwrap_or("no colour").to_string();
            let resp = resp.on_hover_text(&label).on_hover_cursor(egui::CursorIcon::PointingHand);
            resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, &label));
            if resp.clicked() && !on {
                pick = Some(c.map(String::from));
            }
        }
    });
    pick
}
