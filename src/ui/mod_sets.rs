//! Mod sets and the search for the breaking mod, on screen: the sets bar at the top of
//! the mods tab, the sets dialog, the search's start dialog, the banner on the instance
//! while a search runs, and the question after each test game.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use egui::{Align, Layout, Ui, vec2};

use super::content::ModJobDone;
use super::nav::Nav;
use crate::design::kit;
use crate::design::tokens::{self as tk, Tokens};
use super::{App, Confirm, ConfirmAction, Modal, ToastKind};
use crate::bisect::{self, Answer, Bisect, StepKind};
use crate::instance::Instance;
use crate::keymap::Action;
use crate::mod_sets::{self, Entry, ModSets};
use crate::task::{Promise, Task};

const SAVE_INTO: &str = "save enabled mods to set";

/// How often the sets and search files are checked for changes.
const RECHECK: Duration = Duration::from_millis(500);

/// Each instance's sets and search, read from disk in the background and read again when
/// their files change (checked twice a second at most). What was read last stays shown
/// while a new read runs; nothing here reads a file on the UI thread.
#[derive(Default)]
pub struct State {
    cache: HashMap<String, Cached>,
    /// Where reads run, set each frame by [`State::tick`].
    rt: Option<(tokio::runtime::Handle, egui::Context)>,
    /// Reads made by mod jobs as they finish, after their writes: shown before the job
    /// stops counting as busy, so no guard sees the files as they were before it.
    inbox: Arc<Mutex<HashMap<String, Read>>>,
    /// Games that ended mid-search, to ask about once no other dialog is open:
    /// (instance id, what suggests the problem happened).
    asks: Vec<(String, Option<String>)>,
    /// Instances whose sets couldn't be saved when new mods joined: not tried again
    /// every frame.
    absorb_failed: std::collections::HashSet<String>,
}

type Stamps = (Option<SystemTime>, Option<SystemTime>);

/// The two files as read.
pub struct Read {
    stamps: Stamps,
    sets: Result<ModSets, String>,
    bisect: Result<Option<Bisect>, String>,
}

#[derive(Default)]
struct Cached {
    shown: Option<Read>,
    /// When the last read was started.
    checked: Option<Instant>,
    /// A read running, and the generation it was started in.
    reading: Option<(Promise<Option<Read>>, u64)>,
    /// Bumped whenever something newer than a running read is shown: that read is dropped.
    generation: u64,
}

fn stamp(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Read `inst`'s sets and search, unless their files' stamps are still `known`. Reads files: off the UI thread.
pub fn read(inst: &Instance, known: Option<Stamps>) -> Option<Read> {
    let stamps = (stamp(&mod_sets::path(inst)), stamp(&bisect::path(inst)));
    if known == Some(stamps) {
        return None;
    }
    Some(Read { stamps, sets: mod_sets::load(inst).map_err(|e| format!("{e:#}")), bisect: bisect::load(inst).map_err(|e| format!("{e:#}")) })
}

impl State {
    /// Note where background reads run (each frame).
    pub fn tick(&mut self, rt: &tokio::runtime::Handle, ctx: &egui::Context) {
        if self.rt.is_none() {
            self.rt = Some((rt.clone(), ctx.clone()));
        }
    }

    /// What's known of `inst`'s files, starting a read if it's time for one. None until the first read is back.
    fn get(&mut self, inst: &Instance) -> Option<&Read> {
        let c = self.cache.entry(inst.id.clone()).or_default();
        if let Some(read) = self.inbox.lock().unwrap().remove(&inst.id) {
            c.shown = Some(read);
            c.generation += 1;
        }
        if let Some((p, generation)) = &c.reading
            && let Some(r) = p.take()
        {
            let fresh = *generation == c.generation;
            c.reading = None;
            match r {
                Ok(Some(read)) if fresh => c.shown = Some(read),
                Ok(_) => {}
                Err(e) => c.shown = Some(Read { stamps: (None, None), sets: Err(format!("{e:#}")), bisect: Err(format!("{e:#}")) }),
            }
        }
        if c.reading.is_none()
            && c.checked.is_none_or(|t| t.elapsed() >= RECHECK)
            && let Some((rt, ctx)) = &self.rt
        {
            let (i, known) = (inst.clone(), c.shown.as_ref().map(|r| r.stamps));
            let p = Promise::spawn(rt, ctx, async move { Ok(tokio::task::spawn_blocking(move || read(&i, known)).await?) });
            c.reading = Some((p, c.generation));
            c.checked = Some(Instant::now());
        }
        c.shown.as_ref()
    }

    /// `inst`'s sets, once read.
    pub fn sets(&mut self, inst: &Instance) -> Option<Result<ModSets, String>> {
        self.get(inst).map(|r| r.sets.clone())
    }

    /// `inst`'s search, if one is known to exist.
    pub fn bisect(&mut self, inst: &Instance) -> Option<Bisect> {
        self.get(inst).and_then(|r| r.bisect.as_ref().ok().cloned().flatten())
    }

    /// Whether a search has `inst`'s mods; None until its file has been read.
    pub fn search_known(&mut self, inst: &Instance) -> Option<bool> {
        self.get(inst).map(|r| r.bisect.as_ref().is_ok_and(|b| b.is_some()))
    }

    /// Whether a search for the breaking mod has `inst`'s mods (running or showing its
    /// result), or might (not read yet).
    pub fn searching(&mut self, inst: &Instance) -> bool {
        self.search_known(inst) != Some(false)
    }

    /// [`Self::searching`] as last read, for the hints bar.
    pub fn seen_searching(&self, id: &str) -> bool {
        self.cache.get(id).and_then(|c| c.shown.as_ref()).is_some_and(|r| r.bisect.as_ref().is_ok_and(|b| b.is_some()))
    }

    /// Read `inst`'s files again soon (after writing them).
    pub fn forget(&mut self, id: &str) {
        if let Some(c) = self.cache.get_mut(id) {
            c.checked = None;
        }
    }

    /// Show `sets` for instance `id` now (just saved), and check the files again soon.
    fn put_sets(&mut self, id: &str, sets: ModSets) {
        let c = self.cache.entry(id.to_string()).or_default();
        if let Some(r) = &mut c.shown {
            r.sets = Ok(sets);
            r.stamps.0 = None;
            c.generation += 1;
        }
        c.checked = None;
    }

    /// Where a mod job leaves its read of the files once it's done writing.
    fn inbox(&self) -> Arc<Mutex<HashMap<String, Read>>> {
        self.inbox.clone()
    }
}

/// Who has an instance's mods right now: the one admission check every control that
/// switches mods goes through (the mods tab, sets, the crash panel, the search's own actions).
#[derive(Debug, Clone, Copy, Default)]
pub struct ModsBusy {
    pub starting: bool,
    pub running: bool,
    /// A mod job: a switch, update, rollback or a search's step or ending.
    pub jobs: bool,
    /// Anything installing into the instance.
    pub installing: bool,
    /// Whether a search for the breaking mod has them; None until its file has been read.
    pub search: Option<bool>,
}

impl ModsBusy {
    /// Why a mod can't be switched on or off by hand (crash panel), if it can't.
    pub fn by_hand(&self) -> Option<&'static str> {
        if self.starting {
            Some("the game is starting")
        } else if self.jobs {
            Some("mods are being switched or updated")
        } else if self.search != Some(false) {
            Some("the search for the breaking mod has them")
        } else {
            None
        }
    }

    /// Why the mods can't be switched to a set, or a search started, if they can't.
    pub fn switch(&self) -> Option<&'static str> {
        if self.starting {
            Some("the game is starting")
        } else if self.running {
            Some("the game is running")
        } else if self.jobs || self.installing {
            Some("mods are installing or updating")
        } else {
            self.by_hand()
        }
    }

    /// Why a search's own step (answer, stop, finish, try again) can't run now, if it can't:
    /// only one at a time, and never while a game has the mods.
    pub fn search_action(&self) -> Option<&'static str> {
        if self.starting {
            Some("the game is starting")
        } else if self.running {
            Some("the game is running")
        } else if self.jobs || self.installing {
            Some("mods are being switched, installed or updated")
        } else {
            None
        }
    }
}

