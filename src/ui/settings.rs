//! Settings: StarShell's settings page, numbered groups of rows (label left, control on the
//! shared column). Light and dark follow the desktop, so there's no appearance group; only a
//! note when Switzer or Melodrama aren't installed.

use egui::Ui;

use super::{App, Confirm, ConfirmAction, Modal, ToastKind};
use crate::design::glyphs;
use crate::design::kit::{self, Button, SettingRow};
use crate::design::tokens::{self as tk, Tokens};
use crate::keymap::{self, Action};
use crate::shares::Kind;
use crate::{paths, shares};

pub fn show(app: &mut App, ui: &mut Ui) {
    let t = app.look.tokens.clone();
    egui::ScrollArea::vertical().id_salt("settings").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        fonts_note(app, ui, &t);
        let mut changed = false;
        let control = |ui: &Ui| ui.available_width().min(tk::SETTING_CONTROL_WIDTH);

        kit::setting_group(ui, &t, 1, "Java", "", |ui| {
            let mb = app.cfg.default_memory_mb;
            SettingRow::new("Memory").caption("for instances without their own").show(ui, &t, |ui| {
                let mut v = mb as f64;
                kit::Slider::new(&mut v, 1024.0, 32768.0).step(512.0).value_text(format!("{mb} MiB")).width(control(ui)).show(ui, &t);
                if v as u32 != mb {
                    app.cfg.default_memory_mb = v as u32;
                    changed = true;
                }
            });
            SettingRow::new("Java").caption("a java binary for every instance").show(ui, &t, |ui| {
                let w = control(ui);
                changed |= kit::Field::new(&mut app.cfg.java_override).hint("Mojang's runtime for each version").width(w).show(ui, &t).changed();
            });
            SettingRow::new("JVM arguments").caption("for every instance").show(ui, &t, |ui| {
                let w = control(ui);
                changed |= kit::Field::new(&mut app.cfg.global_jvm_args).hint("None").width(w).show(ui, &t).changed();
            });
            SettingRow::new("Wrapper").caption("like prime-run or gamemoderun").show(ui, &t, |ui| {
                let w = control(ui);
                changed |= kit::Field::new(&mut app.cfg.wrapper_command).hint("None").width(w).show(ui, &t).changed();
            });
        });

        kit::setting_group(ui, &t, 2, "Versions", "", |ui| {
            SettingRow::new("Show snapshots").caption("in the version picker, at first").show(ui, &t, |ui| {
                if kit::switch(ui, &t, app.cfg.show_snapshots, true, false, "show snapshots").clicked() {
                    app.cfg.show_snapshots = !app.cfg.show_snapshots;
                    changed = true;
                }
            });
        });

        kit::setting_group(ui, &t, 3, "Backups", "", |ui| {
            let keep = app.cfg.backups_keep;
            let shown = if keep == 0 { "off".to_string() } else { format!("{keep} per world") };
            SettingRow::new("Keep")
                .description("A world is backed up whenever a game that played it exits, keeping this many (0 turns that off). Mods and config are backed up before every update, keeping as many. Backups taken by hand or before a world is played with older mods are never removed.")
                .show(ui, &t, |ui| {
                    let mut v = keep as f64;
                    kit::Slider::new(&mut v, 0.0, 30.0).step(1.0).value_text(shown).mark(5.0).width(control(ui)).show(ui, &t);
                    if v as u32 != keep {
                        app.cfg.backups_keep = v as u32;
                        changed = true;
                    }
                });
            SettingRow::new("Backups folder").caption("on btrfs or xfs, backups share their bytes with the world").show(ui, &t, |ui| {
                if Button::new("open").icon(glyphs::FOLDER).show(ui, &t).clicked() {
                    let _ = std::fs::create_dir_all(crate::backups::root());
                    super::open_path(&crate::backups::root());
                }
            });
        });

        shared(app, ui, &t);
        storage(app, ui, &t);

        kit::setting_group(ui, &t, 6, "Microsoft sign-in", "", |ui| {
            SettingRow::new("Client ID")
                .description("Leave this empty if Microsoft sign-in already works. It's only for using your own Azure app instead of Asteria's.")
                .show(ui, &t, |ui| {
                    let w = control(ui);
                    changed |= kit::Field::new(&mut app.cfg.msa_client_id).hint("Use default").width(w).show(ui, &t).changed();
                });
        });

        keys(app, ui, &t);
        ui.add_space(tk::GUTTER * 2.0);

        if changed && let Err(e) = app.cfg.save() {
            app.toast(ToastKind::Error, format!("Couldn't save settings: {e}"));
        }
    });
}

