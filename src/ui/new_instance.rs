//! The new-instance sheet: make one (a Minecraft version, the instance's name, group and
//! settings, a loader) or import one (a file, folder, link, another launcher's instance, or a
//! Modrinth modpack). Two panes each, over a foot with the keys and the sheet's buttons.

use std::path::PathBuf;
use std::time::Instant;

use egui::{Align, Layout, Ui};

use super::content::{padded, pane_head, sheet_foot, sheet_panes};
use super::nav::Nav;
use super::version_picker::VersionPicker;
use super::{App, ToastKind};
use crate::config::Config;
use crate::design::glyphs;
use crate::design::kit::{self, Button, HoverList, HoverRow, SettingRow, Variant};
use crate::design::tokens::{self as tk, Tokens};
use crate::import::{self, Found};
use crate::instance::Instance;
use crate::keymap::Action;
use crate::modrinth::{self, Hit, SearchResponse, Version};
use crate::task::Promise;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Create,
    Import,
}

/// Where an import comes from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum From {
    Files,
    Modrinth,
}

/// Modrinth modpack search, inside the import dialog.
#[derive(Default)]
struct Packs {
    query: String,
    /// When the query last changed, so a search goes out once typing pauses.
    typed: Option<Instant>,
    /// The last query sent.
    searched: Option<String>,
    search_p: Option<Promise<SearchResponse>>,
    results: Option<Result<Vec<Hit>, String>>,
    pack: Option<Hit>,
    versions_p: Option<Promise<Vec<Version>>>,
    versions: Option<Result<Vec<Version>, String>>,
    /// The chosen version's id.
    version: Option<String>,
    scroll_pack: bool,
    scroll_version: bool,
    focus: bool,
}

impl Packs {
    fn search(&mut self, rt: &tokio::runtime::Handle, ctx: &egui::Context) {
        let q = self.query.clone();
        self.searched = Some(q.clone());
        self.typed = None;
        self.search_p = Some(Promise::spawn(rt, ctx, async move { modrinth::search_packs(&q).await }));
    }

    fn pick(&mut self, hit: &Hit, rt: &tokio::runtime::Handle, ctx: &egui::Context) {
        if self.pack.as_ref().is_some_and(|p| p.project_id == hit.project_id) {
            return;
        }
        self.pack = Some(hit.clone());
        self.versions = None;
        self.version = None;
        let id = hit.project_id.clone();
        self.versions_p = Some(Promise::spawn(rt, ctx, async move { modrinth::pack_versions(&id).await }));
    }

    fn poll(&mut self) {
        if let Some(p) = &self.search_p
            && let Some(r) = p.take()
        {
            self.search_p = None;
            self.results = Some(r.map(|s| s.hits).map_err(|e| format!("{e:#}")));
        }
        if let Some(p) = &self.versions_p
            && let Some(r) = p.take()
        {
            self.versions_p = None;
            // Newest first, so the first one is the pack as it is now.
            self.version = r.as_ref().ok().and_then(|v| v.first()).map(|v| v.id.clone());
            self.versions = Some(r.map_err(|e| format!("{e:#}")));
            self.scroll_version = true;
        }
    }

    /// What the importer downloads: the chosen version's page, which it resolves to the
    /// `.mrpack` and its SHA-1.
    fn source(&self) -> Option<String> {
        let (pack, version) = (self.pack.as_ref()?, self.version.as_ref()?);
        Some(format!("https://modrinth.com/modpack/{}/version/{version}", pack.slug))
    }
}

pub struct NewInstance {
    pub mode: Mode,
    name: String,
    name_touched: bool,
    group: String,
    picker: VersionPicker,
    /// Instance to copy game settings from (id), and the ones that have any (id, name, version).
    settings_from: Option<String>,
    settings_sources: Option<Vec<(String, String, String)>>,
    // import
    pub source: String,
    found: Option<Vec<Found>>,
    browse_p: Option<Promise<Option<PathBuf>>>,
    focus_source: bool,
    from: From,
    packs: Packs,
    pub cancelled: bool,
}