/// [`ModsBusy`] for `inst` now.
pub fn busy(app: &mut App, inst: &Instance) -> ModsBusy {
    ModsBusy {
        starting: app.sessions.get(&inst.id).is_some_and(|s| s.starting()),
        running: app.is_running(&inst.id),
        jobs: app.content.updating(&inst.id),
        installing: app.content.busy_with(&inst.id),
        search: app.mod_sets.search_known(inst),
    }
}

/// Why `inst`'s mods can't be switched to a set right now, if they can't.
fn switch_blocked(app: &mut App, inst: &Instance) -> Option<&'static str> {
    busy(app, inst).switch()
}

/// Run `f` on `inst`'s mods in the background as a mod job (so the instance counts as
/// busy, and launches wait), toasting what it returns.
fn job(app: &mut App, ctx: &egui::Context, inst: &Instance, stage: &str, f: impl FnOnce(&Instance) -> anyhow::Result<String> + Send + 'static) {
    let task = Task::new(format!("{stage} in {}", inst.name));
    task.stage(stage, 0);
    let (i, rt, inbox) = (inst.clone(), app.rt.clone(), app.mod_sets.inbox());
    app.content.start_mod_job(&rt, ctx, &inst.id, None, task, async move {
        let r = tokio::task::spawn_blocking(move || {
            let r = f(&i);
            deliver(&inbox, &i);
            r
        })
        .await;
        match r {
            Ok(Ok(text)) => ModJobDone::new(ToastKind::Info, text),
            Ok(Err(e)) => ModJobDone::new(ToastKind::Error, format!("{e:#}")),
            Err(e) => ModJobDone::new(ToastKind::Error, e.to_string()),
        }
    });
    app.mod_sets.forget(&inst.id);
}

/// Read `inst`'s files as a mod job ends, for [`State`] to show before the job stops counting as busy.
fn deliver(inbox: &Mutex<HashMap<String, Read>>, inst: &Instance) {
    if let Some(r) = read(inst, None) {
        inbox.lock().unwrap().insert(inst.id.clone(), r);
    }
}

/// Change `inst`'s sets on disk with `f`, toasting a failure.
fn edit_sets(app: &mut App, inst: &Instance, f: impl FnOnce(&mut ModSets) -> anyhow::Result<()>) -> bool {
    let r = mod_sets::load(inst).and_then(|mut sets| {
        f(&mut sets)?;
        mod_sets::save(inst, &sets)?;
        Ok(sets)
    });
    app.mod_sets.forget(&inst.id);
    match r {
        Ok(sets) => {
            app.mod_sets.put_sets(&inst.id, sets);
            true
        }
        Err(e) => {
            app.toast(ToastKind::Error, format!("{e:#}"));
            false
        }
    }
}

/// A mod's title for messages.
fn title_of(names: &HashMap<String, String>, key: &str) -> String {
    names.get(key).cloned().unwrap_or_else(|| key.trim_start_matches("file:").to_string())
}

fn list_titles(names: &HashMap<String, String>, keys: &[String]) -> String {
    let mut t: Vec<String> = keys.iter().take(4).map(|k| title_of(names, k)).collect();
    if keys.len() > 4 {
        t.push(format!("{} more", keys.len() - 4));
    }
    t.join(", ")
}

// -- the bar in the mods tab ----------------------------------------------------

/// Text that leads somewhere (a set's name opens the sets): `text_strong`, accent when hot.
fn link_text(ui: &mut Ui, t: &Tokens, text: &str, font: egui::FontId) -> egui::Response {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, egui::Color32::PLACEHOLDER);
    job.wrap.max_width = ui.available_width().max(40.0);
    job.wrap.max_rows = 1;
    let g = ui.painter().layout_job(job);
    let (rect, resp) = ui.allocate_exact_size(g.size(), egui::Sense::click());
    let c = crate::design::motion::color(ui.ctx(), resp.id, resp.hovered() || resp.has_focus(), t.text_strong, t.accent);
    ui.painter().galley(rect.min, g, c);
    let label = text.to_string();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, true, &label));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The mods tab's top line: the active set (and whether the mods still match it), the
/// sets dialog, and the search for the breaking mod. `found` is the mods with their
/// keys and titles, once read.
pub fn bar(app: &mut App, ui: &mut Ui, inst: &Instance, found: Option<(&[Entry], &HashMap<String, String>)>) {
    let t = app.look.tokens.clone();
    let ctx = ui.ctx().clone();
    let empty = HashMap::new();
    let names = found.map_or(&empty, |(_, n)| n);
    // While a search runs its banner says so, and the mods are the search's until it ends.
    if app.mod_sets.bisect(inst).is_some() {
        return;
    }
    // Nothing until the files have been read (a frame or two).
    let Some(sets) = app.mod_sets.sets(inst) else { return };
    let busy = switch_blocked(app, inst);
    let sets = match sets {
        Ok(s) => s,
        Err(e) => {
            super::content::padded(ui, |ui| kit::text::body_colored(ui, &format!("Couldn't read the mod sets: {e}"), t.error()));
            ui.add_space(tk::GAP);
            return;
        }
    };
    // A mod switched on that the active set hasn't seen (just installed, say) joins it.
    if busy.is_none()
        && !app.mod_sets.absorb_failed.contains(&inst.id)
        && let Some((entries, _)) = found
        && sets.active().is_some_and(|s| entries.iter().any(|e| e.file.enabled && !s.seen(&e.key)))
        && !edit_sets(app, inst, |s| {
            s.absorb(entries);
            Ok(())
        })
    {
        app.mod_sets.absorb_failed.insert(inst.id.clone());
    }
    let modified = found.is_some_and(|(e, _)| sets.modified(e));
    let mut open = false;
    let (mut update, mut revert, mut find) = (false, false, false);
    let tools = [kit::Button::new("sets"), kit::Button::new("find breaking mod")];
    let fixes = [kit::Button::new("update set"), kit::Button::new("revert")];
    let show_fixes = modified && busy.is_none();
    super::content::padded(ui, |ui| {
        let row_w = |bs: &[kit::Button]| bs.iter().map(|b| b.width(ui, &t)).sum::<f32>() - tk::LINE;
        // The set's side: its name and, when the mods differ from it, what to do about that.
        let left_w = match sets.active() {
            Some(set) => {
                let name = ui.painter().layout_no_wrap(set.name.clone(), crate::design::fonts::heading(tk::SIZE_HEADING_SMALL), t.text_strong).size().x;
                let fix = if modified { kit::text::caption_galley(ui, "modified", t.text_muted).size().x + tk::GAP + if show_fixes { row_w(&fixes) + tk::GAP } else { 0.0 } } else { 0.0 };
                kit::text::caption_galley(ui, "mod set", t.text_muted).size().x + tk::GAP + name + tk::GAP + fix
            }
            None => tk::LIST_TITLE_MIN * 2.0,
        };
        // Too narrow for both sides on one line: the sets and the search go on a line of their own.
        let one_line = left_w + tk::GUTTER + row_w(&tools) <= ui.available_width();
        let tool_row = |ui: &mut Ui| kit::button_row(ui, &t, &tools);
        let mut tool = None;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), tk::BUTTON_HEIGHT), Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = tk::GAP;
            if one_line {
                tool = tool_row(ui);
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = tk::GAP;
                kit::caption(ui, &t, "mod set");
                match sets.active() {
                    Some(set) => {
                        open |= link_text(ui, &t, &set.name, crate::design::fonts::heading(tk::SIZE_HEADING_SMALL)).clicked();
                        if modified {
                            kit::caption_colored(ui, "modified", t.warning()).on_hover_text("the enabled mods differ from the set: update the set, or revert the mods");
                            if show_fixes {
                                match kit::button_row(ui, &t, &fixes) {
                                    Some(0) => update = true,
                                    Some(_) => revert = true,
                                    None => {}
                                }
                            }
                        }
                    }
                    None if sets.sets.is_empty() => {
                        ui.add(egui::Label::new(egui::RichText::new("None yet. Save these mods as one to switch between arrangements.").font(crate::design::fonts::text(tk::SIZE_BODY)).color(t.text_muted)).truncate());
                    }
                    None => {
                        ui.add(egui::Label::new(egui::RichText::new(format!("None chosen · {} saved", sets.sets.len())).font(crate::design::fonts::text(tk::SIZE_BODY)).color(t.text_muted)).truncate());
                    }
                }
            });
        });
        if !one_line {
            ui.add_space(tk::GAP);
            tool = ui.horizontal(|ui| tool_row(ui)).inner;
        }
        match tool {
            Some(0) => open = true,
            Some(_) => find = true,
            None => {}
        }
    });
    ui.add_space(tk::GAP);

    // Mods the active set has never seen are off; say so once, with a way to add them.
    if let (Some(set), Some((entries, _))) = (sets.active(), found) {
        let unseen = set.unseen(entries);
        if !unseen.is_empty() && busy.is_none() {
            let (mut add, mut keep) = (false, false);
            super::content::padded(ui, |ui| {
                ui.allocate_ui_with_layout(vec2(ui.available_width(), tk::BUTTON_HEIGHT), Layout::right_to_left(Align::Center), |ui| {
                    match kit::button_row(ui, &t, &[kit::Button::new("turn them on"), kit::Button::new("keep them off")]) {
                        Some(0) => add = true,
                        Some(_) => keep = true,
                        None => {}
                    }
                    ui.add_space(tk::GAP);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        let what = if unseen.len() == 1 { "1 mod is".to_string() } else { format!("{} mods are", unseen.len()) };
                        let text = format!("{what} new to “{}” and off: {}", set.name, list_titles(names, &unseen));
                        ui.add(egui::Label::new(egui::RichText::new(text).font(crate::design::fonts::text(tk::SIZE_BODY)).color(t.text_strong)).truncate());
                    });
                });
            });
            ui.add_space(tk::GAP);
            if add {
                let paths: Vec<_> = entries.iter().filter(|e| unseen.contains(&e.key)).map(|e| e.file.path.clone()).collect();
                let mut failed = None;
                for p in paths {
                    if let Err(e) = mod_sets::rename(&p, true) {
                        failed = Some(format!("{e:#}"));
                    }
                }
                if let Some(e) = failed {
                    app.toast(ToastKind::Error, e);
                }
                // Now on, so they join the active set.
                let now: Vec<Entry> = entries
                    .iter()
                    .cloned()
                    .map(|mut e| {
                        e.file.enabled |= unseen.contains(&e.key);
                        e
                    })
                    .collect();
                edit_sets(app, inst, |s| {
                    s.absorb(&now);
                    Ok(())
                });
                app.content.invalidate();
            }
            if keep {
                edit_sets(app, inst, |s| {
                    s.keep_off(&unseen);
                    Ok(())
                });
            }
        }
    }

    if let Some(why) = busy.filter(|_| update || revert) {
        app.toast(ToastKind::Error, format!("Can't change the mods while {why}"));
    } else if update && let (Some(set), Some((entries, _))) = (sets.active(), found) {
        let name = set.name.clone();
        if edit_sets(app, inst, |s| s.update(&name, entries)) {
            app.toast(ToastKind::Info, format!("Saved the enabled mods to “{name}”"));
        }
    } else if revert && let Some(set) = sets.active() {
        switch(app, &ctx, inst, &set.name);
    }
    if open || app.content.key == Some(Action::ModSets) {
        app.content.key = None;
        open_sets(app, &ctx, inst, names.clone());
    }
    if find || app.content.key == Some(Action::FindBreaking) {
        app.content.key = None;
        open_bisect(app, &inst.id, Vec::new());
    }
}

