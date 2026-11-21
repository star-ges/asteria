//! Mods, resource packs and shader packs: what's installed on the left,
//! Modrinth on the right.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Align, Layout, Ui, vec2};

use crate::design::kit;
use crate::design::tokens::{self as tk, Tokens};
use super::nav::{Nav, Zone};
use super::{App, Confirm, ConfirmAction, Modal, Toast, ToastKind};
use crate::instance::{ContentKind, Instance, ModFile};
use crate::content_meta::{self, FileMeta};
use crate::keymap::Action;
use crate::modrinth::{self, Hit, SearchResponse, Update};
use crate::task::{Promise, Task};

#[derive(Default)]
struct KindState {
    query: String,
    /// (instance id, query) of the last search sent.
    searched: Option<(String, String)>,
    /// When the query last changed, so a search goes out once typing pauses.
    typed: Option<Instant>,
    search_p: Option<Promise<SearchResponse>>,
    results: Option<Result<Vec<Hit>, String>>,
    /// (instance id, project id) → the install running for it.
    installing: HashMap<(String, String), Job>,
    selected: Option<String>,
    /// Keyboard selection in the Modrinth results.
    result: Option<usize>,
    /// The result opened for its details (project id), one at a time.
    open: Option<String>,
    cache: Option<(String, Vec<ModFile>, Instant)>,
}

struct Job {
    task: Task,
    promise: Promise<modrinth::Installed>,
    title: String,
    /// Where the files go, so they can be tagged with their project when done.
    dir: PathBuf,
}

#[derive(Default)]
pub struct ContentState {
    kinds: HashMap<ContentKind, KindState>,
    meta: MetaStore,
    pub remote: RemoteIcons,
    pub focus_search: bool,
    /// An update or rollback key for the mods tab, taken while it draws.
    pub key: Option<Action>,
    /// The last mod update check: (instance id, the check).
    updates: Option<(String, Check)>,
    /// Mods being updated or rolled back, any number of instances at once.
    mod_jobs: Vec<ModJob>,
    /// Worlds a rollback left played with newer mods, each to ask about in turn.
    pub world_prompts: std::collections::VecDeque<WorldPrompt>,
}

/// A mods update or rollback running in the background. Kept until its result is
/// taken, so its instance counts as busy for as long as it writes.
struct ModJob {
    task: Task,
    promise: Promise<ModJobDone>,
    inst_id: String,
    /// The saves folder an update backs worlds up from (`worlds::saves_key`); a rollback has none.
    saves: Option<PathBuf>,
}

/// How a mods update or rollback went.
pub struct ModJobDone {
    kind: ToastKind,
    text: String,
    worlds: Vec<WorldPrompt>,
}

impl ModJobDone {
    pub fn new(kind: ToastKind, text: String) -> Self {
        ModJobDone { kind, text, worlds: Vec::new() }
    }
}

enum Check {
    Running(Promise<Vec<Update>>),
    Found(Vec<Update>),
}

/// Name, version and icon of installed files, filled in the background.
#[derive(Default, Clone)]
struct MetaStore {
    known: Arc<Mutex<HashMap<String, FileMeta>>>,
    pending: Arc<Mutex<HashSet<String>>>,
    /// Project of each file we installed ourselves (by enabled path), so it
    /// counts as installed before the hash lookup comes back.
    ours: Arc<Mutex<HashMap<PathBuf, String>>>,
}

/// The file's path as if enabled.
fn base_path(f: &ModFile) -> PathBuf {
    f.path.with_file_name(f.name.trim_end_matches(".disabled"))
}

/// Same key whether or not the file is disabled; a replaced file gets a new one.
fn meta_key(f: &ModFile) -> String {
    let mtime = f.modified.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
    format!("{}|{}|{mtime}", base_path(f).display(), f.size)
}

impl MetaStore {
    fn get(&self, f: &ModFile) -> Option<FileMeta> {
        self.known.lock().unwrap().get(&meta_key(f)).cloned()
    }

    /// Modrinth projects among these files (one instance's, as they are now).
    fn project_ids(&self, files: &[ModFile]) -> HashSet<String> {
        let (known, ours) = (self.known.lock().unwrap(), self.ours.lock().unwrap());
        files
            .iter()
            .filter_map(|f| known.get(&meta_key(f)).and_then(|m| m.project_id.clone()).or_else(|| ours.get(&base_path(f)).cloned()))
            .collect()
    }

    fn installed(&self, dir: &std::path::Path, done: &modrinth::Installed) {
        let mut ours = self.ours.lock().unwrap();
        for (file, project) in done.files.iter().zip(&done.projects) {
            ours.insert(dir.join(file), project.clone());
        }
    }

    fn ensure(&self, files: &[ModFile], kind: ContentKind, rt: &tokio::runtime::Handle, ctx: &egui::Context) {
        let missing: Vec<(String, ModFile)> = {
            let known = self.known.lock().unwrap();
            let mut pending = self.pending.lock().unwrap();
            files
                .iter()
                .map(|f| (meta_key(f), f.clone()))
                .filter(|(k, _)| !known.contains_key(k) && pending.insert(k.clone()))
                .collect()
        };
        if missing.is_empty() {
            return;
        }
        let (store, ctx) = (self.clone(), ctx.clone());
        rt.spawn(async move {
            let keys: Vec<String> = missing.iter().map(|(k, _)| k.clone()).collect();
            let locals = tokio::task::spawn_blocking(move || {
                missing.into_iter().map(|(k, f)| (k, content_meta::read_local(&f.path, f.is_dir, kind))).collect::<Vec<_>>()
            })
            .await
            .unwrap_or_default();
            store.known.lock().unwrap().extend(locals.iter().cloned());
            ctx.request_repaint();

            // Modrinth fills gaps (and is the only source for shader icons).
            let hashes: Vec<String> = locals.iter().filter_map(|(_, m)| m.sha1.clone()).collect();
            if let Ok(found) = content_meta::lookup(hashes).await {
                for (k, m) in locals {
                    let Some(p) = m.sha1.as_ref().and_then(|h| found.get(h)) else { continue };
                    let icon = match (&m.icon, &p.icon_url) {
                        (None, Some(url)) => content_meta::fetch_icon(url).await.ok(),
                        _ => None,
                    };
                    if let Some(e) = store.known.lock().unwrap().get_mut(&k) {
                        e.project_id = Some(p.id.clone());
                        e.name = e.name.take().or(Some(p.title.clone()));
                        e.icon = e.icon.take().or(icon);
                    }
                    ctx.request_repaint();
                }
            }
            let mut pending = store.pending.lock().unwrap();
            for k in keys {
                pending.remove(&k);
            }
        });
    }
}

/// Icons for search results, downloaded once into the cache.
#[derive(Default, Clone)]
pub struct RemoteIcons {
    map: Arc<Mutex<HashMap<String, Option<PathBuf>>>>,
}

impl RemoteIcons {
    pub fn get(&self, url: &str, rt: &tokio::runtime::Handle, ctx: &egui::Context) -> Option<PathBuf> {
        let mut map = self.map.lock().unwrap();
        if let Some(p) = map.get(url) {
            return p.clone();
        }
        map.insert(url.to_string(), None);
        let (map, url, ctx) = (self.map.clone(), url.to_string(), ctx.clone());
        rt.spawn(async move {
            if let Ok(p) = content_meta::fetch_icon(&url).await {
                map.lock().unwrap().insert(url, Some(p));
                ctx.request_repaint();
            }
        });
        None
    }
}

/// Whether a file's name (no extension) is this project's: the slug alone, or the
/// slug then a version, maybe after loader tags (`sodium-0.6.0`, `sodium-fabric-0.5.8+mc1.20.1`),
/// so `sodium-extra-0.5` isn't Sodium.
fn is_slug_file(name: &str, slug: &str) -> bool {
    let (name, slug) = (name.to_lowercase(), slug.to_lowercase());
    let Some(rest) = name.strip_prefix(&slug) else { return false };
    if rest.is_empty() {
        return true;
    }
    let sep = |c: char| matches!(c, '-' | '_' | '+' | ' ');
    if !rest.starts_with(sep) {
        return false;
    }
    let version = |t: &str| {
        let t = t.strip_prefix("mc").or_else(|| t.strip_prefix('v')).unwrap_or(t);
        t.starts_with(|c: char| c.is_ascii_digit())
    };
    for token in rest.split(sep).filter(|t| !t.is_empty()) {
        if version(token) {
            return true;
        }
        if !matches!(token, "fabric" | "quilt" | "forge" | "neoforge") {
            return false;
        }
    }
    false
}

pub fn file_image(p: &std::path::Path) -> egui::ImageSource<'static> {
    egui::ImageSource::Uri(format!("file://{}", p.display()).into())
}

impl ContentState {
    pub fn tasks(&self) -> impl Iterator<Item = &Task> {
        self.kinds.values().flat_map(|k| k.installing.values().map(|j| &j.task)).chain(self.mod_jobs.iter().map(|j| &j.task))
    }

    /// Whether anything is still being installed into this instance, or its mods updated.
    pub fn busy_with(&self, inst_id: &str) -> bool {
        self.kinds.values().any(|k| k.installing.keys().any(|(id, _)| id == inst_id)) || self.updating(inst_id)
    }

    /// Whether mods are being installed into this instance, from Modrinth or dropped,
    /// until the job's result is taken. Pack installs don't count.
    pub fn installing_mods(&self, inst_id: &str) -> bool {
        self.kinds.get(&ContentKind::Mods).is_some_and(|k| k.installing.keys().any(|(id, _)| id == inst_id))
    }