/// What the dialog wants the app to do when it closes.
pub enum Outcome {
    Created(Instance),
    Import(import::Request),
}

impl NewInstance {
    pub fn new(cfg: &Config) -> Self {
        let mut picker = VersionPicker::new(cfg.show_snapshots, None);
        picker.focus_filter = true;
        Self {
            mode: Mode::Create,
            name: String::new(),
            name_touched: false,
            group: String::new(),
            picker,
            settings_from: None,
            settings_sources: None,
            source: String::new(),
            found: None,
            browse_p: None,
            focus_source: false,
            from: From::Files,
            packs: Packs::default(),
            cancelled: false,
        }
    }

    pub fn import(cfg: &Config, source: String) -> Self {
        let mut s = Self::new(cfg);
        s.focus_source = source.is_empty();
        s.set_source(source);
        s
    }

    /// Switch to importing from a file, folder or link (a dropped file).
    pub fn set_source(&mut self, source: String) {
        self.mode = Mode::Import;
        self.from = From::Files;
        self.source = source;
    }

    fn default_name(&self) -> String {
        let v = self.picker.version.clone().unwrap_or_default();
        match self.picker.kind {
            crate::loaders::LoaderKind::Vanilla => v,
            k => format!("{v} {}", k.label()),
        }
    }
}

/// The room a sheet's foot takes: its rule, a gap, the buttons and the sheet's own margin.
fn foot_height() -> f32 {
    tk::LINE + tk::GAP + tk::BUTTON_HEIGHT + tk::GUTTER
}

pub fn show(app: &mut App, s: &mut NewInstance, ui: &mut Ui) -> Option<Outcome> {
    let t = app.look.tokens.clone();
    // The whole content area: lists of versions and packs want the room.
    ui.spacing_mut().item_spacing.y = 0.0;
    // The heading, and new or import as joined buttons at its right.
    ui.horizontal(|ui| {
        kit::heading(ui, &t, if s.mode == Mode::Create { "New instance" } else { "Import an instance" });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let mut mode = s.mode;
            if kit::segmented(ui, &t, &mut mode, &[(Mode::Create, "new"), (Mode::Import, "import")]) {
                s.mode = mode;
                s.focus_source = mode == Mode::Import;
                s.picker.focus_filter = mode == Mode::Create;
            }
        });
    });
    ui.add_space(tk::GUTTER);
    match s.mode {
        Mode::Create => create(app, s, ui),
        Mode::Import => import_ui(app, s, ui),
    }
}

/// A text field with a menu of the existing choices beside it (groups, share sets); the
/// menu's last entry, `none`, clears it. For a settings row's control (laid out right to left).
pub fn group_field(ui: &mut Ui, t: &Tokens, id: egui::Id, value: &mut String, choices: &[String], none: &str, hint: &str, width: f32) -> egui::Response {
    ui.spacing_mut().item_spacing.x = tk::GAP;
    if !choices.is_empty() {
        let mut options: Vec<&str> = choices.iter().map(String::as_str).collect();
        options.push(none);
        let at = choices.iter().position(|c| c == value.trim()).or((value.trim().is_empty()).then_some(choices.len()));
        if let Some(i) = kit::menu(ui, t, id, at, "", &options, 0.0, false) {
            *value = if i == choices.len() { String::new() } else { choices[i].clone() };
        }
    }
    let menu_w = if choices.is_empty() { 0.0 } else { tk::BUTTON_HEIGHT + tk::GAP };
    kit::Field::new(value).hint(hint).width((width - menu_w).max(tk::GUTTER * 4.0)).show(ui, t)
}

