//! Export an instance as an instance zip or a Modrinth pack, choosing which parts of the
//! game folder go in: the format and its explanation as a settings group, then the folder's
//! entries as a checklist with their sizes.

use std::path::PathBuf;

use egui::Ui;

use super::content::{padded, sheet_foot};
use super::nav::Nav;
use super::{App, ToastKind};
use crate::design::kit::{self, Button, QuietRow, SettingRow};
use crate::design::tokens as tk;
use crate::export::{Entry, Format, Request};
use crate::instance::Instance;
use crate::keymap::Action;
use crate::task::Promise;

pub struct ExportDialog {
    inst: Instance,
    format: Format,
    version: String,
    entries: Option<Vec<(Entry, bool)>>,
    entries_p: Option<Promise<Vec<Entry>>>,
    cursor: usize,
    save_p: Option<Promise<Option<PathBuf>>>,
}

impl ExportDialog {
    pub fn new(app: &App, ctx: &egui::Context, inst: Instance) -> Self {
        let i = inst.clone();
        let entries_p = Promise::spawn(&app.rt, ctx, async move { Ok(tokio::task::spawn_blocking(move || crate::export::entries(&i)).await?) });
        Self { inst, format: Format::Mrpack, version: "1.0.0".into(), entries: None, entries_p: Some(entries_p), cursor: 0, save_p: None }
    }
}

