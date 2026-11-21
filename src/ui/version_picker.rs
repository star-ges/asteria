//! Minecraft version + mod loader chooser, shared by the new-instance sheet and an instance's
//! overview (change version). Two panes, each a compact hover list under a pane head: the
//! versions under a query line with the snapshots switch, the loader as joined buttons over
//! its versions.

use egui::{Align, Layout, Rect, Ui, UiBuilder};

use super::App;
use super::content::{padded, pane_head};
use super::nav::Nav;
use crate::design::kit::{self, HoverList, HoverRow, Variant};
use crate::design::tokens::{self as tk, Tokens};
use crate::loaders::{self, Build, Loader, LoaderKind};
use crate::task::Promise;

/// Loader versions listed (newest first); older ones are rarely wanted.
const LOADER_VERSIONS_SHOWN: usize = 60;

pub struct VersionPicker {
    filter: String,
    snapshots: bool,
    pub version: Option<String>,
    pub kind: LoaderKind,
    loader_versions: Option<Result<Vec<Build>, String>>,
    loader_p: Option<(LoaderKind, String, Promise<Vec<Build>>)>,
    loader_version: Option<String>,
    /// Loader version to keep selected once the list arrives.
    prefer: Option<String>,
    /// What the instance plays now (changing version), marked in the lists.
    was: Option<(String, Loader)>,
    pub focus_filter: bool,
    /// Bring the current version into view (the first frame, and after a keyboard move).
    scroll_once: bool,
    scroll_loader: bool,
}

impl VersionPicker {
    pub fn new(snapshots: bool, current: Option<(&str, &Loader)>) -> Self {
        Self {
            filter: String::new(),
            snapshots,
            version: current.map(|(v, _)| v.to_string()),
            kind: current.map(|(_, l)| l.kind()).unwrap_or(LoaderKind::Vanilla),
            loader_versions: None,
            loader_p: None,
            loader_version: None,
            prefer: current.and_then(|(_, l)| l.version().map(String::from)),
            was: current.map(|(v, l)| (v.to_string(), l.clone())),
            focus_filter: false,
            scroll_once: true,
            scroll_loader: true,
        }
    }

    pub fn cycle_kind(&mut self) {
        let all = LoaderKind::ALL;
        let i = all.iter().position(|k| *k == self.kind).unwrap_or(0);
        self.kind = all[(i + 1) % all.len()];
    }