/// The instance's name, group and (when making one) where its game settings come from, under
/// a pane head `head_h` tall (as tall as the head beside it, so their rules are one line).
fn instance_group(app: &App, s: &mut NewInstance, ui: &mut Ui, number: usize, name_hint: &str, settings: bool, head_h: f32) {
    let t = app.look.tokens.clone();
    let groups = app.groups.names();
    pane_head(ui, &t, number, "instance", "", &[], head_h);
    ui.scope(|ui| {
        SettingRow::new("Name").show(ui, &t, |ui| {
            let w = ui.available_width().min(tk::SETTING_CONTROL_WIDTH);
            if kit::Field::new(&mut s.name).hint(name_hint).width(w).show(ui, &t).changed() {
                s.name_touched = true;
            }
        });
        SettingRow::new("Group").show(ui, &t, |ui| {
            let w = ui.available_width().min(tk::SETTING_CONTROL_WIDTH);
            group_field(ui, &t, egui::Id::new("new-instance-group"), &mut s.group, &groups, "no group", "No group", w);
        });
        if settings {
            settings_row(app, s, ui, &t);
        }
    });
    kit::rule(ui, &t, true);
}

/// Picks an instance whose game settings (options.txt) the new one starts with.
fn settings_row(app: &App, s: &mut NewInstance, ui: &mut Ui, t: &Tokens) {
    let sources = s.settings_sources.get_or_insert_with(|| app.instances.iter().filter(|i| i.has_settings()).map(|i| (i.id.clone(), i.name.clone(), i.mc_version.clone())).collect());
    let chosen = s.settings_from.as_ref().and_then(|id| sources.iter().position(|(i, _, _)| i == id));
    // Keybinds from 1.13 on use key names, which older versions can't read (they fall back to defaults).
    let target = s.picker.version.as_deref().unwrap_or_default();
    let old = |v: &str| crate::world_sets::compare_versions(v, "1.13") == std::cmp::Ordering::Less;
    let warn = chosen.is_some_and(|c| !target.is_empty() && old(target) && !old(&sources[c].2));
    let (facts, color) = if warn {
        ("keybinds won't carry over to versions before 1.13", Some(t.warning()))
    } else if sources.is_empty() {
        ("none to copy yet", None)
    } else {
        ("copied once", None)
    };
    SettingRow::new("Game settings").facts(facts, color).show(ui, t, |ui| {
        let mut options: Vec<String> = vec!["game defaults".into()];
        options.extend(sources.iter().map(|(_, name, mc)| format!("{name} · {mc}")));
        let labels: Vec<&str> = options.iter().map(String::as_str).collect();
        let label = match chosen {
            Some(c) => format!("from {}", sources[c].1),
            None => "game defaults".into(),
        };
        let w = ui.available_width().min(tk::SETTING_CONTROL_WIDTH);
        if let Some(i) = kit::menu(ui, t, egui::Id::new("new-instance-settings"), Some(chosen.map_or(0, |c| c + 1)), &label, &labels, w, false) {
            s.settings_from = (i > 0).then(|| sources[i - 1].0.clone());
        }
    });
}

fn create(app: &mut App, s: &mut NewInstance, ui: &mut Ui) -> Option<Outcome> {
    let t = app.look.tokens.clone();
    s.picker.poll(app, ui.ctx());
    let mut out = None;
    let (vnav, lnav) = s.picker.typing_keys(ui);
    let vnav = vnav.or(app.modal_nav());
    if !s.name_touched {
        s.name = s.default_name();
    }
    let (mut left, mut right) = sheet_panes(ui, &t, tk::VERSION_PANE_SHARE, tk::LIST_RAIL_MIN, foot_height());
    s.picker.versions_ui(app, &mut left, vnav, true, 1);
    right.spacing_mut().item_spacing.y = 0.0;
    instance_group(app, s, &mut right, 2, "Instance name", true, 0.0);
    // Both lists follow the keyboard at once (arrows, alt+arrows), so both chosen rows are hot.
    s.picker.loader_ui(app, &mut right, lnav, true, 3);

    let sel = s.picker.selection();
    let ready = sel.is_some() && !s.name.trim().is_empty();
    let k = |a| app.keymap.label(a);
    let hints = [("up/down".to_string(), "version"), ("alt+up/down".into(), "loader version"), ("alt+left/right".into(), "loader"), (k(Action::Activate), "create")];
    let clicked = sheet_foot(ui, &t, &hints, &[Button::new("create").filled().enabled(ready), Button::new("cancel")]);
    if clicked == Some(1) {
        s.cancelled = true;
    }
    let create = clicked == Some(0) || (ready && (vnav == Some(Nav::Activate) || ui.input(|i| i.key_pressed(egui::Key::Enter))));
    if create && let Some((mc, loader)) = sel {
        let mut inst = Instance::new(s.name.trim(), &mc, loader);
        inst.group = s.group.trim().to_string();
        match inst.assign_id().map_err(anyhow::Error::from).and_then(|()| inst.save()) {
            Ok(()) => {
                let from = s.settings_from.as_ref().and_then(|id| app.instances.iter().find(|i| &i.id == id));
                if let Some(from) = from
                    && let Err(e) = inst.copy_settings_from(from)
                {
                    app.toast(ToastKind::Error, format!("Created {}, but couldn't copy settings from {}: {e}", inst.name, from.name));
                }
                out = Some(Outcome::Created(inst));
            }
            Err(e) => app.toast(ToastKind::Error, format!("Couldn't create instance: {e}")),
        }
    }
    out
}