/// Switch `inst`'s mods to set `name` in the background.
fn switch(app: &mut App, ctx: &egui::Context, inst: &Instance, name: &str) {
    if let Some(why) = switch_blocked(app, inst) {
        app.toast(ToastKind::Error, format!("Can't switch sets while {why}"));
        return;
    }
    let name = name.to_string();
    job(app, ctx, inst, "switching mods", move |i| {
        let c = mod_sets::switch(i, &name)?;
        Ok(if c.is_empty() { format!("The mods already match “{name}”") } else { format!("Switched to “{name}”: {}", c.summary()) })
    });
}

// -- the sets dialog -------------------------------------------------------------

pub struct SetsDialog {
    inst: Instance,
    /// The mods with their keys, read from the jars when the dialog opens.
    scan: Option<Promise<(Vec<Entry>, Needs)>>,
    entries: Option<Vec<Entry>>,
    /// Each mod's key → the installed mods it requires, so ticking one ticks those too.
    needs: Needs,
    names: HashMap<String, String>,
    selected: usize,
    new_name: String,
    rename: String,
    renaming: Option<String>,
    focus_new: bool,
    focus_rename: bool,
    /// The keyboard is on the selected set's mods (else on the sets).
    in_mods: bool,
    mod_sel: usize,
    filter: String,
    focus_filter: bool,
    /// What the last tick did besides itself ("also on: Fabric API, which Mod Menu needs").
    note: Option<String>,
}

type Needs = BTreeMap<String, BTreeSet<String>>;

/// Open the sets dialog for `inst`, with mod titles (by key) for its lists.
pub fn open_sets(app: &mut App, ctx: &egui::Context, inst: &Instance, names: HashMap<String, String>) {
    let i = inst.clone();
    let scan = Promise::spawn(&app.rt, ctx, async move {
        Ok(tokio::task::spawn_blocking(move || {
            let entries = mod_sets::scan(&i);
            let needs = mod_sets::needs(&entries);
            (entries, needs)
        })
        .await?)
    });
    let sets = app.mod_sets.sets(inst).and_then(Result::ok).unwrap_or_default();
    let selected = sets.sets.iter().position(|s| s.name == sets.active).unwrap_or(0);
    app.modal = Some(Modal::ModSets(Box::new(SetsDialog {
        inst: inst.clone(),
        scan: Some(scan),
        entries: None,
        needs: Needs::new(),
        names,
        selected,
        new_name: String::new(),
        rename: String::new(),
        renaming: None,
        focus_new: sets.sets.is_empty(),
        focus_rename: false,
        in_mods: false,
        mod_sel: 0,
        filter: String::new(),
        focus_filter: false,
        note: None,
    })));
}

