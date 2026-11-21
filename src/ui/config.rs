//! The configs tab, in three parts: Minecraft's own settings (`options.txt`), the mods'
//! configs (grouped by the mod they belong to, TOML/JSON/JSON5 as fields, the rest as text)
//! and shader packs' options. Saving writes back only what changed (see `mod_config`), and
//! never over a file the game rewrote meanwhile without asking.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Layout, Rect, Sense, Ui, UiBuilder, vec2};

use super::instances::InstTab;
use super::nav::{Nav, Zone};
use super::content::padded;
use super::{App, Confirm, ConfirmAction, Modal, Tab, ToastKind};
use crate::instance::Instance;
use crate::keymap::Action;
use crate::mod_config::{self as mc, Doc, Field, Format, Group, ModInfo, Scalar, Value};
use crate::{fsutil, paths, shader_config};
use crate::task::Promise;
use crate::design::kit::{self, Button, HoverList, HoverRow, SettingGroup, SettingRow, Variant};
use crate::design::tokens::{self as tk, Tokens};
use crate::design::{fonts, glyphs};

/// How often the list of files is read again while the tab is open.
const RELIST: Duration = Duration::from_secs(5);
/// How often the open file is checked for changes on disk (the game rewrites configs).
const RECHECK: Duration = Duration::from_secs(2);
/// Text bigger than this is shown without colours (they're worked out every frame).
const HIGHLIGHT_MAX: usize = 200_000;

struct Listing {
    inst_id: String,
    groups: Vec<Group>,
    packs: Vec<shader_config::Pack>,
    /// Which shader mod picks the pack in use, if any.
    switch: Option<shader_config::Switch>,
    at: Instant,
}

#[derive(Default)]
pub struct ConfigState {
    listing: Option<Listing>,
    listing_p: Option<(String, Promise<(Vec<Group>, Vec<shader_config::Pack>, Option<shader_config::Switch>)>)>,
    /// Jars already read for their names, kept between listings.
    mods: Arc<Mutex<HashMap<String, ModInfo>>>,
    /// Relative to `config/`.
    selected: Option<PathBuf>,
    scroll_file: bool,
    file_filter: String,
    field_filter: String,
    /// The selected field, by [`field_id`].
    field_sel: Option<String>,
    scroll_field: bool,
    pub open: Option<Open>,
    loading: Option<(String, Area, PathBuf, Promise<mc::Loaded>)>,
    load_error: Option<(Area, PathBuf, String)>,
    /// Save or text/fields, routed here by the app (save also works while typing).
    pub key: Option<Action>,
    checked: Option<Instant>,
    /// The open file being checked on disk in the background.
    recheck: Option<Recheck>,
    /// Long lists the user unfolded, by [`field_id`].
    unfolded: HashSet<String>,
    /// File groups and settings headers that are open (all start folded), as left last time: see [`fold_key`].
    folds: Option<BTreeSet<String>>,
    /// Where the keyboard is in the file list: a group's heading or a file.
    cursor: Option<Item>,
    /// Which part of the tab is showing.
    pub area: Area,
    /// The shader packs, read with the file list.
    packs: Vec<shader_config::Pack>,
    /// A key binding waiting for its key (by [`field_id`]); the app sends key presses here meanwhile.
    pub capture: Option<String>,
    /// Modifiers held when the capture began, so a newly pressed shift, ctrl or alt can be told apart.
    capture_mods: Option<egui::Modifiers>,
    /// A captured binding, to apply: (field, `key.keyboard.…`).
    captured: Option<(String, String)>,
    /// When the last capture ended: the click that ended it mustn't start another.
    capture_ended: Option<Instant>,
}

/// The configs tab's three parts.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Area {
    #[default]
    Minecraft,
    Mods,
    Shaders,
}

impl Area {
    const ALL: [(Area, &'static str); 3] = [(Area::Minecraft, "minecraft"), (Area::Mods, "mods"), (Area::Shaders, "shaders")];

    fn key(self) -> &'static str {
        match self {
            Area::Minecraft => "minecraft",
            Area::Mods => "mods",
            Area::Shaders => "shaders",
        }
    }

    fn next(self) -> Area {
        match self {
            Area::Minecraft => Area::Mods,
            Area::Mods => Area::Shaders,
            Area::Shaders => Area::Minecraft,
        }
    }
}

/// Minecraft's own settings file, in the game folder.
const OPTIONS: &str = "options.txt";

/// A line of the file list.
#[derive(Clone, PartialEq)]
enum Item {
    Group(String),
    File(PathBuf),
}

/// Open file groups and settings headers, kept between runs.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Folds {
    #[serde(default)]
    open: BTreeSet<String>,
}

fn folds_path() -> PathBuf {
    paths::data_dir().join("config-view.toml")
}

/// The key a fold is remembered by: the instance, then the group's name or the file and its header.
fn fold_key(inst_id: &str, parts: &[&str]) -> String {
    std::iter::once(inst_id).chain(parts.iter().copied()).collect::<Vec<_>>().join("\u{1f}")
}

impl ConfigState {
    fn folds(&mut self) -> &mut BTreeSet<String> {
        self.folds.get_or_insert_with(|| std::fs::read_to_string(folds_path()).ok().and_then(|t| toml::from_str::<Folds>(&t).ok()).unwrap_or_default().open)
    }

    fn is_open(&mut self, key: &str) -> bool {
        self.folds().contains(key)
    }

    /// Open or fold `key`, and remember it.
    fn set_open(&mut self, key: String, open: bool) {
        let f = self.folds();
        let changed = if open { f.insert(key) } else { f.remove(&key) };
        if changed && let Ok(text) = toml::to_string(&Folds { open: f.clone() }) {
            let _ = fsutil::write_atomic(&folds_path(), text.as_bytes(), 0o644);
        }
    }

    /// Read `inst`'s config files in the background, now and then, while a tab needs them.
    fn refresh(&mut self, inst: &Instance, rt: &tokio::runtime::Handle, ctx: &egui::Context) {
        if self.listing.as_ref().is_some_and(|l| l.inst_id != inst.id) {
            self.listing = None;
            self.selected = None;
            self.cursor = None;
            self.file_filter.clear();
            self.field_filter.clear();
            self.field_sel = None;
        }
        if let Some((id, p)) = &self.listing_p
            && let Some(r) = p.take()
        {
            if *id == inst.id {
                let (groups, packs, switch) = r.unwrap_or_default();
                self.packs = packs.clone();
                self.listing = Some(Listing { inst_id: id.clone(), groups, packs, switch, at: Instant::now() });
                // A file that couldn't be opened gets another try with each fresh listing.
                self.load_error = None;
            }
            self.listing_p = None;
        }
        let stale = self.listing.as_ref().is_none_or(|l| l.at.elapsed() > RELIST);
        if stale && self.listing_p.as_ref().is_none_or(|(id, _)| *id != inst.id) {
            let (i, cache) = (inst.clone(), self.mods.clone());
            let p = Promise::spawn(rt, ctx, async move {
                Ok(tokio::task::spawn_blocking(move || {
                    let files = mc::list(&i);
                    let mods = mc::known_mods(&i, &mut cache.lock().unwrap());
                    (mc::group(&files, &mods), shader_config::packs(&i), shader_config::switch(&i))
                })
                .await?)
            });
            self.listing_p = Some((inst.id.clone(), p));
            ctx.request_repaint_after(RELIST);
        }
    }

    /// `inst`'s shader packs' names, once listed.
    pub fn pack_names(&mut self, inst: &Instance, rt: &tokio::runtime::Handle, ctx: &egui::Context) -> Vec<String> {
        self.refresh(inst, rt, ctx);
        self.listing.as_ref().map(|l| l.packs.iter().map(|p| p.name.clone()).collect()).unwrap_or_default()
    }

    /// `inst`'s mods' config groups, as (the mod ids each answers to, its name), once its files are listed.
    pub fn mod_groups(&mut self, inst: &Instance, rt: &tokio::runtime::Handle, ctx: &egui::Context) -> Vec<(Vec<String>, String)> {
        self.refresh(inst, rt, ctx);
        let Some(l) = &self.listing else { return Vec::new() };
        l.groups.iter().filter(|g| g.name != mc::OTHER).map(|g| (g.ids.clone(), g.name.clone())).collect()
    }
}


/// Show shader pack `name`'s options.
pub fn open_pack(app: &mut App, name: &str) {
    app.inst_tab = InstTab::Config;
    app.focus = Zone::Main;
    app.config.cursor = Some(Item::File(PathBuf::from(name)));
    app.config.scroll_file = true;
    pick(app, Area::Shaders, PathBuf::from(name));
}

/// Show the config files of `inst`'s mod group `name`: the group open and its first file selected.
pub fn open_group(app: &mut App, inst: &Instance, name: &str) {
    let Some(first) = app.config.listing.as_ref().and_then(|l| l.groups.iter().find(|g| g.name == name)).and_then(|g| g.files.first()).map(|f| f.rel.clone()) else { return };
    app.config.set_open(fold_key(&inst.id, &["group", name]), true);
    app.inst_tab = InstTab::Config;
    app.config.area = Area::Mods;
    app.focus = Zone::Main;
    app.config.cursor = Some(Item::File(first.clone()));
    app.config.scroll_file = true;
    pick(app, Area::Mods, first);
}

/// The file being edited.
pub struct Open {
    pub inst_id: String,
    pub area: Area,
    /// Relative to `config/` for mods; the pack's name for shaders; `options.txt` for Minecraft.
    pub rel: PathBuf,
    path: PathBuf,
    format: Format,
    /// The text as it is on disk (as last loaded or saved).
    saved: String,
    text: String,
    stamp: Option<mc::Stamp>,
    doc: Result<Doc, mc::ParseError>,
    /// Each field's value on disk, to mark the ones changed here.
    saved_values: HashMap<String, Value>,
    raw: bool,
    /// The file changed on disk while there were unsaved changes here: its text now.
    conflict: Option<String>,
    /// A quiet word about what just happened (reloaded, saved).
    note: Option<(String, Instant)>,
    /// A shader pack's options, which its settings file's values are laid over.
    base: Option<Arc<Doc>>,
    /// Changes whenever `saved` or `stamp` does (loaded, reloaded, saved), so a check of the file
    /// made before that is dropped rather than applied over it.
    generation: u64,
}

fn next_generation() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Which open file a check was made of: instance, area, file and [`Open::generation`].
#[derive(Debug, Clone, PartialEq)]
struct CheckOf {
    inst_id: String,
    area: Area,
    rel: PathBuf,
    generation: u64,
}

/// A check of the open file on disk, run off the UI thread (the file may be big, or on slow storage).
struct Recheck {
    of: CheckOf,
    promise: Promise<Reread>,
}

/// What a check found: the file's stamp now and, if it changed, its text and fields.
struct Reread {
    stamp: Option<mc::Stamp>,
    changed: Option<(String, Result<Doc, mc::ParseError>)>,
}

/// Read and parse `path` again if its stamp isn't `was` any more. Blocking: run it on a Promise.
fn reread(path: &std::path::Path, was: Option<mc::Stamp>, format: Format, base: Option<&Doc>) -> Reread {
    let stamp = mc::Stamp::of(path);
    if stamp == was {
        return Reread { stamp, changed: None };
    }
    let disk = if path.exists() { mc::read_text(path).ok() } else { Some(String::new()) };
    let changed = disk.map(|d| {
        let doc = mc::parse_with(format, base, &d);
        (d, doc)
    });
    Reread { stamp, changed }
}

fn field_id(f: &Field) -> String {
    format!("{}\u{1f}{}", f.section.join("\u{1f}"), f.key)
}

fn values(doc: &Result<Doc, mc::ParseError>) -> HashMap<String, Value> {
    doc.as_ref().map(|d| d.fields.iter().map(|f| (field_id(f), f.value.clone())).collect()).unwrap_or_default()
}

impl Open {
    fn new(inst_id: String, area: Area, rel: PathBuf, loaded: mc::Loaded) -> Self {
        let format = loaded.format;
        let doc = mc::parse_with(format, loaded.base.as_deref(), &loaded.text);
        // Fields where there are some to show; text otherwise.
        let raw = !format.structured() || doc.as_ref().map_or(true, |d| d.fields.is_empty());
        Open {
            inst_id,
            area,
            rel,
            path: loaded.path,
            format,
            saved_values: values(&doc),
            saved: loaded.text.clone(),
            text: loaded.text,
            stamp: loaded.stamp,
            doc,
            raw,
            conflict: None,
            note: None,
            base: loaded.base,
            generation: next_generation(),
        }
    }

