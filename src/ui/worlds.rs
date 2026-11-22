//! Singleplayer worlds in `saves/` on the left (a compact hover list: you choose one), the
//! selected world's backups on the right (a quiet table: you read them), a hairline between.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Ui, vec2};

use super::content::{self, padded};
use super::nav::{Nav, Zone};
use super::{App, Confirm, ConfirmAction, Modal, ToastKind};
use crate::design::kit;
use crate::design::tokens::{self as tk, Tokens};
use crate::backups::{self, Owner, Snapshot};
use crate::instance::Instance;
use crate::launch::QuickPlay;
use crate::saves::{self, World};
use crate::task::{Promise, Task};

/// An instance's worlds and their backups, read together in the background.
#[derive(Clone, Default)]
pub struct Listing {
    worlds: Vec<World>,
    /// World folder → its snapshots, newest first. Includes worlds that are gone.
    backups: BTreeMap<String, Vec<Snapshot>>,
}

impl Listing {
    fn read(saves: PathBuf, owner: Owner) -> Self {
        backups::recover_worlds(owner).into_iter().for_each(|e| eprintln!("{e}"));
        let mut worlds = saves::list_worlds(&saves);
        for w in &mut worlds {
            w.played_in = crate::world_sets::read_record(&w.dir).map(|r| (r.instance, r.instance_name));
        }
        Listing { worlds, backups: backups::all_worlds(owner) }
    }

    /// World folders with backups but no world any more.
    fn gone(&self) -> Vec<String> {
        self.backups.keys().filter(|f| !self.worlds.iter().any(|w| &w.folder == *f)).cloned().collect()
    }
}

/// A duplicate, backup or restore running in the background.
pub struct WorldJob {
    pub task: Task,
    pub promise: Promise<JobDone>,
    pub inst_id: String,
    /// The folder its worlds really live in ([`saves_key`]), shared by a world set's members.
    pub saves: PathBuf,
}

/// The folder `inst`'s worlds really live in: its `saves`, followed through a world
/// set's link. Two instances whose keys match touch the same worlds.
pub fn saves_key(inst: &Instance) -> PathBuf {
    let saves = inst.saves_dir();
    saves.canonicalize().unwrap_or(saves)
}

/// What a background world job leaves behind: a folder to select, and a note.
pub struct JobDone {
    pub select: Option<String>,
    pub note: Option<String>,
}

#[derive(Default)]
pub struct WorldsState {
    /// Instance id and its listing, kept on screen while a refresh runs.
    list: Option<(String, Listing)>,
    /// When `list` was read; None once it's out of date.
    fetched: Option<Instant>,
    /// Background read of an instance's `saves/` and backups.
    loading: Option<(String, Promise<Listing>)>,
    /// Duplicates, backups and restores, each with the instance and saves folder it's in.
    pub jobs: Vec<WorldJob>,
    selected: Option<String>,
    /// The selected backup, by folder.
    backup: Option<PathBuf>,
    /// World dir being renamed, and the new name.
    rename: Option<(PathBuf, String)>,
}

impl WorldsState {
    pub fn invalidate(&mut self) {
        self.fetched = None;
        // A read already under way may predate the change.
        self.loading = None;
    }

    /// The instance's worlds and backups, re-read in the background every few seconds.
    /// None until the first read for this instance lands.
    fn sync(&mut self, rt: &tokio::runtime::Handle, ctx: &egui::Context, inst: &Instance) -> Option<&Listing> {
        let inst_id = inst.id.as_str();
        if let Some((id, p)) = &self.loading
            && let Some(r) = p.take()
        {
            if id == inst_id {
                self.list = Some((id.clone(), r.unwrap_or_default()));
                self.fetched = Some(Instant::now());
            }
            self.loading = None;
        }
        // Selection and a pending rename belong to the instance they were made in.
        if self.list.as_ref().is_some_and(|(id, _)| id != inst_id) {
            self.list = None;
            self.fetched = None;
            self.selected = None;
            self.backup = None;
            self.rename = None;
        }
        if self.loading.as_ref().is_some_and(|(id, _)| id != inst_id) {
            self.loading = None;
        }
        let fresh = self.fetched.is_some_and(|at| at.elapsed() < Duration::from_secs(3));
        if !fresh && self.loading.is_none() {
            let (saves, i) = (inst.saves_dir(), inst.clone());
            let p = Promise::spawn(rt, ctx, async move { Ok(tokio::task::spawn_blocking(move || Listing::read(saves, Owner::of(&i))).await?) });
            self.loading = Some((inst_id.to_string(), p));
        }
        self.list.as_ref().map(|(_, l)| l)
    }