    /// Whether this instance's mods are being updated or rolled back.
    pub fn updating(&self, inst_id: &str) -> bool {
        self.mod_jobs.iter().any(|j| j.inst_id == inst_id)
    }

    /// Whether a mod update is backing up worlds in this saves folder.
    pub fn updating_saves(&self, saves: &std::path::Path) -> bool {
        self.mod_jobs.iter().any(|j| j.saves.as_deref() == Some(saves))
    }

    /// Whether any mod update is backing up worlds.
    pub fn backing_up_worlds(&self) -> bool {
        self.mod_jobs.iter().any(|j| j.saves.is_some())
    }

    /// Run a mods update or rollback in the background; it toasts its own outcome.
    /// `saves` is the saves folder it backs worlds up from, if it does.
    pub fn start_mod_job(
        &mut self,
        rt: &tokio::runtime::Handle,
        ctx: &egui::Context,
        inst_id: &str,
        saves: Option<PathBuf>,
        task: Task,
        fut: impl Future<Output = ModJobDone> + Send + 'static,
    ) {
        let tk = task.clone();
        let promise = Promise::spawn(rt, ctx, async move {
            let r = fut.await;
            tk.finish(&if r.kind == ToastKind::Error { Err(anyhow::anyhow!("{}", r.text)) } else { Ok(()) });
            Ok(r)
        });
        self.mod_jobs.push(ModJob { task, promise, inst_id: inst_id.to_string(), saves });
    }

    pub fn invalidate(&mut self) {
        for k in self.kinds.values_mut() {
            k.cache = None;
        }
    }

    /// The mods as mod sets name them, with each one's title, once every file's metadata
    /// has been read (None until then): the same keys `mod_sets::scan` reads from the jars.
    pub fn mod_entries(&self, files: &[ModFile]) -> Option<(Vec<crate::mod_sets::Entry>, HashMap<String, String>)> {
        let mut entries = Vec::new();
        let mut titles = HashMap::new();
        for f in files.iter().filter(|f| !f.is_dir) {
            let info = self.meta.get(f)?;
            let key = crate::mod_sets::key(info.mod_id.as_deref(), &f.name);
            titles.entry(key.clone()).or_insert_with(|| info.name.clone().unwrap_or_else(|| f.display_name().to_string()));
            entries.push(crate::mod_sets::Entry { file: f.clone(), key });
        }
        Some((entries, titles))
    }

    /// Icons of `inst`'s `kind` files by file name, as their tab shows them (looked up in the background).
    pub fn icons(&mut self, inst: &Instance, kind: ContentKind, rt: &tokio::runtime::Handle, ctx: &egui::Context) -> HashMap<String, PathBuf> {
        let files = self.kinds.entry(kind).or_default().files(inst, kind);
        self.meta.ensure(&files, kind, rt, ctx);
        files.iter().filter_map(|f| Some((f.name.clone(), self.meta.get(f)?.icon?))).collect()
    }

    /// A mod's name from its metadata, if read yet.
    pub fn title(&self, f: &ModFile) -> Option<String> {
        self.meta.get(f).and_then(|m| m.name)
    }

    pub fn poll(&mut self, toasts: &mut Vec<Toast>) {
        if let Some((_, Check::Running(p))) = &self.updates
            && let Some(r) = p.take()
        {
            match r {
                Ok(list) => {
                    if list.is_empty() {
                        toasts.push(Toast::info("Every mod is up to date"));
                    }
                    self.updates.as_mut().unwrap().1 = Check::Found(list);
                }
                Err(e) => {
                    toasts.push(Toast::error(format!("Couldn't check for updates: {e:#}")));
                    self.updates = None;
                }
            }
        }
        let (done, running): (Vec<_>, Vec<_>) = std::mem::take(&mut self.mod_jobs).into_iter().partition(|j| j.promise.ready());
        self.mod_jobs = running;
        for j in done {
            let done = j.promise.take().unwrap().unwrap_or_else(|e| ModJobDone::new(ToastKind::Error, format!("{e:#}")));
            toasts.push(Toast::new(done.kind, done.text));
            self.world_prompts.extend(done.worlds);
            // What was found for that instance is out of date now.
            if self.updates.as_ref().is_some_and(|(id, _)| *id == j.inst_id) {
                self.updates = None;
            }
            self.invalidate();
        }
        for s in self.kinds.values_mut() {
            let meta = &self.meta;
            if let Some(p) = &s.search_p
                && let Some(r) = p.take()
            {
                s.results = Some(r.map(|s| s.hits).map_err(|e| format!("{e:#}")));
                s.result = None;
                s.search_p = None;
            }
            let done: Vec<(String, String)> = s.installing.iter().filter(|(_, j)| j.promise.ready()).map(|(k, _)| k.clone()).collect();
            for key in done {
                let Job { promise, title, dir, .. } = s.installing.remove(&key).unwrap();
                let dropped = key.1.starts_with("drop:");
                let (kind, text) = match promise.take().unwrap() {
                    Ok(done) if dropped && done.files.is_empty() => (ToastKind::Info, format!("{title} is already there")),
                    Ok(done) if dropped => (ToastKind::Info, format!("Added {}", done.files.join(", "))),
                    Err(e) if dropped => (ToastKind::Error, format!("{e:#}")),
                    Ok(done) if done.files.is_empty() && done.missing.is_empty() => (ToastKind::Info, format!("{title} is already installed")),
                    Ok(done) => {
                        meta.installed(&dir, &done);
                        let extra = done.files.len().saturating_sub(1);
                        let deps = if extra > 0 { format!(" (+{extra} dependencies)") } else { String::new() };
                        if done.missing.is_empty() {
                            (ToastKind::Info, format!("Installed {title}{deps}"))
                        } else {
                            // The mod won't load without these, so it's an error, not a success.
                            (ToastKind::Error, format!("Installed {title}{deps}, but not its required dependencies:\n{}", done.missing.join("\n")))
                        }
                    }
                    Err(e) => (ToastKind::Error, format!("{title}: {e:#}")),
                };
                toasts.push(Toast::new(kind, text));
                s.cache = None;
            }
        }
    }
}

impl KindState {
    fn files(&mut self, inst: &Instance, kind: ContentKind) -> Vec<ModFile> {
        let fresh = self.cache.as_ref().is_some_and(|(id, _, at)| *id == inst.id && at.elapsed() < Duration::from_secs(2));
        if !fresh {
            self.cache = Some((inst.id.clone(), inst.content(kind), Instant::now()));
        }
        self.cache.as_ref().unwrap().1.clone()
    }

    fn search(&mut self, kind: ContentKind, inst: &Instance, rt: &tokio::runtime::Handle, ctx: &egui::Context) {
        let (i, q) = (inst.clone(), self.query.clone());
        // Keep the old results up while the query changes; only another instance's are stale.
        if self.searched.as_ref().is_none_or(|(id, _)| *id != inst.id) {
            self.results = None;
            self.result = None;
        }
        self.searched = Some((inst.id.clone(), q.clone()));
        self.typed = None;
        self.search_p = Some(Promise::spawn(rt, ctx, async move { modrinth::search(kind, &q, &i).await }));
    }
}

/// Why nothing can be installed into `inst`'s `kind` folder right now, if anything stops it.
fn install_blocked(app: &mut App, inst: &Instance, kind: ContentKind) -> Option<&'static str> {
    // Installing into a folder that's being moved into or out of a share set could land
    // the file in the old place or get it renamed on a clash.
    if app.sharing.is_some() && crate::shares::SET_FOLDERS.contains(&kind.dir()) {
        Some("Shared folders are moving; install again in a moment")
    } else if kind == ContentKind::Mods && app.content.updating(&inst.id) {
        Some("Mods are updating; install again in a moment")
    } else if kind == ContentKind::Mods && app.sessions.get(&inst.id).is_some_and(|s| s.starting()) {
        Some("The game is starting; install again once it has")
    } else if kind == ContentKind::Mods && app.mod_sets.searching(inst) {
        // Mods stay as each step of a search for the breaking mod sets them.
        Some("Finding breaking mod; install once it's done")
    } else {
        None
    }
}

/// Add files dropped on a content tab to the instance's folder, as an install job.
pub fn add_dropped(app: &mut App, ctx: &egui::Context, inst: &Instance, kind: ContentKind, paths: Vec<PathBuf>) {
    if kind == ContentKind::Mods && inst.loader.modrinth().is_none() {
        app.toast(ToastKind::Error, "Vanilla instances don't load mods. Pick a mod loader in the version tab.");
        return;
    }
    if let Some(why) = install_blocked(app, inst, kind) {
        app.toast(ToastKind::Error, why);
        return;
    }
    let title = match paths.as_slice() {
        [one] => one.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        many => format!("{} files", many.len()),
    };
    let rt = app.rt.clone();
    app.content.start_drop(&rt, ctx, inst, kind, paths, title);
}

impl ContentState {
    /// Copy dropped `paths` into `inst`'s `kind` folder as an install job (admitted already).
    fn start_drop(&mut self, rt: &tokio::runtime::Handle, ctx: &egui::Context, inst: &Instance, kind: ContentKind, paths: Vec<PathBuf>, title: String) {
        let job = (inst.id.clone(), format!("drop:{}", paths[0].display()));
        let s = self.kinds.entry(kind).or_default();
        if s.installing.contains_key(&job) {
            return;
        }
        let task = Task::new(format!("adding {title}"));
        task.stage(format!("Adding {title}"), 0);
        let (i, tk) = (inst.clone(), task.clone());
        let promise = Promise::spawn(rt, ctx, async move {
            let r = tokio::task::spawn_blocking(move || i.add_content(kind, &paths)).await.map_err(anyhow::Error::from).and_then(|r| r);
            tk.finish(&r.as_ref().map(|_| ()).map_err(|e| anyhow::anyhow!("{e:#}")));
            r.map(|files| modrinth::Installed { files, projects: Vec::new(), missing: Vec::new() })
        });
        let dir = inst.game_dir().join(kind.dir());
        s.installing.insert(job, Job { task, promise, title, dir });
    }
}