/// The sets dialog. Returns true when it should close.
pub fn sets_dialog(app: &mut App, s: &mut SetsDialog, ui: &mut Ui) -> bool {
    let t = app.look.tokens.clone();
    let ctx = ui.ctx().clone();
    if let Some(p) = &s.scan
        && let Some(r) = p.take()
    {
        let (entries, needs) = r.unwrap_or_default();
        s.entries = Some(entries);
        s.needs = needs;
        s.scan = None;
    }
    let inst = s.inst.clone();
    let sets = app.mod_sets.sets(&inst).and_then(Result::ok).unwrap_or_default();
    s.selected = s.selected.min(sets.sets.len().saturating_sub(1));
    let current = sets.sets.get(s.selected).cloned();
    if current.as_ref().map(|c| &c.name) != s.renaming.as_ref() {
        s.renaming = current.as_ref().map(|c| c.name.clone());
        s.rename = s.renaming.clone().unwrap_or_default();
        s.note = None;
    }
    // The selected set's checklist: every installed mod once (by key), the filter applied.
    let rows: Vec<ModRow> = match (&current, &s.entries) {
        (Some(set), Some(entries)) => mod_rows(set, entries, &s.names, &s.needs, &s.filter),
        _ => Vec::new(),
    };

    kit::heading(ui, &t, "Mod sets");
    ui.add_space(tk::UNIT);
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(tk::SETTING_DESCRIPTION_WIDTH));
        kit::body_muted(ui, &t, &format!("Named arrangements of {}'s mods: tick a set's mods on the right. Switching to a set turns mods on and off; nothing is removed.", inst.name));
    });
    ui.add_space(tk::GUTTER);

    let kb = app.modal_nav();
    let key = |n: Nav| kb == Some(n);
    match kb {
        Some(Nav::Right) if current.is_some() => s.in_mods = true,
        Some(Nav::Left) => s.in_mods = false,
        _ => {}
    }
    s.mod_sel = s.mod_sel.min(rows.len().saturating_sub(1));
    let moved = kb.filter(|n| n.is_move());
    if s.in_mods {
        if let Some(n) = moved.and_then(|n| n.step(rows.len(), Some(s.mod_sel))) {
            s.mod_sel = n;
        }
    } else if let Some(n) = moved.and_then(|n| n.step(sets.sets.len(), Some(s.selected))) {
        s.selected = n;
    }
    let mut tick = if s.in_mods && key(Nav::Toggle) { rows.get(s.mod_sel).map(|r| (r.key.clone(), !r.on)) } else { None };
    s.focus_filter |= key(Nav::Search) && current.is_some();
    s.focus_new |= key(Nav::Add);
    s.focus_rename |= key(Nav::Rename) && current.is_some();
    let blocked = switch_blocked(app, &inst);
    let mut close = false;
    let (mut do_switch, mut do_update, mut do_delete) = (key(Nav::Activate), key(Nav::Copy), key(Nav::Delete));

    // Two panes sharing a hairline, down to the footer.
    let footer = tk::BUTTON_HEIGHT + tk::GUTTER * 2.0;
    let area = ui.available_rect_before_wrap();
    let area = egui::Rect::from_min_size(area.min, vec2(area.width(), (area.height() - footer).max(tk::SETTING_ROW_HEIGHT * 4.0)));
    let left_w = tk::SETS_PANE_WIDTH.min((area.width() * 0.4).floor());
    let line_x = area.left() + left_w;
    let left = egui::Rect::from_min_max(area.min, egui::pos2(line_x, area.bottom()));
    let right = egui::Rect::from_min_max(egui::pos2(line_x + tk::LINE, area.top()), area.max);

    // -- left: the sets, and a new one from the mods as they are
    ui.scope_builder(egui::UiBuilder::new().max_rect(left).layout(Layout::top_down(Align::Min)), |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let count = sets.sets.len().to_string();
        kit::SectionLabel::new(1, "sets").aside(&count).show(ui, &t);
        let list_h = (ui.available_height() - tk::FIELD_HEIGHT - tk::GAP * 2.0).max(tk::LIST_ROW_HEIGHT_COMPACT);
        egui::ScrollArea::vertical().id_salt("mod-sets").max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if sets.sets.is_empty() {
                super::content::padded(ui, |ui| kit::loading(ui, &t, "none yet"));
            }
            let mut list = kit::HoverList::begin(ui, kit::Variant::Compact).indent(tk::GUTTER + tk::HOVER_SHIFT_COMPACT);
            for (n, set) in sets.sets.iter().enumerate() {
                let on = n == s.selected;
                let desc = format!("{} on", set.on.len());
                let mut row = kit::HoverRow::new(("mod-set", &set.name), &set.name).description(&desc).selected(on).hot(on && !s.in_mods);
                if set.name == sets.active {
                    row = row.badge("active", t.ok());
                }
                let r = list.row(ui, &t, row);
                if on && !s.in_mods && moved.is_some() {
                    r.scroll_to_me(None);
                }
                if r.clicked() {
                    s.selected = n;
                    s.in_mods = false;
                }
                if r.double_clicked() {
                    do_switch = true;
                }
            }
            list.end(ui);
        });
        ui.add_space(tk::GAP);
        super::content::padded(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = tk::GAP;
                let ok = !s.new_name.trim().is_empty() && s.entries.is_some();
                let add_w = kit::Button::new("add").width(ui, &t);
                let r = kit::Field::new(&mut s.new_name).hint("New set from these mods").width(ui.available_width() - add_w - tk::GAP).focus(std::mem::take(&mut s.focus_new)).show(ui, &t);
                let add = kit::Button::new("add").enabled(ok).show(ui, &t).clicked() || (ok && r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if add && let Some(entries) = &s.entries {
                    let name = s.new_name.trim().to_string();
                    if edit_sets(app, &inst, |x| x.add(&name, entries)) {
                        s.new_name.clear();
                        s.selected = sets.sets.len();
                    }
                }
            });
        });
    });
    kit::vline(ui.painter(), line_x, area.y_range(), t.line_strong);

    // -- right: the selected set
    ui.scope_builder(egui::UiBuilder::new().max_rect(right).layout(Layout::top_down(Align::Min)), |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let Some(set) = &current else {
            super::content::padded(ui, |ui| kit::empty_state(ui, &t, "Save your first set", "Name a set on the left. A new set starts with the mods that are on now; tick or untick mods to change it.", None));
            return;
        };
        let count = s.entries.as_ref().map(|entries| {
            let keys: BTreeSet<&String> = entries.iter().map(|e| &e.key).collect();
            format!("{} of {} on", keys.iter().filter(|k| set.on.contains(**k)).count(), keys.len())
        });
        kit::SectionLabel::new(2, &set.name).aside(count.as_deref().unwrap_or_default()).show(ui, &t);
        ui.add_space(tk::GAP);
        let mut all: Option<bool> = None;
        super::content::padded(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = tk::GAP;
                let rename_w = kit::Button::new("rename").width(ui, &t);
                let r = kit::Field::new(&mut s.rename).label("name").width(ui.available_width() - rename_w - tk::GAP).focus(std::mem::take(&mut s.focus_rename)).show(ui, &t);
                let new = s.rename.trim().to_string();
                let ok = !new.is_empty() && new != set.name;
                let go = kit::Button::new("rename").enabled(ok).show(ui, &t).clicked() || (ok && r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if go && edit_sets(app, &inst, |x| x.rename(&set.name, &new)) {
                    s.renaming = Some(new);
                }
            });
            ui.add_space(tk::GAP);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = tk::GAP;
                let joined = [kit::Button::new("all"), kit::Button::new("none")];
                let joined_w = joined.iter().map(|b| b.width(ui, &t)).sum::<f32>() - tk::LINE;
                if s.entries.as_ref().is_some_and(|e| e.len() > 8) || !s.filter.is_empty() {
                    kit::Field::new(&mut s.filter).label("filter").width(ui.available_width() - joined_w - tk::GAP).focus(std::mem::take(&mut s.focus_filter)).show(ui, &t);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| match kit::button_row(ui, &t, &joined) {
                    Some(0) => all = Some(true),
                    Some(_) => all = Some(false),
                    None => {}
                });
            });
        });
        ui.add_space(tk::GAP);
        kit::rule(ui, &t, false);
        // Room under the list for the note, what switching does, and the buttons.
        let below = tk::BUTTON_HEIGHT + tk::GUTTER * 3.0 + tk::SIZE_BODY * 3.0;
        match &s.entries {
            None => {
                super::content::padded(ui, |ui| kit::loading(ui, &t, "reading the mods"));
            }
            Some(_) => {
                egui::ScrollArea::vertical().id_salt("mod-set-mods").max_height((ui.available_height() - below).max(tk::TABLE_ROW_HEIGHT * 3.0)).auto_shrink([false, true]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    if rows.is_empty() {
                        super::content::padded(ui, |ui| kit::loading(ui, &t, if s.filter.is_empty() { "no mods installed" } else { "no mods match the filter" }));
                    }
                    for (n, r) in rows.iter().enumerate() {
                        let sel = s.in_mods && n == s.mod_sel;
                        // A mod a ticked one needs, unticked: the row wants a look.
                        let (aside, color) = if !r.needed_by.is_empty() {
                            (needed_label(&r.needed_by), None)
                        } else if r.on != r.now_on {
                            let what = if r.on { "turns on" } else { "turns off" };
                            (if r.seen { what.to_string() } else { format!("new · {what}") }, r.on.then(|| t.ok()))
                        } else if !r.seen {
                            ("new".to_string(), None)
                        } else {
                            (String::new(), None)
                        };
                        let (row, _) = kit::QuietRow::new(("set-mod", &r.key), &r.title).check(r.on).aside(&aside, color).attention(!r.needed_by.is_empty()).hot(sel).selected(sel).show(ui, &t, |_| ());
                        if sel && moved.is_some() {
                            row.scroll_to_me(None);
                        }
                        if row.clicked() {
                            s.in_mods = true;
                            s.mod_sel = n;
                            tick = Some((r.key.clone(), !r.on));
                        }
                    }
                });
            }
        }
        ui.add_space(tk::GAP);
        super::content::padded(ui, |ui| {
            if let Some(note) = &s.note {
                kit::body_muted(ui, &t, note);
                ui.add_space(tk::UNIT);
            }
            if let Some(on) = all {
                let keys: Vec<String> = rows.iter().map(|r| r.key.clone()).collect();
                let keys: Vec<String> = if on { mod_sets::with_needs(&keys, &s.needs).into_iter().collect() } else { keys };
                if edit_sets(app, &inst, |x| x.set_mods(&set.name, &keys, on)) {
                    s.note = None;
                }
            }
            if let Some((k, on)) = tick.take() {
                // Ticking a mod ticks what it needs; unticking one that others need only flags it (on its row).
                let keys: Vec<String> = if on { mod_sets::with_needs(std::slice::from_ref(&k), &s.needs).into_iter().filter(|x| *x == k || !set.on.contains(x)).collect() } else { vec![k.clone()] };
                if edit_sets(app, &inst, |x| x.set_mods(&set.name, &keys, on)) {
                    let also: Vec<String> = keys.iter().filter(|x| **x != k).cloned().collect();
                    s.note = (!also.is_empty()).then(|| format!("Also ticked {}, which {} needs.", list_titles(&s.names, &also), title_of(&s.names, &k)));
                }
            }

            // What switching to the set (or, for the active one, applying it) would do; nothing when it matches.
            let c = s.entries.as_ref().map(|e| set.change(e));
            let active = set.name == sets.active;
            if let Some(c) = c.as_ref().filter(|c| !c.is_empty()) {
                kit::body(ui, &t, &format!("{} {}.", if active { "Applying it" } else { "Switching" }, c.summary()));
                ui.add_space(tk::UNIT);
            }
            if let Some(why) = blocked {
                kit::text::body_colored(ui, &format!("Can't switch while {why}."), t.warning());
                ui.add_space(tk::UNIT);
            }
            let changes = c.as_ref().is_some_and(|c| !c.is_empty());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = tk::GAP;
                if changes {
                    do_switch |= kit::Button::new(if active { "apply" } else { "switch" }).filled().enabled(blocked.is_none()).show(ui, &t).clicked();
                }
                do_update |= kit::Button::new(SAVE_INTO).enabled(s.entries.is_some() && changes).show(ui, &t).clicked();
                do_delete |= kit::Button::new("delete").show(ui, &t).clicked();
            });
        });
    });
    ui.allocate_rect(area, egui::Sense::hover());
    kit::rule(ui, &t, true);
    ui.add_space(tk::GAP);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = tk::GUTTER;
        let k = |a| app.keymap.label(a);
        kit::key_hint(ui, &t, &k(Action::Activate), "switch");
        kit::key_hint(ui, &t, &format!("{}/{}", k(Action::FocusLeft), k(Action::FocusRight)), "sets/mods");
        if s.in_mods {
            kit::key_hint(ui, &t, &k(Action::Toggle), "tick");
        }
        kit::key_hint(ui, &t, &k(Action::Add), "new");
        kit::key_hint(ui, &t, &k(Action::Rename), "rename");
        kit::key_hint(ui, &t, &k(Action::Copy), SAVE_INTO);
        kit::key_hint(ui, &t, &k(Action::Delete), "delete");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| close = kit::Button::new("close").show(ui, &t).clicked());
    });

    let Some(set) = current else { return close };
    if do_switch {
        if let Some(why) = blocked {
            app.toast(ToastKind::Error, format!("Can't switch sets while {why}"));
        } else if s.entries.is_some() {
            switch(app, &ctx, &inst, &set.name);
            return true;
        }
    }
    if do_update && let Some(entries) = &s.entries {
        if let Some(why) = blocked.filter(|_| set.name == sets.active) {
            app.toast(ToastKind::Error, format!("Can't change the active set while {why}"));
        } else if edit_sets(app, &inst, |x| x.update(&set.name, entries)) {
            app.toast(ToastKind::Info, format!("Saved the enabled mods to “{}”", set.name));
        }
    }
    if do_delete {
        app.modal = Some(Modal::Confirm(Confirm {
            title: "delete mod set".into(),
            body: format!("Delete the set “{}”? The mods stay as they are; only its name and list go.", set.name),
            button: "delete".into(),
            action: ConfirmAction::DeleteModSet(inst.id.clone(), set.name.clone()),
        }));
        return true;
    }
    close
}