    /// While typing in the filter: ↑/↓ pick a version, alt+↑/↓ a loader
    /// version, alt+←/→ the loader type. For dialogs, where the filter
    /// field holds keyboard focus.
    pub fn typing_keys(&mut self, ui: &Ui) -> (Option<Nav>, Option<Nav>) {
        use egui::{Key, Modifiers};
        ui.input_mut(|i| {
            let mut versions = None;
            let mut loader = None;
            if i.consume_key(Modifiers::ALT, Key::ArrowDown) {
                loader = Some(Nav::Down);
            } else if i.consume_key(Modifiers::ALT, Key::ArrowUp) {
                loader = Some(Nav::Up);
            } else if i.consume_key(Modifiers::ALT, Key::ArrowRight) || i.consume_key(Modifiers::ALT, Key::ArrowLeft) {
                self.cycle_kind();
            } else if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                versions = Some(Nav::Down);
            } else if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                versions = Some(Nav::Up);
            } else if i.consume_key(Modifiers::NONE, Key::PageDown) {
                versions = Some(Nav::PageDown);
            } else if i.consume_key(Modifiers::NONE, Key::PageUp) {
                versions = Some(Nav::PageUp);
            }
            (versions, loader)
        })
    }

    /// The chosen (minecraft version, loader), once both are known.
    pub fn selection(&self) -> Option<(String, Loader)> {
        let loader = match self.kind {
            LoaderKind::Vanilla => Loader::Vanilla,
            k => k.with(self.loader_version.clone()?),
        };
        Some((self.version.clone()?, loader))
    }

    /// The choice in a few words: `1.21.11 · Fabric 0.16.14`.
    pub fn summary(&self) -> Option<String> {
        let (mc, loader) = self.selection()?;
        Some(if loader == Loader::Vanilla { mc } else { format!("{mc} · {}", loader.display()) })
    }

    /// Default to the latest release and fetch loader versions as needed.
    pub fn poll(&mut self, app: &App, ctx: &egui::Context) {
        if self.version.is_none()
            && let Some(Ok(m)) = &app.manifest
        {
            self.version = Some(m.latest.release.clone());
        }
        let (Some(mc), true) = (&self.version, self.kind != LoaderKind::Vanilla) else { return };
        let stale = self.loader_p.as_ref().is_none_or(|(k, v, _)| *k != self.kind || v != mc);
        if stale {
            let (kind, mc2) = (self.kind, mc.clone());
            self.loader_versions = None;
            self.loader_version = None;
            self.loader_p = Some((self.kind, mc.clone(), Promise::spawn(&app.rt, ctx, async move { loaders::versions_for(kind, &mc2).await })));
        }
        if let Some((_, _, p)) = &self.loader_p
            && let Some(r) = p.take()
        {
            let r = r.map_err(|e| format!("{e:#}"));
            if let Ok(list) = &r {
                let keep = self.prefer.take().filter(|v| list.iter().any(|b| &b.version == v));
                self.loader_version = keep.or_else(|| list.iter().find(|b| b.preferred).or(list.first()).map(|b| b.version.clone()));
                self.scroll_loader = true;
            }
            self.loader_versions = Some(r);
        }
    }

    /// The Minecraft versions as a pane: head, query line with the snapshots switch, the list.
    /// `focused`: the list has the keyboard (its current row is hot).
    pub fn versions_ui(&mut self, app: &mut App, ui: &mut Ui, nav: Option<Nav>, focused: bool, number: usize) {
        let t = app.look.tokens.clone();
        ui.spacing_mut().item_spacing.y = 0.0;
        let shown: Vec<Row> = match &app.manifest {
            Some(Ok(m)) => m
                .versions
                .iter()
                .filter(|v| {
                    let current = self.version.as_deref() == Some(v.id.as_str());
                    let kind_ok = v.kind == "release" || (self.snapshots && v.kind == "snapshot") || current;
                    kind_ok && (self.filter.is_empty() || v.id.contains(self.filter.trim()))
                })
                .map(|v| Row { title: v.id.clone(), description: format!("{} · {}", v.kind, v.release_time.format("%Y-%m-%d")), stable: false })
                .collect(),
            _ => Vec::new(),
        };
        let aside = match &app.manifest {
            Some(Ok(_)) if shown.len() == 1 => "1 version".to_string(),
            Some(Ok(_)) => format!("{} versions", shown.len()),
            _ => String::new(),
        };
        pane_head(ui, &t, number, "minecraft version", &aside, &[], 0.0);
        ui.add_space(tk::GAP);
        padded(ui, |ui| {
            let label = "snapshots";
            let side = tk::SWITCH_WIDTH + tk::GAP + kit::text::caption_galley(ui, label, t.text_muted).size().x;
            let right = ui.max_rect().right();
            let q = ui
                .scope(|ui| {
                    ui.set_max_width((ui.available_width() - side - tk::GUTTER).max(tk::GUTTER * 4.0));
                    kit::query(ui, &t, &mut self.filter, "Filter versions", tk::SIZE_HEADING, std::mem::take(&mut self.focus_filter))
                })
                .inner;
            let at = Rect::from_min_max(egui::pos2(q.rect.right() + tk::GUTTER, q.rect.top()), egui::pos2(right, q.rect.bottom()));
            let mut c = ui.new_child(UiBuilder::new().max_rect(at).layout(Layout::right_to_left(Align::Center)));
            c.spacing_mut().item_spacing.x = tk::GAP;
            if kit::switch(&mut c, &t, self.snapshots, true, false, label).clicked() {
                self.snapshots = !self.snapshots;
            }
            kit::caption(&mut c, &t, label);
        });
        ui.add_space(tk::GAP);
        kit::rule(ui, &t, false);
        match &app.manifest {
            None => padded(ui, |ui| {
                kit::loading(ui, &t, "reading versions");
            }),
            Some(Err(e)) => {
                let e = e.clone();
                padded(ui, |ui| {
                    ui.add_space(tk::GUTTER);
                    kit::bracketed(ui, &t, "can't read the version list", |ui| kit::text::body_colored(ui, &e, t.error()));
                    ui.add_space(tk::GAP);
                    if kit::Button::new("try again").show(ui, &t).clicked() {
                        app.load_manifest(ui.ctx());
                    }
                });
            }
            Some(Ok(_)) if shown.is_empty() => padded(ui, |ui| {
                kit::loading(ui, &t, "no version matches the filter");
            }),
            Some(Ok(_)) => {
                let pos = shown.iter().position(|r| self.version.as_deref() == Some(r.title.as_str()));
                if let Some(p) = nav.filter(|n| n.is_move()).and_then(|n| n.step(shown.len(), pos)) {
                    self.version = Some(shown[p].title.clone());
                    self.scroll_once = true;
                }
                let pos = shown.iter().position(|r| self.version.as_deref() == Some(r.title.as_str()));
                let was = self.was.as_ref().map(|(v, _)| v.clone());
                if let Some(v) = list(ui, &t, "versions", &shown, pos, focused, std::mem::take(&mut self.scroll_once), was.as_deref()) {
                    self.version = Some(v);
                }
            }
        }
    }

    /// The loader as a pane: head, the kind as joined buttons (a menu when they don't fit), its versions.
    pub fn loader_ui(&mut self, app: &App, ui: &mut Ui, nav: Option<Nav>, focused: bool, number: usize) {
        let t = app.look.tokens.clone();
        ui.spacing_mut().item_spacing.y = 0.0;
        let aside = self.kind.label().to_string();
        pane_head(ui, &t, number, "mod loader", &aside, &[], 0.0);
        ui.add_space(tk::GAP);
        padded(ui, |ui| kind_choice(ui, &t, &mut self.kind));
        ui.add_space(tk::GAP);
        kit::rule(ui, &t, false);
        if self.kind == LoaderKind::Vanilla {
            padded(ui, |ui| {
                ui.add_space(tk::GUTTER);
                kit::body_muted(ui, &t, "Plain Minecraft, no mods.");
            });
            return;
        }
        match &self.loader_versions {
            None => padded(ui, |ui| {
                kit::loading(ui, &t, &format!("reading {} versions", self.kind.label()));
            }),
            Some(Err(e)) => padded(ui, |ui| {
                ui.add_space(tk::GUTTER);
                kit::bracketed(ui, &t, "can't read the loader's versions", |ui| kit::text::body_colored(ui, e, t.error()));
            }),
            Some(Ok(list)) if list.is_empty() => padded(ui, |ui| {
                ui.add_space(tk::GUTTER);
                let mc = self.version.as_deref().unwrap_or("this version");
                kit::text::body_colored(ui, &format!("{} doesn't support {mc} yet.", self.kind.label()), t.warning());
            }),
            Some(Ok(builds)) => {
                // Stable and recommended builds say so at rest; betas say so while hot.
                let shown: Vec<Row> = builds
                    .iter()
                    .take(LOADER_VERSIONS_SHOWN)
                    .map(|b| {
                        let stable = matches!(b.tag, "stable" | "recommended");
                        Row { title: b.version.clone(), description: b.tag.to_string(), stable }
                    })
                    .collect();
                let pos = shown.iter().position(|r| self.loader_version.as_deref() == Some(r.title.as_str()));
                if let Some(p) = nav.filter(|n| n.is_move()).and_then(|n| n.step(shown.len(), pos)) {
                    self.loader_version = Some(shown[p].title.clone());
                    self.scroll_loader = true;
                }
                let pos = shown.iter().position(|r| self.loader_version.as_deref() == Some(r.title.as_str()));
                let was = self.was.as_ref().filter(|(_, l)| l.kind() == self.kind).and_then(|(_, l)| l.version().map(String::from));
                if let Some(v) = list(ui, &t, "loader-versions", &shown, pos, focused, std::mem::take(&mut self.scroll_loader), was.as_deref()) {
                    self.loader_version = Some(v);
                }
            }
        }
    }
}