/// Pads `add` to the page's gutter (the content panes run edge to edge; text beside them doesn't).
pub fn padded<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new().inner_margin(egui::Margin { left: tk::GUTTER as i8, right: tk::GUTTER as i8, top: 0, bottom: 0 }).show(ui, add).inner
}

/// Two panes side by side running to the page's edges, a strong hairline between, the left one
/// `left_w` wide. Takes the rest of the page; the panes are children that draw where they are.
pub(super) fn panes(ui: &mut Ui, t: &Tokens, left_w: f32) -> (Ui, Ui) {
    let area = ui.available_rect_before_wrap();
    let line_x = area.left() + left_w.floor();
    let left = egui::Rect::from_min_max(area.min, egui::pos2(line_x, area.bottom()));
    let right = egui::Rect::from_min_max(egui::pos2(line_x + tk::LINE, area.top()), area.max);
    kit::vline(ui.painter(), line_x, area.y_range(), t.line_strong);
    ui.allocate_rect(area, egui::Sense::hover());
    let child = |ui: &mut Ui, r, salt| ui.new_child(egui::UiBuilder::new().max_rect(r).id_salt(salt).layout(Layout::top_down(Align::Min)));
    (child(ui, left, "left-pane"), child(ui, right, "right-pane"))
}

/// A sheet's two panes, the left `left_share` of the width (at least `left_min`), down to
/// `footer` above the sheet's foot; the cursor moves past them. Returns the panes.
pub(super) fn sheet_panes(ui: &mut Ui, t: &Tokens, left_share: f32, left_min: f32, footer: f32) -> (Ui, Ui) {
    let area = ui.available_rect_before_wrap();
    let area = egui::Rect::from_min_size(area.min, vec2(area.width(), (area.height() - footer).max(tk::SETTING_ROW_HEIGHT * 4.0)));
    ui.allocate_rect(area, egui::Sense::hover());
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(area).id_salt("sheet-panes").layout(Layout::top_down(Align::Min)));
    let left_w = (area.width() * left_share).max(left_min.min(area.width() / 2.0));
    panes(&mut body, t, left_w)
}

/// A sheet's foot: a strong rule, then key hints on the left and the sheet's buttons on the
/// right (the first filled, as given). Returns the button clicked.
pub(super) fn sheet_foot(ui: &mut Ui, t: &Tokens, hints: &[(String, &str)], buttons: &[kit::Button]) -> Option<usize> {
    kit::rule(ui, t, true);
    ui.add_space(tk::GAP);
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = tk::GUTTER;
        let buttons_w: f32 = buttons.iter().map(|b| b.width(ui, t)).sum::<f32>() + tk::GAP * buttons.len().saturating_sub(1) as f32;
        let row = ui.available_rect_before_wrap();
        let right = egui::Rect::from_min_max(egui::pos2(row.right() - buttons_w, row.top()), egui::pos2(row.right(), row.top() + tk::BUTTON_HEIGHT));
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(right).layout(Layout::left_to_right(Align::Center)));
        c.spacing_mut().item_spacing.x = tk::GAP;
        for (i, b) in buttons.iter().enumerate() {
            if b.clone().show(&mut c, t).clicked() {
                clicked = Some(i);
            }
        }
        // Hints only where they fit beside the buttons.
        ui.set_max_width((ui.available_width() - buttons_w - tk::GUTTER).max(0.0));
        ui.set_min_height(tk::BUTTON_HEIGHT);
        let mut used = 0.0;
        for (key, label) in hints {
            let w = kit::text::caption_galley(ui, &format!("{key} {label}"), t.text_muted).size().x + tk::GUTTER;
            used += w;
            if used > ui.max_rect().width() {
                break;
            }
            kit::key_hint(ui, t, key, label);
        }
    });
    clicked
}

pub fn show(app: &mut App, ui: &mut Ui, inst: &Instance, kind: ContentKind) {
    let t = app.look.tokens.clone();
    let ctx = ui.ctx().clone();
    let has_loader = inst.loader.modrinth().is_some();
    if kind == ContentKind::Mods && !has_loader {
        let go = padded(ui, |ui| kit::empty_state(ui, &t, "Vanilla instances don't load mods", "Pick a mod loader in Overview, with change version.", Some("open overview")));
        if go.is_some_and(|r| r.clicked()) {
            app.inst_tab = super::instances::InstTab::Overview;
        }
        return;
    }
    if kind == ContentKind::ShaderPacks {
        let iris = inst.mods().iter().any(|m| m.enabled && m.name.to_lowercase().starts_with("iris"));
        let note = if !has_loader {
            Some("Shaders need Iris, which needs a Fabric or Quilt instance.")
        } else if !iris {
            Some("Shaders need the Iris mod. Install it from the mods tab.")
        } else {
            None
        };
        if let Some(n) = note {
            padded(ui, |ui| kit::bracketed(ui, &t, "shaders need iris", |ui| kit::body(ui, &t, n)));
            ui.add_space(tk::GAP);
        }
    }

    let rt = app.rt.clone();
    let main_nav = app.nav_for(Zone::Main);
    let side_nav = app.nav_for(Zone::Side);
    let zone = app.focus;
    let focus = std::mem::take(&mut app.content.focus_search) || main_nav == Some(Nav::Search) || side_nav == Some(Nav::Search);
    let (meta, remote) = (app.content.meta.clone(), app.content.remote.clone());
    let s = app.content.kinds.entry(kind).or_default();
    if s.searched.as_ref().is_none_or(|(id, _)| *id != inst.id) {
        s.search(kind, inst, &rt, &ctx);
    }
    let files = s.files(inst, kind);
    meta.ensure(&files, kind, &rt, &ctx);
    let mut out = Out::default();
    let starting = app.sessions.get(&inst.id).is_some_and(|s| s.starting());
    let searching = kind == ContentKind::Mods && app.mod_sets.searching(inst);
    let blocked = install_blocked(app, inst, kind);
    if kind == ContentKind::Mods {
        let found = app.content.mod_entries(&files);
        super::mod_sets::bar(app, ui, inst, found.as_ref().map(|(e, n)| (e.as_slice(), n)));
    }
    let ups = match kind {
        ContentKind::Mods => Some(Updates {
            list: match &app.content.updates {
                Some((id, Check::Found(l))) if *id == inst.id => current_updates(l, inst, &files),
                _ => Vec::new(),
            },
            checking: app.content.updates.as_ref().is_some_and(|(id, c)| *id == inst.id && matches!(c, Check::Running(_))),
            running: app.content.updating(&inst.id) || starting || searching,
            lock: if searching {
                ""
            } else if starting {
                "game starting"
            } else {
                "working"
            },
            key: app.content.key.take(),
        }),
        _ => None,
    };
    // Each mod that has config files → its group in the config tab.
    let mut configs: HashMap<PathBuf, String> = HashMap::new();
    if kind == ContentKind::Mods {
        let groups = app.config.mod_groups(inst, &rt, &ctx);
        for f in &files {
            let Some(info) = meta.get(f) else { continue };
            let title = info.name.clone().unwrap_or_else(|| f.display_name().to_string());
            if let Some((_, name)) = groups.iter().find(|(ids, name)| info.mod_id.as_ref().is_some_and(|id| ids.contains(id)) || *name == title) {
                configs.insert(f.path.clone(), name.clone());
            }
        }
    }
    // Each shader pack → its options, the same way.
    if kind == ContentKind::ShaderPacks {
        let packs = app.config.pack_names(inst, &rt, &ctx);
        for f in &files {
            if packs.contains(&f.name) {
                configs.insert(f.path.clone(), f.name.clone());
            }
        }
    }
    let s = app.content.kinds.entry(kind).or_default();
    // Installed on the left, Modrinth on the right, a hairline between: each pane runs from
    // its edge, so hover rows reach the line.
    if ui.available_width() < tk::CONTENT_TWO_PANES {
        // Too narrow for both: one pane at a time, the one with the keyboard (h/l still switch).
        let mut side = zone == Zone::Side;
        padded(ui, |ui| ui.horizontal(|ui| kit::segmented(ui, &t, &mut side, &[(false, "installed"), (true, "modrinth")])));
        ui.add_space(tk::GAP);
        if side {
            browse(ui, &t, s, &meta, &remote, inst, kind, &rt, &ctx, focus, side_nav, zone == Zone::Side, blocked, &mut out);
            out.to_results |= zone != Zone::Side;
        } else {
            installed(ui, &t, s, &meta, inst, kind, main_nav, zone == Zone::Main, ups.as_ref(), &configs, &mut out);
            out.to_installed |= zone == Zone::Side;
        }
    } else {
        two_panes(ui, &t, s, &meta, &remote, inst, kind, &rt, &ctx, focus, main_nav, side_nav, zone, blocked, ups.as_ref(), &configs, &mut out);
    }
    if out.check {
        let i = inst.clone();
        app.content.updates = Some((inst.id.clone(), Check::Running(Promise::spawn(&rt, &ctx, async move { modrinth::check_updates(&i).await }))));
    }
    if out.dismiss_updates {
        app.content.updates = None;
    }
    if let Some(list) = out.update {
        start_update(app, &ctx, inst, list);
    }
    if out.roll_back {
        open_roll_back(app, inst);
    }
    if let Some(name) = out.config {
        if kind == ContentKind::ShaderPacks {
            super::config::open_pack(app, &name);
        } else {
            super::config::open_group(app, inst, &name);
        }
        return;
    }
    if focus || out.to_results {
        app.focus = Zone::Side;
    }
    if out.to_installed {
        app.focus = Zone::Main;
    }
    if let Some(e) = out.error {
        app.toast(ToastKind::Error, e);
    }
    if let Some(m) = out.remove {
        app.modal = Some(Modal::Confirm(Confirm {
            title: format!("remove {}", kind.noun()),
            body: format!("Remove {} from this instance?", m.display_name()),
            button: "remove".into(),
            action: ConfirmAction::DeletePath(m.path),
        }));
    }
}

