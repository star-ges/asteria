//! Screenshots as a tile grid sharing its hairlines: the instance's own, or every instance's
//! with each one's name in its caption. Choosing one opens it inside the section (it takes over
//! the grid, crossfading as you step). The folders are listed and pictures decoded off the UI
//! thread.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use egui::{Rect, TextureHandle, Ui, vec2};

use super::content::padded;
use super::nav::{Nav, Zone};
use crate::design::kit::{self, tiles};
use crate::design::tokens::{self as tk, Tokens};
use crate::design::glyphs;
use super::{App, Confirm, ConfirmAction, Modal, ToastKind};
use crate::instance::Instance;
use crate::task::Promise;

const MAX_DECODING: usize = 4;
/// Thumbnails fit in this; the picture opened large in this.
const THUMB_SIZE: (u32, u32) = (480, 270);
const VIEW_SIZE: (u32, u32) = (2560, 1440);

enum Thumb {
    Loading,
    Ready(TextureHandle),
    Failed,
}

#[derive(Clone)]
struct Shot {
    path: PathBuf,
    modified: Option<SystemTime>,
    /// Id of the instance it belongs to.
    owner: String,
}

/// What's listed: one instance's id, or every instance's.
#[derive(Clone, PartialEq)]
enum Scope {
    One(String),
    All,
}

#[derive(Default)]
pub struct ShotsState {
    /// The listed screenshots, newest first, kept on screen while a refresh runs.
    list: Option<(Scope, Vec<Shot>)>,
    /// When `list` was read; None once it's out of date.
    fetched: Option<Instant>,
    loading: Option<(Scope, Promise<Vec<Shot>>)>,
    thumbs: Arc<Mutex<HashMap<PathBuf, Thumb>>>,
    /// Pictures decoded for the viewer, dropped when it closes.
    large: Arc<Mutex<HashMap<PathBuf, Thumb>>>,
    selected: Option<PathBuf>,
    /// The picture opened inside the section, if any.
    viewing: Option<PathBuf>,
    /// The one shown before it (crossfading out), and a count of changes (the fade's id).
    previous: Option<PathBuf>,
    changes: u64,
}

impl ShotsState {
    pub fn invalidate(&mut self) {
        self.fetched = None;
        // A read already under way may predate the change.
        self.loading = None;
    }

    /// The screenshots in `dirs` (instance id, folder), re-read in the background
    /// every few seconds. None until the first read for this scope lands.
    fn sync(&mut self, rt: &tokio::runtime::Handle, ctx: &egui::Context, scope: Scope, dirs: Vec<(String, PathBuf)>) -> Option<&[Shot]> {
        if let Some((sc, p)) = &self.loading
            && let Some(r) = p.take()
        {
            if *sc == scope {
                self.list = Some((scope.clone(), r.unwrap_or_default()));
                self.fetched = Some(Instant::now());
            }
            self.loading = None;
        }
        if self.list.as_ref().is_some_and(|(sc, _)| *sc != scope) {
            self.list = None;
            self.fetched = None;
        }
        if self.loading.as_ref().is_some_and(|(sc, _)| *sc != scope) {
            self.loading = None;
        }
        let fresh = self.fetched.is_some_and(|at| at.elapsed() < Duration::from_secs(3));
        if !fresh && self.loading.is_none() {
            let p = Promise::spawn(rt, ctx, async move { Ok(tokio::task::spawn_blocking(move || list(&dirs)).await?) });
            self.loading = Some((scope, p));
        }
        self.list.as_ref().map(|(_, l)| l.as_slice())
    }

    fn thumb(&self, path: &Path, ctx: &egui::Context, rt: &tokio::runtime::Handle) -> Option<TextureHandle> {
        decoded(&self.thumbs, path, THUMB_SIZE, ctx, rt)
    }

    /// Whether a picture is open inside the section.
    pub fn viewing(&self) -> bool {
        self.viewing.is_some()
    }

    /// Back to the grid.
    pub fn close(&mut self) {
        self.viewing = None;
        self.previous = None;
        self.large.lock().unwrap().clear();
    }

