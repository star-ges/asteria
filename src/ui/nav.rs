//! Keyboard navigation. The screen is split into focus zones (the instance
//! list, the current tab's main pane, and a side pane where there is one);
//! the same keys move and act in whichever zone has focus. Keys come from
//! the user's [`Keymap`]; `App::keys` turns them into a [`Nav`], and each
//! pane takes it with `App::nav_for(zone)` while drawing, since that's where
//! its items live.

use egui::Ui;

use super::instances::InstTab;
use crate::design::kit::{self, SectionLabel};
use crate::design::tokens as tk;
use crate::keymap::{self, Action, Keymap};
use super::{App, Tab};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    List,
    Main,
    Side,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    Top,
    Bottom,
    PageUp,
    PageDown,
    Activate,
    Toggle,
    Delete,
    Rename,
    Duplicate,
    Edit,
    Add,
    Copy,
    Folder,
    MoveUp,
    MoveDown,
    Search,
    /// Dialogs only (on the main screen h/l move focus between panes).
    Left,
    Right,
    /// Cycle a setting, e.g. a group's colour.
    Cycle,
}

impl Nav {
    /// Apply a movement to a selection in a list of `len` items.
    pub fn step(self, len: usize, cur: Option<usize>) -> Option<usize> {
        if len == 0 {
            return None;
        }
        let last = len as isize - 1;
        let page = 10;
        let at = cur.map(|c| c as isize);
        let to = match self {
            Nav::Down => at.map_or(0, |c| c + 1),
            Nav::Up => at.map_or(0, |c| c - 1),
            Nav::PageDown => at.map_or(0, |c| c + page),
            Nav::PageUp => at.map_or(0, |c| c - page),
            Nav::Top => 0,
            Nav::Bottom => last,
            _ => return cur,
        };
        Some(to.clamp(0, last) as usize)
    }

    pub fn is_move(self) -> bool {
        matches!(self, Nav::Up | Nav::Down | Nav::Top | Nav::Bottom | Nav::PageUp | Nav::PageDown)
    }

    /// Pixels to scroll a free-scrolling pane (logs) by.
    pub fn scroll_delta(self, line: f32, page: f32) -> Option<f32> {
        Some(match self {
            Nav::Down => -3.0 * line,
            Nav::Up => 3.0 * line,
            Nav::PageDown => -page,
            Nav::PageUp => page,
            Nav::Top => 1e9,
            Nav::Bottom => -1e9,
            _ => return None,
        })
    }
}

/// Which zones the current instance tab has.
pub fn has_side(app: &App) -> bool {
    app.inst_tab.content_kind().is_some() || matches!(app.inst_tab, InstTab::LogHistory | InstTab::Worlds | InstTab::Config) || app.inst_tab == InstTab::Overview
}

/// Actions that drive panes and dialogs.
pub const PANE_ACTIONS: &[Action] = &[
    Action::Down,
    Action::Up,
    Action::Top,
    Action::Bottom,
    Action::PageDown,
    Action::PageUp,
    Action::Activate,
    Action::Toggle,
    Action::Delete,
    Action::Rename,
    Action::Duplicate,
    Action::Edit,
    Action::Add,
    Action::Copy,
    Action::Folder,
    Action::MoveDown,
    Action::MoveUp,
    Action::Search,
    Action::Cycle,
];

/// The pane-level meaning of an action. In dialogs the focus keys move
/// left/right instead.
pub fn to_nav(a: Action, modal: bool) -> Option<Nav> {
    Some(match a {
        Action::Down => Nav::Down,
        Action::Up => Nav::Up,
        Action::Top => Nav::Top,
        Action::Bottom => Nav::Bottom,
        Action::PageDown => Nav::PageDown,
        Action::PageUp => Nav::PageUp,
        Action::Activate => Nav::Activate,
        Action::Toggle => Nav::Toggle,
        Action::Delete => Nav::Delete,
        Action::Rename => Nav::Rename,
        Action::Duplicate => Nav::Duplicate,
        Action::Edit => Nav::Edit,
        Action::Add => Nav::Add,
        Action::Copy => Nav::Copy,
        Action::Folder => Nav::Folder,
        Action::MoveDown => Nav::MoveDown,
        Action::MoveUp => Nav::MoveUp,
        Action::Search => Nav::Search,
        Action::Cycle => Nav::Cycle,
        Action::FocusLeft if modal => Nav::Left,
        Action::FocusRight if modal => Nav::Right,
        _ => return None,
    })
}