fn import_ui(app: &mut App, s: &mut NewInstance, ui: &mut Ui) -> Option<Outcome> {
    let t = app.look.tokens.clone();
    if let Some(p) = &s.browse_p
        && let Some(r) = p.take()
    {
        s.browse_p = None;
        match r {
            Ok(Some(path)) => s.source = path.display().to_string(),
            Ok(None) => {}
            Err(e) => app.toast(ToastKind::Error, format!("File picker failed: {e:#}")),
        }
    }
    s.packs.poll();
    // An import is named after what it imports unless named here: the new-instance default
    // (the version) isn't carried over.
    if !s.name_touched {
        s.name.clear();
    }
    let found = s.found.get_or_insert_with(import::detect).clone();
    let (pack_nav, version_nav) = if s.from == From::Modrinth { list_keys(ui) } else { (None, None) };
    let nav = app.modal_nav();
    let pack_nav = pack_nav.or(nav.filter(|n| n.is_move()));
    let mut out = None;
    let mut submit = nav == Some(Nav::Activate);

    let (mut left, mut right) = sheet_panes(ui, &t, 0.58, tk::LIST_RAIL_MIN, foot_height());
    left.spacing_mut().item_spacing.y = 0.0;
    // The source, files or Modrinth, as joined buttons in the pane's head.
    let aside = match s.from {
        From::Files if found.is_empty() => String::new(),
        From::Files => format!("{} found", found.len()),
        From::Modrinth if s.packs.search_p.is_some() => "searching".into(),
        From::Modrinth => match &s.packs.results {
            Some(Ok(h)) => format!("{} packs", h.len()),
            _ => String::new(),
        },
    };
    let head = [Button::new("files and launchers").checked(s.from == From::Files), Button::new("modrinth").checked(s.from == From::Modrinth)];
    let (clicked, _) = pane_head(&mut left, &t, 1, "import from", &aside, &head, 0.0);
    let from = match clicked {
        Some(0) => Some(From::Files),
        Some(1) => Some(From::Modrinth),
        _ => None,
    };
    if let Some(f) = from.filter(|f| *f != s.from) {
        s.from = f;
        match f {
            From::Files => s.focus_source = true,
            From::Modrinth => s.packs.focus = true,
        }
    }
    match s.from {
        From::Files => submit |= files_ui(app, s, &mut left, &found),
        From::Modrinth => submit |= packs_ui(app, s, &mut left, pack_nav),
    }

    right.spacing_mut().item_spacing.y = 0.0;
    let hint = match (&s.from, &s.packs.pack) {
        (From::Modrinth, Some(p)) => p.title.clone(),
        _ => "From the pack".to_string(),
    };
    instance_group(app, s, &mut right, 2, &hint, false, tk::BUTTON_HEIGHT + tk::GAP * 2.0);
    if s.from == From::Modrinth {
        versions_ui(app, s, &mut right, version_nav);
    } else {
        padded(&mut right, |ui| {
            ui.add_space(tk::GUTTER);
            ui.set_max_width(ui.available_width().min(tk::SETTING_DESCRIPTION_WIDTH));
            kit::body_muted(ui, &t, "Files are copied, so the original stays as it is. On btrfs the copy is instant and takes no extra space.");
        });
    }

    let source = match s.from {
        From::Files => Some(s.source.trim().to_string()).filter(|s| !s.is_empty()),
        From::Modrinth => s.packs.source(),
    };
    let ready = source.is_some();
    let k = |a| app.keymap.label(a);
    let hints: Vec<(String, &str)> = match s.from {
        From::Files => vec![(k(Action::Activate), "import")],
        From::Modrinth => vec![("up/down".into(), "pack"), ("alt+up/down".into(), "version"), (k(Action::Activate), "import")],
    };
    let clicked = sheet_foot(ui, &t, &hints, &[Button::new("import").filled().enabled(ready), Button::new("cancel")]);
    if clicked == Some(1) {
        s.cancelled = true;
    }
    let go = clicked == Some(0) || (ready && submit);
    if go && let Some(source) = source {
        let name = match (&s.from, &s.packs.pack) {
            // The pack's own name is often its slug or carries a version; the title is what Modrinth shows.
            (From::Modrinth, Some(p)) if !s.name_touched => Some(p.title.clone()),
            _ => Some(s.name.trim().to_string()).filter(|n| s.name_touched && !n.is_empty()),
        };
        out = Some(Outcome::Import(import::Request { source, name, group: s.group.trim().to_string() }));
    }
    out
}