    fn check_of(&self) -> CheckOf {
        CheckOf { inst_id: self.inst_id.clone(), area: self.area, rel: self.rel.clone(), generation: self.generation }
    }

    /// Take what a background check found, if it was made of this very file as it is here now;
    /// false (and nothing changed) if it wasn't.
    fn take_reread(&mut self, of: &CheckOf, r: Reread) -> bool {
        if *of != self.check_of() {
            return false;
        }
        self.stamp = r.stamp;
        if let Some((disk, doc)) = r.changed
            && disk != self.saved
        {
            if self.dirty() {
                self.conflict = Some(disk);
            } else {
                self.saved = disk.clone();
                self.text = disk;
                self.doc = doc;
                self.saved_values = values(&self.doc);
                self.conflict = None;
                self.generation = next_generation();
                self.note = Some(("reloaded: the file changed on disk".into(), Instant::now()));
            }
        }
        true
    }

    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }

    /// Shown as text rather than fields.
    pub fn raw_mode(&self) -> bool {
        self.raw || !self.format.structured()
    }

    /// Whether it can be shown as fields at all.
    pub fn has_fields(&self) -> bool {
        self.format.structured()
    }

    fn set_text(&mut self, text: String) {
        self.text = text;
        self.doc = mc::parse_with(self.format, self.base.as_deref(), &self.text);
    }

    /// Take the file as it is on disk now.
    fn reload(&mut self, text: String) {
        self.saved = text.clone();
        self.set_text(text);
        self.saved_values = values(&self.doc);
        self.stamp = mc::Stamp::of(&self.path);
        self.conflict = None;
        self.generation = next_generation();
    }

    fn name(&self) -> String {
        self.rel.file_name().unwrap_or_default().to_string_lossy().into_owned()
    }
}

/// Somewhere to go once unsaved changes are dealt with.
pub enum Away {
    File(Area, PathBuf),
    View { tab: Tab, inst_tab: InstTab, selected: Option<String> },
    Quit,
}

/// "Unsaved changes" before leaving a file.
pub struct Leave {
    away: Away,
}

/// Files in list order, as filtered.
fn visible(groups: &[Group], filter: &str) -> Vec<(usize, Vec<PathBuf>)> {
    let f = filter.trim().to_lowercase();
    groups
        .iter()
        .enumerate()
        .map(|(n, g)| {
            let all = f.is_empty() || g.name.to_lowercase().contains(&f);
            (n, g.files.iter().filter(|c| all || c.rel.to_string_lossy().to_lowercase().contains(&f)).map(|c| c.rel.clone()).collect::<Vec<_>>())
        })
        .filter(|(_, files)| !files.is_empty())
        .collect()
}

fn shown_fields<'a>(doc: &'a Doc, filter: &str) -> Vec<(usize, &'a Field)> {
    let f = filter.trim().to_lowercase();
    doc.fields
        .iter()
        .enumerate()
        .filter(|(_, x)| f.is_empty() || x.key.to_lowercase().contains(&f) || x.doc.to_lowercase().contains(&f) || x.section.iter().any(|s| s.to_lowercase().contains(&f)))
        .collect()
}

/// Ask before leaving a file with unsaved changes; otherwise go straight there.
fn pick_file(app: &mut App, rel: PathBuf) {
    pick(app, app.config.area, rel);
}

/// Show `rel` in `area`, asking first if the open file has unsaved changes.
fn pick(app: &mut App, area: Area, rel: PathBuf) {
    let s = &mut app.config;
    if s.open.as_ref().is_some_and(|o| o.dirty() && (o.rel != rel || o.area != area)) {
        app.modal = Some(Modal::ConfigLeave(Box::new(Leave { away: Away::File(area, rel) })));
        return;
    }
    if s.area != area {
        s.area = area;
        s.selected = None;
    }
    if s.selected.as_ref() != Some(&rel) {
        s.selected = Some(rel);
        s.field_sel = None;
        s.field_filter.clear();
    }
}

/// What a frame of the tab asked for, done once drawing is over.
#[derive(Default)]
struct Out {
    pick: Option<PathBuf>,
    /// Open or fold this group or header ([`fold_key`]).
    fold: Option<String>,
    /// Wait for a key for this binding.
    capture: Option<String>,
    /// Put every setting with a known default back to it.
    defaults: bool,
    /// Switch shaders to this pack, or off.
    use_pack: Option<Option<String>>,
    scroll_file: bool,
    edit: Option<(usize, Value)>,
    save: bool,
    revert: bool,
    raw: Option<bool>,
    take_disk: bool,
    keep_mine: bool,
    focus: Option<Zone>,
}

/// Read `rel` of `area` for editing.
fn load_for(area: Area, inst: &Instance, rel: &std::path::Path) -> anyhow::Result<mc::Loaded> {
    match area {
        Area::Mods => mc::load(inst, rel),
        Area::Shaders => shader_config::load(inst, &rel.to_string_lossy()),
        Area::Minecraft => {
            let path = inst.game_dir().join(OPTIONS);
            let text = mc::read_text(&path)?;
            Ok(mc::Loaded { stamp: mc::Stamp::of(&path), format: Format::Options, base: None, path, text })
        }
    }
}