/// One mod in the sets dialog's checklist.
struct ModRow {
    key: String,
    title: String,
    /// Ticked in the set.
    on: bool,
    /// Enabled in the mods folder now.
    now_on: bool,
    seen: bool,
    /// Ticked mods that require this one, when it isn't ticked.
    needed_by: Vec<String>,
}

/// "needed by Mod Menu", "needed by Mod Menu, Jade", "needed by Mod Menu + 5 more".
fn needed_label(by: &[String]) -> String {
    match by {
        [a] => format!("needed by {a}"),
        [a, b] => format!("needed by {a}, {b}"),
        [a, rest @ ..] => format!("needed by {a} + {} more", rest.len()),
        [] => String::new(),
    }
}

fn mod_rows(set: &mod_sets::ModSet, entries: &[Entry], names: &HashMap<String, String>, needs: &Needs, filter: &str) -> Vec<ModRow> {
    let mut by_key: BTreeMap<&str, bool> = BTreeMap::new();
    for e in entries {
        *by_key.entry(&e.key).or_default() |= e.file.enabled;
    }
    let filter = filter.trim().to_lowercase();
    let mut rows: Vec<ModRow> = by_key
        .into_iter()
        .map(|(k, now_on)| {
            let on = set.on.contains(k);
            let needed_by = if on { Vec::new() } else { set.on.iter().filter(|o| needs.get(*o).is_some_and(|n| n.contains(k))).map(|o| title_of(names, o)).collect() };
            ModRow { key: k.to_string(), title: title_of(names, k), on, now_on, seen: set.seen(k), needed_by }
        })
        .filter(|r| filter.is_empty() || r.title.to_lowercase().contains(&filter) || r.key.contains(&filter))
        .collect();
    rows.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    rows
}

/// Delete set `name` of instance `id` (confirmed already).
pub fn delete_set(app: &mut App, id: &str, name: &str) {
    let Some(inst) = app.instances.iter().find(|i| i.id == id).cloned() else { return };
    if edit_sets(app, &inst, |s| {
        s.remove(name);
        Ok(())
    }) {
        app.toast(ToastKind::Info, format!("Deleted the set “{name}”"));
    }
}

// -- the search for the breaking mod --------------------------------------------

pub enum BisectDialog {
    Start(StartDialog),
    Ask(AskDialog),
}

pub struct StartDialog {
    inst: Instance,
    rows: Vec<Row>,
    selected: usize,
}

struct Row {
    file: String,
    title: String,
    mark: Mark,
    /// Named by whoever opened the dialog (a crash report): tried first.
    first: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Mark {
    Test,
    Fine,
    Keep,
}

impl Mark {
    fn next(self) -> Mark {
        match self {
            Mark::Test => Mark::Fine,
            Mark::Fine => Mark::Keep,
            Mark::Keep => Mark::Test,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Mark::Test => "test",
            Mark::Fine => "known fine",
            Mark::Keep => "keep on",
        }
    }
}

pub struct AskDialog {
    inst_id: String,
    /// What suggests the problem happened (a crash), if anything.
    why: Option<String>,
    choice: usize,
}

/// Start finding the breaking mod in instance `id`: opens the start dialog, where every
/// enabled mod is to be tested unless marked fine or kept on. `first` names files (as
/// enabled, e.g. `sodium-0.6.0.jar`) to try first, such as a crash report's suspects;
/// they're listed at the top. If a search is already running, asks about its step instead.
pub fn open_bisect(app: &mut App, id: &str, first: Vec<String>) {
    let Some(inst) = app.instances.iter().find(|i| i.id == id).cloned() else { return };
    if inst.loader.modrinth().is_none() {
        app.toast(ToastKind::Error, "Vanilla instances don't load mods");
        return;
    }
    if app.mod_sets.search_known(&inst).is_none() {
        app.toast(ToastKind::Info, "Still reading the mods' state; try again in a moment");
        return;
    }
    if let Some(b) = app.mod_sets.bisect(&inst) {
        if b.done.is_none() {
            app.modal = Some(Modal::Bisect(Box::new(BisectDialog::Ask(AskDialog { inst_id: inst.id.clone(), why: None, choice: 0 }))));
        }
        return;
    }
    let first: Vec<String> = first.iter().map(|f| bisect::base(f)).collect();
    let mut rows: Vec<Row> = inst
        .mods()
        .into_iter()
        .filter(|f| f.enabled && !f.is_dir)
        .map(|f| Row { title: app.content.title(&f).unwrap_or_else(|| f.display_name().to_string()), first: first.contains(&f.name), file: f.name, mark: Mark::Test })
        .collect();
    rows.sort_by(|a, b| b.first.cmp(&a.first).then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())));
    app.modal = Some(Modal::Bisect(Box::new(BisectDialog::Start(StartDialog { inst, rows, selected: 0 }))));
}