/// Arrow keys move through packs, alt+arrows through the chosen pack's versions, also while
/// the search field has focus.
fn list_keys(ui: &Ui) -> (Option<Nav>, Option<Nav>) {
    use egui::{Key, Modifiers};
    ui.input_mut(|i| {
        if i.consume_key(Modifiers::ALT, Key::ArrowDown) {
            (None, Some(Nav::Down))
        } else if i.consume_key(Modifiers::ALT, Key::ArrowUp) {
            (None, Some(Nav::Up))
        } else if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
            (Some(Nav::Down), None)
        } else if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
            (Some(Nav::Up), None)
        } else if i.consume_key(Modifiers::NONE, Key::PageDown) {
            (Some(Nav::PageDown), None)
        } else if i.consume_key(Modifiers::NONE, Key::PageUp) {
            (Some(Nav::PageUp), None)
        } else {
            (None, None)
        }
    })
}

/// A path, link or another launcher's instance. Returns whether enter was pressed in the field.
fn files_ui(app: &App, s: &mut NewInstance, ui: &mut Ui, found: &[Found]) -> bool {
    let t = &app.look.tokens;
    let mut submit = false;
    ui.add_space(tk::GAP);
    padded(ui, |ui| {
        let browse = [Button::new("file").icon(glyphs::FILE).enabled(s.browse_p.is_none()), Button::new("folder").icon(glyphs::FOLDER).enabled(s.browse_p.is_none())];
        let browse_w = browse.iter().map(|b| b.width(ui, t)).sum::<f32>() - tk::LINE;
        let row = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = tk::GAP;
            let w = (ui.available_width() - browse_w - tk::GAP).max(tk::GUTTER * 4.0);
            let r = kit::Field::new(&mut s.source).label("path or link").hint("~/Downloads/pack.mrpack").width(w).focus(std::mem::take(&mut s.focus_source)).show(ui, t);
            submit = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            kit::button_row(ui, t, &browse)
        });
        if let Some(i) = row.inner {
            s.browse_p = Some(pick(app, ui.ctx(), i == 1));
        }
        ui.add_space(tk::GAP);
        ui.scope(|ui| {
            ui.set_max_width(ui.available_width().min(tk::SETTING_DESCRIPTION_WIDTH));
            kit::body_muted(ui, t, "Prism or MultiMC exports (.zip), instance folders, Modrinth packs (.mrpack) or modrinth.com modpack links. You can also drop a file onto the window.");
        });
    });
    ui.add_space(tk::GAP);
    egui::ScrollArea::vertical().id_salt("found").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if found.is_empty() {
            padded(ui, |ui| kit::loading(ui, t, "no Prism, PolyMC or MultiMC instances found"));
            return;
        }
        // Each launcher under its own label.
        let mut launchers: Vec<&str> = found.iter().map(|f| f.launcher).collect();
        launchers.dedup();
        for (n, launcher) in launchers.iter().enumerate() {
            let mine: Vec<&Found> = found.iter().filter(|f| f.launcher == *launcher).collect();
            kit::SectionLabel::new(n + 2, launcher).aside(&mine.len().to_string()).show(ui, t);
            let mut list = HoverList::begin(ui, Variant::Compact).indent(tk::GUTTER + tk::HOVER_SHIFT_COMPACT);
            for f in mine {
                let path = f.dir.display().to_string();
                let detail = match &f.unsupported {
                    Some(why) => why.clone(),
                    None if f.loader.is_empty() || f.loader.eq_ignore_ascii_case("vanilla") => f.mc_version.clone(),
                    None => format!("{} · {}", f.loader, f.mc_version),
                };
                let on = s.source == path;
                let r = list.row(ui, t, HoverRow::new(("found", &path), &f.name).description(&detail).dim(f.unsupported.is_some()).selected(on));
                if r.clicked() && f.unsupported.is_none() {
                    s.source = path;
                }
                if r.double_clicked() && f.unsupported.is_none() {
                    submit = true;
                }
            }
            list.end(ui);
        }
    });
    submit
}