    pub fn tasks(&self) -> impl Iterator<Item = &Task> {
        self.jobs.iter().map(|j| &j.task)
    }

    pub fn busy_with(&self, inst_id: &str) -> bool {
        self.jobs.iter().any(|j| j.inst_id == inst_id)
    }

    /// Whether a job is copying worlds in this saves folder, through any instance.
    pub fn busy_on(&self, saves: &std::path::Path) -> bool {
        self.jobs.iter().any(|j| j.saves == saves)
    }

    /// A finished job selects its world if its instance is still on screen.
    pub fn finished(&mut self, inst_id: &str, select: Option<String>) {
        if let Some(folder) = select
            && self.list.as_ref().is_some_and(|(id, _)| id == inst_id)
        {
            self.selected = Some(folder);
        }
        self.invalidate();
    }

    fn start(&mut self, rt: &tokio::runtime::Handle, ctx: &egui::Context, inst: &Instance, label: &str, stage: String, job: impl FnOnce() -> anyhow::Result<JobDone> + Send + 'static) {
        let task = Task::new(label);
        task.stage(stage, 0);
        let promise = Promise::spawn(rt, ctx, async move { tokio::task::spawn_blocking(job).await? });
        self.jobs.push(WorldJob { task, promise, inst_id: inst.id.clone(), saves: saves_key(inst) });
    }
}

/// Why the worlds in saves folder `saves` can't be copied right now: a world job or a
/// mod update's backups already run there, through whichever instance.
pub fn jobs_on(content: &super::content::ContentState, worlds: &WorldsState, saves: &std::path::Path) -> Option<&'static str> {
    if worlds.busy_on(saves) {
        Some("another world job is running")
    } else if content.updating_saves(saves) {
        Some("a mod update is backing up its worlds")
    } else {
        None
    }
}

/// Why `inst` can't start right now, if it can't: its mods are being swapped or added
/// (a mod moved in after the launch reads them would be missing from its play records),
/// or its worlds copied or swapped, through it or any instance sharing its saves.
pub fn launch_blocked(content: &super::content::ContentState, worlds: &WorldsState, inst: &Instance) -> Option<&'static str> {
    if content.updating(&inst.id) {
        return Some("its mods are being updated or rolled back");
    }
    if content.installing_mods(&inst.id) {
        return Some("mods are being added to it");
    }
    let saves = saves_key(inst);
    if worlds.busy_with(&inst.id) || worlds.busy_on(&saves) {
        Some("one of its worlds is being copied or restored")
    } else if content.updating_saves(&saves) {
        Some("a mod update is backing up its worlds")
    } else {
        None
    }
}

/// Why `inst`'s worlds can't be backed up or restored right now, if they can't. Its
/// world set's members count by where their saves really lead, not just by setting.
pub fn worlds_busy(app: &App, inst: &Instance) -> Option<String> {
    let saves = saves_key(inst);
    if let Some(why) = jobs_on(&app.content, &app.worlds, &saves) {
        return Some(why.into());
    }
    if app.sharing.is_some() {
        return Some("shared folders are moving".into());
    }
    let running = app.instances.iter().filter(|i| app.is_running(&i.id)).find(|i| i.id == inst.id || (!inst.world_set.is_empty() && i.world_set == inst.world_set) || saves_key(i) == saves);
    running.map(|i| if i.id == inst.id { "it's running".to_string() } else { format!("{} is running in the same world set", i.name) })
}

/// Restore a backup (confirmed already) in the background.
pub fn start_restore(app: &mut App, ctx: &egui::Context, id: &str, snap: Snapshot) {
    let Some(inst) = app.instances.iter().find(|i| i.id == id).cloned() else { return };
    if let Some(why) = worlds_busy(app, &inst) {
        app.toast(ToastKind::Error, format!("Can't restore while {why}"));
        return;
    }
    let folder = snap.dir.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (rt, i) = (app.rt.clone(), inst.clone());
    app.worlds.start(&rt, ctx, &inst, "restore", format!("Restoring {folder}"), move || {
        backups::restore_world(&snap, &i.saves_dir())?;
        Ok(JobDone { select: Some(folder.clone()), note: Some(format!("Restored {folder} from {}", snap.when())) })
    });
}