/// The search's dialogs. Returns true when it should close.
pub fn bisect_dialog(app: &mut App, d: &mut BisectDialog, ui: &mut Ui) -> bool {
    match d {
        BisectDialog::Start(s) => start_dialog(app, s, ui),
        BisectDialog::Ask(a) => ask_dialog(app, a, ui),
    }
}

/// About how many steps a search over `n` suspects takes (halving, then one to confirm).
fn steps_for(n: usize) -> usize {
    if n <= 1 { 1 } else { (usize::BITS - (n - 1).leading_zeros()) as usize + 1 }
}

/// The worlds warning, before a search starts.
fn worlds_note(ui: &mut Ui, t: &Tokens) {
    kit::bracketed(ui, t, "before you start", |ui| {
        kit::body(ui, t, "Test from the title screen or a new world: a world opened with mods missing can lose what those mods added. Asteria backs a world up first when that might happen.");
    });
}

fn start_dialog(app: &mut App, s: &mut StartDialog, ui: &mut Ui) -> bool {
    let t = app.look.tokens.clone();
    let ctx = ui.ctx().clone();
    ui.set_max_width(ui.available_width().min(tk::SHEET_WIDTH));
    let kb = app.modal_nav();
    let moved = kb.filter(|n| n.is_move()).and_then(|n| n.step(s.rows.len(), Some(s.selected)));
    if let Some(n) = moved {
        s.selected = n;
    }
    if kb == Some(Nav::Toggle)
        && let Some(r) = s.rows.get_mut(s.selected)
    {
        r.mark = r.mark.next();
    }
    let testing = s.rows.iter().filter(|r| r.mark == Mark::Test).count();

    kit::heading(ui, &t, "Find breaking mod");
    ui.add_space(tk::UNIT);
    kit::body(ui, &t, "Asteria turns about half the mods off at a time. You play, then say whether the problem occurred, and the suspects halve each time.");
    ui.add_space(tk::UNIT);
    let about = if testing == 0 { "Mark at least one mod to test.".to_string() } else { format!("{testing} mods to test: about {} games. Your mods go back exactly as they are now when the search ends.", steps_for(testing)) };
    kit::body_muted(ui, &t, &about);
    ui.add_space(tk::GUTTER);
    worlds_note(ui, &t);
    ui.add_space(tk::GUTTER);
    ui.spacing_mut().item_spacing.y = 0.0;
    let aside = format!("{testing} to test");
    kit::SectionLabel::new(1, "the enabled mods").aside(&aside).show(ui, &t);
    let height = (ui.available_height() - tk::BUTTON_HEIGHT - tk::GUTTER * 3.0).max(tk::TABLE_ROW_HEIGHT * 4.0);
    egui::ScrollArea::vertical().max_height(height).auto_shrink([false, true]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if s.rows.is_empty() {
            super::content::padded(ui, |ui| kit::loading(ui, &t, "no mods are on"));
        }
        for (n, r) in s.rows.iter_mut().enumerate() {
            let on = n == s.selected;
            // Each mod's part in the search, as three joined buttons.
            let aside = if r.first { "suspect" } else { "" };
            let (row, _) = kit::QuietRow::new(("bisect-mod", &r.file), &r.title).tall().aside(aside, None).hot(on).selected(on).show(ui, &t, |ui| {
                kit::segmented(ui, &t, &mut r.mark, &[(Mark::Test, Mark::Test.label()), (Mark::Fine, Mark::Fine.label()), (Mark::Keep, Mark::Keep.label())]);
            });
            if on && moved.is_some() {
                row.scroll_to_me(None);
            }
            if row.clicked() {
                s.selected = n;
            }
        }
    });
    ui.add_space(tk::GUTTER);
    let mut go = kb == Some(Nav::Activate);
    let mut close = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = tk::GAP;
        go |= kit::Button::new("start").filled().enabled(testing > 0).show(ui, &t).clicked();
        close = kit::Button::new("cancel").show(ui, &t).clicked();
        ui.add_space(tk::GAP);
        kit::key_hint(ui, &t, &app.keymap.label(Action::Toggle), "next mark");
    });
    if go && testing > 0 {
        let inst = s.inst.clone();
        if let Some(why) = switch_blocked(app, &inst) {
            app.toast(ToastKind::Error, format!("Can't start while {why}"));
            return false;
        }
        let of = |m: Mark| s.rows.iter().filter(|r| r.mark == m).map(|r| r.file.clone()).collect::<Vec<_>>();
        let opts = bisect::Start { keep: of(Mark::Keep), good: of(Mark::Fine), first: s.rows.iter().filter(|r| r.first).map(|r| r.file.clone()).collect() };
        job(app, &ctx, &inst, "finding breaking mod", move |i| {
            let b = bisect::start(i, &opts)?;
            let on = b.step.as_ref().map_or(0, |s| s.on.len());
            Ok(format!("Step 1: {on} of {} mods on. Play, then say whether the problem happened.", b.original.values().filter(|o| **o).count()))
        });
        app.inst_tab = super::instances::InstTab::Mods;
        return true;
    }
    close
}

const ANSWERS: [(Answer, &str, &str); 3] = [
    (Answer::Broken, "Problem occurred", "The problem showed up, or the game crashed."),
    (Answer::Works, "Problem did not occur", "The game ran fine."),
    (Answer::Skip, "Can't tell", "Try a different half of the mods instead."),
];

fn ask_dialog(app: &mut App, a: &mut AskDialog, ui: &mut Ui) -> bool {
    let t = app.look.tokens.clone();
    let ctx = ui.ctx().clone();
    ui.set_max_width(ui.available_width().min(tk::SHEET_WIDTH));
    let Some(inst) = app.instances.iter().find(|i| i.id == a.inst_id).cloned() else { return true };
    let Some(b) = app.mod_sets.bisect(&inst).filter(|b| b.done.is_none()) else { return true };
    let kb = app.modal_nav();
    if let Some(n) = kb.filter(|n| n.is_move()).and_then(|n| n.step(ANSWERS.len(), Some(a.choice))) {
        a.choice = n;
    }
    let step = b.step.clone();
    kit::heading(ui, &t, "Did the problem occur?");
    ui.add_space(tk::GUTTER);
    if let Some(why) = &a.why {
        kit::bracketed(ui, &t, "the last game", |ui| kit::body(ui, &t, why));
        ui.add_space(tk::GUTTER);
    }
    let (n, of) = b.progress();
    let on = step.as_ref().map_or(0, |s| s.on.len());
    let confirm = step.as_ref().is_some_and(|s| s.kind == StepKind::Confirm);
    let aside = if confirm { format!("checking the last suspect · {on} mods on") } else { format!("{} suspects · {on} mods on", b.suspects.len()) };
    let mut pick = kb == Some(Nav::Activate);
    kit::setting_group(ui, &t, 1, &format!("Step {n} of ~{of}"), &aside, |ui| {
        for (i, (_, label, hint)) in ANSWERS.iter().enumerate() {
            // A click only selects: nothing is sent until "answer", so a slip can be corrected.
            if kit::choice_row(ui, &t, label, hint, i == a.choice, false).clicked() {
                a.choice = i;
            }
        }
    });
    ui.add_space(tk::GUTTER);
    let mut close = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = tk::GAP;
        pick |= kit::Button::new("answer").filled().show(ui, &t).clicked();
        close = kit::Button::new("later").show(ui, &t).on_hover_text("answer later from the banner on the instance").clicked();
    });
    if pick {
        answer(app, &ctx, &inst, ANSWERS[a.choice].0);
        return true;
    }
    close
}