/// Installed on the left, Modrinth on the right, a hairline between: each pane runs from its
/// edge, so hover rows reach the line.
#[allow(clippy::too_many_arguments)]
fn two_panes(
    ui: &mut Ui,
    t: &Tokens,
    s: &mut KindState,
    meta: &MetaStore,
    remote: &RemoteIcons,
    inst: &Instance,
    kind: ContentKind,
    rt: &tokio::runtime::Handle,
    ctx: &egui::Context,
    focus: bool,
    main_nav: Option<Nav>,
    side_nav: Option<Nav>,
    zone: Zone,
    blocked: Option<&str>,
    ups: Option<&Updates>,
    configs: &HashMap<PathBuf, String>,
    out: &mut Out,
) {
    let area = ui.available_rect_before_wrap();
    let left_w = (area.width() / 2.0).floor();
    let line_x = area.left() + left_w;
    let left = egui::Rect::from_min_max(area.min, egui::pos2(line_x, area.bottom()));
    let right = egui::Rect::from_min_max(egui::pos2(line_x + tk::LINE, area.top()), area.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(left).layout(Layout::top_down(Align::Min)), |ui| {
        installed(ui, t, s, meta, inst, kind, main_nav, zone == Zone::Main, ups, configs, out);
    });
    kit::vline(ui.painter(), line_x, area.y_range(), t.line_strong);
    ui.scope_builder(egui::UiBuilder::new().max_rect(right).layout(Layout::top_down(Align::Min)), |ui| {
        browse(ui, t, s, meta, remote, inst, kind, rt, ctx, focus, side_nav, zone == Zone::Side, blocked, out);
    });
    ui.allocate_rect(area, egui::Sense::hover());
}

/// What the panes ask of the app after drawing.
#[derive(Default)]
struct Out {
    remove: Option<ModFile>,
    error: Option<String>,
    to_results: bool,
    to_installed: bool,
    /// Check for mod updates, update these, put the found ones away, or open the rollback dialog.
    check: bool,
    update: Option<Vec<Update>>,
    dismiss_updates: bool,
    roll_back: bool,
    /// Show this config group in the config tab.
    config: Option<String>,
    /// How tall the installed pane's head came out, for the Modrinth pane's beside it.
    head_h: f32,
}

/// The mods tab's update state, for the installed pane.
struct Updates {
    list: Vec<Update>,
    checking: bool,
    /// An update, rollback or set switch is running in this instance, the game is
    /// starting, or a search for the breaking mod has the mods.
    running: bool,
    /// Why, in a word or two.
    lock: &'static str,
    key: Option<Action>,
}

/// An update's file as if enabled, like [`base_path`].
fn update_base(u: &Update) -> PathBuf {
    u.path.with_file_name(u.path.file_name().unwrap_or_default().to_string_lossy().trim_end_matches(".disabled"))
}

/// The updates in `list` still good for `inst` as it is now: picked for its game, for a
/// file it still has (enabled or not). `update` checks each file's contents again.
fn current_updates(list: &[Update], inst: &Instance, files: &[ModFile]) -> Vec<Update> {
    let game = modrinth::game_key(inst);
    let names: HashSet<PathBuf> = files.iter().map(base_path).collect();
    list.iter()
        .filter(|u| u.game == game && names.contains(&update_base(u)))
        .cloned()
        .collect()
}

/// Update `list`'s mods in the background, unless the instance is busy.
fn start_update(app: &mut App, ctx: &egui::Context, inst: &Instance, list: Vec<Update>) {
    // It backs up worlds first, which another member of a world set may be copying.
    if let Some(why) = app.instance_busy(&inst.id).map(str::to_string).or_else(|| super::worlds::worlds_busy(app, inst)) {
        app.toast(ToastKind::Error, format!("Can't update mods while {why}"));
        return;
    }
    let task = Task::new(format!("updating {}", inst.name));
    let (i, keep, tk) = (inst.clone(), app.cfg.backups_keep, task.clone());
    let rt = app.rt.clone();
    app.content.start_mod_job(&rt, ctx, &inst.id, Some(super::worlds::saves_key(inst)), task, async move {
        let (kind, text) = match modrinth::update(&i, list, keep, &tk).await {
            Ok(done) => {
                let mut text = match done.done.as_slice() {
                    [one] => format!("Updated {one}"),
                    many => format!("Updated {} mods:\n{}", many.len(), many.join("\n")),
                };
                if !done.added.is_empty() {
                    text.push_str(&format!("\nAdded {}", done.added.join(", ")));
                }
                if done.missing.is_empty() {
                    (ToastKind::Info, text)
                } else {
                    (ToastKind::Error, format!("{text}\nbut not these required dependencies:\n{}", done.missing.join("\n")))
                }
            }
            Err(e) => (ToastKind::Error, format!("{e:#}")),
        };
        ModJobDone::new(kind, text)
    });
}

/// The rollback dialog, listing the instance's backups of its mods and config.
fn open_roll_back(app: &mut App, inst: &Instance) {
    let list = crate::backups::instance_snapshots(&inst.id);
    app.modal = Some(Modal::RollBack(Box::new(RollBack { id: inst.id.clone(), name: inst.name.clone(), list, selected: 0 })));
}

pub struct RollBack {
    id: String,
    name: String,
    list: Vec<crate::backups::Snapshot>,
    selected: usize,
}

/// Snapshots as a quiet table to choose from: a click selects, a double click picks. Returns
/// whether one was picked.
fn snapshot_table(ui: &mut Ui, t: &Tokens, id: &str, list: &[crate::backups::Snapshot], selected: &mut Option<usize>, moved: bool) -> bool {
    let cols = [kit::Column::fixed("taken", tk::BACKUP_WHEN_WIDTH), kit::Column::fill("why"), kit::Column::fixed("kind", tk::UPDATE_VERSION_WIDTH)];
    ui.spacing_mut().item_spacing.y = 0.0;
    kit::table::header(ui, t, &cols);
    let mut pick = false;
    let height = (ui.available_height() - tk::BUTTON_HEIGHT - tk::GUTTER * 2.0).max(tk::TABLE_ROW_HEIGHT * 3.0);
    egui::ScrollArea::vertical().id_salt(id).max_height(height).auto_shrink([false, true]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (n, snap) in list.iter().enumerate() {
            let on = *selected == Some(n);
            let kind = if snap.info.auto { "auto" } else { "kept" };
            let cells = [kit::Cell::Text(snap.when()), kit::Cell::Muted(snap.info.reason.clone()), kit::Cell::Caption(kind.into(), None)];
            let r = kit::table::row(ui, t, &cols, &cells, on);
            if on && moved {
                r.scroll_to_me(None);
            }
            if r.clicked() {
                *selected = Some(n);
            }
            if r.double_clicked() {
                pick = true;
            }
        }
    });
    pick
}

/// The rollback dialog. Returns true once it's done.
pub fn roll_back_dialog(app: &mut App, s: &mut RollBack, ui: &mut Ui) -> bool {
    let t = app.look.tokens.clone();
    let kb = app.modal_nav();
    ui.set_max_width(ui.available_width().min(tk::SHEET_WIDTH));
    kit::heading(ui, &t, "Roll back mods");
    ui.add_space(tk::UNIT);
    if s.list.is_empty() {
        kit::body_muted(ui, &t, &format!("No backups of {}'s mods yet. One is taken before every update.", s.name));
        ui.add_space(tk::GUTTER);
        return kit::Button::new("close").show(ui, &t).clicked() || kb == Some(Nav::Activate);
    }
    kit::body_muted(ui, &t, &format!("Put {}'s mods and config back as they were. What's there now is backed up first.", s.name));
    ui.add_space(tk::GUTTER);
    let moved = kb.filter(|n| n.is_move()).and_then(|n| n.step(s.list.len(), Some(s.selected)));
    if let Some(p) = moved {
        s.selected = p;
    }
    let mut sel = Some(s.selected);
    let mut pick = snapshot_table(ui, &t, "roll-back", &s.list, &mut sel, moved.is_some()) || kb == Some(Nav::Activate);
    s.selected = sel.unwrap_or(0);
    ui.add_space(tk::GUTTER);
    let mut done = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = tk::GAP;
        pick |= kit::Button::new("roll back").filled().show(ui, &t).clicked();
        done = kit::Button::new("cancel").show(ui, &t).clicked();
    });
    if pick && let Some(snap) = s.list.get(s.selected) {
        app.modal = Some(Modal::Confirm(Confirm {
            title: "roll back mods".into(),
            body: format!("Put {}'s mods and config back as they were on {}? What's there now is backed up first.", s.name, snap.when()),
            button: "roll back".into(),
            action: ConfirmAction::RollBack(s.id.clone(), snap.clone()),
        }));
        return true;
    }
    done
}