/// Modrinth modpack search. Returns whether enter was pressed with nothing new to search for.
fn packs_ui(app: &App, s: &mut NewInstance, ui: &mut Ui, nav: Option<Nav>) -> bool {
    let (t, ctx, rt) = (&app.look.tokens, ui.ctx().clone(), app.rt.clone());
    let p = &mut s.packs;
    if p.searched.is_none() && p.search_p.is_none() {
        p.search(&rt, &ctx);
    }
    ui.add_space(tk::GAP);
    let r = padded(ui, |ui| kit::query(ui, t, &mut p.query, "Search Modrinth modpacks", tk::SIZE_HEADING, std::mem::take(&mut p.focus)));
    ui.add_space(tk::GAP);
    kit::rule(ui, t, false);
    if r.changed() {
        p.typed = Some(Instant::now());
        ctx.request_repaint_after(super::content::SEARCH_DELAY);
    }
    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let unsent = p.searched.as_ref() != Some(&p.query);
    if unsent && (enter || p.typed.is_some_and(|at| at.elapsed() >= super::content::SEARCH_DELAY)) {
        p.search(&rt, &ctx);
    }
    let hits = match &p.results {
        None => {
            padded(ui, |ui| kit::loading(ui, t, "searching"));
            return false;
        }
        Some(Err(e)) => {
            let e = e.clone();
            padded(ui, |ui| {
                ui.add_space(tk::GUTTER);
                kit::bracketed(ui, t, "can't search modrinth", |ui| kit::text::body_colored(ui, &e, t.error()));
                ui.add_space(tk::GAP);
                if Button::new("try again").show(ui, t).clicked() {
                    p.search(&rt, &ctx);
                }
            });
            return false;
        }
        Some(Ok(h)) if h.is_empty() => {
            padded(ui, |ui| kit::loading(ui, t, "no modpacks found"));
            return false;
        }
        Some(Ok(h)) => h.clone(),
    };
    let at = hits.iter().position(|h| p.pack.as_ref().is_some_and(|k| k.project_id == h.project_id));
    if let Some(n) = nav.and_then(|n| n.step(hits.len(), at)) {
        p.pick(&hits[n], &rt, &ctx);
        p.scroll_pack = true;
    }
    let mut picked = None;
    egui::ScrollArea::vertical().id_salt("packs").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut list = HoverList::begin(ui, Variant::Compact).indent(tk::GUTTER + tk::HOVER_SHIFT_COMPACT);
        for h in &hits {
            let on = p.pack.as_ref().is_some_and(|k| k.project_id == h.project_id);
            let desc = format!("by {} · {}", h.author, modrinth::format_count(h.downloads));
            let mut row = HoverRow::new(("pack", &h.project_id), &h.title).description(&desc).selected(on).hot(on);
            if let Some(path) = h.icon_url.as_deref().and_then(|u| app.content.remote.get(u, &rt, &ctx)) {
                row = row.image(super::content::file_image(&path), 1.0);
            }
            let r = list.row(ui, t, row);
            if on && std::mem::take(&mut p.scroll_pack) {
                r.scroll_to_me(None);
            }
            if r.clicked() {
                picked = Some(h.clone());
            }
        }
        list.end(ui);
    });
    if let Some(h) = picked {
        p.pick(&h, &rt, &ctx);
    }
    enter && !unsent
}