pub fn show(app: &mut App, ui: &mut Ui, inst: &Instance) {
    let t = app.look.tokens.clone();
    let ctx = ui.ctx().clone();
    let running = app.is_running(&inst.id);
    let list_nav = app.nav_for(Zone::Main);
    let edit_nav = app.nav_for(Zone::Side);
    let focus = app.focus;
    let key = app.config.key.take();
    let rt = app.rt.clone();
    let icons = if app.config.area == Area::Shaders { app.content.icons(inst, crate::instance::ContentKind::ShaderPacks, &rt, &ctx) } else { HashMap::new() };
    let s = &mut app.config;

    // Another instance's file with nothing unsaved: let it go (unsaved ones wait in `guard`).
    if s.open.as_ref().is_some_and(|o| o.inst_id != inst.id && !o.dirty()) {
        s.open = None;
    }
    s.refresh(inst, &rt, &ctx);
    s.folds();
    let mut out = Out::default();

    // The three parts, as the page's sub-tabs. Switching shows the file left open there, if any.
    let mut area = s.area;
    // Too narrow for the list and the editor side by side: one at a time, the one with the keyboard.
    let narrow = area != Area::Minecraft && ui.available_width() < tk::CONTENT_TWO_PANES;
    let mut side = focus == Zone::Side;
    padded(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = tk::GAP;
            kit::segmented(ui, &t, &mut area, &Area::ALL);
            if narrow {
                if kit::segmented(ui, &t, &mut side, &[(false, if area == Area::Shaders { "packs" } else { "files" }), (true, "settings")]) {
                    out.focus = Some(if side { Zone::Side } else { Zone::Main });
                }
                return;
            }
            let what = match area {
                Area::Minecraft => "the game's own settings: options.txt",
                Area::Mods => "each mod's files in config/",
                Area::Shaders => "each shader pack's options",
            };
            ui.add(egui::Label::new(kit::text::caption_job(what, t.text_muted)).selectable(false).truncate());
        });
    });
    ui.add_space(tk::GAP);
    if key == Some(Action::ConfigArea) {
        area = s.area.next();
    }
    if area != s.area {
        s.area = area;
        s.selected = s.open.as_ref().filter(|o| o.area == area && o.inst_id == inst.id).map(|o| o.rel.clone());
        s.field_sel = None;
        s.field_filter.clear();
        s.cursor = None;
    }
    let Some(listing) = &s.listing else {
        padded(ui, |ui| kit::loading(ui, &t, "looking for config files"));
        return;
    };
    let groups = listing.groups.clone();
    let packs = listing.packs.clone();
    let switch = listing.switch;

    // What to list on the left, and what the editor's keys are.
    let mut edit_nav = edit_nav;
    let mut editor_focused = focus == Zone::Side;
    let mut items: Vec<Item> = Vec::new();
    match s.area {
        Area::Minecraft => {
            if !inst.game_dir().join(OPTIONS).is_file() {
                padded(ui, |ui| kit::empty_state(ui, &t, "No options yet", "Minecraft writes options.txt the first time it starts. Play once, then come back here.", None));
                return;
            }
            s.selected = Some(PathBuf::from(OPTIONS));
            // One file, no list: the editor has the keyboard.
            edit_nav = list_nav.or(edit_nav);
            editor_focused = matches!(focus, Zone::Main | Zone::Side);
        }
        Area::Mods => {
            if groups.is_empty() {
                padded(ui, |ui| kit::empty_state(ui, &t, "No config files yet", "Mods write their settings into config/ the first time the game starts with them. Play once, then come back here.", None));
                return;
            }
            let shown = visible(&groups, &s.file_filter);
            let all: Vec<&PathBuf> = groups.iter().flat_map(|g| g.files.iter().map(|f| &f.rel)).collect();
            // A file that's gone (and not being edited) drops its selection.
            if s.selected.as_ref().is_some_and(|sel| !all.contains(&sel) && s.open.as_ref().is_none_or(|o| o.rel != *sel || !o.dirty())) {
                s.selected = None;
            }
            // A mod with one file is that file; others are a heading, and their files when open (always while
            // filtering, so what matches shows).
            let filtering = !s.file_filter.trim().is_empty();
            for (n, files) in &shown {
                let g = &groups[*n];
                if g.files.len() == 1 {
                    items.push(Item::File(g.files[0].rel.clone()));
                    continue;
                }
                items.push(Item::Group(g.name.clone()));
                if filtering || s.is_open(&fold_key(&inst.id, &["group", &g.name])) {
                    items.extend(files.iter().cloned().map(Item::File));
                }
            }
        }
        Area::Shaders => {
            if packs.is_empty() {
                padded(ui, |ui| kit::empty_state(ui, &t, "No shader packs", "Add packs in mods & packs, under shader packs; their options show up here.", None));
                return;
            }
            if s.selected.as_ref().is_some_and(|sel| !packs.iter().any(|p| *sel == PathBuf::from(&p.name))) {
                s.selected = None;
            }
            items = packs.iter().map(|p| Item::File(PathBuf::from(&p.name))).collect();
        }
    }
    if s.cursor.as_ref().is_none_or(|c| !items.contains(c)) {
        s.cursor = s.selected.clone().map(Item::File).filter(|c| items.contains(c)).or_else(|| items.first().cloned());
    }

    // Keyboard on the list: moving onto a file opens it; enter or space on a heading folds it.
    if s.area != Area::Minecraft
        && let Some(n) = list_nav
    {
        let pos = s.cursor.as_ref().and_then(|c| items.iter().position(|i| i == c));
        let here = pos.and_then(|p| items.get(p)).cloned();
        match (n, here) {
            (n, _) if n.is_move() => {
                if let Some(p) = n.step(items.len(), pos) {
                    s.cursor = Some(items[p].clone());
                    if let Item::File(rel) = &items[p] {
                        out.pick = Some(rel.clone());
                    }
                    out.scroll_file = true;
                }
            }
            (Nav::Activate | Nav::Toggle, Some(Item::Group(name))) => out.fold = Some(fold_key(&inst.id, &["group", &name])),
            (Nav::Activate | Nav::Edit, Some(Item::File(_))) => out.focus = Some(Zone::Side),
            (Nav::Folder, _) => super::open_path(&if s.area == Area::Shaders { shader_config::dir(inst) } else { mc::config_dir(inst) }),
            _ => {}
        }
    }

    // Load the selected file (in the background) unless it's the one open.
    if let Some((id, area, rel, p)) = &s.loading
        && let Some(r) = p.take()
    {
        let (id, area, rel) = (id.clone(), *area, rel.clone());
        s.loading = None;
        if id == inst.id && area == s.area && s.selected.as_ref() == Some(&rel) {
            match r {
                Ok(loaded) => {
                    s.open = Some(Open::new(id, area, rel, loaded));
                    s.load_error = None;
                }
                Err(e) => s.load_error = Some((area, rel, format!("{e:#}"))),
            }
        }
    }
    let area = s.area;
    if let Some(sel) = s.selected.clone() {
        let have = s.open.as_ref().is_some_and(|o| o.rel == sel && o.area == area && o.inst_id == inst.id);
        let pending = s.loading.as_ref().is_some_and(|(id, a, r, _)| *r == sel && *a == area && *id == inst.id);
        let failed = s.load_error.as_ref().is_some_and(|(a, r, _)| *r == sel && *a == area);
        // An unsaved file elsewhere stays open (leaving the tab asks about it); anything else is replaced.
        let blocked = s.open.as_ref().is_some_and(|o| o.dirty() && !(o.rel == sel && o.area == area));
        if !have && !pending && !failed && !blocked {
            let (i, r) = (inst.clone(), sel.clone());
            let p = Promise::spawn(&rt, &ctx, async move { tokio::task::spawn_blocking(move || load_for(area, &i, &r)).await? });
            s.loading = Some((inst.id.clone(), area, sel, p));
        }
    }

    // The game (or anything else) may rewrite the open file. It's checked, read and parsed on a
    // Promise; the file stays as shown until the result comes, and a result for a file that has
    // since been reloaded, saved or closed is dropped.
    if let Some(c) = &s.recheck
        && let Some(r) = c.promise.take()
    {
        let c = s.recheck.take().unwrap();
        if let (Ok(r), Some(o)) = (r, &mut s.open) {
            o.take_reread(&c.of, r);
        }
    }
    if s.recheck.is_none()
        && let Some(o) = &s.open
        && s.checked.is_none_or(|c| c.elapsed() > RECHECK)
    {
        s.checked = Some(Instant::now());
        ctx.request_repaint_after(RECHECK);
        let (path, was, format, base) = (o.path.clone(), o.stamp, o.format, o.base.clone());
        let promise = Promise::spawn(&rt, &ctx, async move { Ok(tokio::task::spawn_blocking(move || reread(&path, was, format, base.as_deref())).await?) });
        s.recheck = Some(Recheck { of: o.check_of(), promise });
    }

    // A binding's captured key.
    if let Some((id, v)) = s.captured.take()
        && let Some(o) = &s.open
        && let Ok(doc) = &o.doc
        && let Some(n) = doc.fields.iter().position(|f| field_id(f) == id)
    {
        out.edit = Some((n, Value::Scalar(Scalar::Str(v))));
    }

    // Keys routed from the app.
    match key {
        Some(Action::SaveConfig) => out.save = true,
        Some(Action::RawConfig) => out.raw = s.open.as_ref().filter(|o| o.format.structured()).map(|o| !o.raw),
        _ => {}
    }

    // The file list on the left (not for Minecraft's one file), the editor on the right, a strong
    // hairline between; both panes run from their edges, so rows reach the line.
    let full = ui.available_rect_before_wrap();
    let edit_rect = if area == Area::Minecraft || (narrow && side) {
        full
    } else if narrow {
        ui.scope_builder(UiBuilder::new().max_rect(full).layout(Layout::top_down(Align::Min)), |ui| {
            if area == Area::Shaders {
                packs_pane(ui, &t, s, &packs, &icons, focus == Zone::Main, &mut out);
            } else {
                files_pane(ui, &t, s, &inst.id, &groups, &items, list_nav == Some(Nav::Search), focus == Zone::Main, &mut out);
            }
        });
        ui.allocate_rect(full, Sense::hover());
        apply(app, out);
        return;
    } else {
        let w = (full.width() * tk::CONFIG_FILES_SHARE).clamp(tk::CONFIG_FILES_MIN, tk::CONFIG_FILES_MAX).min(full.width() * 0.45).floor();
        let line_x = full.left() + w;
        let left = Rect::from_min_max(full.min, egui::pos2(line_x, full.bottom()));
        ui.scope_builder(UiBuilder::new().max_rect(left).layout(Layout::top_down(Align::Min)), |ui| {
            if area == Area::Shaders {
                packs_pane(ui, &t, s, &packs, &icons, focus == Zone::Main, &mut out);
            } else {
                files_pane(ui, &t, s, &inst.id, &groups, &items, list_nav == Some(Nav::Search), focus == Zone::Main, &mut out);
            }
        });
        kit::vline(ui.painter(), line_x, full.y_range(), t.line_strong);
        Rect::from_min_max(egui::pos2(line_x + tk::LINE, full.top()), full.max)
    };
    let number = if area == Area::Minecraft { 1 } else { 2 };

    // The pack in use, switched here as the game's shader menu would (Iris's or OptiFine's own file).
    let pack = (area == Area::Shaders).then(|| {
        let sel = s.selected.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let why = match (switch, running) {
            (None, _) => Some("There's no shader mod (Iris or OptiFine) in this instance."),
            (_, true) => Some("Close the game first: it saves its own choice when it quits."),
            _ => None,
        };
        PackSwitch { active: packs.iter().any(|p| p.name == sel && p.active), why, optifine: switch == Some(shader_config::Switch::OptiFine), name: sel }
    });
    // The open file's group: the mod it belongs to, for the first settings group's title.
    let base_title = match area {
        Area::Minecraft => "Options".to_string(),
        Area::Shaders => "Main screen".to_string(),
        Area::Mods => s.selected.as_ref().and_then(|sel| groups.iter().find(|g| g.files.iter().any(|f| f.rel == *sel))).filter(|g| g.name != mc::OTHER).map(|g| g.name.clone()).unwrap_or_else(|| "Settings".into()),
    };
    ui.scope_builder(UiBuilder::new().max_rect(edit_rect).layout(Layout::top_down(Align::Min)), |ui| match (&mut s.open, &s.load_error) {
        (Some(o), _) if s.selected.as_ref() == Some(&o.rel) && o.area == area && o.inst_id == inst.id => {
            let folds = s.folds.get_or_insert_with(Default::default);
            let cooling = s.capture_ended.is_some_and(|e| e.elapsed() < Duration::from_millis(400));
            let st = EditorState { pack: pack.as_ref(), folds, field_filter: &mut s.field_filter, field_sel: &mut s.field_sel, scroll_field: &mut s.scroll_field, unfolded: &mut s.unfolded, capture: s.capture.as_deref(), cooling, number, base_title: &base_title };
            editor(ui, &t, o, st, running, edit_nav, editor_focused, &mut out);
        }
        (_, Some((a, rel, e))) if *a == area && s.selected.as_ref() == Some(rel) => {
            head(ui, &t, &format!("{} / {}", kit::text::number(number), rel.display()), "can't open");
            let what = if area == Area::Shaders { "Can't read this pack's options" } else { "Can't open this file" };
            padded(ui, |ui| kit::empty_state(ui, &t, what, e, None));
        }
        (Some(o), _) if o.dirty() && s.selected.is_some() => {
            head(ui, &t, &format!("{} / settings", kit::text::number(number)), "");
            let name = o.rel.display().to_string();
            padded(ui, |ui| kit::empty_state(ui, &t, "Unsaved changes elsewhere", &format!("{name} has unsaved changes. Save or discard them there first."), None));
        }
        _ if s.selected.is_none() => {
            head(ui, &t, &format!("{} / settings", kit::text::number(number)), "");
            padded(ui, |ui| match area {
                Area::Shaders => kit::empty_state(ui, &t, "Pick a shader pack", "Its options show here, named and grouped as in the game's shader menu.", None),
                _ => kit::empty_state(ui, &t, "Pick a config file", "Mods are listed on the left. Open one to see its config files, then pick a file to edit its settings.", None),
            });
        }
        _ => {
            let name = s.selected.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
            head(ui, &t, &format!("{} / {name}", kit::text::number(number)), "");
            padded(ui, |ui| kit::loading(ui, &t, if area == Area::Shaders { "reading the pack's options" } else { "reading the file" }));
        }
    });
    ui.allocate_rect(full, Sense::hover());

    apply(app, out);
}

/// A pane's head: `01 / FILES · 84` on the left, a strong rule under. Both panes' heads are this
/// tall, so their rules are one line.
fn head(ui: &mut Ui, t: &Tokens, label: &str, aside: &str) {
    let text = if aside.is_empty() { label.to_string() } else { format!("{label} · {aside}") };
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, tk::SECTION_LABEL_HEIGHT), Sense::hover());
    let g = ui.painter().layout_job(kit::text::caption_wrapped(&text, t.text_muted, (width - tk::GUTTER * 2.0).max(40.0), 1));
    ui.painter().galley(egui::pos2(rect.left() + tk::GUTTER, rect.center().y - g.size().y / 2.0), g, t.text_muted);
    kit::rule::hline(ui.painter(), rect.x_range(), rect.bottom() - 0.5, t.line_strong);
}

/// The shader packs, the one in use marked.
fn packs_pane(ui: &mut Ui, t: &Tokens, s: &mut ConfigState, packs: &[shader_config::Pack], icons: &HashMap<String, PathBuf>, focused: bool, out: &mut Out) {
    head(ui, t, "01 / packs", &packs.len().to_string());
    let open_dirty = s.open.as_ref().filter(|o| o.dirty() && o.area == Area::Shaders).map(|o| o.rel.clone());
    let scroll = std::mem::take(&mut s.scroll_file);
    let any_icon = packs.iter().any(|p| icons.contains_key(&p.name));
    egui::ScrollArea::vertical().id_salt("shader-packs").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut list = HoverList::begin(ui, Variant::Quiet);
        for p in packs {
            let rel = PathBuf::from(&p.name);
            let item = Item::File(rel.clone());
            let at = s.cursor.as_ref() == Some(&item);
            // Pack names are long and alike ("ComplementaryReimagined_r5.8.1 + EuphoriaPatches_1.9.3").
            let shown = p.name.strip_suffix(".zip").or_else(|| p.name.strip_suffix(".ZIP")).unwrap_or(&p.name).replace('_', " ");
            let mut row = HoverRow::new(("shader-pack", &p.name), &shown).selected(s.selected.as_ref() == Some(&rel)).hot(at && focused);
            row = match icons.get(&p.name) {
                Some(path) => row.lead(super::content::file_image(path)),
                None if any_icon => row.lead_blank(),
                None => row,
            };
            if open_dirty.as_ref() == Some(&rel) {
                row = row.badge("unsaved", t.accent);
            } else if p.active {
                row = row.badge("in use", t.ok());
            }
            let r = list.row(ui, t, row).on_hover_text(&p.name);
            if at && scroll {
                r.scroll_to_me(None);
            }
            if r.clicked() {
                s.cursor = Some(item);
                out.pick = Some(rel);
                out.focus = Some(Zone::Main);
            }
            if r.double_clicked() {
                out.focus = Some(Zone::Side);
            }
        }
        list.end(ui);
    });
}