/// Add worlds dropped on the worlds tab (folders or zips) to the instance's saves.
pub fn add_dropped(app: &mut App, ctx: &egui::Context, inst: &Instance, paths: Vec<PathBuf>) {
    // One job per saves folder at a time, as with duplicates; not while shared folders move.
    let why = if app.sharing.is_some() { Some("shared folders are moving") } else { jobs_on(&app.content, &app.worlds, &saves_key(inst)) };
    if let Some(why) = why {
        app.toast(ToastKind::Error, format!("Can't add worlds while {why}"));
        return;
    }
    let (rt, saves) = (app.rt.clone(), inst.saves_dir());
    let title = match paths.as_slice() {
        [one] => one.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        many => format!("{} worlds", many.len()),
    };
    app.worlds.start(&rt, ctx, inst, "add world", format!("Adding {title}"), move || {
        let mut added = Vec::new();
        for p in &paths {
            match saves::add_world(p, &saves) {
                Ok(folder) => added.push(folder),
                Err(e) if added.is_empty() => return Err(e),
                Err(e) => anyhow::bail!("{e:#} (added {} first)", added.join(", ")),
            }
        }
        Ok(JobDone { select: added.first().cloned(), note: Some(format!("Added {}", added.join(", "))) })
    });
}

pub fn show(app: &mut App, ui: &mut Ui, inst: &Instance) {
    let t = app.look.tokens.clone();
    let Some(listing) = app.worlds.sync(&app.rt, ui.ctx(), inst).cloned() else {
        padded(ui, |ui| kit::loading(ui, &t, "reading worlds"));
        return;
    };
    let gone = listing.gone();
    if listing.worlds.is_empty() && gone.is_empty() {
        let go = padded(ui, |ui| kit::empty_state(ui, &t, "No worlds yet", "Create one in game, or drop a world folder or .zip here.", Some("open saves folder")));
        if go.is_some_and(|r| r.clicked()) {
            super::open_path(&inst.saves_dir());
        }
        return;
    }
    // Worlds, then the ones that only have backups left. Selection is by folder.
    let folders: Vec<String> = listing.worlds.iter().map(|w| w.folder.clone()).chain(gone.iter().cloned()).collect();
    let typing = app.worlds.rename.is_some();
    let main_kb = if typing { None } else { app.nav_for(Zone::Main) };
    let side_kb = if typing { None } else { app.nav_for(Zone::Side) };
    let zone = app.focus;
    let s = &mut app.worlds;
    let pos = s.selected.as_ref().and_then(|f| folders.iter().position(|x| x == f));
    let mut scroll = false;
    if let Some(p) = main_kb.filter(|n| n.is_move()).and_then(|n| n.step(folders.len(), pos)) {
        s.selected = Some(folders[p].clone());
        s.backup = None;
        scroll = true;
    }
    let sel_folder = s.selected.clone().filter(|f| folders.contains(f));
    let sel = sel_folder.as_ref().and_then(|f| listing.worlds.iter().find(|w| &w.folder == f)).cloned();
    let snaps: Vec<Snapshot> = sel_folder.as_ref().and_then(|f| listing.backups.get(f)).cloned().unwrap_or_default();
    // Back up now works from either pane, like the other world keys.
    let back_up_key = main_kb == Some(Nav::Add) || side_kb == Some(Nav::Add);

    let mut out = Out::default();
    let worlds = Pane { listing: &listing, gone: &gone, sel: sel.as_ref(), sel_folder: sel_folder.as_deref() };
    if ui.available_width() < tk::CONTENT_TWO_PANES {
        // Too narrow for both: one pane at a time, the one with the keyboard (h/l still switch).
        let mut side = zone == Zone::Side;
        super::content::pane_switch(ui, &t, app.tab_line.take(), &mut side, ["worlds", "backups"]);
        if side {
            backup_pane(ui, app, &worlds, &snaps, side_kb, back_up_key, zone == Zone::Side, &mut out);
            out.to_side |= zone != Zone::Side;
        } else {
            world_pane(ui, app, inst, &worlds, main_kb, back_up_key, zone == Zone::Main, scroll, &mut out);
            out.to_main |= zone == Zone::Side;
        }
    } else {
        let left_w = ui.available_width() * tk::WORLDS_SHARE;
        let (mut left, mut right) = content::panes(ui, &t, left_w);
        world_pane(&mut left, app, inst, &worlds, main_kb, false, zone == Zone::Main, scroll, &mut out);
        backup_pane(&mut right, app, &worlds, &snaps, side_kb, back_up_key, zone == Zone::Side, &mut out);
    }
    if out.to_main {
        app.focus = Zone::Main;
    }
    if out.to_side {
        app.focus = Zone::Side;
    }
    if let Some(a) = out.act {
        act(app, ui.ctx(), inst, &listing, a);
    }
}