/// Returns the export to start, or `Err(())` to close without one.
pub fn show(app: &mut App, s: &mut ExportDialog, ui: &mut Ui) -> Option<Result<Request, ()>> {
    let t = app.look.tokens.clone();
    if let Some(r) = s.entries_p.as_ref().and_then(|p| p.take()) {
        s.entries_p = None;
        match r {
            Ok(list) => {
                s.entries = Some(list.into_iter().map(|e| {
                    let on = e.default;
                    (e, on)
                }).collect())
            }
            Err(e) => app.toast(ToastKind::Error, format!("Couldn't read the instance: {e:#}")),
        }
    }
    let mut out = None;
    if let Some(r) = s.save_p.as_ref().and_then(|p| p.take()) {
        s.save_p = None;
        match r {
            Ok(Some(mut dest)) => {
                if Format::from_path(&dest) != Some(s.format) {
                    dest.as_mut_os_string().push(format!(".{}", s.format.ext()));
                }
                let include = s.entries.iter().flatten().filter(|(_, on)| *on).map(|(e, _)| e.name.clone()).collect();
                out = Some(Ok(Request { inst: s.inst.clone(), format: s.format, dest, include, version: s.version.trim().to_string() }));
            }
            Ok(None) => {}
            Err(e) => app.toast(ToastKind::Error, format!("File picker failed: {e:#}")),
        }
    }

    ui.set_max_width(ui.available_width().min(tk::SHEET_WIDTH));
    ui.spacing_mut().item_spacing.y = 0.0;
    kit::heading(ui, &t, "Export instance");
    ui.add_space(tk::GUTTER);
    let nav = app.modal_nav();
    let key = |n: Nav| nav == Some(n);
    let busy = s.save_p.is_some();

    let explain = match s.format {
        Format::Mrpack => "Mods and packs that Modrinth hosts are listed as downloads; everything else is bundled in the pack. Opens in Asteria, Prism and the Modrinth App.",
        Format::Zip => "A full copy of the chosen files, with the instance's settings and icon. Opens in Asteria, Prism, PolyMC and MultiMC, and works offline.",
    };
    kit::setting_group(ui, &t, 1, "Format", "", |ui| {
        SettingRow::new("Format").description(explain).show(ui, &t, |ui| {
            kit::segmented(ui, &t, &mut s.format, &[(Format::Zip, "instance zip"), (Format::Mrpack, "modrinth pack")]);
        });
        if s.format == Format::Mrpack {
            SettingRow::new("Pack version").caption("shown by launchers that import it").show(ui, &t, |ui| {
                let w = ui.available_width().min(tk::SETTING_CONTROL_WIDTH);
                kit::Field::new(&mut s.version).hint("1.0.0").width(w).show(ui, &t);
            });
        }
    });

    // What goes in: the game folder's entries, ticked by default as the format suggests.
    let aside = match &s.entries {
        Some(list) => {
            let (n, size) = list.iter().filter(|(_, on)| *on).fold((0, 0), |(n, b), (e, _)| (n + 1, b + e.size));
            format!("{n} of {} · {} before compression", list.len(), kit::text::size_label(size))
        }
        None => "measuring".into(),
    };
    kit::SectionLabel::new(2, "include").aside(&aside).show(ui, &t);
    let foot = tk::LINE + tk::GAP + tk::BUTTON_HEIGHT + tk::GUTTER * 2.0;
    match &mut s.entries {
        None => padded(ui, |ui| {
            kit::loading(ui, &t, "measuring the game folder");
        }),
        Some(list) if list.is_empty() => padded(ui, |ui| {
            kit::loading(ui, &t, "the game folder is empty");
        }),
        Some(list) => {
            let moved = nav.filter(|n| n.is_move());
            if let Some(n) = moved {
                s.cursor = n.step(list.len(), Some(s.cursor)).unwrap_or(0);
            }
            s.cursor = s.cursor.min(list.len() - 1);
            if key(Nav::Toggle) {
                list[s.cursor].1 ^= true;
            }
            egui::ScrollArea::vertical().id_salt("export-entries").max_height((ui.available_height() - foot).max(tk::TABLE_ROW_HEIGHT * 3.0)).auto_shrink([false, true]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, (e, on)) in list.iter_mut().enumerate() {
                    let name = if e.is_dir { format!("{}/", e.name) } else { e.name.clone() };
                    let size = kit::text::size_label(e.size);
                    let cur = i == s.cursor;
                    let (r, _) = QuietRow::new(("export-entry", &e.name), &name).check(*on).aside(&size, None).hot(cur).selected(cur).show(ui, &t, |_| ());
                    if cur && moved.is_some() {
                        r.scroll_to_me(None);
                    }
                    if r.clicked() {
                        *on ^= true;
                        s.cursor = i;
                    }
                }
            });
        }
    }

    ui.add_space(tk::GUTTER);
    let ready = s.entries.is_some() && !busy;
    let k = |a| app.keymap.label(a);
    let hints = [(k(Action::Toggle), "include"), (k(Action::Activate), "export")];
    let clicked = sheet_foot(ui, &t, &hints, &[Button::new("export").icon(crate::design::glyphs::UPLOAD).filled().enabled(ready), Button::new("cancel")]);
    if clicked == Some(1) {
        out = Some(Err(()));
    }
    let go = clicked == Some(0) || (ready && (key(Nav::Activate) || ui.input(|i| i.key_pressed(egui::Key::Enter))));
    if go {
        s.save_p = Some(save_as(app, ui.ctx(), &s.inst.name, s.format));
    }
    out
}

/// Ask where to save, via the XDG portal.
fn save_as(app: &App, ctx: &egui::Context, name: &str, format: Format) -> Promise<Option<PathBuf>> {
    let file = format!("{}.{}", name.replace('/', "-"), format.ext());
    Promise::spawn(&app.rt, ctx, async move {
        let (label, ext) = match format {
            Format::Mrpack => ("Modrinth pack", "mrpack"),
            Format::Zip => ("Instance zip", "zip"),
        };
        let picked = rfd::AsyncFileDialog::new()
            .set_directory(dirs::download_dir().or_else(dirs::home_dir).unwrap_or_default())
            .set_title("Export instance")
            .set_file_name(file)
            .add_filter(label, &[ext])
            .save_file()
            .await;
        Ok(picked.map(|f| f.path().to_path_buf()))
    })
}