/// Fontshare's licence doesn't allow bundling Switzer or Melodrama: say so when they're missing.
fn fonts_note(app: &App, ui: &mut Ui, t: &Tokens) {
    let missing = &app.look.fonts.missing;
    if missing.is_empty() {
        return;
    }
    let names = missing.join(" and ");
    let verb = if missing.len() == 1 { "isn't" } else { "aren't" };
    let sentence = format!("{names} {verb} installed, so Asteria uses your system's sans-serif instead. They're free from Fontshare.");
    if kit::banner(ui, t, "fonts", Some(t.warning()), &sentence, &[Button::new("fontshare.com").icon(glyphs::GLOBE)]) == Some(0) {
        super::open_url("https://www.fontshare.com/?q=switzer,melodrama");
    }
}

/// The share sets and world sets, each a row with its members; a set nobody uses can be deleted.
fn shared(app: &mut App, ui: &mut Ui, t: &Tokens) {
    let busy = app.sharing.is_some();
    let sets = Kind::ALL.map(|k| app.all_sets(k));
    let count = |n: usize, one: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {one}s") };
    let aside = format!("{} · {}", count(sets[0].len(), "share set"), count(sets[1].len(), "world set"));
    let mut delete = None;
    kit::setting_group(ui, t, 4, "Shared folders", &aside, |ui| {
        if sets.iter().all(|s| s.is_empty()) {
            SettingRow::new("No sets yet").description("Edit an instance to pick its sets. Joining moves the instance's files in, keeping both on a name clash; leaving gives it its own copy.").show(ui, t, |_| ());
        }
        for (kind, sets) in Kind::ALL.into_iter().zip(&sets) {
            let what = match kind {
                Kind::Packs => "share set · resource packs, shader packs, schematics",
                Kind::Worlds => "world set · worlds, one version and loader",
            };
            for set in sets {
                // Instances that left on paper but still link in (a leave that failed) count too.
                let members: Vec<String> = super::instances::set_members(kind, set, &app.instances, app.set_links(kind))
                    .map(|(i, leaving)| if leaving { format!("{} (leaving)", i.name) } else { i.name.clone() })
                    .collect();
                let who = if members.is_empty() { "No instance uses it.".to_string() } else { members.join(", ") };
                SettingRow::new(set).id_salt(("set", kind.noun(), set)).caption(what).description(&who).show(ui, t, |ui| {
                    let mut buttons = vec![Button::new("folder").icon(glyphs::FOLDER)];
                    if members.is_empty() {
                        buttons.push(Button::new("delete").enabled(!busy));
                    }
                    // The row's control column runs right to left; a joined row reads left to right.
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| match kit::button_row(ui, t, &buttons) {
                        Some(0) => super::open_path(&shares::set_dir(kind, set)),
                        Some(_) => delete = Some((kind, set.clone())),
                        None => {}
                    });
                });
            }
        }
    });
    if let Some((kind, set)) = delete {
        let what = match kind {
            Kind::Packs => "the packs and schematics",
            Kind::Worlds => "the worlds",
        };
        app.modal = Some(Modal::Confirm(Confirm {
            title: format!("delete {}", kind.noun()),
            body: format!("No instance uses “{set}”. Permanently delete it and {what} in it?"),
            button: "delete".into(),
            action: ConfirmAction::DeleteSet(kind, set),
        }));
    }
}

/// Where Asteria keeps things, and reclaiming disk space (its result as a figure).
fn storage(app: &mut App, ui: &mut Ui, t: &Tokens) {
    let running = app.reclaim.is_some();
    let mut reclaim = false;
    kit::setting_group(ui, t, 5, "Storage", "", |ui| {
        let data = paths::data_dir().display().to_string();
        SettingRow::new("Data").caption(&data).show(ui, t, |ui| {
            if Button::new("open").icon(glyphs::FOLDER).show(ui, t).clicked() {
                super::open_path(&paths::data_dir());
            }
        });
        let config = paths::config_file().display().to_string();
        SettingRow::new("Settings file").caption(&config).show(ui, t, |ui| {
            if Button::new("open").icon(glyphs::FOLDER).show(ui, t).clicked()
                && let Some(dir) = paths::config_file().parent()
            {
                super::open_path(dir);
            }
        });
        SettingRow::new("Reclaim disk space")
            .description("Mods and packs are downloaded once and shared between instances on btrfs or xfs. Reclaim merges identical files that came in another way (imports, jars added by hand) and clears cached downloads no instance uses.")
            .show(ui, t, |ui| {
                reclaim = Button::new(if running { "reclaiming" } else { "reclaim" }).enabled(!running).show(ui, t).clicked();
            });
        if let Some(r) = &app.reclaimed {
            let freed = r.merged_bytes + r.pruned_bytes;
            SettingRow::new("Last reclaim").caption(&r.summary()).show(ui, t, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = tk::GAP;
                    kit::text::display(ui, t, &crate::store::human(freed), tk::SIZE_DISPLAY_SMALL);
                    kit::caption(ui, t, "freed");
                });
            });
        }
    });
    if reclaim {
        app.start_reclaim(ui.ctx());
    }
}