/// Send an answer for `inst`'s current step, then switch to the next.
fn answer(app: &mut App, ctx: &egui::Context, inst: &Instance, a: Answer) {
    if let Some(why) = busy(app, inst).search_action() {
        app.toast(ToastKind::Error, format!("Can't answer while {why}"));
        return;
    }
    let (i, rt) = (inst.clone(), app.rt.clone());
    let task = Task::new(format!("finding breaking mod in {}", inst.name));
    task.stage("switching mods", 0);
    let inbox = app.mod_sets.inbox();
    app.content.start_mod_job(&rt, ctx, &inst.id, None, task, async move {
        let done = answered(i.clone(), a).await;
        let _ = tokio::task::spawn_blocking(move || deliver(&inbox, &i)).await;
        done
    });
    app.mod_sets.forget(&inst.id);
}

/// Answer `i`'s step (off the UI thread), and name the culprits once it's over.
async fn answered(i: Instance, a: Answer) -> ModJobDone {
    let r = tokio::task::spawn_blocking({
        let i = i.clone();
        move || bisect::answer(&i, a)
    })
    .await;
    let b = match r {
        Ok(Ok(b)) => b,
        Ok(Err(e)) => return ModJobDone::new(ToastKind::Error, format!("{e:#}")),
        Err(e) => return ModJobDone::new(ToastKind::Error, e.to_string()),
    };
    let Some(done) = &b.done else {
        let (n, of) = b.progress();
        let on = b.step.as_ref().map_or(0, |s| s.on.len());
        return ModJobDone::new(ToastKind::Info, format!("Step {n} of ~{of}: {on} mods on, {} suspects. Play again.", b.suspects.len()));
    };
    // Its Modrinth page, if it has one, for "open its page".
    if !done.culprits.is_empty() {
        let dir = i.game_dir().join("mods");
        let files: Vec<(String, Option<String>)> = done.culprits.iter().map(|c| (c.file.clone(), crate::net::sha1_file(&dir.join(&c.file)))).collect();
        let hashes: Vec<String> = files.iter().filter_map(|(_, h)| h.clone()).collect();
        if let Ok(found) = crate::content_meta::lookup(hashes).await {
            let _ = tokio::task::spawn_blocking(move || {
                bisect::edit(&i, |b| {
                    for c in b.done.iter_mut().flat_map(|d| d.culprits.iter_mut()) {
                        let hash = files.iter().find(|(f, _)| *f == c.file).and_then(|(_, h)| h.as_ref());
                        if let Some(p) = hash.and_then(|h| found.get(h)) {
                            c.project = Some(p.id.clone());
                            c.name = c.name.take().or(Some(p.title.clone()));
                        }
                    }
                })
            })
            .await;
        }
    }
    match done.culprits.as_slice() {
        [] => ModJobDone::new(ToastKind::Info, "Couldn't narrow the problem to one mod; the mods tab says why".into()),
        [c] => ModJobDone::new(ToastKind::Info, format!("Breaking mod found: {}", c.label())),
        many => ModJobDone::new(ToastKind::Info, format!("The breaking mod is one of {} mods that need each other", many.len())),
    }
}

/// A game in `id` ended: if it was a step of a search, ask how it went. `exit` is its
/// exit code, or why it has none.
pub fn game_ended(app: &mut App, id: &str, exit: &Result<i32, String>) {
    let Some(inst) = app.instances.iter().find(|i| i.id == id).cloned() else { return };
    let Some(step) = app.mod_sets.bisect(&inst).filter(|b| b.done.is_none()).and_then(|b| b.step) else { return };
    let why = match exit {
        _ if bisect::crashed_since(&inst, step.began) => Some("The game wrote a crash report, so the problem most likely occurred.".to_string()),
        Ok(0) => None,
        Ok(c) => Some(format!("The game exited with code {c}, so the problem most likely occurred.")),
        Err(e) if e == "killed by signal" => None,
        Err(e) => Some(format!("The game {e}.")),
    };
    app.mod_sets.asks.retain(|(i, _)| i != id);
    app.mod_sets.asks.push((id.to_string(), why));
    show_ask(app);
}

/// Ask about the oldest ended game, if no other dialog is open.
fn show_ask(app: &mut App) {
    if app.modal.is_some() || app.mod_sets.asks.is_empty() {
        return;
    }
    let (inst_id, why) = app.mod_sets.asks.remove(0);
    let choice = if why.is_some() { 0 } else { 1 };
    app.modal = Some(Modal::Bisect(Box::new(BisectDialog::Ask(AskDialog { inst_id, why, choice }))));
}

/// The banner on an instance while a search for its breaking mod runs, or shows its result:
/// a strip under the spec strip, sharing its line.
pub fn banner(app: &mut App, ui: &mut Ui, inst: &Instance) {
    show_ask(app);
    let Some(b) = app.mod_sets.bisect(inst) else { return };
    let t = app.look.tokens.clone();
    let ctx = ui.ctx().clone();
    let blocked = busy(app, inst).search_action();
    let running = app.is_running(&inst.id);
    let busy = blocked.is_some();
    let free = !running && !busy;
    let mut act: Option<Act> = None;
    // A switch that didn't finish, or an ending that couldn't find every mod, comes first.
    if !b.unresolved.is_empty() {
        let what = list_titles(&HashMap::new(), &b.unresolved);
        let sentence = format!("Couldn't find {what} in the mods folder, so the search is kept to remember how the mods were. Put the files back and try again, or forget the search and sort them out by hand.");
        let buttons = [kit::Button::new("try again").enabled(free), kit::Button::new("forget the search").enabled(free)];
        match kit::banner(ui, &t, "find breaking mod · mods missing", Some(t.error()), &sentence, &buttons) {
            Some(0) => act = Some(Act::Recover),
            Some(_) => act = Some(Act::Forget),
            None => {}
        }
    } else if !b.applied {
        let mut buttons = vec![kit::Button::new("try again").enabled(free)];
        if b.ending.is_none() {
            buttons.push(kit::Button::new("stop and put my mods back").enabled(free));
        }
        match kit::banner(ui, &t, "find breaking mod · switch cut short", Some(t.error()), "The mods didn't all switch. Nothing is lost: the search knows how they should be.", &buttons) {
            Some(0) => act = Some(Act::Recover),
            Some(_) => act = Some(Act::Restore),
            None => {}
        }
    } else {
        match &b.done {
            None => {
                let (n, of) = b.progress();
                let on = b.step.as_ref().map_or(0, |s| s.on.len());
                let confirm = b.step.as_ref().is_some_and(|s| s.kind == StepKind::Confirm);
                let what = if confirm { "checking the last suspect".to_string() } else { format!("{} suspects left", b.suspects.len()) };
                let total = b.original.values().filter(|o| **o).count();
                let caption = format!("finding breaking mod · step {n} of ~{of} · {what} · {on} of {total} mods on");
                let say = if running { "Playing. Close the game once you've seen whether the problem occurs." } else { "Play, then say whether the problem occurred. Test from the title screen or a new world: worlds opened with mods missing can lose what those mods added." };
                // Answering goes through the dialog, where a choice can be changed before it's sent.
                let buttons = [kit::Button::new("answer").enabled(free), kit::Button::new("stop and put my mods back").enabled(!busy)];
                match kit::banner(ui, &t, &caption, None, say, &buttons) {
                    Some(0) => act = Some(Act::Ask),
                    Some(_) => act = Some(Act::Restore),
                    None => {}
                }
            }
            Some(done) => {
                let (caption, color, title) = match done.culprits.as_slice() {
                    [] => ("couldn't narrow it down", t.warning(), "Couldn't narrow the problem to one mod.".to_string()),
                    [c] => ("breaking mod found", t.ok(), format!("{} ({}).", c.label(), c.file)),
                    many => ("breaking mod found", t.ok(), format!("One of: {}.", many.iter().map(|c| c.label()).collect::<Vec<_>>().join(", "))),
                };
                let sentence = format!("{title} {}", done.note);
                let disable = (!done.culprits.is_empty()).then(|| {
                    let what = if done.culprits.len() == 1 { "mod" } else { "mods" };
                    let culprits = done.culprits.iter().map(|c| c.file.clone()).collect();
                    match b.dependents(&culprits).len() {
                        0 => format!("turn {what} off"),
                        1 => format!("turn {what} + 1 dependent off"),
                        n => format!("turn {what} + {n} dependents off"),
                    }
                });
                let mut buttons = Vec::new();
                let mut acts = Vec::new();
                if let Some(label) = &disable {
                    buttons.push(kit::Button::new(label).enabled(free));
                    acts.push(Act::Disable);
                }
                buttons.push(kit::Button::new("put my mods back").enabled(free));
                acts.push(Act::Restore);
                let page = done.culprits.iter().find_map(|c| c.project.clone());
                if page.is_some() {
                    buttons.push(kit::Button::new("modrinth page"));
                    acts.push(Act::Page);
                }
                if let Some(i) = kit::banner(ui, &t, caption, Some(color), &sentence, &buttons) {
                    match acts.swap_remove(i) {
                        Act::Page => super::open_url(&format!("https://modrinth.com/project/{}", page.unwrap_or_default())),
                        a => act = Some(a),
                    }
                }
            }
        }
    }

    // Checked again as it runs: a job may have started since the buttons were drawn.
    if let Some(why) = blocked.filter(|_| act.as_ref().is_some_and(|a| !matches!(a, Act::Ask | Act::Forget))) {
        app.toast(ToastKind::Error, format!("Can't do that while {why}"));
        return;
    }
    match act {
        Some(Act::Ask) => open_bisect(app, &inst.id, Vec::new()),
        Some(Act::Restore) => job(app, &ctx, inst, "putting the mods back", |i| {
            bisect::restore(i)?;
            Ok("Put the mods back as they were".into())
        }),
        Some(Act::Recover) => job(app, &ctx, inst, "switching mods", |i| {
            bisect::recover(i)?;
            Ok(match bisect::load(i)? {
                Some(b) if !b.unresolved.is_empty() => format!("Still couldn't find {}", b.unresolved.join(", ")),
                Some(_) => "Switched the mods for the search".into(),
                None => "Put the mods back as they were".into(),
            })
        }),
        Some(Act::Forget) => {
            app.modal = Some(Modal::Confirm(Confirm {
                title: "forget the search".into(),
                body: "Forget the search for the breaking mod? The mods stay as they are now, and the ones it couldn't find aren't put back.".into(),
                button: "forget".into(),
                action: ConfirmAction::ForgetSearch(inst.id.clone()),
            }));
        }
        Some(Act::Disable) => job(app, &ctx, inst, "putting the mods back", |i| {
            let also = bisect::disable_culprits(i)?;
            Ok(match also.len() {
                0 => "Put the mods back with the breaking mod off".into(),
                n => format!("Put the mods back with the breaking mod off, plus {n} that depend on the breaking mod: {}", also.join(", ")),
            })
        }),
        Some(Act::Page) | None => {}
    }
}