/// Carry out what the frame asked for.
fn apply(app: &mut App, out: Out) {
    if let Some(z) = out.focus {
        app.focus = z;
    }
    if let Some(k) = out.fold {
        let open = !app.config.is_open(&k);
        app.config.set_open(k, open);
    }
    if let Some(pack) = out.use_pack
        && let Some(inst) = app.selected_instance().cloned()
    {
        match shader_config::use_pack(&inst, pack.as_deref()) {
            Ok(()) => {
                // Shown at once; the next listing reads it back from the file.
                for l in app.config.listing.iter_mut().flat_map(|l| l.packs.iter_mut()) {
                    l.active = pack.as_deref() == Some(l.name.as_str());
                }
                let what = match &pack {
                    Some(p) => format!("Shaders now use {}", p.trim_end_matches(".zip")),
                    None => "Shaders turned off".to_string(),
                };
                app.toast(ToastKind::Info, what);
            }
            Err(e) => app.toast(ToastKind::Error, format!("{e:#}")),
        }
    }
    if let Some(id) = out.capture {
        app.config.capture = Some(id);
        app.config.capture_mods = None;
    }
    if let Some(rel) = out.pick {
        pick_file(app, rel);
        app.config.scroll_file = out.scroll_file;
    }
    let s = &mut app.config;
    if let Some(o) = &mut s.open {
        if let Some((n, v)) = out.edit
            && let Ok(doc) = &o.doc
            && let Some(f) = doc.fields.get(n)
        {
            match mc::set(o.format, &o.text, f, &v) {
                Ok(text) => o.set_text(text),
                Err(e) => app.toasts.push(super::Toast::error(format!("{e:#}"))),
            }
        }
        if out.defaults {
            // One at a time: each edit moves the others' places in the text.
            for _ in 0..=o.doc.as_ref().map_or(0, |d| d.fields.len()) {
                let Ok(doc) = &o.doc else { break };
                let Some((f, d)) = doc.fields.iter().find_map(|f| default_value(f).filter(|d| Value::Scalar(d.clone()) != f.value).map(|d| (f.clone(), d))) else { break };
                match mc::set(o.format, &o.text, &f, &Value::Scalar(d)) {
                    Ok(text) => o.set_text(text),
                    Err(e) => {
                        app.toasts.push(super::Toast::error(format!("{e:#}")));
                        break;
                    }
                }
            }
            o.note = Some(("defaults restored; save to keep them".into(), Instant::now()));
        }
        if let Some(raw) = out.raw {
            if !raw && o.doc.is_err() {
                app.toasts.push(super::Toast::error("Fix the error in the text first; then it opens as fields again"));
            } else {
                o.raw = raw;
            }
        }
        if out.take_disk
            && let Some(disk) = o.conflict.take()
        {
            o.reload(disk);
            o.note = Some(("took the file as it is on disk".into(), Instant::now()));
        }
        if out.keep_mine
            && let Some(disk) = o.conflict.take()
        {
            // What's on disk now is what saving will replace.
            o.saved = disk;
            o.saved_values = values(&mc::parse_with(o.format, o.base.as_deref(), &o.saved));
            o.stamp = mc::Stamp::of(&o.path);
            o.generation = next_generation();
        }
    }
    if out.revert && app.config.open.as_ref().is_some_and(|o| o.dirty()) {
        let name = app.config.open.as_ref().map(|o| o.name()).unwrap_or_default();
        app.modal = Some(Modal::Confirm(Confirm {
            title: "discard changes".into(),
            body: format!("Put {name} back as it is on disk? The changes made here are lost."),
            button: "discard".into(),
            action: ConfirmAction::RevertConfig,
        }));
    }
    if out.save {
        save(app, false);
    }
}

/// Put the open file back as it is on disk.
pub fn revert(app: &mut App) {
    if let Some(o) = &mut app.config.open {
        let saved = o.saved.clone();
        o.set_text(saved);
        o.conflict = None;
        o.note = Some(("changes discarded".into(), Instant::now()));
    }
}

/// Save the open file; false if it couldn't be (an error, or it changed on disk).
fn save(app: &mut App, force: bool) -> bool {
    let Some(o) = &mut app.config.open else { return false };
    if !o.dirty() && !force {
        return true;
    }
    match mc::save(&o.path, &o.saved, &o.text, force) {
        Ok(mc::Saved::Done(stamp)) => {
            o.saved = o.text.clone();
            o.saved_values = values(&o.doc);
            o.stamp = stamp;
            o.conflict = None;
            o.generation = next_generation();
            // Text that doesn't parse may still be saved: it's the user's file. Say so.
            let note = if o.format.structured() && o.doc.is_err() { "saved, with an error in it: the mod may not read it" } else { "saved" };
            o.note = Some((note.into(), Instant::now()));
            true
        }
        Ok(mc::Saved::Changed(now)) => {
            o.conflict = Some(now);
            false
        }
        Err(e) => {
            let name = o.name();
            app.toast(ToastKind::Error, format!("Couldn't save {name}: {e:#}"));
            false
        }
    }
}

// -- the file list ----------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn files_pane(ui: &mut Ui, t: &Tokens, s: &mut ConfigState, inst_id: &str, groups: &[Group], items: &[Item], focus_filter: bool, focused: bool, out: &mut Out) {
    let total: usize = groups.iter().map(|g| g.files.len()).sum();
    head(ui, t, "01 / files", &total.to_string());
    // The filter takes over the list's head as a query line, as on the instance list.
    let resp = padded(ui, |ui| kit::query(ui, t, &mut s.file_filter, "filter", tk::SIZE_HEADING, focus_filter));
    if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        s.file_filter.clear();
        resp.surrender_focus();
    }
    let open_dirty = s.open.as_ref().filter(|o| o.dirty()).map(|o| o.rel.clone());
    let scroll = std::mem::take(&mut s.scroll_file);
    // Icons where some mods have them; the others keep their room, so names line up.
    let any_icon = groups.iter().any(|g| g.icon.is_some());
    egui::ScrollArea::vertical().id_salt("config-files").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if items.is_empty() {
            padded(ui, |ui| {
                ui.add_space(tk::GAP);
                kit::body_muted(ui, t, "Nothing matches.");
            });
        }
        let mut list = HoverList::begin(ui, Variant::Quiet);
        for item in items {
            let at = s.cursor.as_ref() == Some(item);
            match item {
                Item::Group(name) => {
                    let Some(g) = groups.iter().find(|g| g.name == *name) else { continue };
                    let count = g.files.len().to_string();
                    let row = with_lead(HoverRow::new(("config-group", name), &g.name).aside(&count).hot(at && focused), g, any_icon);
                    let r = list.row(ui, t, row).on_hover_text(format!("{} config files", g.files.len()));
                    if at && scroll {
                        r.scroll_to_me(None);
                    }
                    if r.clicked() {
                        s.cursor = Some(item.clone());
                        out.fold = Some(fold_key(inst_id, &["group", name]));
                        out.focus = Some(Zone::Main);
                    }
                }
                Item::File(rel) => {
                    let on = s.selected.as_ref() == Some(rel);
                    // A mod with one file: the mod's own row (the file's path is on hover).
                    let single = groups.iter().find(|g| g.files.len() == 1 && g.files[0].rel == *rel);
                    let title = single.map_or_else(|| rel.file_name().unwrap_or_default().to_string_lossy().into_owned(), |g| g.name.clone());
                    let mut row = HoverRow::new(("config-file", rel), &title).selected(on).hot(at && focused);
                    row = match single {
                        Some(g) => with_lead(row, g, any_icon),
                        // Under its mod's heading, a step in (past the heading's icon, where there are icons).
                        None => row.depth(if any_icon { 2 } else { 1 }),
                    };
                    if open_dirty.as_ref() == Some(rel) {
                        row = row.badge("unsaved", t.accent);
                    }
                    let r = list.row(ui, t, row).on_hover_text(rel.to_string_lossy());
                    if at && scroll {
                        r.scroll_to_me(None);
                    }
                    if r.clicked() {
                        s.cursor = Some(item.clone());
                        out.pick = Some(rel.clone());
                        out.focus = Some(Zone::Main);
                    }
                    if r.double_clicked() {
                        out.focus = Some(Zone::Side);
                    }
                }
            }
        }
        list.end(ui);
    });
}

/// A mod's icon before its name; where other mods have icons and this one hasn't, an empty square keeps its room.
fn with_lead<'a>(row: HoverRow<'a>, g: &Group, any_icon: bool) -> HoverRow<'a> {
    match &g.icon {
        Some(p) => row.lead(super::content::file_image(p)),
        None if any_icon => row.lead_blank(),
        None => row,
    }
}

// -- the editor -------------------------------------------------------------------

/// The shader pack open in the editor: whether it's the one in use, and why it can't be switched to if it can't.
struct PackSwitch {
    name: String,
    active: bool,
    why: Option<&'static str>,
    optifine: bool,
}

struct EditorState<'a> {
    /// For a shader pack: switching to it, in a banner.
    pack: Option<&'a PackSwitch>,
    /// Open settings headers ([`fold_key`]).
    folds: &'a BTreeSet<String>,
    field_filter: &'a mut String,
    field_sel: &'a mut Option<String>,
    scroll_field: &'a mut bool,
    unfolded: &'a mut HashSet<String>,
    /// The key binding waiting for a key, if any.
    capture: Option<&'a str>,
    /// A capture just ended (its click mustn't start another).
    cooling: bool,
    /// The pane's number in its head.
    number: usize,
    /// The first settings group's title (the mod, "Options", "Main screen").
    base_title: &'a str,
}

/// How many fields differ from the file on disk.
fn changed_fields(o: &Open) -> usize {
    o.doc.as_ref().map_or(0, |d| d.fields.iter().filter(|f| o.saved_values.get(&field_id(f)) != Some(&f.value)).count())
}

