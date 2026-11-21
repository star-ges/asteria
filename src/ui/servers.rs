//! The in-game multiplayer server list (`servers.dat`): a compact hover list; a server being
//! edited opens into its fields in its own place.

use std::time::{Duration, Instant};

use egui::{Ui, vec2};

use super::content::padded;
use super::nav::{Nav, Zone};
use crate::design::glyphs;
use crate::design::kit;
use crate::design::tokens::{self as tk, Tokens};
use super::{App, ToastKind};
use crate::instance::Instance;
use crate::launch::QuickPlay;
use crate::saves::{self, Server};

struct Editing {
    index: Option<usize>,
    name: String,
    ip: String,
    focus: bool,
}

#[derive(Default)]
pub struct ServersState {
    cache: Option<(String, Result<Vec<Server>, String>, Instant)>,
    selected: Option<usize>,
    editing: Option<Editing>,
}

pub fn show(app: &mut App, ui: &mut Ui, inst: &Instance) {
    let t = app.look.tokens.clone();
    let running = app.is_running(&inst.id);
    let s = &mut app.servers;
    let fresh = s.cache.as_ref().is_some_and(|(id, _, at)| *id == inst.id && at.elapsed() < Duration::from_secs(5));
    if !fresh {
        if s.cache.as_ref().is_some_and(|(id, _, _)| *id != inst.id) {
            s.selected = None;
            s.editing = None;
        }
        s.cache = Some((inst.id.clone(), saves::read_servers(&inst.game_dir()).map_err(|e| format!("{e:#}")), Instant::now()));
    }
    let mut servers = match &s.cache.as_ref().unwrap().1 {
        Ok(v) => v.clone(),
        Err(e) => {
            padded(ui, |ui| {
                ui.add_space(tk::GUTTER);
                kit::bracketed(ui, &t, "can't read the server list", |ui| kit::text::body_colored(ui, e, t.error()));
            });
            return;
        }
    };
    let kb = if s.editing.is_some() { None } else { app.nav_for(Zone::Main) };
    let focused = app.focus == Zone::Main;
    let s = &mut app.servers;
    let mut scroll = false;
    if let Some(p) = kb.filter(|n| n.is_move()).and_then(|n| n.step(servers.len(), s.selected)) {
        s.selected = Some(p);
        scroll = true;
    }
    let key = |n: Nav| kb == Some(n);
    let sel = s.selected.filter(|&i| i < servers.len());
    let mut changed = false;
    let mut join = None;
    let mut act: Option<Act> = None;

    // The head: what's here, and what can be done with the selection.
    ui.spacing_mut().item_spacing.y = 0.0;
    let n = servers.len();
    let aside = if n == 1 { "1 server".to_string() } else { format!("{n} servers") };
    let mut buttons = Vec::new();
    if let Some(i) = sel {
        buttons.extend([
            kit::Button::new("join"),
            kit::Button::new("edit"),
            kit::Button::new("").icon(glyphs::ARROW_UP).enabled(i > 0),
            kit::Button::new("").icon(glyphs::ARROW_DOWN).enabled(i + 1 < n),
            kit::Button::new("remove"),
        ]);
    }
    let add_i = buttons.len();
    buttons.push(kit::Button::new("add server").icon(glyphs::PLUS));
    let (clicked, _) = super::content::pane_head(ui, &t, 1, "servers", &aside, &buttons, 0.0);
    let pick = |i: usize| clicked == Some(i);
    if pick(add_i) || key(Nav::Add) {
        act = Some(Act::Add);
    }
    if let Some(i) = sel {
        if pick(0) || key(Nav::Activate) {
            act = Some(Act::Join(i));
        } else if pick(1) || key(Nav::Edit) || key(Nav::Rename) {
            act = Some(Act::Edit(i));
        } else if (pick(2) || key(Nav::MoveUp)) && i > 0 {
            act = Some(Act::Move(i, i - 1));
        } else if (pick(3) || key(Nav::MoveDown)) && i + 1 < n {
            act = Some(Act::Move(i, i + 1));
        } else if pick(4) || key(Nav::Delete) {
            act = Some(Act::Remove(i));
        }
    }
    if running {
        kit::banner(ui, &t, "game running", Some(t.warning()), "It saves its own server list when it quits, which may undo changes made here.", &[]);
    }

    let mut done = None;
    egui::ScrollArea::vertical().id_salt("servers").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if servers.is_empty() && s.editing.is_none() {
            padded(ui, |ui| kit::empty_state(ui, &t, "No servers yet", "Add one here, or from the multiplayer screen in game.", None));
            return;
        }
        let mut list = kit::HoverList::begin(ui, kit::Variant::Compact).indent(tk::GUTTER + tk::HOVER_SHIFT_COMPACT);
        for (i, srv) in servers.iter().enumerate() {
            // The server being edited opens into its fields, in its place.
            if let Some(e) = s.editing.as_mut().filter(|e| e.index == Some(i)) {
                done = edit_fields(ui, &t, e);
                continue;
            }
            let on = s.selected == Some(i);
            let mut row = kit::HoverRow::new(("server", i, &srv.ip), &srv.name).description(&srv.ip).selected(on).hot(on && focused);
            if let Some(png) = &srv.icon {
                row = row.image(egui::ImageSource::Bytes { uri: format!("bytes://server/{}/{}.png", inst.id, crate::net::short_hash(png)).into(), bytes: png.clone().into() }, 1.0);
            }
            let r = list.row(ui, &t, row);
            if on && scroll {
                r.scroll_to_me(None);
            }
            if r.clicked() {
                s.selected = Some(i);
                act = act.take().or(Some(Act::Focus));
            }
            if r.double_clicked() {
                act = Some(Act::Join(i));
            }
            r.context_menu(|ui| {
                for (label, a) in [("join", Act::Join(i)), ("edit", Act::Edit(i)), ("remove", Act::Remove(i))] {
                    if ui.button(label).clicked() {
                        act = Some(a);
                        ui.close();
                    }
                }
            });
        }
        list.end(ui);
        if let Some(e) = s.editing.as_mut().filter(|e| e.index.is_none()) {
            done = edit_fields(ui, &t, e);
        }
    });

    if let Some(save) = done {
        if let Some(e) = s.editing.take().filter(|_| save) {
            let name = if e.name.trim().is_empty() { "Minecraft Server" } else { e.name.trim() };
            match e.index {
                Some(i) if i < servers.len() => {
                    servers[i].name = name.to_string();
                    servers[i].ip = e.ip.trim().to_string();
                }
                _ => {
                    servers.push(Server::new(name, e.ip.trim()));
                    s.selected = Some(servers.len() - 1);
                }
            }
            changed = true;
        }
    }
    match act {
        Some(Act::Join(i)) => join = servers.get(i).map(|x| x.ip.clone()),
        Some(Act::Edit(i)) => s.editing = Some(Editing { index: Some(i), name: servers[i].name.clone(), ip: servers[i].ip.clone(), focus: true }),
        Some(Act::Add) => s.editing = Some(Editing { index: None, name: String::new(), ip: String::new(), focus: true }),
        Some(Act::Move(from, to)) => {
            servers.swap(from, to);
            s.selected = Some(to);
            changed = true;
        }
        Some(Act::Remove(i)) => {
            servers.remove(i);
            s.selected = None;
            changed = true;
        }
        Some(Act::Focus) | None => {}
    }
    if matches!(act, Some(Act::Focus | Act::Join(_) | Act::Edit(_))) {
        app.focus = Zone::Main;
    }
    if changed {
        match saves::write_servers(&inst.game_dir(), &servers) {
            Ok(()) => app.servers.cache = None,
            Err(e) => app.toast(ToastKind::Error, format!("Couldn't save servers: {e:#}")),
        }
    }
    if let Some(ip) = join {
        app.launch(ui.ctx(), &inst.id, Some(QuickPlay::Server(ip)));
    }
}