/// Roll `id`'s mods back to `snap` in the background (confirmed already).
pub fn start_roll_back(app: &mut App, ctx: &egui::Context, id: &str, snap: crate::backups::Snapshot) {
    let Some(inst) = app.instances.iter().find(|i| i.id == id).cloned() else { return };
    if let Some(why) = app.instance_busy(id) {
        app.toast(ToastKind::Error, format!("Can't roll back while {why}"));
        return;
    }
    let task = Task::new(format!("rolling back {}", inst.name));
    task.stage("Rolling back mods", 0);
    let keep = app.cfg.backups_keep;
    let rt = app.rt.clone();
    app.content.start_mod_job(&rt, ctx, id, None, task, async move {
        let when = snap.when();
        let r = tokio::task::spawn_blocking(move || {
            crate::backups::roll_back(&inst, &snap, keep)?;
            Ok::<_, anyhow::Error>(world_prompts(&inst, snap.info.taken))
        })
        .await;
        match r {
            Ok(Ok(worlds)) => ModJobDone { kind: ToastKind::Info, text: format!("Rolled mods back to {when}"), worlds },
            Ok(Err(e)) => ModJobDone::new(ToastKind::Error, format!("Couldn't roll back: {e:#}")),
            Err(e) => ModJobDone::new(ToastKind::Error, format!("Couldn't roll back: {e}")),
        }
    });
}

/// A world last played with mods a rollback just took away or made older: offer to
/// roll it back too.
pub struct WorldPrompt {
    inst_id: String,
    name: String,
    what: String,
    snaps: Vec<crate::backups::Snapshot>,
    /// None when no backup is from before the rolled-back mods: nothing is picked for the player.
    selected: Option<usize>,
}

/// The worlds `inst` (just rolled back to mods from `before`) last played with newer
/// mods, each with its backups. The newest backup from before `before` is picked: the
/// world as it was before those mods were installed. If there's none, none is picked.
fn world_prompts(inst: &Instance, before: chrono::DateTime<chrono::Utc>) -> Vec<WorldPrompt> {
    let owner = crate::backups::Owner::of(inst);
    crate::world_sets::behind(inst)
        .into_iter()
        .map(|(w, d)| {
            let snaps = crate::backups::snapshots(owner, &w.folder);
            let selected = snaps.iter().position(|s| s.info.taken <= before);
            WorldPrompt { inst_id: inst.id.clone(), name: w.name, what: d.describe(), snaps, selected }
        })
        .collect()
}

/// Asks whether to roll a world back too. Returns true once it's answered.
pub fn world_prompt_dialog(app: &mut App, p: &mut WorldPrompt, ui: &mut Ui) -> bool {
    let t = app.look.tokens.clone();
    let kb = app.modal_nav();
    ui.set_max_width(ui.available_width().min(tk::SHEET_WIDTH));
    kit::heading(ui, &t, "Roll back the world too?");
    ui.add_space(tk::UNIT);
    kit::body(ui, &t, &format!("“{}” was last played with {}, which the rollback took away. Restore the world from a backup as well?", p.name, p.what));
    ui.add_space(tk::GUTTER);
    if p.snaps.is_empty() {
        kit::body_muted(ui, &t, "It has no backups to restore from.");
        ui.add_space(tk::GUTTER);
        return kit::Button::new("ok").show(ui, &t).clicked() || kb == Some(Nav::Activate);
    }
    if p.selected.is_none() {
        kit::bracketed(ui, &t, "none from before those mods", |ui| {
            kit::body(ui, &t, "None of its backups is from before those mods. A newer one may already have been played with them.");
        });
        ui.add_space(tk::GUTTER);
    }
    let moved = kb.filter(|n| n.is_move()).and_then(|n| n.step(p.snaps.len(), p.selected));
    if let Some(n) = moved {
        p.selected = Some(n);
    }
    let mut restore = snapshot_table(ui, &t, "world-roll-back", &p.snaps, &mut p.selected, moved.is_some()) || kb == Some(Nav::Activate);
    ui.add_space(tk::GUTTER);
    let mut no = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = tk::GAP;
        restore |= kit::Button::new("restore").filled().enabled(p.selected.is_some()).show(ui, &t).clicked();
        no = kit::Button::new("no").show(ui, &t).clicked();
    });
    if restore && let Some(snap) = p.selected.and_then(|n| p.snaps.get(n)).cloned() {
        super::worlds::start_restore(app, ui.ctx(), &p.inst_id.clone(), snap);
        return true;
    }
    no
}

/// A pane's head: `01 / MODS · 72 OF 72 ON` on the left, joined buttons on the right (under
/// it when the pane is too narrow for both), a strong rule under. Returns the button clicked.
/// `min_h` keeps it as tall as the pane head beside it, so their rules are one line.
pub(super) fn pane_head(ui: &mut Ui, t: &Tokens, number: usize, label: &str, aside: &str, buttons: &[kit::Button], min_h: f32) -> (Option<usize>, f32) {
    let mut text = format!("{} / {label}", kit::text::number(number));
    if !aside.is_empty() {
        text = format!("{text} · {aside}");
    }
    let cap = kit::text::caption_galley(ui, &text, t.text_muted);
    let buttons_w: f32 = if buttons.is_empty() { 0.0 } else { buttons.iter().map(|b| b.width(ui, t)).sum::<f32>() - (buttons.len() - 1) as f32 * tk::LINE };
    let width = ui.available_width();
    let row_h = tk::BUTTON_HEIGHT + tk::GAP * 2.0;
    let side_by_side = buttons.is_empty() || cap.size().x + buttons_w + tk::GUTTER * 3.0 <= width;
    let h = if side_by_side { if buttons.is_empty() { tk::SECTION_LABEL_HEIGHT } else { row_h } } else { tk::SECTION_LABEL_HEIGHT + row_h - tk::GAP }.max(min_h);
    let (rect, _) = ui.allocate_exact_size(vec2(width, h), egui::Sense::hover());
    // A head stacked (buttons under the caption), or as tall as a stacked neighbour, has its
    // caption at the top, so neighbours' captions line up as well as their rules.
    let stacked_h = tk::SECTION_LABEL_HEIGHT + row_h - tk::GAP;
    let cap_y = if !side_by_side || h >= stacked_h { rect.top() + tk::SECTION_LABEL_HEIGHT / 2.0 } else { rect.center().y };
    let mut job = kit::text::caption_job(&text, t.text_muted);
    job.wrap.max_width = if side_by_side { width - buttons_w - tk::GUTTER * 3.0 } else { width - tk::GUTTER * 2.0 }.max(40.0);
    job.wrap.max_rows = 1;
    let g = ui.painter().layout_job(job);
    ui.painter().galley(egui::pos2(rect.left() + tk::GUTTER, cap_y - g.size().y / 2.0), g, t.text_muted);
    let mut clicked = None;
    if !buttons.is_empty() {
        let (x, y) = if side_by_side { (rect.right() - tk::GUTTER - buttons_w, rect.center().y) } else { (rect.left() + tk::GUTTER, rect.bottom() - tk::GAP - tk::BUTTON_HEIGHT / 2.0) };
        let r = egui::Rect::from_min_size(egui::pos2(x, y - tk::BUTTON_HEIGHT / 2.0), vec2(buttons_w, tk::BUTTON_HEIGHT));
        clicked = kit::button_row(&mut ui.new_child(egui::UiBuilder::new().max_rect(r)), t, buttons);
    }
    kit::rule::hline(ui.painter(), rect.x_range(), rect.bottom() - 0.5, t.line_strong);
    (clicked, h)
}