/// The chosen pack's summary and its versions, newest first and preselected.
fn versions_ui(app: &App, s: &mut NewInstance, ui: &mut Ui, nav: Option<Nav>) {
    let t = &app.look.tokens;
    let p = &mut s.packs;
    let aside = p.pack.as_ref().map(|k| k.title.clone()).unwrap_or_default();
    pane_head(ui, t, 3, "pack version", &aside, &[], 0.0);
    let Some(pack) = &p.pack else {
        padded(ui, |ui| kit::loading(ui, t, "pick a modpack"));
        return;
    };
    if !pack.description.is_empty() {
        padded(ui, |ui| {
            ui.add_space(tk::GAP);
            ui.set_max_width(ui.available_width().min(tk::SETTING_DESCRIPTION_WIDTH));
            kit::body_muted(ui, t, &pack.description);
        });
        ui.add_space(tk::GAP);
        kit::rule(ui, t, false);
    }
    let versions = match &p.versions {
        None => {
            padded(ui, |ui| kit::loading(ui, t, "reading versions"));
            return;
        }
        Some(Err(e)) => {
            padded(ui, |ui| {
                ui.add_space(tk::GUTTER);
                kit::bracketed(ui, t, "can't read the pack's versions", |ui| kit::text::body_colored(ui, e, t.error()));
            });
            return;
        }
        Some(Ok(v)) if v.is_empty() => {
            padded(ui, |ui| kit::loading(ui, t, &format!("{} has no versions", pack.title)));
            return;
        }
        Some(Ok(v)) => v,
    };
    let at = versions.iter().position(|v| p.version.as_ref() == Some(&v.id));
    if let Some(n) = nav.and_then(|n| n.step(versions.len(), at)) {
        p.version = Some(versions[n].id.clone());
        p.scroll_version = true;
    }
    let mut picked = None;
    egui::ScrollArea::vertical().id_salt("pack-versions").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut list = HoverList::begin(ui, Variant::Compact).indent(tk::GUTTER + tk::HOVER_SHIFT_COMPACT);
        for v in versions {
            let on = p.version.as_ref() == Some(&v.id);
            let mut game = v.game_versions.join(", ");
            if let Some(l) = v.loaders.iter().find(|l| *l != "minecraft") {
                game = format!("{l} · {game}");
            }
            if !v.version_type.is_empty() && v.version_type != "release" {
                game = format!("{} · {game}", v.version_type);
            }
            let r = list.row(ui, t, HoverRow::new(("pack-version", &v.id), &v.version_number).description(&game).selected(on).hot(on));
            if on && std::mem::take(&mut p.scroll_version) {
                r.scroll_to_me(Some(egui::Align::Center));
            }
            if r.clicked() {
                picked = Some(v.id.clone());
            }
        }
        list.end(ui);
    });
    if picked.is_some() {
        p.version = picked;
    }
}

/// Open the desktop file picker (via the XDG portal).
pub fn pick(app: &App, ctx: &egui::Context, folder: bool) -> Promise<Option<PathBuf>> {
    Promise::spawn(&app.rt, ctx, async move {
        let d = rfd::AsyncFileDialog::new().set_directory(dirs::download_dir().or_else(dirs::home_dir).unwrap_or_default());
        let picked = if folder {
            d.set_title("Import instance folder").pick_folder().await
        } else {
            d.set_title("Import instance or modpack").add_filter("Instances and modpacks", &["zip", "mrpack"]).pick_file().await
        };
        Ok(picked.map(|f| f.path().to_path_buf()))
    })
}