#[allow(clippy::too_many_arguments)]
fn editor(ui: &mut Ui, t: &Tokens, o: &mut Open, st: EditorState, running: bool, kb: Option<Nav>, focused: bool, out: &mut Out) {
    let dirty = o.dirty();
    let structured = o.format.structured();
    let key = |n: Nav| kb == Some(n);

    // The head: the file, its format and where it stands (a note from a save or reload takes over for a moment).
    let note = o.note.as_ref().filter(|(_, at)| at.elapsed() < Duration::from_secs(4)).map(|(n, _)| n.clone());
    if note.is_some() {
        ui.ctx().request_repaint_after(Duration::from_millis(500));
    }
    let status = match (&note, dirty) {
        (Some(n), _) => n.clone(),
        (None, true) => match changed_fields(o) {
            0 => "unsaved changes".to_string(),
            1 => "1 unsaved change".to_string(),
            n => format!("{n} unsaved changes"),
        },
        (None, false) => "no changes".to_string(),
    };
    let label = format!("{} / {}", kit::text::number(st.number), o.rel.display());
    head(ui, t, &label, &format!("{} · {status}", o.format.label()));

    // States that hold the editor for a while, under its head.
    if let Some(p) = st.pack {
        let (caption, color, sentence) = match (p.active, p.why) {
            (_, Some(why)) => (if p.active { "in use" } else { "not in use" }, p.active.then(|| t.ok()), why.to_string()),
            (true, None) => ("in use", Some(t.ok()), "The game loads this pack.".to_string()),
            (false, None) => ("not in use", None, format!("{} loads another pack, or none.", if p.optifine { "OptiFine" } else { "Iris" })),
        };
        let b = if p.active { Button::new("turn shaders off") } else { Button::new("use this pack") };
        if kit::banner(ui, t, caption, color, &sentence, &[b.enabled(p.why.is_none())]).is_some() {
            out.use_pack = Some(if p.active { None } else { Some(p.name.clone()) });
        }
    }
    if running {
        kit::banner(ui, t, "game running", Some(t.warning()), "Most mods read their config when the game starts, and some write it back when it quits, so a change here may need a restart, or be overwritten when the game closes.", &[]);
    }
    if o.conflict.is_some() {
        match kit::banner(ui, t, "changed on disk", Some(t.warning()), "This file changed on disk while you were editing it; the game rewrites its configs as it starts and quits. Keep your changes (saving replaces the new file), or take the file as it is now?", &[Button::new("keep my changes"), Button::new("use the file on disk")]) {
            Some(0) => out.keep_mine = true,
            Some(1) => out.take_disk = true,
            _ => {}
        }
    }
    let parse_error = if structured && !o.raw_mode() { o.doc.as_ref().err().map(|e| format!("This file doesn't read as {} ({e}), so it can only be edited as text for now.", o.format.label())) } else { None };
    if let Some(msg) = &parse_error
        && kit::banner(ui, t, "can't read as fields", Some(t.error()), msg, &[Button::new("edit as text")]).is_some()
    {
        out.raw = Some(true);
    }

    // The toolbar: the settings filter as a query line (level with the file list's), the actions at the right.
    let fields_shown = !o.raw_mode() && o.doc.as_ref().is_ok_and(|d| !d.fields.is_empty());
    let differ = o.doc.as_ref().map_or(0, |d| d.fields.iter().filter(|f| default_value(f).is_some_and(|d| Value::Scalar(d) != f.value)).count());
    // Restoring defaults needs fields that say what theirs are: not for files that are only ever text.
    let file_buttons: Vec<Button> = if structured { vec![Button::new("folder"), Button::new("restore defaults").enabled(differ > 0 && !o.raw_mode())] } else { vec![Button::new("folder")] };
    let save_buttons = [Button::new("revert").enabled(dirty), Button::new("save").filled().enabled(dirty)];
    let mode = [Button::new("fields").checked(!o.raw), Button::new("text").checked(o.raw)];
    let joined = |bs: &[Button]| bs.iter().map(|b| b.width(ui, t)).sum::<f32>() - (bs.len().saturating_sub(1)) as f32 * tk::LINE;
    let widths = [joined(&file_buttons), if structured { joined(&mode) } else { 0.0 }, joined(&save_buttons)];
    let buttons_w = widths[0] + tk::GAP + if structured { widths[1] + tk::GAP } else { 0.0 } + widths[2];
    let inner = ui.available_rect_before_wrap().shrink2(vec2(tk::GUTTER, 0.0));
    let beside = inner.width() - buttons_w - tk::GUTTER >= tk::LIST_TITLE_MIN;
    let query_w = if beside { inner.width() - buttons_w - tk::GUTTER } else { inner.width() };
    let top = ui.cursor().top();
    let mut q = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(egui::pos2(inner.left(), top), vec2(query_w, inner.height()))).layout(Layout::top_down(Align::Min)));
    let query_rect = if fields_shown {
        let r = kit::query(&mut q, t, st.field_filter, "filter settings", tk::SIZE_HEADING, key(Nav::Search));
        if r.has_focus() && q.input(|i| i.key_pressed(egui::Key::Escape)) {
            st.field_filter.clear();
            r.surrender_focus();
        }
        r.rect
    } else {
        // The same room, so the buttons hold still between fields and text.
        let h = q.fonts_mut(|f| f.row_height(&fonts::heading(tk::SIZE_HEADING))) + tk::UNIT * 2.0;
        let (r, _) = q.allocate_exact_size(vec2(query_w, h), Sense::hover());
        let what = if o.raw_mode() { "editing as text" } else { "nothing to show as fields" };
        kit::text::paint_caption(q.painter(), egui::pos2(r.left(), r.center().y), what, t.text_muted);
        r
    };
    // The groups in a row beside the filter, or under it, wrapping a group at a time when even that's too narrow.
    let row_top = if beside { query_rect.center().y - tk::BUTTON_HEIGHT / 2.0 } else { query_rect.bottom() + tk::GAP };
    let mut at = egui::pos2(if beside { inner.right() - buttons_w } else { inner.left() }, row_top);
    let mut place = |w: f32| {
        if !beside && at.x > inner.left() && at.x + w > inner.right() {
            at = egui::pos2(inner.left(), at.y + tk::BUTTON_HEIGHT + tk::UNIT);
        }
        let r = Rect::from_min_size(at, vec2(w, tk::BUTTON_HEIGHT));
        at.x += w + tk::GAP;
        r
    };
    let mut group = |r: Rect| ui.new_child(UiBuilder::new().max_rect(r).layout(Layout::left_to_right(Align::Center)));
    match kit::button_row(&mut group(place(widths[0])), t, &file_buttons) {
        Some(0) => super::open_path(o.path.parent().unwrap_or(&o.path)),
        Some(1) if structured => out.defaults = true,
        _ => {}
    }
    if key(Nav::Folder) {
        super::open_path(o.path.parent().unwrap_or(&o.path));
    }
    if structured {
        let mut raw = o.raw;
        if kit::segmented(&mut group(place(widths[1])), t, &mut raw, &[(false, "fields"), (true, "text")]) {
            out.raw = Some(raw);
        }
    }
    match kit::button_row(&mut group(place(widths[2])), t, &save_buttons) {
        Some(0) => out.revert = true,
        Some(1) => out.save = true,
        _ => {}
    }
    let last_row = at.y;
    if key(Nav::Delete) && dirty && o.raw_mode() {
        out.revert = true;
    }
    let bottom = if beside { query_rect.bottom() } else { last_row + tk::BUTTON_HEIGHT };
    ui.allocate_rect(Rect::from_min_max(egui::pos2(inner.left(), top), egui::pos2(inner.right(), bottom)), Sense::hover());
    ui.add_space(tk::GAP);

    if o.raw_mode() {
        padded(ui, |ui| raw_editor(ui, t, o, key(Nav::Activate) || key(Nav::Edit), focused));
        return;
    }
    let Ok(doc) = &o.doc else { return };
    if doc.fields.is_empty() {
        padded(ui, |ui| kit::empty_state(ui, t, "No settings to show as fields", "Switch to text to see and edit the whole file.", None));
        return;
    }

    // Runs of settings that share a header, in the file's order: each a settings group, folded unless
    // opened (all open while filtering). Settings outside any header are a group that doesn't fold.
    let shown = shown_fields(doc, st.field_filter);
    let filtering = !st.field_filter.trim().is_empty();
    let fold_of = |section: &[String]| fold_key(&o.inst_id, &["section", o.area.key(), &o.rel.to_string_lossy(), &section.join("\u{1e}")]);
    let mut runs: Vec<(&[String], Vec<(usize, &Field)>)> = Vec::new();
    for (n, f) in &shown {
        match runs.last_mut() {
            Some((sec, fs)) if *sec == f.section.as_slice() => fs.push((*n, *f)),
            _ => runs.push((f.section.as_slice(), vec![(*n, *f)])),
        }
    }
    let header_id = |sec: &[String]| format!("§{}", sec.join("\u{1e}"));
    let open = |sec: &[String]| sec.is_empty() || filtering || st.folds.contains(&fold_of(sec));
    // The keyboard's lines: headers, then their settings when open.
    let mut ids: Vec<String> = Vec::new();
    for (sec, fs) in &runs {
        if !sec.is_empty() {
            ids.push(header_id(sec));
        }
        if open(sec) {
            ids.extend(fs.iter().map(|(_, f)| field_id(f)));
        }
    }
    let mut pos = st.field_sel.as_ref().and_then(|id| ids.iter().position(|x| x == id));
    if pos.is_none() && !ids.is_empty() {
        pos = Some(0);
        *st.field_sel = Some(ids[0].clone());
    }
    if let Some(n) = kb.filter(|n| n.is_move())
        && let Some(p) = n.step(ids.len(), pos)
    {
        *st.field_sel = Some(ids[p].clone());
        pos = Some(p);
        *st.scroll_field = true;
    }
    let current = pos.and_then(|p| ids.get(p)).cloned().unwrap_or_default();
    if current.starts_with('§')
        && (key(Nav::Activate) || key(Nav::Toggle))
        && let Some((sec, _)) = runs.iter().find(|(sec, _)| !sec.is_empty() && header_id(sec) == current)
    {
        out.fold = Some(fold_of(sec));
    }
    let on_field = !current.is_empty() && !current.starts_with('§');
    // Key bindings that share a key, by key: each one's row says what else is on it.
    let mut clashes: HashMap<&str, Vec<String>> = HashMap::new();
    for f in &doc.fields {
        if let Some(v) = binding_of(f).filter(|v| !v.ends_with(".unknown")) {
            clashes.entry(v).or_default().push(f.label.clone().unwrap_or_else(|| f.key.clone()));
        }
    }
    let focus_widget = on_field && (key(Nav::Activate) || key(Nav::Edit));
    let scroll = std::mem::take(st.scroll_field);
    let saved_values = &o.saved_values;

    egui::ScrollArea::vertical().id_salt(("config-fields", &o.rel)).auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (i, (sec, fs)) in runs.iter().enumerate() {
            let count = if fs.len() == 1 { "1 setting".to_string() } else { format!("{} settings", fs.len()) };
            let (title, aside) = match sec.split_last() {
                Some((last, parents)) if !parents.is_empty() => (last.as_str(), format!("{} · {count}", parents.join(" › "))),
                Some((last, _)) => (last.as_str(), count),
                None => (st.base_title, count),
            };
            let hid = header_id(sec);
            let mut group = SettingGroup::new(i + 1, title).aside(&aside).id_salt(("config-group", &hid, i));
            if !sec.is_empty() {
                group = group.foldable(open(sec)).hot(current == hid && focused);
            }
            let (head, ()) = {
                let (r, inner) = group.show(ui, t, |ui| {
                    if let Some(d) = (!sec.is_empty()).then(|| doc.section_doc(sec)).flatten() {
                        egui::Frame::new().inner_margin(egui::Margin { left: tk::SETTING_INDENT as i8, right: tk::GUTTER as i8, top: 0, bottom: tk::GAP as i8 }).show(ui, |ui| {
                            ui.set_max_width(ui.available_width().min(tk::SETTING_DESCRIPTION_WIDTH));
                            kit::body_muted(ui, t, d);
                        });
                    }
                    for (n, f) in fs {
                        let id = field_id(f);
                        let selected = current == id;
                        let clash: Vec<String> = binding_of(f).and_then(|v| clashes.get(v)).map(|names| names.iter().filter(|n| **n != f.label.as_deref().unwrap_or(&f.key)).cloned().collect()).unwrap_or_default();
                        let r = Row { id: &id, hot: selected && focused, capturing: st.capture == Some(id.as_str()), clash, cooling: st.cooling };
                        let changed = saved_values.get(&id) != Some(&f.value);
                        let kb_here = if selected { kb } else { None };
                        let (rect, pressed, edit) = field_row(ui, t, &r, f, changed, kb_here, selected && focus_widget, st.unfolded, out);
                        if let Some(v) = edit {
                            out.edit = Some((*n, v));
                        }
                        if pressed {
                            *st.field_sel = Some(id.clone());
                            out.focus = Some(Zone::Side);
                        }
                        if selected && scroll {
                            ui.scroll_to_rect(rect, Some(Align::Center));
                        }
                    }
                });
                (r, inner.unwrap_or(()))
            };
            if !sec.is_empty() {
                if head.clicked() {
                    *st.field_sel = Some(hid.clone());
                    out.fold = Some(fold_of(sec));
                    out.focus = Some(Zone::Side);
                }
                if current == hid && scroll {
                    ui.scroll_to_rect(head.rect, Some(Align::Center));
                }
            }
        }
        ui.add_space(tk::GUTTER);
    });
}

struct Row<'a> {
    id: &'a str,
    /// Keyboard-current while the editor has the keyboard.
    hot: bool,
    /// Key bindings: waiting for this one's key, the other bindings on the same key, and whether a capture just
    /// ended.
    capturing: bool,
    clash: Vec<String>,
    cooling: bool,
}

/// One setting as a settings row: its name, the comment's description, a line of facts (range, default) with
/// "use default" when it isn't, and its control on the shared column. Returns its rect, whether it was
/// pressed, and a new value.
#[allow(clippy::too_many_arguments)]
fn field_row(ui: &mut Ui, t: &Tokens, r: &Row, f: &Field, changed: bool, kb: Option<Nav>, focus_widget: bool, unfolded: &mut HashSet<String>, out: &mut Out) -> (Rect, bool, Option<Value>) {
    let name = f.label.as_deref().unwrap_or(&f.key);
    let default = default_value(f);
    let current = if let Value::Scalar(s) = &f.value { Some(s) } else { None };
    let info_only = f.hint.allowed.len() == 1;
    let mut facts: Vec<String> = Vec::new();
    if !info_only {
        if let Some(r) = range_text(&f.hint) {
            facts.push(r);
        }
        if let Some(d) = &default {
            let shown = match d {
                Scalar::Bool(b) if f.hint.on_off => (if *b { "on" } else { "off" }).to_string(),
                Scalar::Str(v) if f.hint.binding => crate::vanilla_options::binding_value(v).unwrap_or_else(|| v.clone()),
                _ => f.hint.name_of(&d.display()).to_string(),
            };
            facts.push(format!("default {shown}"));
        }
    }
    let warn = !r.clash.is_empty();
    if warn {
        facts.insert(0, format!("also on {}", r.clash.join(", ")));
    }
    let facts = facts.join(" · ");
    let resettable = !info_only && default.as_ref().is_some_and(|d| Some(d) != current);
    let mut row = SettingRow::new(name).dense().description(&f.doc).facts(&facts, warn.then(|| t.warning())).unsaved(changed).hot(r.hot).id_salt(("config-field", r.id));
    if resettable {
        row = row.action("use default");
    }
    let (ro, mut edit) = row.show_row(ui, t, |ui| value_part(ui, t, f, r, kb, focus_widget, unfolded, out));
    if (ro.action || (resettable && kb == Some(Nav::Delete)))
        && let Some(d) = default
    {
        edit = Some(Value::Scalar(d));
    }
    if name != f.key {
        ro.response.clone().on_hover_text(&f.key);
    }
    (ro.response.rect, ro.pressed, edit)
}