#[allow(clippy::too_many_arguments)]
fn installed(
    ui: &mut Ui,
    t: &Tokens,
    s: &mut KindState,
    meta: &MetaStore,
    inst: &Instance,
    kind: ContentKind,
    kb: Option<Nav>,
    focused: bool,
    ups: Option<&Updates>,
    configs: &HashMap<PathBuf, String>,
    out: &mut Out,
) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let files = s.files(inst, kind);
    let update_for = |m: &ModFile| ups.and_then(|u| u.list.iter().find(|x| update_base(x) == base_path(m)));
    // Mods aren't switched or removed under an update, rollback or set switch, as the game
    // starts, or while a search for the breaking mod has them.
    let busy = ups.is_some_and(|u| u.running);
    // Selection is by file stem, so it survives enable/disable renames.
    let stem = |m: &ModFile| m.name.trim_end_matches(".disabled").to_string();
    let pos = s.selected.as_ref().and_then(|n| files.iter().position(|m| stem(m) == *n));
    let mut scroll = false;
    if let Some(p) = kb.filter(|n| n.is_move()).and_then(|n| n.step(files.len(), pos)) {
        s.selected = Some(stem(&files[p]));
        scroll = true;
    }
    let key = |n: Nav| kb == Some(n);
    let sel = s.selected.as_ref().and_then(|n| files.iter().find(|m| stem(m) == *n)).cloned();
    let mut toggle: Option<ModFile> = None;

    // The selection's keys, as its row's switch, links and menu do.
    if let Some(m) = &sel {
        if !busy {
            if key(Nav::Toggle) || key(Nav::Activate) {
                toggle = Some(m.clone());
            }
            if key(Nav::Delete) {
                out.remove = Some(m.clone());
            }
            if let Some(u) = update_for(m)
                && ups.is_some_and(|u| u.key == Some(Action::UpdateOne))
            {
                out.update = Some(vec![u.clone()]);
            }
        }
        // Configs can be looked at whatever the mods are doing.
        if let Some(name) = configs.get(&m.path)
            && key(Nav::Cycle)
        {
            out.config = Some(name.clone());
        }
    }
    if key(Nav::Folder) {
        super::open_path(&inst.game_dir().join(kind.dir()));
    }

    // The head: what's here, and the folder, updates and rollback.
    let on = files.iter().filter(|f| f.enabled).count();
    let mut aside = format!("{on} of {} on", files.len());
    let mut buttons = vec![kit::Button::new("folder")];
    let (mut check_i, mut back_i) = (None, None);
    if let Some(u) = ups {
        if u.running {
            if !u.lock.is_empty() {
                aside = format!("{aside} · {}", u.lock);
            }
        } else {
            if u.checking {
                aside = format!("{aside} · checking for updates");
            } else if u.list.is_empty() {
                check_i = Some(buttons.len());
                buttons.push(kit::Button::new("check updates"));
            }
            // A rollback waits for whatever has the mods now.
            back_i = Some(buttons.len());
            buttons.push(kit::Button::new("roll back"));
        }
        if !u.running && !u.checking && u.list.is_empty() && u.key == Some(Action::Update) {
            out.check = true;
        }
        if !u.running && u.key == Some(Action::RollBack) {
            out.roll_back = true;
        }
    }
    let label = format!("{}s", kind.noun());
    let (clicked, head_h) = pane_head(ui, t, 1, &label, &aside, &buttons, 0.0);
    out.head_h = head_h;
    match clicked {
        Some(0) => super::open_path(&inst.game_dir().join(kind.dir())),
        Some(i) if Some(i) == check_i => out.check = true,
        Some(i) if Some(i) == back_i => out.roll_back = true,
        _ => {}
    }

    // Found updates take over the top of the pane: a quiet table, then update all.
    if let Some(u) = ups.filter(|u| !u.list.is_empty()) {
        let cols = [kit::Column::fill("mod"), kit::Column::fixed("from", tk::UPDATE_VERSION_WIDTH), kit::Column::fixed("to", tk::UPDATE_VERSION_WIDTH)];
        kit::table::header(ui, t, &cols);
        egui::ScrollArea::vertical().id_salt(("updates", kind)).max_height(tk::TABLE_ROW_HEIGHT * 5.0).show(ui, |ui| {
            for up in &u.list {
                let cells = [kit::Cell::Text(up.title.clone()), kit::Cell::Muted(up.from.clone()), kit::Cell::Text(up.to.version_number.clone())];
                let picked = sel.as_ref().is_some_and(|m| update_base(up) == base_path(m));
                kit::table::row(ui, t, &cols, &cells, picked);
            }
        });
        ui.add_space(tk::GAP);
        padded(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = tk::GAP;
                let what = if u.list.len() == 1 { "update 1 mod".to_string() } else { format!("update {} mods", u.list.len()) };
                if (kit::Button::new(&what).filled().enabled(!u.running).show(ui, t).clicked() || u.key == Some(Action::Update)) && !u.running {
                    out.update = Some(u.list.clone());
                }
                if kit::Button::new("not now").show(ui, t).clicked() {
                    out.dismiss_updates = true;
                }
            });
        });
        ui.add_space(tk::GAP);
        kit::rule(ui, t, true);
    }

    egui::ScrollArea::vertical().id_salt(("installed", kind)).auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if files.is_empty() {
            let what = match kind {
                ContentKind::Mods => "No mods yet",
                ContentKind::ResourcePacks => "No resource packs yet",
                ContentKind::ShaderPacks => "No shader packs yet",
            };
            padded(ui, |ui| kit::empty_state(ui, t, what, "Search Modrinth, or drop files here.", None));
            return;
        }
        let mut list = kit::HoverList::begin(ui, kit::Variant::Compact).indent(tk::GUTTER + tk::HOVER_SHIFT_COMPACT);
        for m in &files {
            let selected = sel.as_ref().is_some_and(|x| stem(x) == stem(m));
            let info = meta.get(m).unwrap_or_default();
            let title = info.name.clone().unwrap_or_else(|| m.display_name().to_string());
            let size = if m.is_dir { "folder".to_string() } else { kit::text::size_label(m.size) };
            let update = update_for(m);
            let mut badge = match &info.version {
                Some(v) => format!("{v} · {size}"),
                None => size,
            };
            if let Some(u) = update {
                badge = format!("{badge} · update {}", u.to.version_number);
            }
            // The links a hot row shows beside its switch: each leads somewhere real.
            let config = configs.get(&m.path);
            let mut links: Vec<(&str, Link)> = Vec::new();
            if config.is_some() {
                links.push((if kind == ContentKind::ShaderPacks { "options" } else { "configs" }, Link::Config));
            }
            if update.is_some() && !busy {
                links.push(("update", Link::Update));
            }
            let links_w: f32 = links.iter().map(|(l, _)| kit::text::caption_galley(ui, l, t.text_muted).size().x + tk::GAP).sum();
            let image = info.icon.as_ref().map(|p| file_image(p));
            let mut row = kit::HoverRow::new(("installed", kind, &m.path), &title)
                .description(&m.name)
                .badge(&badge, if update.is_some() { t.text_strong } else { t.text_muted })
                .dim(!m.enabled)
                .selected(selected)
                .hot(selected && focused)
                .trail(tk::SWITCH_WIDTH, links_w);
            row = match image {
                Some(src) => row.lead(src),
                None => row.lead_blank(),
            };
            let tr = list.row_trail(ui, t, row);
            let r = tr.response.clone();
            if let Some(rect) = tr.rect {
                // The switch at the right end, the links (while hot) before it.
                let sw = egui::Rect::from_center_size(egui::pos2(rect.right() - tk::SWITCH_WIDTH / 2.0, rect.center().y), vec2(tk::SWITCH_WIDTH, tk::SWITCH_HEIGHT));
                let label = format!("{title} on");
                if kit::switch(&mut ui.new_child(egui::UiBuilder::new().max_rect(sw)), t, m.enabled, !busy, false, &label).clicked() {
                    toggle = Some(m.clone());
                }
                if tr.hot {
                    let mut x = rect.left();
                    for (l, link) in &links {
                        let g = kit::text::caption_galley(ui, l, t.text_muted);
                        let lr = egui::Rect::from_min_size(egui::pos2(x, rect.center().y - g.size().y / 2.0), g.size());
                        if caption_link(ui, t, lr, ("link", &m.path, *l), l).clicked() {
                            match link {
                                Link::Config => out.config = config.cloned(),
                                Link::Update => out.update = update.map(|u| vec![u.clone()]),
                            }
                        }
                        x += g.size().x + tk::GAP;
                    }
                }
            }
            if selected && scroll {
                r.scroll_to_me(None);
            }
            if r.clicked() {
                s.selected = Some(stem(m));
                out.to_installed = true;
            }
            if r.double_clicked() && !busy {
                toggle = Some(m.clone());
            }
            r.context_menu(|ui| {
                s.selected = Some(stem(m));
                if ui.add_enabled(!busy, egui::Button::new(if m.enabled { "turn off" } else { "turn on" })).clicked() {
                    toggle = Some(m.clone());
                    ui.close();
                }
                if let Some(name) = config
                    && ui.button(if kind == ContentKind::ShaderPacks { "options" } else { "configs" }).clicked()
                {
                    out.config = Some(name.clone());
                    ui.close();
                }
                if let Some(u) = update.filter(|_| !busy)
                    && ui.button(format!("update to {}", u.to.version_number)).clicked()
                {
                    out.update = Some(vec![u.clone()]);
                    ui.close();
                }
                if ui.add_enabled(!busy, egui::Button::new("remove")).clicked() {
                    out.remove = Some(m.clone());
                    ui.close();
                }
            });
        }
        list.end(ui);
    });
    if let Some(m) = toggle.filter(|_| !busy) {
        if let Err(e) = m.toggle() {
            out.error = Some(format!("{e:#}"));
        }
        s.cache = None;
    }
}

#[derive(Clone, Copy)]
enum Link {
    Config,
    Update,
}

/// A caption that leads somewhere, at `rect`: muted, accent when hovered.
pub(super) fn caption_link(ui: &mut Ui, t: &Tokens, rect: egui::Rect, id: impl std::hash::Hash + std::fmt::Debug, text: &str) -> egui::Response {
    let resp = ui.interact(rect.expand(4.0), ui.id().with(id), egui::Sense::click());
    let c = crate::design::motion::color(ui.ctx(), resp.id, resp.hovered() || resp.has_focus(), t.text_muted, t.accent);
    kit::text::paint_caption(ui.painter(), rect.left_center(), text, c);
    let label = text.to_string();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, true, &label));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[derive(Clone, Copy, PartialEq)]
enum Get {
    Install,
    Installing,
    Installed,
}

enum Details {
    Page,
    Install,
}