/// The loader kind: joined buttons when they fit, else a menu.
fn kind_choice(ui: &mut Ui, t: &Tokens, kind: &mut LoaderKind) {
    let opts: Vec<(LoaderKind, &str)> = LoaderKind::ALL.iter().map(|k| (*k, k.label())).collect();
    let joined: f32 = opts.iter().map(|(_, l)| kit::Button::new(l).width(ui, t)).sum::<f32>() - (opts.len() - 1) as f32 * tk::LINE;
    if joined <= ui.available_width() {
        kit::segmented(ui, t, kind, &opts);
    } else {
        let labels: Vec<&str> = opts.iter().map(|(_, l)| *l).collect();
        let at = opts.iter().position(|(k, _)| k == kind);
        if let Some(i) = kit::menu(ui, t, ui.id().with("loader-kind"), at, kind.label(), &labels, tk::SETTING_CONTROL_WIDTH * 0.6, false) {
            *kind = opts[i].0;
        }
    }
}

/// One row of a picker list.
struct Row {
    title: String,
    description: String,
    /// A build the loader calls stable or recommended: its tag shows at rest, not only while hot.
    stable: bool,
}

/// A compact hover list filling the rest of the pane, virtualised. The chosen row is selected
/// (and hot while the list has the keyboard); `was` gets a `CURRENT` mark. Returns the title clicked.
#[allow(clippy::too_many_arguments)]
fn list(ui: &mut Ui, t: &Tokens, salt: &str, rows: &[Row], chosen: Option<usize>, focused: bool, scroll: bool, was: Option<&str>) -> Option<String> {
    let h = Variant::Compact.height();
    let mut area = egui::ScrollArea::vertical().id_salt(salt).auto_shrink(false);
    if scroll && let Some(i) = chosen {
        let view = ui.available_height();
        area = area.vertical_scroll_offset((i as f32 * h - (view - h) / 2.0).max(0.0));
    }
    let mut picked = None;
    area.show_rows(ui, h, rows.len(), |ui, range| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut list = HoverList::begin(ui, Variant::Compact).indent(tk::GUTTER + tk::HOVER_SHIFT_COMPACT);
        for i in range {
            let r = &rows[i];
            let on = chosen == Some(i);
            let current = was == Some(r.title.as_str());
            // A stable build's tag is its badge, so it isn't said twice while hot.
            let desc = if r.stable { "" } else { r.description.as_str() };
            let badge = match (r.stable, current) {
                (true, true) => Some((format!("{} · current", r.description), t.ok())),
                (true, false) => Some((r.description.clone(), t.ok())),
                (false, true) => Some(("current".to_string(), t.text_muted)),
                (false, false) => None,
            };
            let mut row = HoverRow::new((salt, &r.title), &r.title).description(desc).selected(on).hot(on && focused);
            if let Some((text, color)) = &badge {
                row = row.badge(text, *color);
            }
            if list.row(ui, t, row).clicked() {
                picked = Some(r.title.clone());
            }
        }
        list.end(ui);
    });
    picked
}