/// A key binding's key, as written (`key.keyboard.w`).
fn binding_of(f: &Field) -> Option<&str> {
    match &f.value {
        Value::Scalar(Scalar::Str(v)) if f.hint.binding => Some(v),
        _ => None,
    }
}

/// Minecraft's name for a key (GLFW's, as `options.txt` writes it).
fn mc_key(k: egui::Key) -> Option<String> {
    use egui::Key::*;
    let name = match k {
        Space => "space",
        Enter => "enter",
        Tab => "tab",
        Backspace => "backspace",
        Insert => "insert",
        Delete => "delete",
        Home => "home",
        End => "end",
        PageUp => "page.up",
        PageDown => "page.down",
        ArrowUp => "up",
        ArrowDown => "down",
        ArrowLeft => "left",
        ArrowRight => "right",
        Minus => "minus",
        Equals | Plus => "equal",
        Comma => "comma",
        Period => "period",
        Slash => "slash",
        Backslash => "backslash",
        Semicolon | Colon => "semicolon",
        Quote => "apostrophe",
        OpenBracket => "left.bracket",
        CloseBracket => "right.bracket",
        Backtick => "grave.accent",
        _ => {
            let n = k.name();
            // Letters, digits ("1" or "Num1") and F-keys.
            let n = n.strip_prefix("Num").unwrap_or(n);
            return (n.len() == 1 && n.chars().all(|c| c.is_ascii_alphanumeric()) || (n.starts_with('F') && n[1..].parse::<u8>().is_ok())).then(|| format!("key.keyboard.{}", n.to_ascii_lowercase()));
        }
    };
    Some(format!("key.keyboard.{name}"))
}

/// While a binding waits for its key: take the next key, mouse button or newly held shift/ctrl/alt.
pub fn capture(app: &mut App, ctx: &egui::Context) {
    let Some(id) = app.config.capture.clone() else { return };
    let Some(before) = app.config.capture_mods else {
        app.config.capture_mods = Some(ctx.input(|i| i.modifiers));
        return;
    };
    let (event, mods) = ctx.input_mut(|i| {
        let n = i.events.iter().position(|e| matches!(e, egui::Event::Key { pressed: true, .. } | egui::Event::PointerButton { pressed: true, .. }));
        (n.map(|n| i.events.remove(n)), i.modifiers)
    });
    let value = match event {
        Some(egui::Event::Key { key: egui::Key::Escape, .. }) => None,
        Some(egui::Event::Key { key, .. }) => mc_key(key),
        Some(egui::Event::PointerButton { button, .. }) => Some(
            match button {
                egui::PointerButton::Primary => "key.mouse.left",
                egui::PointerButton::Secondary => "key.mouse.right",
                egui::PointerButton::Middle => "key.mouse.middle",
                egui::PointerButton::Extra1 => "key.mouse.4",
                egui::PointerButton::Extra2 => "key.mouse.5",
            }
            .to_string(),
        ),
        _ => match () {
            _ if mods.shift && !before.shift => Some("key.keyboard.left.shift".into()),
            _ if mods.ctrl && !before.ctrl => Some("key.keyboard.left.control".into()),
            _ if mods.alt && !before.alt => Some("key.keyboard.left.alt".into()),
            _ => return,
        },
    };
    app.config.capture = None;
    app.config.capture_ended = Some(Instant::now());
    // esc (or a key Minecraft has no name for) leaves the binding as it was.
    if let Some(v) = value {
        app.config.captured = Some((id, v));
    }
}

/// "1 – 64", "at least 0"…; nothing for the type's own limits (Forge writes
/// `Range: > -2147483648` for "any int").
fn range_text(h: &mc::Hint) -> Option<String> {
    let n = |v: f64| if v.fract() == 0.0 && v.abs() < 1e15 { format!("{}", v as i64) } else { format!("{v}") };
    let limit = |v: f64| v.abs() >= 2147483647.0;
    match (h.min.filter(|v| !limit(*v)), h.max.filter(|v| !limit(*v))) {
        (Some(a), Some(b)) => Some(format!("{} – {}", n(a), n(b))),
        (Some(a), None) => Some(format!("at least {}", n(a))),
        (None, Some(b)) => Some(format!("at most {}", n(b))),
        _ => None,
    }
}

/// The default from the comment as a value of this field's type.
fn default_value(f: &Field) -> Option<Scalar> {
    let d = f.hint.default.as_deref()?;
    Some(match &f.value {
        Value::Scalar(Scalar::Bool(_)) => Scalar::Bool(d.parse().ok()?),
        Value::Scalar(Scalar::Int(_)) => Scalar::Int(d.parse().ok()?),
        Value::Scalar(Scalar::Float(_)) => Scalar::Float(d.parse().ok()?),
        Value::Scalar(Scalar::Str(_)) => Scalar::Str(d.to_string()),
        _ => return None,
    })
}

/// The value's control, on the row's control column. Returns a new value when the user changes it.
#[allow(clippy::too_many_arguments)]
fn value_part(ui: &mut Ui, t: &Tokens, f: &Field, r: &Row, kb: Option<Nav>, focus: bool, unfolded: &mut HashSet<String>, out: &mut Out) -> Option<Value> {
    let id = r.id;
    let key = |n: Nav| kb == Some(n);
    let wid = egui::Id::new(("config-field", id));
    let width = ui.available_width();
    match &f.value {
        // A key binding: click (or enter), then press the key; right-click unbinds. Like Asteria's own keys.
        Value::Scalar(Scalar::Str(v)) if f.hint.binding => {
            if r.capturing {
                kit::caption_colored(ui, "press a key · esc cancels", t.accent);
                return None;
            }
            let shown = crate::vanilla_options::binding_value(v).unwrap_or_else(|| v.clone());
            let resp = Button::new(&shown).min_width(width.min(tk::SETTING_CONTROL_WIDTH / 2.0)).hot(r.hot).show(ui, t).on_hover_text("click to change, right-click to unbind");
            if (resp.clicked() && !r.cooling) || focus {
                out.capture = Some(id.to_string());
            }
            resp.secondary_clicked().then(|| Value::Scalar(Scalar::Str("key.keyboard.unknown".into())))
        }
        Value::Scalar(s) => scalar_widget(ui, t, s, &f.hint, default_value(f).as_ref(), wid, kb, focus, r.hot, width).map(Value::Scalar),
        Value::List { items, editable: true } => list_widget(ui, t, items, wid, id, unfolded, focus, width).map(|items| Value::List { items, editable: true }),
        Value::List { .. } | Value::Other(_) => {
            let preview = match &f.value {
                Value::List { items, .. } => format!("{} items", items.len()),
                Value::Other(raw) => raw.lines().next().unwrap_or_default().chars().take(60).collect(),
                _ => String::new(),
            };
            let mut edit_text = false;
            ui.with_layout(Layout::top_down(Align::Max), |ui| {
                edit_text = Button::new("edit as text").hot(r.hot).show(ui, t).clicked() || key(Nav::Activate);
                ui.add_space(tk::UNIT / 2.0);
                ui.add(egui::Label::new(egui::RichText::new(preview).font(fonts::raw(tk::SIZE_MONO)).color(t.text_muted)).truncate());
            });
            if edit_text {
                out.raw = Some(true);
            }
            None
        }
    }
}

/// A number with no range to slide over: typed or dragged, in a 1px frame like the kit's fields.
/// It never changes the value by being drawn (`clamp_existing_to_range(false)`).
pub(super) fn number<N: egui::emath::Numeric>(ui: &mut Ui, t: &Tokens, x: &mut N, range: std::ops::RangeInclusive<N>, speed: f64, hot: bool, width: f32) -> egui::Response {
    ui.scope(|ui| {
        let style = ui.style_mut();
        style.override_font_id = Some(fonts::text(tk::SIZE_BODY));
        style.spacing.interact_size = vec2(width, tk::BUTTON_HEIGHT);
        style.spacing.button_padding = vec2(tk::GAP, 0.0);
        let v = &mut style.visuals;
        v.extreme_bg_color = egui::Color32::TRANSPARENT;
        v.text_edit_bg_color = Some(egui::Color32::TRANSPARENT);
        v.text_cursor.stroke = egui::Stroke::new(tk::CARET_WIDTH, t.accent);
        v.selection.bg_fill = t.wash(0.16);
        v.selection.stroke = egui::Stroke::new(1.0, t.text_strong);
        let rest = if hot { t.accent } else { t.line_strong };
        for (w, line, ink) in [(&mut v.widgets.inactive, rest, if hot { t.accent } else { t.text_strong }), (&mut v.widgets.hovered, t.accent, t.accent), (&mut v.widgets.active, t.accent, t.accent)] {
            w.bg_fill = egui::Color32::TRANSPARENT;
            w.weak_bg_fill = egui::Color32::TRANSPARENT;
            w.bg_stroke = egui::Stroke::new(tk::LINE, line);
            w.fg_stroke = egui::Stroke::new(1.0, ink);
            w.corner_radius = egui::CornerRadius::ZERO;
            w.expansion = 0.0;
        }
        ui.add(egui::DragValue::new(x).range(range).clamp_existing_to_range(false).speed(speed))
    })
    .inner
}

/// Values within at most 1000 steps (and that a float holds exactly) get a slider.
fn slides(lo: Option<f64>, hi: Option<f64>) -> Option<(f64, f64)> {
    let (l, h) = (lo?, hi?);
    let exact = 9_007_199_254_740_992.0;
    (h > l && h - l <= 1000.0 && l.abs() < exact && h.abs() < exact).then_some((l, h))
}

/// A float slider's step: about a hundredth of the range, rounded to a power of ten.
fn float_step(lo: f64, hi: f64) -> f64 {
    10f64.powf(((hi - lo) / 100.0).log10().floor())
}

/// A user's slider move as written: to the step's own decimals, so `0.1 + 0.2` is written `0.3`.
fn to_step(v: f64, step: f64) -> f64 {
    let decimals = (-step.log10().floor()).max(0.0) as i32;
    let k = 10f64.powi(decimals);
    (v * k).round() / k
}