/// A Modrinth result opened for its details, under its row: what it's about in full, a line of
/// facts, then its Modrinth page (left) and install (right). Opens and closes over the medium
/// motion, the content rising with it; nothing is drawn while closed.
fn details(ui: &mut Ui, t: &Tokens, kind: ContentKind, h: &Hit, open: bool, get: Get, hot: bool) -> Option<Details> {
    use crate::design::motion::{self, Motion};
    let e = motion::toggle(ui.ctx(), egui::Id::new(("result-details", kind, &h.project_id)), open, Motion::Medium);
    if e <= 0.0 {
        return None;
    }
    let width = ui.available_width();
    // Lined up with the row's title.
    let x = tk::GUTTER + tk::HOVER_SHIFT_COMPACT;
    let text_w = (width - x - tk::GUTTER).max(120.0);
    let mut facts = vec![format!("{} downloads", modrinth::format_count(h.downloads))];
    if h.follows > 0 {
        facts.push(format!("{} followers", modrinth::format_count(h.follows)));
    }
    if h.date_modified.is_some() {
        facts.push(format!("updated {}", kit::text::ago(h.date_modified)));
    }
    facts.extend(h.categories.iter().take(4).map(|c| c.replace('-', " ")));
    let p = ui.painter();
    let facts_g = p.layout_job(kit::text::caption_wrapped(&facts.join(" · "), egui::Color32::PLACEHOLDER, text_w, 2));
    let mut job = egui::text::LayoutJob::simple(h.description.clone(), crate::design::fonts::text(tk::SIZE_BODY), egui::Color32::PLACEHOLDER, text_w.min(tk::SETTING_DESCRIPTION_WIDTH));
    job.wrap.max_rows = 8;
    let desc_g = p.layout_job(job);
    let full = tk::UNIT + desc_g.size().y + tk::UNIT + facts_g.size().y + tk::GAP + tk::BUTTON_HEIGHT + tk::GAP;
    let (rect, _) = ui.allocate_exact_size(vec2(width, (full * e).round()), egui::Sense::hover());
    let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    p.rect_filled(rect, 0.0, t.wash(0.06));
    let a = |c| tk::alpha_of(c, e);
    // The content rises into place as the row opens.
    let top = rect.top() - (1.0 - e) * tk::UNIT;
    let mut y = top + tk::UNIT;
    p.galley(egui::pos2(rect.left() + x, y), desc_g.clone(), a(t.text_strong));
    y += desc_g.size().y + tk::UNIT;
    p.galley(egui::pos2(rect.left() + x, y), facts_g.clone(), a(t.text_muted));
    y += facts_g.size().y + tk::GAP;
    let bar = egui::Rect::from_min_max(egui::pos2(rect.left() + x, y), egui::pos2(rect.right() - tk::GUTTER, y + tk::BUTTON_HEIGHT));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bar).layout(Layout::left_to_right(Align::Center)));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child.multiply_opacity(e);
    let mut out = None;
    if kit::Button::new("modrinth page").show(&mut child, t).clicked() {
        out = Some(Details::Page);
    }
    child.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let b = match get {
            Get::Install => kit::Button::new("install").hot(hot),
            Get::Installing => kit::Button::new("installing").enabled(false),
            Get::Installed => kit::Button::new("installed").icon(crate::design::glyphs::CHECK).enabled(false),
        };
        if b.show(ui, t).clicked() && get == Get::Install {
            out = Some(Details::Install);
        }
    });
    kit::rule::hline(ui.painter(), rect.x_range(), rect.bottom() - 0.5, t.line);
    out
}

/// How long typing has to pause before the Modrinth search goes out.
pub const SEARCH_DELAY: Duration = Duration::from_millis(250);