/// What the worlds pane shows: the listing and the selection.
struct Pane<'a> {
    listing: &'a Listing,
    gone: &'a [String],
    sel: Option<&'a World>,
    sel_folder: Option<&'a str>,
}

/// What a head button, key or row menu asked for.
enum Act {
    Play(String),
    Rename(World),
    Duplicate(World),
    Delete(World),
    Folder(PathBuf),
    BackUp(World),
    Restore(String, Snapshot),
    DeleteBackup(String, Snapshot),
}

#[derive(Default)]
struct Out {
    act: Option<Act>,
    to_main: bool,
    to_side: bool,
    /// How tall the worlds pane's head came out, for the backups pane's beside it.
    head_h: f32,
}

fn act(app: &mut App, ctx: &egui::Context, inst: &Instance, listing: &Listing, a: Act) {
    match a {
        Act::Play(folder) => app.launch(ctx, &inst.id, Some(QuickPlay::World(folder))),
        Act::Rename(wd) => app.worlds.rename = Some((wd.dir.clone(), wd.name.clone())),
        Act::Duplicate(wd) => {
            // One at a time per saves folder, so two copies can't pick the same folder name.
            // Not while shared folders move: the copy could land in a folder on its way out.
            let why = if app.sharing.is_some() { Some("shared folders are moving") } else { jobs_on(&app.content, &app.worlds, &saves_key(inst)) };
            if let Some(why) = why {
                app.toast(ToastKind::Error, format!("Can't duplicate while {why}"));
                return;
            }
            let rt = app.rt.clone();
            let name = wd.name.clone();
            app.worlds.start(&rt, ctx, inst, "duplicate", format!("Duplicating {name}"), move || Ok(JobDone { select: Some(saves::duplicate_world(&wd)?), note: None }));
        }
        Act::Delete(wd) => {
            let kept = if listing.backups.contains_key(&wd.folder) { " Its backups are kept." } else { "" };
            app.modal = Some(Modal::Confirm(Confirm {
                title: "delete world".into(),
                body: format!("Permanently delete “{}”?{kept}", wd.name),
                button: "delete".into(),
                action: ConfirmAction::DeletePath(wd.dir.clone()),
            }));
        }
        Act::Folder(dir) => super::open_path(&dir),
        Act::BackUp(wd) => match worlds_busy(app, inst) {
            Some(why) => app.toast(ToastKind::Error, format!("Can't back up while {why}")),
            None => {
                let (dir, i, name) = (wd.dir.clone(), inst.clone(), wd.name.clone());
                let rt = app.rt.clone();
                app.worlds.start(&rt, ctx, inst, "back up", format!("Backing up {name}"), move || {
                    backups::snapshot_world(&dir, Owner::of(&i), "backed up by hand", false)?;
                    Ok(JobDone { select: None, note: Some(format!("Backed up {name}")) })
                });
            }
        },
        Act::Restore(folder, snap) => {
            let exists = listing.worlds.iter().any(|w| w.folder == folder);
            let body = if exists {
                format!("Put “{folder}” back as it was on {}? The world as it is now is backed up first.", snap.when())
            } else {
                format!("Bring “{folder}” back as it was on {}?", snap.when())
            };
            app.modal = Some(Modal::Confirm(Confirm { title: "restore world".into(), body, button: "restore".into(), action: ConfirmAction::RestoreWorld(inst.id.clone(), snap) }));
        }
        Act::DeleteBackup(folder, snap) => {
            app.modal = Some(Modal::Confirm(Confirm {
                title: "delete backup".into(),
                body: format!("Permanently delete the backup of “{folder}” from {}?", snap.when()),
                button: "delete".into(),
                action: ConfirmAction::DeleteBackup(snap),
            }));
        }
    }
}