/// The control for one value; returns the new value when the user changes it. `kb` is the keyboard's
/// action on it, `focus` asks for it to take the keyboard.
#[allow(clippy::too_many_arguments)]
fn scalar_widget(ui: &mut Ui, t: &Tokens, s: &Scalar, hint: &mc::Hint, default: Option<&Scalar>, wid: egui::Id, kb: Option<Nav>, focus: bool, hot: bool, max_w: f32) -> Option<Scalar> {
    let key = |n: Nav| kb == Some(n);
    let width = max_w.min(tk::SETTING_CONTROL_WIDTH);
    match s {
        Scalar::Bool(b) => {
            let r = kit::switch(ui, t, *b, true, hot, if *b { "on" } else { "off" });
            (r.clicked() || key(Nav::Toggle) || focus).then_some(Scalar::Bool(!b))
        }
        Scalar::Int(v) => match slides(hint.min, hint.max) {
            Some((l, h)) => {
                let mut x = *v as f64;
                let mut slider = kit::Slider::new(&mut x, l.ceil(), h.floor()).step(1.0).value_text(v.to_string()).width(width).hot(hot);
                if let Some(Scalar::Int(d)) = default {
                    slider = slider.mark(*d as f64);
                }
                let r = slider.show(ui, t);
                if focus {
                    r.request_focus();
                }
                (r.changed() && x.round() as i64 != *v).then(|| Scalar::Int(x.round() as i64))
            }
            None => {
                let mut x = *v;
                let (a, b) = (hint.min.map_or(i64::MIN, |m| m.max(i64::MIN as f64) as i64), hint.max.map_or(i64::MAX, |m| m.min(i64::MAX as f64) as i64));
                let r = number(ui, t, &mut x, a..=b, ((v.abs() as f64) * 0.01).max(0.1), hot, width.min(tk::SETTING_CONTROL_WIDTH / 2.0));
                if focus {
                    r.request_focus();
                }
                (r.changed() && x != *v).then_some(Scalar::Int(x))
            }
        },
        Scalar::Float(v) => match slides(hint.min, hint.max) {
            Some((l, h)) => {
                let step = float_step(l, h);
                let mut x = *v;
                let mut slider = kit::Slider::new(&mut x, l, h).step(step).value_text(format!("{v}")).width(width).hot(hot);
                if let Some(Scalar::Float(d)) = default {
                    slider = slider.mark(*d);
                }
                let r = slider.show(ui, t);
                if focus {
                    r.request_focus();
                }
                let x = to_step(x, step);
                (r.changed() && x != *v).then_some(Scalar::Float(x))
            }
            None => {
                let mut x = *v;
                let r = number(ui, t, &mut x, hint.min.unwrap_or(f64::MIN)..=hint.max.unwrap_or(f64::MAX), (v.abs() * 0.01).max(0.01), hot, width.min(tk::SETTING_CONTROL_WIDTH / 2.0));
                if focus {
                    r.request_focus();
                }
                (r.changed() && x != *v).then_some(Scalar::Float(x))
            }
        },
        // A pack's information "button": one value, nothing to choose. Its name and text are the point.
        Scalar::Str(_) if hint.allowed.len() == 1 => None,
        // Values along a scale (a shader's sliders): a slider over the steps, showing the step's name.
        Scalar::Str(v) if hint.steps && hint.allowed.len() > 1 => {
            let steps = &hint.allowed;
            let at = steps.iter().position(|x| x == v);
            let mut i = at.unwrap_or(0) as f64;
            let mut slider = kit::Slider::new(&mut i, 0.0, (steps.len() - 1) as f64).step(1.0).value_text(hint.name_of(v)).width(width).hot(hot);
            if let Some(d) = default.and_then(|d| match d {
                Scalar::Str(d) => steps.iter().position(|x| x == d),
                _ => None,
            }) {
                slider = slider.mark(d as f64);
            }
            let r = slider.show(ui, t);
            if focus {
                r.request_focus();
            }
            let i = i.round() as usize;
            let mut pick = (r.changed() && Some(i) != at).then(|| steps[i].clone());
            if key(Nav::Toggle) {
                pick = Some(steps[at.map_or(0, |a| a + 1) % steps.len()].clone());
            }
            pick.map(Scalar::Str)
        }
        Scalar::Str(v) if !hint.allowed.is_empty() => {
            let mut options = hint.allowed.clone();
            if !options.contains(v) {
                options.insert(0, v.clone());
            }
            let at = options.iter().position(|o| o == v).unwrap_or(0);
            let mut pick = None;
            // Few short choices read best side by side, joined; more go in a menu.
            let buttons: Vec<Button> = options.iter().map(|o| Button::new(hint.name_of(o))).collect();
            let joined = buttons.iter().map(|b| b.width(ui, t)).sum::<f32>() - (buttons.len() - 1) as f32 * tk::LINE;
            if options.len() <= 4 && joined <= max_w {
                let mut i = at;
                let labels: Vec<(usize, &str)> = options.iter().enumerate().map(|(n, o)| (n, hint.name_of(o))).collect();
                if kit::segmented(ui, t, &mut i, &labels) {
                    pick = Some(options[i].clone());
                }
            } else {
                let resp = Button::new(hint.name_of(v)).icon(glyphs::CARET_DOWN).min_width(width.min(tk::SETTING_CONTROL_WIDTH * 0.75)).hot(hot).show(ui, t);
                egui::Popup::menu(&resp).id(wid.with("menu")).width(resp.rect.width()).show(|ui| {
                    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        for o in &options {
                            if ui.selectable_label(o == v, hint.name_of(o)).clicked() && o != v {
                                pick = Some(o.clone());
                            }
                        }
                    });
                });
            }
            if key(Nav::Toggle) || focus {
                pick = Some(options[(at + 1) % options.len()].clone());
            }
            pick.map(Scalar::Str)
        }
        Scalar::Str(v) => {
            let mut buf = v.clone();
            let r = kit::Field::new(&mut buf).hint("empty").width(width).focus(focus).show(ui, t);
            (r.changed() && buf != *v).then_some(Scalar::Str(buf))
        }
    }
}

/// A list of plain values: a field each, with remove, and add under them. Long ones start folded.
#[allow(clippy::too_many_arguments)]
fn list_widget(ui: &mut Ui, t: &Tokens, items: &[Scalar], wid: egui::Id, id: &str, unfolded: &mut HashSet<String>, focus: bool, width: f32) -> Option<Vec<Scalar>> {
    let mut new: Option<Vec<Scalar>> = None;
    let long = items.len() > tk::CONFIG_LIST_FOLD;
    let open = !long || unfolded.contains(id);
    ui.with_layout(Layout::top_down(Align::Max), |ui| {
        ui.spacing_mut().item_spacing.y = tk::UNIT;
        if long {
            let label = if open { format!("fold {} items", items.len()) } else { format!("show {} items", items.len()) };
            if Button::new(&label).show(ui, t).clicked() || (focus && !open) {
                if open {
                    unfolded.remove(id);
                } else {
                    unfolded.insert(id.to_string());
                }
            }
        }
        if !open {
            let preview: Vec<String> = items.iter().take(4).map(Scalar::display).collect();
            ui.add(egui::Label::new(kit::text::caption_job(&format!("{} …", preview.join(", ")), t.text_muted)).selectable(false).truncate());
            return;
        }
        if items.is_empty() {
            kit::caption(ui, t, "empty");
        }
        let hint = mc::Hint::default();
        // Every row the same width, with room for its remove button.
        let field_w = (width - tk::BUTTON_HEIGHT - tk::UNIT).max(80.0);
        for (i, it) in items.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = tk::UNIT;
                if Button::new("").icon(glyphs::X).show(ui, t).on_hover_text("remove").clicked() {
                    let mut v2 = items.to_vec();
                    v2.remove(i);
                    new = Some(v2);
                }
                if let Some(v) = scalar_widget(ui, t, it, &hint, None, wid.with(i), None, focus && i == 0, false, field_w) {
                    let mut v2 = items.to_vec();
                    v2[i] = v;
                    new = Some(v2);
                }
            });
        }
        if Button::new("add").icon(glyphs::PLUS).show(ui, t).clicked() {
            let blank = match items.first() {
                Some(Scalar::Bool(_)) => Scalar::Bool(false),
                Some(Scalar::Int(_)) => Scalar::Int(0),
                Some(Scalar::Float(_)) => Scalar::Float(0.0),
                _ => Scalar::Str(String::new()),
            };
            let mut v2 = items.to_vec();
            v2.push(blank);
            new = Some(v2);
        }
    });
    new
}

// -- text -------------------------------------------------------------------------

/// The whole file as text, in JetBrains Mono (raw text: an approved exception), in a 1px frame
/// with line numbers; an error the parser found is named under it in the error colour.
fn raw_editor(ui: &mut Ui, t: &Tokens, o: &mut Open, focus: bool, focused: bool) {
    let size = tk::SIZE_MONO;
    let font = fonts::raw(size);
    let error = if o.format.structured() { o.doc.as_ref().err().cloned() } else { None };
    // The error line goes at the bottom; the editor takes the rest.
    let bottom = if error.is_some() { tk::SIZE_BODY * 2.6 } else { 0.0 };
    let height = (ui.available_height() - bottom - tk::GAP).max(120.0);
    let mut changed = false;
    let resp = egui::Frame::new().inner_margin(egui::Margin::same(tk::GAP as i8)).show(ui, |ui| {
        ui.set_height(height - tk::GAP * 2.0);
        ui.set_width(ui.available_width());
        egui::ScrollArea::both().id_salt(("config-text", &o.rel)).auto_shrink(false).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = tk::GAP;
                // Line numbers, the same height as the text's lines.
                let lines = o.text.split('\n').count().max(1);
                let digits = lines.to_string().len();
                let err_line = error.as_ref().map(|e| e.line);
                let mut job = LayoutJob::default();
                for n in 1..=lines {
                    let c = if err_line == Some(n) { t.error() } else { t.code_comment };
                    job.append(&format!("{n:>digits$}"), 0.0, TextFormat::simple(font.clone(), c));
                    if n < lines {
                        job.append("\n", 0.0, TextFormat::simple(font.clone(), c));
                    }
                }
                ui.add(egui::Label::new(job).selectable(false));
                let format = o.format;
                let tokens = t.clone();
                let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, _wrap: f32| {
                    let mut job = highlight(&tokens, format, buf.as_str(), size);
                    job.wrap.max_width = f32::INFINITY;
                    ui.fonts_mut(|f| f.layout_job(job))
                };
                let edit = egui::TextEdit::multiline(&mut o.text)
                    .id_salt(("config-text-edit", &o.rel))
                    .code_editor()
                    .frame(egui::Frame::NONE)
                    .margin(vec2(0.0, 0.0))
                    .desired_width(f32::INFINITY)
                    .desired_rows(lines.max(10))
                    .lock_focus(true)
                    .layouter(&mut layouter);
                let r = ui
                    .scope(|ui| {
                        let v = ui.visuals_mut();
                        v.text_cursor.stroke = egui::Stroke::new(tk::CARET_WIDTH, t.accent);
                        v.selection.bg_fill = t.wash(0.16);
                        v.selection.stroke = egui::Stroke::new(1.0, t.text_strong);
                        ui.add(edit)
                    })
                    .inner;
                if focus {
                    r.request_focus();
                }
                changed = r.changed();
            });
        });
    });
    let hot = focused || ui.rect_contains_pointer(resp.response.rect);
    kit::rule::frame(ui.painter(), resp.response.rect, crate::design::motion::color(ui.ctx(), ui.id().with("config-text-frame"), hot, t.line_strong, t.accent));
    if changed {
        o.doc = mc::parse(o.format, &o.text);
    }
    if let Some(e) = error {
        ui.add_space(tk::UNIT);
        kit::text::body_colored(ui, &format!("{e}"), t.error());
    }
}

/// Colours for config text (StarShell's code palette): comments, keys, strings, values, section headers;
/// punctuation muted, the rest in the text colour. The accent stays off code.
fn highlight(t: &Tokens, format: Format, text: &str, size: f32) -> LayoutJob {
    let font = fonts::raw(size);
    let mut job = LayoutJob::default();
    if text.len() > HIGHLIGHT_MAX {
        job.append(text, 0.0, TextFormat::simple(font, t.text_strong));
        return job;
    }
    let (plain_c, punct, comment, key_c, string_c, value_c, section_c) = (t.text_strong, t.text_muted, t.code_comment, t.code_function, t.code_string, t.code_constant, t.code_keyword);
    let mut push = |range: std::ops::Range<usize>, c: egui::Color32| {
        if !range.is_empty() {
            job.append(&text[range], 0.0, TextFormat::simple(font.clone(), c));
        }
    };
    let b = text.as_bytes();
    let string_end = |from: usize, q: u8| {
        let mut j = from + 1;
        while j < b.len() && b[j] != q && b[j] != b'\n' {
            if b[j] == b'\\' && !(format == Format::Toml && q == b'\'') {
                j += 1;
            }
            j += 1;
        }
        (j + 1).min(b.len())
    };
    let word_end = |from: usize| {
        let mut j = from;
        while j < b.len() && (b[j].is_ascii_alphanumeric() || matches!(b[j], b'.' | b'+' | b'-' | b'_')) {
            j += 1;
        }
        j
    };
    let is_value_word = |w: &str| matches!(w, "true" | "false" | "null" | "inf" | "nan" | "Infinity" | "NaN") || w.trim_start_matches(['+', '-']).starts_with(|c: char| c.is_ascii_digit() || c == '.');
    match format {
        Format::Json | Format::Json5 => {
            let mut i = 0;
            let mut plain = 0;
            while i < b.len() {
                let (end, color) = match b[i] {
                    b'/' if b.get(i + 1) == Some(&b'/') => (text[i..].find('\n').map_or(b.len(), |n| i + n), comment),
                    b'/' if b.get(i + 1) == Some(&b'*') => (text[i + 2..].find("*/").map_or(b.len(), |n| i + 2 + n + 2), comment),
                    q @ (b'"' | b'\'') => {
                        let end = string_end(i, q);
                        let after = text[end..].trim_start_matches([' ', '\t']);
                        (end, if after.starts_with(':') { key_c } else { string_c })
                    }
                    c if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'+' | b'.' | b'_') => {
                        let end = word_end(i);
                        let w = &text[i..end];
                        let after = text[end..].trim_start_matches([' ', '\t']);
                        if after.starts_with(':') {
                            (end, key_c)
                        } else if is_value_word(w) {
                            (end, value_c)
                        } else {
                            (end, plain_c)
                        }
                    }
                    _ => {
                        i += 1;
                        continue;
                    }
                };
                push(plain..i, punct);
                push(i..end, color);
                i = end;
                plain = end;
            }
            push(plain..b.len(), punct);
        }
        Format::Toml | Format::Text | Format::Options | Format::ShaderSettings => {
            let mut start = 0;
            for line in text.split_inclusive('\n') {
                let end = start + line.len();
                let trimmed = line.trim_start();
                let lead = start + (line.len() - trimmed.len());
                push(start..lead, plain_c);
                let comment_start = |c: u8| c == b'#' || (format != Format::Toml && (c == b';' || c == b'!'));
                if trimmed.bytes().next().is_some_and(comment_start) || trimmed.starts_with("//") {
                    push(lead..end, comment);
                } else if trimmed.starts_with('[') {
                    push(lead..end, section_c);
                } else {
                    // key = value  (or key: value in .properties and YAML)
                    let sep = line[lead - start..].find(['=', ':']).map(|n| lead + n);
                    let mut i = lead;
                    if let Some(s) = sep {
                        push(lead..s, key_c);
                        push(s..s + 1, punct);
                        i = s + 1;
                    }
                    let mut plain = i;
                    while i < end {
                        let c = b[i];
                        let (stop, color) = if c == b'#' && format == Format::Toml {
                            (end, comment)
                        } else if c == b'"' || c == b'\'' {
                            (string_end(i, c).min(end), string_c)
                        } else if (c.is_ascii_alphanumeric() || matches!(c, b'-' | b'+' | b'.')) && (i == plain || !b[i - 1].is_ascii_alphanumeric()) {
                            let we = word_end(i).min(end);
                            (we, if is_value_word(&text[i..we]) { value_c } else { plain_c })
                        } else {
                            i += 1;
                            continue;
                        };
                        push(plain..i, punct);
                        push(i..stop, color);
                        i = stop;
                        plain = stop;
                    }
                    push(plain..end, punct);
                }
                start = end;
            }
        }
    }
    job
}