/// The keys: each action a dense row with its keys as buttons. Click a key to change it,
/// right-click to remove it, + to add another; "use default" puts the defaults back.
fn keys(app: &mut App, ui: &mut Ui, t: &Tokens) {
    let mut set: Option<(Action, Vec<keymap::Chord>)> = None;
    let mut reset_all = false;
    let custom = app.cfg.keys.len();
    let aside = if custom == 0 { "all default".to_string() } else { format!("{custom} changed") };
    kit::setting_group(ui, t, 7, "Keys", &aside, |ui| {
        SettingRow::new("Changing keys")
            .dense()
            .description("Click a key to change it, then press the new one (Esc cancels). Right-click a key to remove it, + adds another. Saved under [keys] in config.toml.")
            .show(ui, t, |ui| {
                if custom > 0 && Button::new("reset all").show(ui, t).clicked() {
                    reset_all = true;
                }
            });
        let mut section = "";
        for info in keymap::ACTIONS {
            if info.section != section {
                section = info.section;
                section_caption(ui, t, section);
            }
            let chords = app.keymap.chords(info.action).to_vec();
            let defaults: Vec<_> = info.defaults.iter().filter_map(|d| keymap::Chord::parse(d)).collect();
            let clashes: Vec<&str> = chords.iter().flat_map(|c| app.keymap.conflicts(info.action, c)).map(|a| keymap::info(a).what).collect();
            let facts = if clashes.is_empty() { String::new() } else { format!("also {}", clashes.join(", ")) };
            let mut row = SettingRow::new(info.what).dense().id_salt(("key", info.id));
            if !facts.is_empty() {
                row = row.facts(&facts, Some(t.warning()));
            }
            if chords != defaults {
                row = row.action("use default");
            }
            let capture = app.capture;
            let (out, picked) = row.show_row(ui, t, |ui| {
                let mut picked = None;
                // Right to left: add first, then the keys from the last.
                let adding = capture == Some((info.action, None));
                let add = Button::new(if adding { "press a key" } else { "" }).icon(glyphs::PLUS).hot(adding).show(ui, t);
                if add.clicked() {
                    picked = Some(KeyPick::Capture(None));
                }
                for (n, c) in chords.iter().enumerate().rev() {
                    let capturing = capture == Some((info.action, Some(n)));
                    let label = if capturing { "press a key".to_string() } else { c.label() };
                    let r = if capturing { Button::new(&label).hot(true) } else { Button::new(&label).key() }.show(ui, t);
                    if r.clicked() {
                        picked = Some(KeyPick::Capture(Some(n)));
                    }
                    if r.secondary_clicked() {
                        picked = Some(KeyPick::Remove(n));
                    }
                }
                picked
            });
            match picked {
                Some(KeyPick::Capture(n)) => app.capture = Some((info.action, n)),
                Some(KeyPick::Remove(n)) => {
                    let mut rest = chords.clone();
                    rest.remove(n);
                    set = Some((info.action, rest));
                }
                None => {}
            }
            if out.action {
                set = Some((info.action, defaults));
            }
        }
    });
    if let Some((a, chords)) = set {
        app.set_keys(a, chords);
    }
    if reset_all {
        app.cfg.keys.clear();
        app.keymap = keymap::Keymap::new(&app.cfg.keys);
        if let Err(e) = app.cfg.save() {
            app.toast(ToastKind::Error, format!("Couldn't save keys: {e}"));
        }
    }
}

enum KeyPick {
    Capture(Option<usize>),
    Remove(usize),
}

/// A keymap section's name over its rows, on the group's bracket.
fn section_caption(ui: &mut Ui, t: &Tokens, section: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), tk::SECTION_LABEL_HEIGHT), egui::Sense::hover());
    kit::rule::hline(ui.painter(), (rect.left() + tk::GUTTER)..=rect.right(), rect.top() + 0.5, t.line);
    kit::text::paint_caption(ui.painter(), egui::pos2(rect.left() + tk::SETTING_INDENT, rect.center().y), section, t.text_strong);
}