/// A label inside a list that isn't a section of its own (`GONE · BACKUPS ONLY`): a caption over a strong rule.
fn list_label(ui: &mut Ui, t: &Tokens, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), tk::SECTION_LABEL_HEIGHT), egui::Sense::hover());
    let p = ui.painter();
    kit::text::paint_caption(p, egui::pos2(rect.left() + tk::GUTTER, rect.center().y), text, t.text_muted);
    kit::rule::hline(p, rect.x_range(), rect.bottom() - 0.5, t.line_strong);
}

/// The worlds as a compact hover list under their head: what's selected can be played,
/// renamed, duplicated, deleted or opened from the head, its keys or its row's menu.
#[allow(clippy::too_many_arguments)]
fn world_pane(ui: &mut Ui, app: &mut App, inst: &Instance, w: &Pane, kb: Option<Nav>, back_up_key: bool, focused: bool, scroll: bool, out: &mut Out) {
    let t = app.look.tokens.clone();
    ui.spacing_mut().item_spacing.y = 0.0;
    let key = |n: Nav| kb == Some(n);
    let dup_ok = app.sharing.is_none() && jobs_on(&app.content, &app.worlds, &saves_key(inst)).is_none();
    let n = w.listing.worlds.len();
    let mut aside = if n == 1 { "1 world".to_string() } else { format!("{n} worlds") };
    if !inst.world_set.is_empty() {
        aside = format!("{aside} · world set {}", inst.world_set);
    }
    let buttons = match w.sel {
        Some(_) => vec![kit::Button::new("play"), kit::Button::new("rename"), kit::Button::new("duplicate").enabled(dup_ok), kit::Button::new("delete"), kit::Button::new("folder")],
        None => vec![kit::Button::new("saves folder")],
    };
    let (clicked, head_h) = content::pane_head(ui, &t, 1, "worlds", &aside, &buttons, 0.0);
    out.head_h = head_h;
    if let Some(wd) = w.sel {
        let pick = |i: usize| clicked == Some(i);
        out.act = if pick(0) || key(Nav::Activate) {
            Some(Act::Play(wd.folder.clone()))
        } else if pick(1) || key(Nav::Rename) {
            Some(Act::Rename(wd.clone()))
        } else if pick(2) || key(Nav::Duplicate) {
            Some(Act::Duplicate(wd.clone()))
        } else if pick(3) || key(Nav::Delete) {
            Some(Act::Delete(wd.clone()))
        } else if pick(4) || key(Nav::Folder) {
            Some(Act::Folder(wd.dir.clone()))
        } else if back_up_key {
            Some(Act::BackUp(wd.clone()))
        } else {
            None
        };
    } else if clicked == Some(0) || key(Nav::Folder) {
        out.act = Some(Act::Folder(inst.saves_dir()));
    }

    // Renaming takes over the top of the list: the name as a field, save and cancel.
    if let Some((dir, name)) = &mut app.worlds.rename {
        let mut done = None;
        ui.add_space(tk::GAP);
        padded(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = tk::GAP;
                let buttons = kit::Button::new("save").width(ui, &t) + kit::Button::new("cancel").width(ui, &t) + tk::GAP * 2.0;
                let r = kit::Field::new(name).label("new name").width((ui.available_width() - buttons).min(tk::SETTING_CONTROL_WIDTH * 1.5)).focus(true).show(ui, &t);
                if kit::Button::new("save").filled().show(ui, &t).clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    done = Some(true);
                }
                if kit::Button::new("cancel").show(ui, &t).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    done = Some(false);
                }
            });
        });
        ui.add_space(tk::GAP);
        kit::rule(ui, &t, true);
        match done {
            Some(true) => {
                if let Err(e) = saves::rename_world(dir, name.trim()) {
                    app.toast(ToastKind::Error, format!("Couldn't rename: {e:#}"));
                }
                app.worlds.rename = None;
                app.worlds.invalidate();
            }
            Some(false) => app.worlds.rename = None,
            None => {}
        }
    }

    let s = &mut app.worlds;
    egui::ScrollArea::vertical().id_salt("worlds").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut list = kit::HoverList::begin(ui, kit::Variant::Compact).indent(tk::GUTTER + tk::HOVER_SHIFT_COMPACT);
        let mut row = |ui: &mut Ui, folder: &str, r: &egui::Response| {
            let on = w.sel_folder == Some(folder);
            if on && scroll {
                r.scroll_to_me(None);
            }
            if r.clicked() {
                s.selected = Some(folder.to_string());
                s.backup = None;
                out.to_main = true;
            }
            let _ = ui;
        };
        for wd in &w.listing.worlds {
            let on = w.sel_folder == Some(wd.folder.as_str());
            let mut desc = vec![if wd.hardcore { "hardcore" } else { wd.mode }.to_string()];
            if let Some(v) = &wd.version {
                desc.push(v.clone());
            }
            desc.push(format!("played {}", kit::text::ago(wd.last_played)));
            if wd.folder != wd.name {
                desc.push(wd.folder.clone());
            }
            if let Some((id, name)) = &wd.played_in
                && *id != inst.id
            {
                desc.push(format!("last played in {name}"));
            }
            let desc = desc.join(" · ");
            let n = w.listing.backups.get(&wd.folder).map_or(0, Vec::len);
            let badge = match n {
                0 => String::new(),
                1 => "1 backup".into(),
                n => format!("{n} backups"),
            };
            let mut hr = kit::HoverRow::new(("world", &wd.folder), &wd.name).description(&desc).selected(on).hot(on && focused);
            if let Some(icon) = &wd.icon {
                hr = hr.image(egui::ImageSource::Uri(format!("file://{}", icon.display()).into()), 1.0);
            }
            if n > 0 {
                hr = hr.badge(&badge, t.text_muted);
            }
            let r = list.row(ui, &t, hr);
            row(ui, &wd.folder, &r);
            if r.double_clicked() {
                out.act = Some(Act::Play(wd.folder.clone()));
            }
            r.context_menu(|ui| {
                let mut pick = |a: Act, ui: &mut Ui| {
                    out.act = Some(a);
                    ui.close();
                };
                if ui.button("play").clicked() {
                    pick(Act::Play(wd.folder.clone()), ui);
                }
                if ui.button("rename").clicked() {
                    pick(Act::Rename(wd.clone()), ui);
                }
                if ui.add_enabled(dup_ok, egui::Button::new("duplicate")).clicked() {
                    pick(Act::Duplicate(wd.clone()), ui);
                }
                if ui.button("back up now").clicked() {
                    pick(Act::BackUp(wd.clone()), ui);
                }
                if ui.button("folder").clicked() {
                    pick(Act::Folder(wd.dir.clone()), ui);
                }
                if ui.button("delete").clicked() {
                    pick(Act::Delete(wd.clone()), ui);
                }
            });
        }
        if !w.gone.is_empty() {
            list_label(ui, &t, "gone · backups only");
        }
        for folder in w.gone {
            let on = w.sel_folder == Some(folder.as_str());
            let n = w.listing.backups.get(folder).map_or(0, Vec::len);
            let desc = format!("deleted · {n} backup{}", if n == 1 { "" } else { "s" });
            let r = list.row(ui, &t, kit::HoverRow::new(("gone", folder), folder).description(&desc).dim(true).selected(on).hot(on && focused));
            row(ui, folder, &r);
        }
        list.end(ui);
    });
}