enum Act {
    Ask,
    Restore,
    Disable,
    Recover,
    Forget,
    /// Open the culprit's Modrinth page (no job).
    Page,
}

/// Forget instance `id`'s search (confirmed already), leaving its mods as they are.
pub fn forget_search(app: &mut App, ctx: &egui::Context, id: &str) {
    let Some(inst) = app.instances.iter().find(|i| i.id == id).cloned() else { return };
    if let Some(why) = busy(app, &inst).search_action() {
        app.toast(ToastKind::Error, format!("Can't forget the search while {why}"));
        return;
    }
    job(app, ctx, &inst, "forgetting the search", |i| {
        bisect::forget(i)?;
        Ok("Forgot the search; the mods stay as they are".into())
    });
}

/// The `b` key: answer the running search's step, or start one from the mods tab.
pub fn key(app: &mut App, inst_id: &str) {
    let Some(inst) = app.instances.iter().find(|i| i.id == inst_id).cloned() else { return };
    match app.mod_sets.bisect(&inst) {
        Some(b) if b.done.is_none() => open_bisect(app, inst_id, Vec::new()),
        Some(_) => {}
        None if app.inst_tab == super::instances::InstTab::Mods => app.content.key = Some(Action::FindBreaking),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_estimates() {
        assert_eq!([1, 2, 3, 4, 5, 8, 9, 14, 100].map(steps_for), [1, 2, 3, 3, 4, 4, 5, 5, 8]);
    }

    #[test]
    fn search_controls_share_one_admission_check() {
        let idle = ModsBusy { search: Some(false), ..Default::default() };
        assert!(idle.by_hand().is_none() && idle.switch().is_none() && idle.search_action().is_none());
        // The crash panel's toggles: never while a search has the mods, or before that's known.
        let searching = ModsBusy { search: Some(true), ..idle };
        assert!(searching.by_hand().is_some() && searching.switch().is_some());
        assert!(searching.search_action().is_none(), "the search's own actions still run");
        assert!(ModsBusy { search: None, ..idle }.by_hand().is_some());
        // Stop, answer or try again while a step is being switched: refused.
        let switching = ModsBusy { jobs: true, ..searching };
        assert!(switching.search_action().is_some() && switching.by_hand().is_some());
        for b in [ModsBusy { running: true, ..searching }, ModsBusy { starting: true, ..searching }, ModsBusy { installing: true, ..searching }] {
            assert!(b.search_action().is_some(), "{b:?}");
        }
        assert!(ModsBusy { starting: true, ..idle }.by_hand().is_some());
    }

    #[test]
    fn state_files_are_read_in_the_background_and_the_old_ones_stay_shown() {
        use crate::loaders::Loader;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let ctx = egui::Context::default();
        let mut inst = Instance::new("sets reads", "1.21.1", Loader::Fabric("0.16.5".into()));
        inst.assign_id().unwrap();
        let named = |name: &str| ModSets { active: name.into(), sets: vec![mod_sets::ModSet { name: name.into(), ..Default::default() }] };
        let shown = |s: &mut State| s.sets(&inst).and_then(Result::ok).map(|x| x.active);
        let wait_for = |s: &mut State, want: &str| {
            let start = Instant::now();
            while shown(s).as_deref() != Some(want) {
                assert!(start.elapsed() < Duration::from_secs(5), "never showed {want}");
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        mod_sets::save(&inst, &named("one")).unwrap();
        let mut s = State::default();
        s.tick(rt.handle(), &ctx);
        assert!(s.sets(&inst).is_none(), "nothing is read on the drawing thread");
        assert!(s.searching(&inst) && s.search_known(&inst).is_none(), "unknown counts as searching for the guards");
        wait_for(&mut s, "one");
        assert_eq!(s.search_known(&inst), Some(false));

        // Changed on disk: the old sets stay up until the new read is back.
        std::thread::sleep(Duration::from_millis(20));
        mod_sets::save(&inst, &named("two")).unwrap();
        assert_eq!(shown(&mut s).as_deref(), Some("one"));
        wait_for(&mut s, "two");

        // A mod job's read, made after its writes, is shown at once.
        mod_sets::save(&inst, &named("three")).unwrap();
        deliver(&s.inbox(), &inst);
        assert_eq!(shown(&mut s).as_deref(), Some("three"));

        // An oversized file is an error shown, not a frame held up.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(mod_sets::path(&inst), vec![b'#'; (crate::fsutil::METADATA_LIMIT + 1) as usize]).unwrap();
        let start = Instant::now();
        while !s.sets(&inst).is_some_and(|r| r.is_err()) {
            assert!(start.elapsed() < Duration::from_secs(5), "the error never showed");
            std::thread::sleep(Duration::from_millis(10));
        }
        std::fs::remove_dir_all(inst.dir()).unwrap();
    }

    #[test]
    fn marks_cycle() {
        assert!(Mark::Test.next() == Mark::Fine && Mark::Fine.next() == Mark::Keep && Mark::Keep.next() == Mark::Test);
    }
}