    fn view(&mut self, path: PathBuf) {
        if self.viewing.as_ref() != Some(&path) {
            self.previous = self.viewing.replace(path.clone());
            self.changes += 1;
            // Keep only the two pictures on screen decoded.
            let keep = [self.previous.clone(), Some(path.clone())];
            self.large.lock().unwrap().retain(|k, _| keep.iter().flatten().any(|p| p == k));
        }
        self.selected = Some(path);
    }
}

/// `path` decoded to fit in `max`, from `cache`, or None while it's decoded in the background.
fn decoded(cache: &Arc<Mutex<HashMap<PathBuf, Thumb>>>, path: &Path, max: (u32, u32), ctx: &egui::Context, rt: &tokio::runtime::Handle) -> Option<TextureHandle> {
    let mut map = cache.lock().unwrap();
    match map.get(path) {
        Some(Thumb::Ready(tex)) => return Some(tex.clone()),
        Some(_) => return None,
        None => {}
    }
    if map.values().filter(|t| matches!(t, Thumb::Loading)).count() >= MAX_DECODING {
        ctx.request_repaint_after(Duration::from_millis(50));
        return None;
    }
    map.insert(path.to_path_buf(), Thumb::Loading);
    let (path, slot, ctx) = (path.to_path_buf(), cache.clone(), ctx.clone());
    rt.spawn_blocking(move || {
        let result = image::open(&path).map(|img| {
            let small = if img.width() > max.0 || img.height() > max.1 { img.thumbnail(max.0, max.1) } else { img }.to_rgba8();
            let size = [small.width() as usize, small.height() as usize];
            let color = egui::ColorImage::from_rgba_unmultiplied(size, small.as_raw());
            ctx.load_texture(format!("{}@{}", path.display(), max.0), color, egui::TextureOptions::LINEAR)
        });
        slot.lock().unwrap().insert(path, result.map(Thumb::Ready).unwrap_or(Thumb::Failed));
        ctx.request_repaint();
    });
    None
}

fn list(dirs: &[(String, PathBuf)]) -> Vec<Shot> {
    let mut out: Vec<Shot> = dirs
        .iter()
        .flat_map(|(id, dir)| {
            std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().to_lowercase();
                    n.ends_with(".png") || n.ends_with(".jpg") || n.ends_with(".jpeg")
                })
                .map(|e| Shot { path: e.path(), modified: e.metadata().and_then(|m| m.modified()).ok(), owner: id.clone() })
        })
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

/// Put an image file on the Wayland clipboard.
fn copy_image(path: &Path) -> std::io::Result<()> {
    let file = std::fs::File::open(path)?;
    std::process::Command::new("wl-copy").args(["--type", "image/png"]).stdin(file).spawn().map(|_| ())
}