/// Hints for the bottom bar: what the focused zone can do right now.
pub fn hints(app: &App) -> Vec<(String, &'static str)> {
    use Action::*;
    let k: &Keymap = &app.keymap;
    let l = |a| k.label(a);
    let p = |a, b| k.pair(a, b);
    if app.capture.is_some() {
        return vec![("any key".into(), "bind it"), ("esc".into(), "cancel")];
    }
    match &app.modal {
        Some(super::Modal::Confirm(_)) => return vec![(l(Help), "keys"), (l(Activate), "confirm"), ("esc".into(), "cancel")],
        Some(_) => return vec![(l(Help), "keys"), ("esc".into(), "close"), (l(Activate), "confirm"), (p(Down, Up), "move")],
        None => {}
    }
    // `?` leads, so the way to every other key is always in the same place.
    let mut h: Vec<(String, &str)> = vec![(l(Help), "keys")];
    match app.tab {
        Tab::Accounts if app.acct.has_pending() => {
            h.extend([(l(Activate), "upload skin"), (l(Back), "cancel"), (l(Toggle), "classic/slim"), (p(TurnLeft, TurnRight), "turn")])
        }
        Tab::Accounts => {
            h.extend([(p(Down, Up), "accounts"), (l(Activate), "make active"), (l(Delete), "remove"), (p(TurnLeft, TurnRight), "turn")]);
            if super::accounts::current_is_microsoft(app) {
                h.extend([(l(UploadSkin), "skin"), (l(Toggle), "classic/slim"), (l(Cycle), "cape")]);
            }
            h.extend([(l(Microsoft), "microsoft"), (l(Add), "offline")]);
        }
        Tab::Settings => h.extend([("tab".into(), "next field")]),
        Tab::Instances if app.focus == Zone::List => h.extend([
            (l(Activate), "play"),
            (p(Down, Up), "instances"),
            (l(FocusRight), "into tab"),
            (p(PrevTab, NextTab), "tabs"),
            (l(Groups), "groups"),
            (l(New), "new"),
            (l(Import), "import"),
            (l(Export), "export"),
            (l(Edit), "edit"),
            (l(ChangeIcon), "icon"),
            (l(Search), "filter"),
        ]),
        Tab::Instances => {
            let side = app.focus == Zone::Side;
            let lt = l(LoaderType);
            h.extend(match (app.inst_tab, side) {
                (InstTab::Overview, false) if app.version.picker_open() => vec![(p(Down, Up), "versions"), (l(FocusRight), "loader"), (lt, "loader type"), (l(Activate), "apply"), (l(Back), "cancel")],
                (InstTab::Overview, true) if app.version.picker_open() => vec![(p(Down, Up), "loader versions"), (lt, "loader type"), (l(Activate), "apply"), (l(Back), "cancel")],
                (InstTab::Overview, false) => vec![(l(Activate), "change version"), (l(FocusRight), "notes")],
                (InstTab::Overview, true) => vec![(l(Activate), "write"), ("esc".into(), "stop writing")],
                (InstTab::Log, _) => {
                    let mut v = vec![(p(Down, Up), "scroll"), (l(Bottom), "follow"), (l(Search), "filter"), (l(Copy), "copy")];
                    if app.selected.as_ref().is_some_and(|id| app.logs.notices.contains_key(id)) {
                        v.insert(0, (l(Why), "why it crashed"));
                    }
                    v
                }
                (InstTab::Worlds, false) => vec![(p(Down, Up), "worlds"), (l(Activate), "play world"), (l(Rename), "rename"), (l(Duplicate), "duplicate"), (l(Add), "back up"), (l(Delete), "delete"), (l(FocusRight), "backups")],
                (InstTab::Worlds, true) => vec![(p(Down, Up), "backups"), (l(Activate), "restore"), (l(Add), "back up now"), (l(Delete), "delete"), (l(Folder), "folder")],
                (InstTab::Servers, _) => vec![(p(Down, Up), "servers"), (l(Activate), "join"), (l(Add), "add"), (l(Edit), "edit"), (p(MoveDown, MoveUp), "reorder"), (l(Delete), "remove")],
                (InstTab::Screenshots, _) if app.shots.viewing() => vec![(p(Down, Up), "step"), (l(Activate), "open"), (l(Copy), "copy"), (l(Delete), "delete"), (l(Folder), "folder"), (l(Back), "all screenshots")],
                (InstTab::Screenshots, _) => vec![(p(Down, Up), "screenshots"), (l(Activate), "view"), (l(Copy), "copy"), (l(Delete), "delete"), (l(Folder), "folder"), (l(Toggle), "all instances")],
                (InstTab::Config, false) if app.config.area != super::config::Area::Minecraft => {
                    let what = if app.config.area == super::config::Area::Shaders { "packs" } else { "files" };
                    vec![(l(ConfigArea), "minecraft/mods/shaders"), (p(Down, Up), what), (l(Toggle), "open/fold"), (l(FocusRight), "edit"), (l(Search), "filter"), (l(SaveConfig), "save"), (l(Folder), "folder")]
                }
                (InstTab::Config, true) if app.config.open.as_ref().is_some_and(|o| o.raw_mode()) => {
                    let mut v = vec![(l(Activate), "type"), ("esc".into(), "stop typing"), (l(SaveConfig), "save")];
                    if app.config.open.as_ref().is_some_and(|o| o.has_fields()) {
                        v.push((l(RawConfig), "fields"));
                    }
                    v.push((l(Delete), "revert"));
                    v
                }
                (InstTab::Config, _) => vec![(l(ConfigArea), "minecraft/mods/shaders"), (p(Down, Up), "settings"), (l(Toggle), "switch / fold"), (l(Activate), "edit"), (l(Delete), "default"), (l(SaveConfig), "save"), (l(RawConfig), "text"), (l(Search), "filter")],
                (InstTab::LogHistory, true) if app.logs.panel_focused() => {
                    vec![(p(Down, Up), "suspects"), (l(Activate), "show in log"), (l(Toggle), "disable/enable"), (l(Copy), "copy summary"), (l(Why), "back to the log")]
                }
                (InstTab::LogHistory, side) => {
                    let mut v = if side {
                        vec![(p(Down, Up), "scroll"), (l(Search), "filter"), (l(Copy), "copy")]
                    } else {
                        vec![(p(Down, Up), "files"), (l(FocusRight), "read"), (l(Activate), "open"), (l(Folder), "folder")]
                    };
                    if app.logs.analysis().is_some() {
                        v.insert(0, (l(Why), "why it crashed"));
                    }
                    v
                }
                // A search for the breaking mod has the mods: only answering it and browsing work.
                (InstTab::Mods, false) if app.selected.as_ref().is_some_and(|id| app.mod_sets.seen_searching(id)) => {
                    vec![(p(Down, Up), "installed"), (l(FindBreaking), "answer the step"), (l(FocusRight), "modrinth"), (l(Search), "search")]
                }
                (InstTab::Mods, false) => vec![(p(Down, Up), "installed"), (l(Toggle), "enable/disable"), (l(Delete), "remove"), (l(Cycle), "config"), (l(Update), "updates"), (l(RollBack), "roll back"), (l(ModSets), "sets"), (l(FindBreaking), "find breaking mod"), (l(FocusRight), "modrinth"), (l(Search), "search")],
                (InstTab::ShaderPacks, false) => vec![(p(Down, Up), "installed"), (l(Toggle), "enable/disable"), (l(Delete), "remove"), (l(Cycle), "options"), (l(FocusRight), "modrinth"), (l(Search), "search")],
                (_, false) => vec![(p(Down, Up), "installed"), (l(Toggle), "enable/disable"), (l(Delete), "remove"), (l(FocusRight), "modrinth"), (l(Search), "search")],
                (_, true) => vec![(p(Down, Up), "results"), (l(Activate), "install"), (l(Search), "search")],
            });
            h.extend([(l(FocusLeft), "back"), (p(PrevTab, NextTab), "tabs")]);
        }
    }
    // The list plays with its own key already; one "play" is enough.
    if !h.iter().any(|(_, what)| *what == "play") {
        h.push((l(Play), "play"));
    }
    h
}