#[allow(clippy::too_many_arguments)]
fn browse(
    ui: &mut Ui,
    t: &Tokens,
    s: &mut KindState,
    meta: &MetaStore,
    remote: &RemoteIcons,
    inst: &Instance,
    kind: ContentKind,
    rt: &tokio::runtime::Handle,
    ctx: &egui::Context,
    focus: bool,
    kb: Option<Nav>,
    focused: bool,
    blocked: Option<&str>,
    out: &mut Out,
) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let count = s.results.as_ref().and_then(|r| r.as_ref().ok()).map_or(0, |h| h.len());
    let aside = match &s.results {
        _ if s.search_p.is_some() => "searching".to_string(),
        Some(Ok(_)) => format!("{count} results"),
        _ => String::new(),
    };
    pane_head(ui, t, 2, "modrinth", &aside, &[], out.head_h);
    ui.add_space(tk::GAP);
    let hint = format!("Search {}s", kind.noun());
    let r = padded(ui, |ui| kit::query(ui, t, &mut s.query, &hint, tk::SIZE_HEADING, focus));
    if r.changed() {
        s.typed = Some(Instant::now());
        ctx.request_repaint_after(SEARCH_DELAY);
    }
    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let unsent = s.searched.as_ref().is_none_or(|(_, q)| *q != s.query);
    if unsent && (enter || s.typed.is_some_and(|at| at.elapsed() >= SEARCH_DELAY)) {
        s.search(kind, inst, rt, ctx);
    }
    if enter {
        out.to_results = true;
    }
    let mut scroll = false;
    if let Some(p) = kb.filter(|n| n.is_move()).and_then(|n| n.step(count, s.result)) {
        s.result = Some(p);
        scroll = true;
    }
    let install_key = kb == Some(Nav::Activate);
    ui.add_space(tk::GAP);
    let files = s.files(inst, kind);
    let installed: Vec<String> = files.iter().map(|m| m.display_name().to_lowercase()).collect();
    let installed_projects = meta.project_ids(&files);
    let mut install: Option<Hit> = None;
    egui::ScrollArea::vertical().id_salt(("results", kind)).auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        match &s.results {
            None => {
                padded(ui, |ui| kit::loading(ui, t, "searching modrinth"));
            }
            Some(Err(e)) => {
                padded(ui, |ui| {
                    ui.add_space(tk::GUTTER);
                    kit::text::body_colored(ui, e, t.error());
                });
            }
            Some(Ok(hits)) if hits.is_empty() => {
                padded(ui, |ui| kit::empty_state(ui, t, "No results", "Try other words, or fewer of them.", None));
            }
            Some(Ok(hits)) => {
                kit::rule(ui, t, false);
                let mut list = kit::HoverList::begin(ui, kit::Variant::Compact).indent(tk::GUTTER + tk::HOVER_SHIFT_COMPACT);
                for (n, h) in hits.iter().enumerate() {
                    let picked = s.result == Some(n);
                    let open = s.open.as_deref() == Some(h.project_id.as_str());
                    let job = (inst.id.clone(), h.project_id.clone());
                    let have = installed_projects.contains(&h.project_id) || installed.iter().any(|n| is_slug_file(n, &h.slug));
                    let busy = s.installing.contains_key(&job);
                    // Hot reveals who made it; a click opens the row for the rest.
                    let by = format!("by {}", h.author);
                    let downloads = format!("{} downloads", modrinth::format_count(h.downloads));
                    let icon = h.icon_url.as_deref().and_then(|u| remote.get(u, rt, ctx));
                    let mut row = kit::HoverRow::new(("result", kind, &h.project_id), &h.title).description(&by).hot(picked && focused).selected(picked || open);
                    row = if busy {
                        row.badge("installing", t.text_muted)
                    } else if have {
                        row.badge("installed", t.text_muted).badge_icon(crate::design::glyphs::CHECK)
                    } else {
                        row.badge(&downloads, t.text_muted)
                    };
                    if let Some(p) = &icon {
                        row = row.image(file_image(p), 1.0);
                    }
                    let r = list.row(ui, t, row);
                    // Enter opens the keyboard's row, and installs from it once it's open.
                    let enter = picked && install_key;
                    if r.clicked() || (enter && !open) {
                        s.open = if open && r.clicked() { None } else { Some(h.project_id.clone()) };
                        s.result = Some(n);
                        out.to_results = true;
                    }
                    let state = if busy { Get::Installing } else if have { Get::Installed } else { Get::Install };
                    match details(ui, t, kind, h, open, state, picked && focused) {
                        Some(Details::Page) => super::open_url(&format!("https://modrinth.com/project/{}", h.slug)),
                        Some(Details::Install) => install = Some(h.clone()),
                        None => {}
                    }
                    if enter && open && state == Get::Install {
                        install = Some(h.clone());
                    }
                    if picked && scroll {
                        r.scroll_to_me(None);
                    }
                }
                list.end(ui);
            }
        }
    });
    if let Some(h) = install {
        if let Some(why) = blocked {
            out.error = Some(why.into());
        } else {
            let job = (inst.id.clone(), h.project_id.clone());
            let task = Task::new(format!("installing {}", h.title));
            task.stage(format!("Installing {}", h.title), 0);
            let (i, pid, tk) = (inst.clone(), h.project_id.clone(), task.clone());
            let p = Promise::spawn(rt, ctx, async move {
                let r = modrinth::install(kind, &pid, &i).await;
                tk.finish(&r.as_ref().map(|_| ()).map_err(|e| anyhow::anyhow!("{e:#}")));
                r
            });
            let dir = inst.game_dir().join(kind.dir());
            s.installing.insert(job, Job { task, promise: p, title: h.title.clone(), dir });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mod jar with a fabric.mod.json, as `mods/<id>.jar`.
    fn jar(inst: &Instance, id: &str, version: &str) {
        use std::io::Write;
        let dir = inst.game_dir().join("mods");
        std::fs::create_dir_all(&dir).unwrap();
        let mut z = zip::ZipWriter::new(std::fs::File::create(dir.join(format!("{id}.jar"))).unwrap());
        z.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default()).unwrap();
        write!(z, r#"{{"schemaVersion": 1, "id": "{id}", "version": "{version}"}}"#).unwrap();
        z.finish().unwrap();
    }

    #[test]
    fn a_rollback_offers_the_worlds_played_with_newer_mods() {
        use crate::backups::{self, Owner};
        let mut inst = Instance::new("prompt", "1.21.1", crate::loaders::Loader::Fabric("0.16.5".into()));
        inst.assign_id().unwrap();
        jar(&inst, "create", "6.0.0");
        let saves = inst.saves_dir();
        for w in ["Played", "Untouched"] {
            std::fs::create_dir_all(saves.join(w)).unwrap();
            std::fs::write(saves.join(w).join("level.dat"), "").unwrap();
        }
        let world = saves.join("Played");
        crate::world_sets::write_records(&inst, &[world.clone(), saves.join("Untouched")]);

        // Update create, backing up as `update` does (the world has no backup yet), then
        // play the world with the new one.
        std::thread::sleep(Duration::from_millis(10));
        modrinth::back_up_for_update(&inst, "before the update", 5).unwrap();
        let mods = backups::instance_snapshots(&inst.id)[0].dir.clone();
        jar(&inst, "create", "6.1.0");
        crate::world_sets::write_records(&inst, std::slice::from_ref(&world));
        std::thread::sleep(Duration::from_millis(10));
        backups::snapshot_world(&world, Owner::of(&inst), "after playing", true).unwrap();

        let snap = backups::instance_snapshots(&inst.id).into_iter().find(|s| s.dir == mods).unwrap();
        backups::roll_back(&inst, &snap, 5).unwrap();
        let prompts = world_prompts(&inst, snap.info.taken);
        assert_eq!(prompts.len(), 1, "only the world played with the newer create");
        let p = &prompts[0];
        assert_eq!(p.name, "Played");
        assert!(p.what.contains("create 6.1.0"), "{}", p.what);
        // The world as it was before the update is picked, not the newer one.
        assert_eq!(p.snaps.len(), 2);
        assert_eq!(p.snaps[p.selected.unwrap()].info.reason, "before the update");
        // With no backup from before the cutoff, none is picked (not the newest).
        let early = p.snaps[1].info.taken - chrono::Duration::seconds(1);
        assert_eq!(world_prompts(&inst, early)[0].selected, None);
        std::fs::remove_dir_all(inst.dir()).unwrap();
        backups::forget_instance(&inst.id).unwrap();
    }

    /// G17: a found update stays on offer only for the game it was checked for and a file
    /// still there, enabled or not.
    #[test]
    fn found_updates_go_stale_with_the_game_or_the_file() {
        let mut inst = Instance::new("stale", "1.21.1", crate::loaders::Loader::Fabric("0.16.5".into()));
        inst.id = "stale".into();
        let dir = std::path::Path::new("/i/mods");
        let up = |name: &str| Update {
            path: dir.join(name),
            sha1: String::new(),
            game: modrinth::game_key(&inst),
            title: name.into(),
            from: "1".into(),
            to: modrinth::Version { id: "x".into(), project_id: "p".into(), version_number: "2".into(), files: vec![], loaders: vec![], game_versions: vec![], version_type: String::new(), dependencies: vec![], date_published: None },
        };
        let list = [up("a.jar"), up("b.jar")];
        // a was disabled since the check, b removed.
        let files = [file(dir, "a.jar.disabled", 1, 1)];
        assert_eq!(current_updates(&list, &inst, &files).iter().map(|u| u.title.as_str()).collect::<Vec<_>>(), ["a.jar"]);
        inst.mc_version = "1.21.4".into();
        assert!(current_updates(&list, &inst, &files).is_empty(), "checked for another game");
    }

    /// G16: a second instance's update doesn't drop the first one's: both stay busy
    /// until each is done, and each reports.
    #[test]
    fn every_mod_job_stays_tracked_until_it_ends() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let ctx = egui::Context::default();
        let mut s = ContentState::default();
        let (go_a, wait_a) = tokio::sync::oneshot::channel::<()>();
        let (go_b, wait_b) = tokio::sync::oneshot::channel::<()>();
        s.start_mod_job(rt.handle(), &ctx, "a", None, Task::new("a"), async move {
            let _ = wait_a.await;
            ModJobDone::new(ToastKind::Info, "a done".into())
        });
        s.start_mod_job(rt.handle(), &ctx, "b", None, Task::new("b"), async move {
            let _ = wait_b.await;
            ModJobDone::new(ToastKind::Info, "b done".into())
        });
        assert!(s.busy_with("a") && s.busy_with("b"));
        assert_eq!(s.tasks().count(), 2);

        let mut toasts = Vec::new();
        let settle = |s: &mut ContentState, toasts: &mut Vec<Toast>, want: usize| {
            let start = Instant::now();
            while toasts.len() < want {
                assert!(start.elapsed() < Duration::from_secs(5), "job never finished");
                std::thread::sleep(Duration::from_millis(5));
                s.poll(toasts);
            }
        };
        go_b.send(()).unwrap();
        settle(&mut s, &mut toasts, 1);
        assert!(s.busy_with("a") && !s.busy_with("b"));
        go_a.send(()).unwrap();
        settle(&mut s, &mut toasts, 2);
        assert!(!s.busy_with("a"));
        assert_eq!(toasts.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["b done", "a done"]);
    }

    fn file(dir: &std::path::Path, name: &str, size: u64, secs: u64) -> ModFile {
        let modified = Some(std::time::UNIX_EPOCH + Duration::from_secs(secs));
        ModFile { path: dir.join(name), name: name.into(), enabled: !name.ends_with(".disabled"), size, modified, is_dir: false }
    }

    #[test]
    fn slug_matches_whole_name_only() {
        for n in ["sodium", "sodium-0.6.0", "Sodium-fabric-0.5.8+mc1.20.1", "sodium_v1.2", "sodium-mc1.21-0.6", "sodium-neoforge-0.6.0"] {
            assert!(is_slug_file(n, "sodium"), "{n}");
        }
        for n in ["sodium-extra-0.5.4", "sodiumextra-1.0", "sodium-fabric", "reeses-sodium-options-1.7", "sodium-", "sod"] {
            assert!(!is_slug_file(n, "sodium"), "{n}");
        }
    }

    /// B8: "installed" comes from this instance's files as they are now, not from any instance ever seen.
    #[test]
    fn installed_projects_are_per_instance_and_current() {
        let (a, b) = (std::path::Path::new("/a/mods"), std::path::Path::new("/b/mods"));
        let meta = MetaStore::default();
        let sodium = file(a, "sodium.jar", 10, 1);
        let m = FileMeta { project_id: Some("AANobbMI".into()), ..Default::default() };
        meta.known.lock().unwrap().insert(meta_key(&sodium), m);

        assert!(meta.project_ids(std::slice::from_ref(&sodium)).contains("AANobbMI"));
        // Disabling keeps it; another instance, a removed file or a replacement (new mtime) don't.
        assert!(meta.project_ids(&[file(a, "sodium.jar.disabled", 10, 1)]).contains("AANobbMI"));
        assert!(meta.project_ids(&[file(b, "sodium.jar", 10, 1)]).is_empty());
        assert!(meta.project_ids(&[]).is_empty());
        assert!(meta.project_ids(&[file(a, "sodium.jar", 10, 2)]).is_empty());

        // Our own installs count before the hash lookup, in that instance only.
        let done = modrinth::Installed { files: vec!["lithium.jar".into()], projects: vec!["gvQqBUqZ".into()], missing: vec![] };
        meta.installed(a, &done);
        assert!(meta.project_ids(&[file(a, "lithium.jar", 5, 1)]).contains("gvQqBUqZ"));
        assert!(meta.project_ids(&[file(b, "lithium.jar", 5, 1)]).is_empty());
    }

    /// F4 (G28): a mod added while the game starts would be loaded but missing from the
    /// mods read for its play records, so drops and launches hold each other off.
    #[test]
    fn mod_drops_and_launches_wait_for_each_other() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let ctx = egui::Context::default();
        let mut inst = Instance::new("drop launch", "1.21.1", crate::loaders::Loader::Fabric("0.16.5".into()));
        inst.assign_id().unwrap();
        let src = crate::paths::test_root().join(format!("drop-launch-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(src.join("Pack")).unwrap();
        std::fs::write(src.join("late.jar"), "j").unwrap();
        let worlds = super::super::worlds::WorldsState::default();
        let blocked = |c: &ContentState| super::super::worlds::launch_blocked(c, &worlds, &inst);

        // Drop, then launch: the launch waits while the drop's job is tracked, done or not.
        let mut content = ContentState::default();
        content.start_drop(rt.handle(), &ctx, &inst, ContentKind::ResourcePacks, vec![src.join("Pack")], "Pack".into());
        assert!(blocked(&content).is_none(), "a pack install doesn't hold off a launch");
        content.start_drop(rt.handle(), &ctx, &inst, ContentKind::Mods, vec![src.join("late.jar")], "late.jar".into());
        assert_eq!(blocked(&content), Some("mods are being added to it"));
        let start = Instant::now();
        while !content.kinds[&ContentKind::Mods].installing.values().all(|j| j.promise.ready()) {
            assert!(start.elapsed() < Duration::from_secs(5), "the drop never finished");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(blocked(&content).is_some(), "a finished drop counts until its result is taken");
        content.kinds.get_mut(&ContentKind::Mods).unwrap().installing.clear();
        assert!(blocked(&content).is_none());

        // Launch, then drop: the game counts as starting, which install_blocked refuses mod
        // drops for, until Java has a pid (after the mods were read and it was spawned).
        let log: crate::launch::SharedLog = Default::default();
        let s = super::super::Session { log: log.clone(), task: Task::new("launch"), phase: super::super::Phase::Game(Promise::spawn(rt.handle(), &ctx, std::future::pending())) };
        assert!(s.starting());
        log.lock().unwrap().pid = Some(1);
        assert!(!s.starting() && s.running());
        std::fs::remove_dir_all(inst.dir()).unwrap();
        std::fs::remove_dir_all(&src).unwrap();
    }
}