/// When a screenshot was taken, for its caption.
fn taken(shot: &Shot) -> String {
    shot.modified.map(|m| chrono::DateTime::<chrono::Local>::from(m).format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default()
}

pub fn show(app: &mut App, ui: &mut Ui, inst: &Instance) {
    let t = app.look.tokens.clone();
    let ctx = ui.ctx().clone();
    let rt = app.rt.clone();
    let all = app.cfg.all_screenshots;
    let (scope, dirs) = if all {
        (Scope::All, app.instances.iter().map(|i| (i.id.clone(), i.screenshots_dir())).collect())
    } else {
        (Scope::One(inst.id.clone()), vec![(inst.id.clone(), inst.screenshots_dir())])
    };
    let listed = app.shots.sync(&rt, &ctx, scope, dirs).map(<[Shot]>::to_vec);
    let kb = app.nav_for(Zone::Main);
    let focused = app.focus == Zone::Main;
    let key = |n: Nav| kb == Some(n);
    let read = listed.is_some();
    let shots = listed.unwrap_or_default();
    let names: HashMap<String, String> = app.instances.iter().map(|i| (i.id.clone(), i.name.clone())).collect();
    let s = &mut app.shots;
    // A picture that's gone (deleted, or the view changed) closes the viewer.
    if s.viewing.as_ref().is_some_and(|p| !shots.iter().any(|q| q.path == *p)) && read {
        s.close();
    }
    let mut scroll = false;
    if let Some(n) = kb.filter(|n| n.is_move()) {
        let pos = s.selected.as_ref().and_then(|p| shots.iter().position(|q| q.path == *p));
        if let Some(i) = n.step(shots.len(), pos) {
            if s.viewing.is_some() {
                s.view(shots[i].path.clone());
            } else {
                s.selected = Some(shots[i].path.clone());
            }
            scroll = true;
        }
    }
    let sel = s.selected.clone().filter(|p| shots.iter().any(|q| q.path == *p));
    let pos = sel.as_ref().and_then(|p| shots.iter().position(|q| q.path == *p));
    let viewing = s.viewing.is_some();
    let owner = |shot: &Shot| names.get(&shot.owner).cloned().unwrap_or_else(|| shot.owner.clone());

    // The head: how many, the selection's actions, and (on the grid) every instance or this one.
    ui.spacing_mut().item_spacing.y = 0.0;
    let n = shots.len();
    let mut caption = format!("{} / screenshots · {n}", kit::text::number(1));
    if all {
        caption = format!("{caption} · every instance");
    }
    let mut buttons = Vec::new();
    if viewing {
        let shot = pos.map(|i| &shots[i]);
        if let (Some(i), Some(shot)) = (pos, shot) {
            caption = format!("{} / screenshots · {} of {n} · {}", kit::text::number(1), i + 1, taken(shot));
            if all {
                caption = format!("{caption} · {}", owner(shot));
            }
        }
        buttons.extend([
            kit::Button::new("all").icon(glyphs::IMAGES),
            kit::Button::new("").named("previous screenshot").icon(glyphs::CARET_LEFT).enabled(pos.is_some_and(|i| i > 0)),
            kit::Button::new("").named("next screenshot").icon(glyphs::CARET_RIGHT).enabled(pos.is_some_and(|i| i + 1 < n)),
        ]);
    } else if sel.is_some() {
        buttons.push(kit::Button::new("view"));
    }
    let first = buttons.len();
    if sel.is_some() {
        buttons.extend([kit::Button::new("open"), kit::Button::new("copy"), kit::Button::new("delete")]);
    }
    let folder_i = buttons.len();
    buttons.push(kit::Button::new("folder"));
    let (clicked, toggled) = head(ui, &t, &caption, (!viewing).then_some(all), &buttons);
    let pick = |i: usize| clicked == Some(i);
    let s = &mut app.shots;
    if viewing {
        if pick(0) {
            s.close();
        } else if let Some(i) = pos.filter(|_| pick(1)).and_then(|i| i.checked_sub(1)) {
            s.view(shots[i].path.clone());
        } else if let Some(i) = pos.filter(|&i| pick(2) && i + 1 < n) {
            s.view(shots[i + 1].path.clone());
        }
    } else if let Some(p) = &sel
        && (pick(0) || key(Nav::Activate))
    {
        s.view(p.clone());
    }
    if let Some(p) = &sel {
        if pick(first) || (viewing && key(Nav::Activate)) {
            super::open_url(&p.display().to_string());
        }
        if pick(first + 1) || key(Nav::Copy) {
            match copy_image(p) {
                Ok(()) => app.toasts.push(super::Toast::info("Screenshot copied")),
                Err(e) => app.toasts.push(super::Toast::error(format!("Couldn't copy: {e}"))),
            }
        }
        if pick(first + 2) || key(Nav::Delete) {
            app.modal = Some(Modal::Confirm(Confirm {
                title: "delete screenshot".into(),
                body: format!("Delete {}?", p.file_name().unwrap_or_default().to_string_lossy()),
                button: "delete".into(),
                action: ConfirmAction::DeletePath(p.clone()),
            }));
        }
    }
    if pick(folder_i) || key(Nav::Folder) {
        // In the merged view, the selected screenshot's own instance folder.
        let dir = sel.as_ref().filter(|_| all).and_then(|p| p.parent()).map(Path::to_path_buf).unwrap_or_else(|| inst.screenshots_dir());
        super::open_path(&dir);
    }
    if (toggled || key(Nav::Toggle)) && !viewing {
        app.cfg.all_screenshots = !all;
        if let Err(e) = app.cfg.save() {
            app.toast(ToastKind::Error, format!("Couldn't save settings: {e}"));
        }
    }

    if !read {
        padded(ui, |ui| kit::loading(ui, &t, "reading screenshots"));
        return;
    }
    if shots.is_empty() {
        let title = if all { "No screenshots in any instance" } else { "No screenshots yet" };
        padded(ui, |ui| kit::empty_state(ui, &t, title, "", None));
        return;
    }
    let s = &mut app.shots;
    if let Some(path) = s.viewing.clone() {
        view(ui, &t, s, &path, &ctx, &rt);
        return;
    }
    let mut chosen = None;
    egui::ScrollArea::vertical().id_salt("shots").auto_shrink(false).show(ui, |ui| {
        padded(ui, |ui| {
            ui.add_space(tk::GUTTER);
            let columns = (ui.available_width() / tk::SHOT_TILE_MIN).floor().max(1.0) as usize;
            let tiles = kit::Tiles { count: shots.len(), columns, aspect: 16.0 / 9.0, selected: pos, hot: pos.filter(|_| focused) };
            let out = tiles.show(ui, &t, |ui, i, rect| {
                let shot = &shots[i];
                match s.thumb(&shot.path, &ctx, &rt) {
                    Some(tex) => cover(ui, &tex, rect),
                    None if matches!(s.thumbs.lock().unwrap().get(&shot.path), Some(Thumb::Failed)) => {
                        kit::text::paint_glyph(ui.painter(), rect.center(), glyphs::IMAGE, crate::design::fonts::icons(tk::ICON_SIZE * 2.0), t.text_muted);
                    }
                    None => {}
                }
                if all { format!("{} · {}", taken(shot), owner(shot)) } else { taken(shot) }
            });
            if scroll && let Some(r) = pos.and_then(|i| out.rects.get(i)) {
                ui.scroll_to_rect(*r, None);
            }
            chosen = out.clicked;
            ui.add_space(tk::GUTTER);
        });
    });
    // Choosing a screenshot opens it.
    if let Some(i) = chosen {
        app.shots.view(shots[i].path.clone());
        app.focus = Zone::Main;
    }
}

/// A texture filling `rect` edge to edge, cropped to its shape (no letterboxing in a tile).
fn cover(ui: &Ui, tex: &TextureHandle, rect: Rect) {
    let [tw, th] = tex.size().map(|v| v as f32);
    let (ta, ra) = (tw / th, rect.width() / rect.height());
    let uv = if ta > ra {
        let w = ra / ta;
        Rect::from_min_max(egui::pos2((1.0 - w) / 2.0, 0.0), egui::pos2((1.0 + w) / 2.0, 1.0))
    } else {
        let h = ta / ra;
        Rect::from_min_max(egui::pos2(0.0, (1.0 - h) / 2.0), egui::pos2(1.0, (1.0 + h) / 2.0))
    };
    ui.painter().image(tex.id(), rect, uv, egui::Color32::WHITE);
}

/// The picture opened large in what was the grid's space, fitted whole and framed by a hairline;
/// stepping to another crossfades (700ms, no blur or zoom). The thumbnail stands in until the
/// large one is decoded.
fn view(ui: &mut Ui, t: &Tokens, s: &ShotsState, path: &Path, ctx: &egui::Context, rt: &tokio::runtime::Handle) {
    let area = ui.available_rect_before_wrap().shrink2(vec2(tk::GUTTER, 0.0));
    let area = Rect::from_min_max(egui::pos2(area.left(), area.top() + tk::GUTTER), egui::pos2(area.right(), area.bottom() - tk::GAP));
    ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
    let tex = |p: &Path| decoded(&s.large, p, VIEW_SIZE, ctx, rt).or_else(|| s.thumb(p, ctx, rt));
    let Some(cur) = tex(path) else {
        if matches!(s.thumbs.lock().unwrap().get(path), Some(Thumb::Failed)) {
            padded(ui, |ui| kit::body_muted(ui, t, "This picture can't be read."));
        }
        return;
    };
    let fit = |tex: &TextureHandle| {
        let [w, h] = tex.size().map(|v| v as f32);
        let scale = (area.width() / w).min(area.height() / h);
        Rect::from_center_size(egui::pos2(area.center().x, area.top() + h * scale / 2.0), vec2(w * scale, h * scale))
    };
    let r = fit(&cur);
    let src = |tex: &TextureHandle| egui::ImageSource::Texture(egui::load::SizedTexture::new(tex.id(), tex.size_vec2()));
    let previous = s.previous.as_deref().and_then(|p| tex(p)).map(|p| src(&p));
    tiles::crossfade(ui, r, egui::Id::new(("shot-view", s.changes)), previous, src(&cur));
    kit::rule::frame(ui.painter(), r.expand(tk::LINE), t.line_strong);
}

/// The tab's head: a caption on the left; on the right the joined buttons and, on the grid, the
/// "all instances" switch before them. Returns the button clicked and whether the switch was.
fn head(ui: &mut Ui, t: &Tokens, caption: &str, all: Option<bool>, buttons: &[kit::Button]) -> (Option<usize>, bool) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, tk::BUTTON_HEIGHT + tk::GAP * 2.0), egui::Sense::hover());
    let inner = rect.shrink2(vec2(tk::GUTTER, 0.0));
    let mut row = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::right_to_left(egui::Align::Center)));
    row.spacing_mut().item_spacing.x = tk::UNIT;
    let clicked = kit::button_row(&mut row, t, buttons);
    let mut toggled = false;
    if let Some(on) = all {
        row.add_space(tk::GUTTER - tk::UNIT);
        toggled = kit::switch(&mut row, t, on, true, false, "all instances").clicked();
        toggled |= kit::caption(&mut row, t, "all instances").interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
    }
    let used = inner.right() - row.min_rect().left();
    let mut job = kit::text::caption_job(caption, t.text_muted);
    job.wrap.max_width = (inner.width() - used - tk::GUTTER).max(0.0);
    job.wrap.max_rows = 1;
    let g = ui.painter().layout_job(job);
    ui.painter().galley(egui::pos2(inner.left(), rect.center().y - g.size().y / 2.0), g, t.text_muted);
    kit::rule::hline(ui.painter(), rect.x_range(), rect.bottom() - 0.5, t.line_strong);
    (clicked, toggled)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(s: &mut ShotsState, rt: &tokio::runtime::Runtime, scope: Scope, dirs: &[(String, PathBuf)], want: usize) -> Vec<Shot> {
        let ctx = egui::Context::default();
        let start = Instant::now();
        loop {
            if let Some(l) = s.sync(rt.handle(), &ctx, scope.clone(), dirs.to_vec())
                && l.len() == want
            {
                return l.to_vec();
            }
            assert!(start.elapsed() < Duration::from_secs(5), "screenshots never loaded");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn merged_view_lists_every_instance_with_its_owner() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let ctx = egui::Context::default();
        let root = crate::paths::test_root().join(format!("shots-{}", uuid::Uuid::new_v4()));
        let dirs: Vec<(String, PathBuf)> = ["a", "b"].iter().map(|id| (id.to_string(), root.join(id))).collect();
        for (id, dir) in &dirs {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join(format!("{id}.png")), "png").unwrap();
            // The same name in two instances is two screenshots.
            std::fs::write(dir.join("same.png"), "png").unwrap();
            std::fs::write(dir.join("notes.txt"), "not a picture").unwrap();
        }
        let mut s = ShotsState::default();

        // The first frame doesn't list folders itself; the list arrives later.
        assert!(s.sync(rt.handle(), &ctx, Scope::One("a".into()), dirs[..1].to_vec()).is_none());
        let one = wait(&mut s, &rt, Scope::One("a".into()), &dirs[..1], 2);
        assert!(one.iter().all(|x| x.owner == "a"));

        // Switching to every instance never shows the one-instance list as the merged one.
        assert!(s.sync(rt.handle(), &ctx, Scope::All, dirs.clone()).is_none());
        let all = wait(&mut s, &rt, Scope::All, &dirs, 4);
        let mut got: Vec<(String, String)> = all.iter().map(|x| (x.owner.clone(), x.path.file_name().unwrap().to_string_lossy().into_owned())).collect();
        got.sort();
        assert_eq!(got, [("a", "a.png"), ("a", "same.png"), ("b", "b.png"), ("b", "same.png")].map(|(o, n)| (o.to_string(), n.to_string())));
        std::fs::remove_dir_all(root).unwrap();
    }
}