/// The selected world's backups as a quiet table under their head: back up now, and restore,
/// delete or open the one picked.
#[allow(clippy::too_many_arguments)]
fn backup_pane(ui: &mut Ui, app: &mut App, w: &Pane, snaps: &[Snapshot], kb: Option<Nav>, back_up_key: bool, focused: bool, out: &mut Out) {
    let t = app.look.tokens.clone();
    ui.spacing_mut().item_spacing.y = 0.0;
    let s = &mut app.worlds;
    let pos = s.backup.as_ref().and_then(|d| snaps.iter().position(|x| &x.dir == d));
    let mut scroll = false;
    if let Some(p) = kb.filter(|n| n.is_move()).and_then(|n| n.step(snaps.len(), pos)) {
        s.backup = Some(snaps[p].dir.clone());
        scroll = true;
    }
    let key = |n: Nav| kb == Some(n);
    let picked = s.backup.as_ref().and_then(|d| snaps.iter().find(|x| &x.dir == d)).cloned();
    let name = w.sel.map(|wd| wd.name.as_str()).or(w.sel_folder).unwrap_or_default();
    let mut buttons = Vec::new();
    if w.sel.is_some() {
        buttons.push(kit::Button::new("back up now"));
    }
    let first_snap = buttons.len();
    if picked.is_some() {
        buttons.extend([kit::Button::new("restore"), kit::Button::new("delete"), kit::Button::new("folder")]);
    }
    let (clicked, _) = content::pane_head(ui, &t, 2, "backups", name, &buttons, out.head_h);
    if let Some(wd) = w.sel
        && (clicked == Some(0) || back_up_key)
    {
        out.act = Some(Act::BackUp(wd.clone()));
    }
    if let (Some(snap), Some(folder)) = (&picked, w.sel_folder) {
        let pick = |i: usize| clicked == Some(first_snap + i);
        if pick(0) || key(Nav::Activate) {
            out.act = Some(Act::Restore(folder.to_string(), snap.clone()));
        } else if pick(1) || key(Nav::Delete) {
            out.act = Some(Act::DeleteBackup(folder.to_string(), snap.clone()));
        } else if pick(2) || key(Nav::Folder) {
            out.act = Some(Act::Folder(snap.dir.clone()));
        }
    }
    let Some(folder) = w.sel_folder else {
        padded(ui, |ui| {
            ui.add_space(tk::GUTTER);
            kit::body_muted(ui, &t, "Select a world to see its backups.");
        });
        return;
    };
    if snaps.is_empty() {
        padded(ui, |ui| {
            ui.add_space(tk::GUTTER);
            kit::body_muted(ui, &t, "No backups yet. Backups are taken automatically on game exit, or click 'Back Up Now'.");
        });
        return;
    }
    let cols = [kit::Column::fixed("taken", tk::BACKUP_WHEN_WIDTH), kit::Column::fill("why"), kit::Column::fixed("kind", tk::BACKUP_KIND_WIDTH)];
    kit::table::header(ui, &t, &cols);
    let s = &mut app.worlds;
    egui::ScrollArea::vertical().id_salt("backups").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for snap in snaps {
            let on = s.backup.as_ref() == Some(&snap.dir);
            let kind = if snap.info.auto { "auto" } else { "kept" };
            let cells = [kit::Cell::Figure(snap.when()), kit::Cell::Muted(snap.info.reason.clone()), kit::Cell::Caption(kind.into(), None)];
            let r = kit::table::row(ui, &t, &cols, &cells, on);
            if on && focused {
                kit::table::mark(ui, &t, r.rect);
            }
            if on && scroll {
                r.scroll_to_me(None);
            }
            if r.clicked() {
                s.backup = Some(snap.dir.clone());
                out.to_side = true;
            }
            if r.double_clicked() {
                out.act = Some(Act::Restore(folder.to_string(), snap.clone()));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loaders::Loader;

    fn add_world(saves: &std::path::Path, folder: &str) {
        std::fs::create_dir_all(saves.join(folder)).unwrap();
        std::fs::write(saves.join(folder).join("level.dat"), b"").unwrap();
    }

    /// Calls `sync` like frames would until the list matches `want` worlds.
    fn wait(s: &mut WorldsState, rt: &tokio::runtime::Runtime, inst: &Instance, want: usize) {
        let ctx = egui::Context::default();
        let start = Instant::now();
        while s.sync(rt.handle(), &ctx, inst).map(|l| l.worlds.len()) != Some(want) {
            assert!(start.elapsed() < Duration::from_secs(5), "worlds never loaded");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// G12, G14: world jobs and mod jobs hold off launches and each other through any
    /// instance whose saves lead to the same folder, not just the one that started them.
    #[test]
    fn jobs_on_shared_worlds_block_every_member() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let ctx = egui::Context::default();
        let mut a = Instance::new("jobs a", "1.21.1", Loader::Vanilla);
        a.assign_id().unwrap();
        let mut b = Instance::new("jobs b", "1.21.1", Loader::Vanilla);
        b.assign_id().unwrap();
        let mut c = Instance::new("jobs c", "1.21.1", Loader::Vanilla);
        c.assign_id().unwrap();
        // a and b share worlds through a link (whatever their settings say); c has its own.
        let shared = crate::paths::test_root().join(format!("shared-saves-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&shared).unwrap();
        for i in [&a, &b] {
            std::fs::create_dir_all(i.game_dir()).unwrap();
            std::os::unix::fs::symlink(&shared, i.saves_dir()).unwrap();
        }
        std::fs::create_dir_all(c.saves_dir()).unwrap();
        assert_eq!(saves_key(&a), saves_key(&b));

        let mut worlds = WorldsState::default();
        let mut content = super::super::content::ContentState::default();
        for i in [&a, &b, &c] {
            assert!(launch_blocked(&content, &worlds, i).is_none());
        }
        // A restore started through b holds off a and b, not c.
        let (done, wait) = std::sync::mpsc::channel::<()>();
        worlds.start(rt.handle(), &ctx, &b, "restore", "Restoring".into(), move || {
            let _ = wait.recv();
            Ok(JobDone { select: None, note: None })
        });
        assert!(launch_blocked(&content, &worlds, &a).is_some(), "a plays the worlds b is restoring");
        assert!(launch_blocked(&content, &worlds, &b).is_some());
        assert!(launch_blocked(&content, &worlds, &c).is_none());
        assert!(jobs_on(&content, &worlds, &saves_key(&a)).is_some(), "a second world job through a waits");
        drop(done);
        worlds.jobs.clear();

        // A mod update in c holds off c only; one in a (backing up its worlds) holds off b too.
        content.start_mod_job(rt.handle(), &ctx, &c.id, Some(saves_key(&c)), Task::new("c"), std::future::pending());
        assert!(launch_blocked(&content, &worlds, &c).is_some());
        assert!(launch_blocked(&content, &worlds, &a).is_none());
        content.start_mod_job(rt.handle(), &ctx, &a.id, Some(saves_key(&a)), Task::new("a"), std::future::pending());
        assert!(launch_blocked(&content, &worlds, &b).is_some());
        assert!(jobs_on(&content, &worlds, &saves_key(&b)).is_some(), "b's world backups wait for a's update");
        assert!(content.busy_with(&c.id), "c's job still counts after a's started");
        for i in [&a, &b, &c] {
            std::fs::remove_dir_all(i.dir()).unwrap();
        }
        std::fs::remove_dir_all(shared).unwrap();
    }

    #[test]
    fn worlds_load_off_the_ui_thread_and_stay_up_while_refreshing() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let ctx = egui::Context::default();
        let mut one = Instance::new("worlds one", "1.21.1", Loader::Vanilla);
        one.assign_id().unwrap();
        let saves = one.saves_dir();
        add_world(&saves, "a");
        let mut s = WorldsState::default();

        // The first frame doesn't read `saves/` itself; the list arrives later.
        assert!(s.sync(rt.handle(), &ctx, &one).is_none());
        wait(&mut s, &rt, &one, 1);

        // After a change the old list stays on screen until the new one lands.
        add_world(&saves, "b");
        s.invalidate();
        assert_eq!(s.sync(rt.handle(), &ctx, &one).map(|l| l.worlds.len()), Some(1));
        wait(&mut s, &rt, &one, 2);

        // A world with backups but no folder is listed as gone.
        backups::snapshot_world(&saves.join("b"), Owner::of(&one), "test", false).unwrap();
        std::fs::remove_dir_all(saves.join("b")).unwrap();
        s.invalidate();
        wait(&mut s, &rt, &one, 1);
        assert_eq!(s.sync(rt.handle(), &ctx, &one).unwrap().gone(), ["b"]);

        // Another instance never shows this one's worlds.
        let mut two = one.clone();
        two.id = format!("{}-two", one.id);
        s.selected = Some("a".into());
        assert!(s.sync(rt.handle(), &ctx, &two).is_none());
        assert!(s.selected.is_none());
        std::fs::remove_dir_all(one.dir()).unwrap();
        backups::forget_instance(&one.id).unwrap();
    }
}
