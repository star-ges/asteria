//! Choosing an instance icon: the built-in icons and your own as tile grids sharing their
//! hairlines (Prism's pixel art is content), the current one selected, hjkl and Enter.

use std::path::PathBuf;

use egui::{Rect, Ui, vec2};

use super::content::{padded, sheet_foot};
use super::nav::Nav;
use super::{App, ToastKind};
use crate::design::glyphs;
use crate::design::kit::{self, Button, Tiles};
use crate::design::tokens::{self as tk, Tokens};
use crate::icons;
use crate::keymap::Action;
use crate::task::Promise;

pub struct IconPicker {
    pub id: String,
    add_p: Option<Promise<Option<PathBuf>>>,
    /// Keyboard cursor in the grid, as a key.
    cursor: Option<String>,
    scroll: bool,
}

impl IconPicker {
    pub fn new(id: String) -> Self {
        Self { id, add_p: None, cursor: None, scroll: false }
    }
}

/// Columns for a grid `width` wide.
fn columns(width: f32) -> usize {
    ((width / tk::ICON_TILE).floor() as usize).max(1)
}

/// Returns true when the picker should close.
pub fn show(app: &mut App, s: &mut IconPicker, ui: &mut Ui) -> bool {
    let t = app.look.tokens.clone();
    let Some(current) = app.instances.iter().find(|i| i.id == s.id).map(|i| i.icon.clone()) else { return true };
    let mut choose = None;
    let mut remove = None;

    if let Some(p) = &s.add_p
        && let Some(r) = p.take()
    {
        s.add_p = None;
        match r.and_then(|path| path.map(|p| icons::add_custom(&p)).transpose()) {
            Ok(Some(key)) => choose = Some(key),
            Ok(None) => {}
            Err(e) => app.toast(ToastKind::Error, format!("Couldn't add icon: {e:#}")),
        }
    }

    ui.spacing_mut().item_spacing.y = 0.0;
    kit::heading(ui, &t, "Instance icon");
    ui.add_space(tk::GUTTER);

    let builtin: Vec<String> = icons::BUILTIN.iter().map(|(n, _)| n.to_string()).collect();
    let custom = icons::custom();
    let all: Vec<String> = builtin.iter().chain(custom.iter()).cloned().collect();
    // Keyboard: hjkl over the grids as one, Enter to choose.
    let cols = columns(ui.available_width() - tk::GUTTER * 2.0) as isize;
    let cursor = s.cursor.clone().unwrap_or_else(|| current.clone());
    let at = all.iter().position(|k| *k == cursor).unwrap_or(0) as isize;
    let nav = app.modal_nav();
    let moved = match nav {
        Some(Nav::Left) => Some(at - 1),
        Some(Nav::Right) => Some(at + 1),
        Some(Nav::Down) => Some(at + cols),
        Some(Nav::Up) => Some(at - cols),
        Some(Nav::Top) => Some(0),
        Some(Nav::Bottom) => Some(all.len() as isize - 1),
        _ => None,
    };
    if let Some(i) = moved {
        s.cursor = Some(all[i.clamp(0, all.len() as isize - 1) as usize].clone());
        s.scroll = true;
    }
    let cursor = s.cursor.clone();
    if let Some(c) = &cursor
        && (nav == Some(Nav::Activate) || ui.input(|i| i.key_pressed(egui::Key::Enter)))
    {
        choose = Some(c.clone());
    }

    let foot = tk::LINE + tk::GAP + tk::BUTTON_HEIGHT + tk::GUTTER * 2.0;
    let scroll = std::mem::take(&mut s.scroll);
    egui::ScrollArea::vertical().id_salt("icons").max_height((ui.available_height() - foot).max(tk::TABLE_ROW_HEIGHT)).auto_shrink([false, true]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        kit::SectionLabel::new(1, "built in").aside(&builtin.len().to_string()).show(ui, &t);
        ui.add_space(tk::GAP);
        if let Some(k) = grid(ui, &t, "builtin", &builtin, &current, cursor.as_deref(), scroll) {
            choose = Some(k);
        }
        ui.add_space(tk::GUTTER);
        kit::SectionLabel::new(2, "user added").aside(&custom.len().to_string()).show(ui, &t);
        if custom.is_empty() {
            padded(ui, |ui| kit::loading(ui, &t, "add an image: png, jpg, svg or webp"));
        } else {
            ui.add_space(tk::GAP);
            if let Some(k) = grid(ui, &t, "custom", &custom, &current, cursor.as_deref(), scroll) {
                choose = Some(k);
            }
        }
        ui.add_space(tk::GUTTER);
    });

    ui.add_space(tk::GUTTER);
    // Remove acts on your own icon under the keyboard's cursor, or the current one.
    let removable = cursor.clone().or(Some(current.clone())).filter(|k| custom.contains(k));
    let busy = s.add_p.is_some();
    let k = |a| app.keymap.label(a);
    let hints = [("hjkl".to_string(), "move"), (k(Action::Activate), "choose")];
    let buttons = [Button::new("add image").icon(glyphs::PLUS).enabled(!busy), Button::new("remove").icon(glyphs::TRASH).enabled(removable.is_some()), Button::new("close")];
    let mut close = false;
    match sheet_foot(ui, &t, &hints, &buttons) {
        Some(0) => {
            s.add_p = Some(Promise::spawn(&app.rt, ui.ctx(), async {
                let f = rfd::AsyncFileDialog::new().set_title("Choose an icon").add_filter("Images", &["png", "jpg", "jpeg", "svg", "webp"]).pick_file().await;
                Ok(f.map(|f| f.path().to_path_buf()))
            }));
        }
        Some(1) => remove = removable,
        Some(2) => close = true,
        _ => {}
    }

    if let Some(key) = remove {
        icons::remove_custom(&key);
        for i in app.instances.iter_mut().filter(|i| i.icon == key) {
            i.icon = icons::DEFAULT.into();
            let _ = i.save();
        }
        s.cursor = None;
    }
    if let Some(key) = choose
        && let Some(i) = app.instances.iter_mut().find(|i| i.id == s.id)
    {
        i.icon = key;
        if let Err(e) = i.save() {
            app.toast(ToastKind::Error, format!("Save failed: {e}"));
        }
        close = true;
    }
    close
}

/// One grid of icons on the gutter; returns the key clicked.
fn grid(ui: &mut Ui, t: &Tokens, salt: &str, keys: &[String], current: &str, cursor: Option<&str>, scroll: bool) -> Option<String> {
    padded(ui, |ui| {
        let tiles = Tiles {
            count: keys.len(),
            columns: columns(ui.available_width()),
            aspect: 1.0,
            selected: keys.iter().position(|k| k == current),
            hot: cursor.and_then(|c| keys.iter().position(|k| k == c)),
        };
        let hot = tiles.hot;
        let out = ui
            .push_id(salt, |ui| {
                tiles.show(ui, t, |ui, i, pic| {
                    // Pixel art at a size that keeps it crisp, centred in its cell.
                    let side = (pic.width().min(pic.height()) * 0.56).round();
                    egui::Image::new(icons::source(&keys[i])).paint_at(ui, Rect::from_center_size(pic.center(), vec2(side, side)));
                    keys[i].trim_start_matches("custom:").to_string()
                })
            })
            .inner;
        if scroll && let Some(r) = hot.and_then(|h| out.rects.get(h)) {
            ui.scroll_to_rect(*r, None);
        }
        out.clicked.map(|i| keys[i].clone())
    })
}