/// The full reference, shown with `?` in a sheet: each section numbered, a key column and
/// what it does. Returns true when it should close.
pub fn help(app: &App, ui: &mut Ui) -> bool {
    let t = app.look.tokens.clone();
    let mut sections: Vec<(&str, Vec<(String, &str)>)> = Vec::new();
    for i in keymap::ACTIONS {
        let keys = app.keymap.chords(i.action).iter().map(|c| c.label()).collect::<Vec<_>>().join(" / ");
        match sections.iter_mut().find(|(s, _)| *s == i.section) {
            Some((_, v)) => v.push((keys, i.what)),
            None => sections.push((i.section, vec![(keys, i.what)])),
        }
    }
    sections.push(("dialogs", vec![("tab".into(), "next field"), ("↑ / ↓".into(), "pick a version while typing a filter"), (app.keymap.label(Action::Activate), "confirm"), ("esc".into(), "close")]));
    let mut close = false;
    ui.horizontal(|ui| {
        kit::heading(ui, &t, "Keys");
        ui.add_space(tk::GAP);
        kit::caption(ui, &t, "make changes in settings");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            close = kit::Button::new("close").show(ui, &t).clicked();
        });
    });
    ui.add_space(tk::GAP);
    // Two columns, split near the middle by line count.
    let half = sections.iter().map(|(_, v)| v.len() + 2).sum::<usize>() / 2;
    let key_w = 120.0;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.columns(2, |cols| {
            let mut count = 0;
            for (n, (title, keys)) in sections.iter().enumerate() {
                let ui = &mut cols[if count < half { 0 } else { 1 }];
                count += keys.len() + 2;
                ui.spacing_mut().item_spacing.y = 0.0;
                SectionLabel::new(n + 1, title).show(ui, &t);
                for (k, what) in keys {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), tk::TABLE_ROW_HEIGHT), egui::Sense::hover());
                    let p = ui.painter().with_clip_rect(rect);
                    // On the section label's gutter.
                    let rect = rect.with_min_x(rect.left() + tk::GUTTER);
                    // Keys keep their own case (`e` and `E` differ), so not a caption.
                    let kg = p.layout_no_wrap(k.clone(), crate::design::fonts::raw(tk::SIZE_MONO), t.text_strong);
                    p.galley(egui::pos2(rect.left(), rect.center().y - kg.size().y / 2.0), kg, t.text_strong);
                    let mut job = egui::text::LayoutJob::simple_singleline(what.to_string(), crate::design::fonts::text(tk::SIZE_BODY), t.text_muted);
                    job.wrap.max_width = rect.width() - key_w;
                    job.wrap.max_rows = 1;
                    let g = p.layout_job(job);
                    p.galley(egui::pos2(rect.left() + key_w, rect.center().y - g.size().y / 2.0), g, t.text_muted);
                    kit::hline(ui.painter(), rect.x_range(), rect.bottom() - 0.5, t.line);
                }
                ui.add_space(tk::GAP);
            }
        });
    });
    close
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The modal key route takes ⏎ off egui's event list, so dialogs must get it as `Activate`.
    #[test]
    fn enter_reaches_dialogs() {
        let km = Keymap::new(&Default::default());
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() });
        ctx.begin_pass(input);
        let nav = km.take(&ctx, PANE_ACTIONS).and_then(|a| to_nav(a, true));
        assert_eq!(nav, Some(Nav::Activate));
        ctx.end_pass().textures_delta.clear();
    }
}