// -- leaving with unsaved changes ---------------------------------------------------

/// Before the view moves off a file with unsaved changes (another instance, another
/// tab, quitting), put it back and ask. Run once a frame, after the panes.
pub fn guard(app: &mut App, ctx: &egui::Context) {
    let Some(o) = &app.config.open else { return };
    if !o.dirty() {
        return;
    }
    let inst_id = o.inst_id.clone();
    if !app.instances.iter().any(|i| i.id == inst_id) {
        app.config.open = None;
        return;
    }
    let back = |app: &mut App| {
        app.tab = Tab::Instances;
        app.inst_tab = InstTab::Config;
        app.selected = Some(inst_id.clone());
    };
    if ctx.input(|i| i.viewport().close_requested()) {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        if app.modal.is_none() {
            back(app);
            app.modal = Some(Modal::ConfigLeave(Box::new(Leave { away: Away::Quit })));
        }
        return;
    }
    let here = app.tab == Tab::Instances && app.inst_tab == InstTab::Config && app.selected.as_deref() == Some(inst_id.as_str());
    if here || app.modal.is_some() {
        return;
    }
    let away = Away::View { tab: app.tab, inst_tab: app.inst_tab, selected: app.selected.clone() };
    back(app);
    app.modal = Some(Modal::ConfigLeave(Box::new(Leave { away })));
}

/// Go where the user was heading.
fn go(app: &mut App, away: &Away, ctx: &egui::Context) {
    match away {
        Away::File(area, rel) => {
            app.config.area = *area;
            app.config.selected = Some(rel.clone());
            app.config.field_sel = None;
            app.config.field_filter.clear();
        }
        Away::View { tab, inst_tab, selected } => {
            app.tab = *tab;
            app.inst_tab = *inst_tab;
            app.selected = selected.clone();
        }
        Away::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
    }
}
/// The "unsaved changes" dialog; true when it's done.
pub fn leave_dialog(app: &mut App, s: &Leave, ui: &mut Ui) -> bool {
    let t = app.look.tokens.clone();
    let nav = app.modal_nav();
    let Some(o) = &app.config.open else { return true };
    let name = o.rel.display().to_string();
    ui.set_max_width(ui.available_width().min(tk::SHEET_WIDTH));
    kit::heading(ui, &t, "Unsaved changes");
    ui.add_space(tk::UNIT);
    kit::body(ui, &t, &format!("{name} has changes that aren't saved yet."));
    ui.add_space(tk::GUTTER);
    enum Choice {
        Save,
        Discard,
        Stay,
    }
    let mut choice = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = tk::GAP;
        if Button::new("save").filled().show(ui, &t).clicked() || nav == Some(Nav::Activate) {
            choice = Some(Choice::Save);
        }
        if Button::new("discard").show(ui, &t).clicked() {
            choice = Some(Choice::Discard);
        }
        if Button::new("keep editing").show(ui, &t).clicked() {
            choice = Some(Choice::Stay);
        }
    });
    let ctx = ui.ctx().clone();
    match choice {
        // If it can't be saved (it changed on disk, or an error), stay: the editor says why.
        Some(Choice::Save) => {
            if save(app, false) {
                go(app, &s.away, &ctx);
            }
            true
        }
        Some(Choice::Discard) => {
            app.config.open = None;
            go(app, &s.away, &ctx);
            true
        }
        Some(Choice::Stay) => true,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlighting_keeps_every_byte() {
        let t = Tokens::new(false);
        for (format, text) in [
            (Format::Toml, "# c\n[a]\nk = \"v # not\" # yes\nn = 1.5\nlist = ['x', 2]\n"),
            (Format::Json5, "{\n  // c\n  'k': \"v\", n: -1e3, /* b */ x: [true, null]\n}"),
            (Format::Text, "; c\nkey=value\n[s]\nb: true\nunterminated = \"abc"),
        ] {
            let job = highlight(&t, format, text, 12.0);
            assert_eq!(job.text, text, "{format:?}");
        }
    }

    /// The check for a file rewritten on disk reads and parses it off the UI thread: the old text stays
    /// shown until its result is taken, and a result for a file since reloaded, saved or swapped for
    /// another is dropped.
    #[test]
    fn disk_checks_apply_only_to_the_file_they_were_made_of() {
        let dir = crate::paths::test_root().join(format!("config-recheck-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.toml");
        std::fs::write(&path, "a = 1\n").unwrap();
        let open = |text: &str| Open::new("inst".into(), Area::Mods, "a.toml".into(), mc::Loaded { stamp: mc::Stamp::of(&path), format: Format::Toml, base: None, path: path.clone(), text: text.into() });
        let a_is = |o: &Open| o.doc.as_ref().unwrap().fields[0].value.clone();
        let mut o = open("a = 1\n");
        // Nothing changed: nothing is read.
        let r = reread(&o.path, o.stamp, o.format, None);
        assert!(r.changed.is_none());
        assert!(o.take_reread(&o.check_of(), r));
        // The game rewrites it: the check finds it, but the editor shows the old file until it's taken.
        std::fs::write(&path, "a = 22\n").unwrap();
        let of = o.check_of();
        let r = reread(&o.path, o.stamp, o.format, None);
        assert_eq!(o.text, "a = 1\n");
        assert!(o.take_reread(&of, r));
        assert_eq!((o.text.as_str(), o.saved.as_str()), ("a = 22\n", "a = 22\n"));
        assert_eq!(a_is(&o), Value::Scalar(Scalar::Int(22)));
        // A check made before the file was reloaded (or saved) is stale: dropped.
        std::fs::write(&path, "a = 333\n").unwrap();
        let of = o.check_of();
        let r = reread(&o.path, o.stamp, o.format, None);
        o.reload("a = 4\n".into());
        assert!(!o.take_reread(&of, r));
        assert_eq!(o.text, "a = 4\n");
        // So is one made of another instance's or area's file.
        let r = reread(&o.path, None, o.format, None);
        assert!(!o.take_reread(&CheckOf { inst_id: "other".into(), ..o.check_of() }, r));
        let r = reread(&o.path, None, o.format, None);
        assert!(!o.take_reread(&CheckOf { area: Area::Shaders, ..o.check_of() }, r));
        assert_eq!(o.text, "a = 4\n");
        // With unsaved changes here, a change on disk is a conflict, not a reload.
        let mut o = open("a = 333\n");
        o.set_text("a = 5\n".into());
        std::fs::write(&path, "a = 6\n").unwrap();
        let r = reread(&o.path, o.stamp, o.format, None);
        assert!(o.take_reread(&o.check_of(), r));
        assert_eq!(o.text, "a = 5\n");
        assert_eq!(o.conflict.as_deref(), Some("a = 6\n"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A setting's control never writes by being drawn: values outside their range, between steps, not among the
    /// allowed ones, or past what a float holds exactly are shown as they are, frame after frame.
    #[test]
    fn drawing_a_setting_never_changes_it() {
        let ctx = egui::Context::default();
        let hint = |min: Option<f64>, max: Option<f64>, allowed: &[&str], steps: bool| mc::Hint { min, max, allowed: allowed.iter().map(|a| a.to_string()).collect(), steps, ..Default::default() };
        let cases = [
            (Scalar::Int(5000), hint(Some(0.0), Some(100.0), &[], false)),
            (Scalar::Int(-7), hint(Some(0.0), Some(100.0), &[], false)),
            (Scalar::Int(i64::MAX), hint(Some(0.0), None, &[], false)),
            (Scalar::Float(0.123456789), hint(Some(0.0), Some(1.0), &[], false)),
            (Scalar::Float(1e300), hint(None, None, &[], false)),
            (Scalar::Str("medium".into()), hint(None, None, &["low", "high"], true)),
            (Scalar::Str("odd".into()), hint(None, None, &["a", "b", "c", "d", "e", "f"], false)),
            (Scalar::Str("odd".into()), hint(None, None, &["a", "b"], false)),
            (Scalar::Str("text".into()), hint(None, None, &[], false)),
            (Scalar::Bool(true), hint(None, None, &[], false)),
        ];
        for (n, (value, h)) in cases.iter().enumerate() {
            for frame in 0..3 {
                crate::design::test_frame(&ctx, frame as f64, |ui| {
                    let t = Tokens::new(false);
                    let got = scalar_widget(ui, &t, value, h, None, egui::Id::new(("case", n)), None, false, false, 300.0);
                    assert_eq!(got, None, "case {n} ({value:?}) changed on frame {frame}");
                });
            }
        }
    }

    /// A slider's move is written on the step's own decimals, and only ranges a float holds exactly slide.
    #[test]
    fn slider_moves_are_written_cleanly() {
        let step = float_step(0.0, 1.0);
        assert_eq!(step, 0.01);
        assert_eq!(to_step(0.1 + 0.2, 0.1), 0.3);
        assert_eq!(to_step(0.30000000000000004, step), 0.3);
        assert_eq!(to_step(7.0, 1.0), 7.0);
        assert_eq!(float_step(0.0, 1000.0), 10.0);
        assert!(slides(Some(0.0), Some(1000.0)).is_some());
        assert!(slides(Some(0.0), Some(1001.0)).is_none());
        assert!(slides(Some(1e17), Some(1e17 + 10.0)).is_none());
        assert!(slides(None, Some(10.0)).is_none());
    }

    #[test]
    fn files_filter_by_path_or_mod() {
        let f = |p: &str| mc::ConfigFile { rel: p.into(), size: 1, modified: None };
        let groups = vec![Group { name: "Sodium".into(), icon: None, ids: vec!["sodium".into()], files: vec![f("sodium-options.json"), f("sodium-mixins.properties")] }, Group { name: "other".into(), icon: None, ids: Vec::new(), files: vec![f("mixins.json")] }];
        let all: Vec<PathBuf> = visible(&groups, "").into_iter().flat_map(|(_, f)| f).collect();
        assert_eq!(all.len(), 3);
        let sodium: Vec<PathBuf> = visible(&groups, "sodium").into_iter().flat_map(|(_, f)| f).collect();
        assert_eq!(sodium.len(), 2);
        let mixins: Vec<(usize, Vec<PathBuf>)> = visible(&groups, "MIXINS");
        assert_eq!(mixins.iter().map(|(n, f)| (*n, f.len())).collect::<Vec<_>>(), [(0, 1), (1, 1)]);
    }
}