/// What a head button, key or row menu asked for.
enum Act {
    Join(usize),
    Edit(usize),
    Add,
    Move(usize, usize),
    Remove(usize),
    /// A row was clicked: the list takes the keyboard.
    Focus,
}

/// A server opened for editing, in its row's place: the name and the address as fields side by
/// side (stacked when narrow), save and cancel. Enter saves, Esc cancels. Some(save) when done.
fn edit_fields(ui: &mut Ui, t: &Tokens, e: &mut Editing) -> Option<bool> {
    let focus = std::mem::take(&mut e.focus);
    let ok = !e.ip.trim().is_empty();
    let mut done = None;
    ui.add_space(tk::GAP);
    padded(ui, |ui| {
        let buttons = kit::Button::new("save").width(ui, t) + kit::Button::new("cancel").width(ui, t) + tk::GAP;
        let room = ui.available_width();
        let side = (room - buttons - tk::GAP * 3.0) / 2.0 >= tk::SERVER_FIELD_MIN;
        let field_w = if side { ((room - buttons - tk::GAP * 3.0) / 2.0).min(tk::SETTING_CONTROL_WIDTH * 1.2) } else { room };
        let mut fields = |ui: &mut Ui| {
            ui.spacing_mut().item_spacing = vec2(tk::GAP, tk::GAP);
            let a = kit::Field::new(&mut e.name).label("name").hint("Minecraft Server").width(field_w).focus(focus).show(ui, t);
            let b = kit::Field::new(&mut e.ip).label("address").hint("play.example.com").width(field_w).show(ui, t);
            [a, b]
        };
        let rs = if side { ui.horizontal(|ui| fields(ui)).inner } else { fields(ui) };
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if ok && enter && rs.iter().any(|r| r.lost_focus()) {
            done = Some(true);
        }
        if esc && rs.iter().any(|r| r.lost_focus() || r.has_focus()) {
            done = Some(false);
        }
        let mut row = |ui: &mut Ui| {
            ui.spacing_mut().item_spacing.x = tk::GAP;
            if kit::Button::new("save").filled().enabled(ok).show(ui, t).clicked() {
                done = Some(true);
            }
            if kit::Button::new("cancel").show(ui, t).clicked() {
                done = Some(false);
            }
        };
        if side {
            // Beside the fields, on their line.
            let r = rs[1].rect;
            let at = egui::Rect::from_min_max(egui::pos2(r.right() + tk::GAP, r.top()), egui::pos2(r.right() + tk::GAP + buttons, r.bottom()));
            row(&mut ui.new_child(egui::UiBuilder::new().max_rect(at).layout(egui::Layout::left_to_right(egui::Align::Center))));
        } else {
            ui.add_space(tk::GAP);
            ui.horizontal(row);
        }
    });
    ui.add_space(tk::GAP);
    kit::rule(ui, t, false);
    done
}
