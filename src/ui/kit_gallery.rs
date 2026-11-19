//! The kit gallery (`ASTERIA_DEBUG_VIEW=kit`): every primitive of the design kit and the
//! plan's new patterns, in the system's palette, light, dark, or both side by side, to tune
//! by eye. A hidden view: nothing in the app leads here. `ASTERIA_KIT_SECTION=<n>`,
//! `ASTERIA_KIT_PALETTE=light|dark|both` and `ASTERIA_KIT_KEY_ROW=<n>` (a keyboard-current
//! row) set where it opens.

use egui::{Rect, Sense, Ui, UiBuilder, vec2};

use crate::design::glyphs as g;
use crate::design::kit::{self, Button, Cell, Column, HoverList, HoverRow, SectionLabel, SettingRow, Spec, Variant};
use crate::design::tokens::{self as tk, Tokens};
use crate::design::{Look, fonts};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Palette {
    System,
    Light,
    Dark,
    Both,
}

const SECTIONS: [&str; 12] = [
    "Type and icons",
    "Colour",
    "Lines and buttons",
    "Controls",
    "Settings",
    "Hover list",
    "Compact and quiet lists",
    "Table and tiles",
    "Bars and notices",
    "Sheets and strips",
    "Instance page patterns",
    "Flows",
];

/// Demo state, shared by both palettes so they stay in step.
pub struct Gallery {
    palette: Palette,
    section: usize,
    /// The keyboard-current row in the demo lists (j/k or the arrows), to show keys get the
    /// same response as the pointer.
    key_row: Option<usize>,
    switches: [bool; 4],
    checks: [bool; 3],
    loader: usize,
    name: String,
    jvm: String,
    password: String,
    notes: String,
    /// The big picture: (previous, current, change count).
    picture: (Option<usize>, usize, u32),
    filter: String,
    memory: f64,
    fov: f64,
    selected: usize,
    table_sel: Option<usize>,
    tile_sel: Option<usize>,
    answer: Option<usize>,
    sheet_open: u32,
    strip_open: u32,
    notice: Option<(kit::NoticeKind, u32)>,
    progress: f32,
}

impl Default for Gallery {
    fn default() -> Self {
        let section = std::env::var("ASTERIA_KIT_SECTION").ok().and_then(|s| s.parse::<usize>().ok()).map_or(0, |n| n.saturating_sub(1).min(SECTIONS.len() - 1));
        let palette = match std::env::var("ASTERIA_KIT_PALETTE").as_deref() {
            Ok("light") => Palette::Light,
            Ok("dark") => Palette::Dark,
            Ok("both") => Palette::Both,
            _ => Palette::System,
        };
        Self {
            palette,
            section,
            key_row: std::env::var("ASTERIA_KIT_KEY_ROW").ok().and_then(|s| s.parse().ok()),
            switches: [true, false, true, false],
            checks: [true, false, true],
            loader: 1,
            name: "Vanilla+".into(),
            jvm: String::new(),
            password: String::new(),
            notes: "Modded survival with friends. Back up before updating Create.\nServer: play.example.com".into(),
            picture: (None, 0, 0),
            filter: String::new(),
            memory: 4096.0,
            fov: 70.0,
            selected: 0,
            table_sel: Some(1),
            tile_sel: None,
            answer: None,
            sheet_open: 0,
            strip_open: 0,
            notice: Some((kit::NoticeKind::Error, 1)),
            progress: 0.62,
        }
    }
}

impl Gallery {
    pub fn show(&mut self, ui: &mut Ui, look: &Look) {
        let ctx = ui.ctx().clone();
        // j/k and the arrows move the keyboard-current row.
        let (down, up) = ctx.input(|i| (i.key_pressed(egui::Key::J) || i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::K) || i.key_pressed(egui::Key::ArrowUp)));
        if ctx.memory(|m| m.focused().is_none()) {
            if down {
                self.key_row = Some(self.key_row.map_or(0, |r| (r + 1).min(5)));
            }
            if up {
                self.key_row = self.key_row.and_then(|r| r.checked_sub(1));
            }
        }
        let sys = look.tokens.clone();
        let light = Tokens { reduced_motion: sys.reduced_motion, ..Tokens::new(false) };
        let dark = Tokens { reduced_motion: sys.reduced_motion, ..Tokens::new(true) };
        egui::CentralPanel::default().frame(egui::Frame::new().fill(sys.background)).show(ui, |ui| {
            let full = ui.max_rect();
            // Head: the gallery's own controls, in the system palette.
            let head = Rect::from_min_size(full.min, vec2(full.width(), tk::BAR_HEIGHT));
            ui.scope_builder(UiBuilder::new().max_rect(head), |ui| {
                kit::bar(ui, &sys, |ui| {
                    kit::mark_cell(ui, &sys, "Asteria");
                    kit::BarCell::new("kit gallery").passive().open(true).show(ui, &sys);
                    ui.add_space(tk::GAP);
                    let mut p = self.palette;
                    kit::segmented(ui, &sys, &mut p, &[(Palette::System, "system"), (Palette::Light, "light"), (Palette::Dark, "dark"), (Palette::Both, "both")]);
                    self.palette = p;
                    ui.add_space(tk::GAP);
                    kit::caption(ui, &sys, if sys.reduced_motion { "reduced motion" } else { "j / k: keyboard row" });
                });
            });
            let body = Rect::from_min_max(egui::pos2(full.left(), head.bottom()), full.max);
            let nav = Rect::from_min_size(body.min, vec2(260.0, body.height()));
            ui.scope_builder(UiBuilder::new().max_rect(nav).id_salt("kit-nav"), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut list = HoverList::begin(ui, Variant::Quiet);
                for (i, s) in SECTIONS.iter().enumerate() {
                    let label = format!("{}  {s}", kit::text::number(i + 1));
                    if list.row(ui, &sys, HoverRow::new(("kit-section", i), &label).selected(self.section == i)).clicked() {
                        self.section = i;
                    }
                }
                list.end(ui);
            });
            kit::vline(ui.painter(), nav.right(), body.y_range(), sys.line_strong);
            let content = Rect::from_min_max(egui::pos2(nav.right() + 1.0, body.top()), body.max);
            let columns: Vec<(&Tokens, &str)> = match self.palette {
                Palette::System => vec![(&sys, "system")],
                Palette::Light => vec![(&light, "light")],
                Palette::Dark => vec![(&dark, "dark")],
                Palette::Both => vec![(&light, "light"), (&dark, "dark")],
            };
            let w = (content.width() / columns.len() as f32).floor();
            for (i, (t, salt)) in columns.into_iter().enumerate() {
                let r = Rect::from_min_size(content.min + vec2(i as f32 * w, 0.0), vec2(w, content.height()));
                ui.painter().rect_filled(r, 0.0, t.background);
                ui.scope_builder(UiBuilder::new().max_rect(r).id_salt(salt), |ui| {
                    ui.set_clip_rect(r);
                    egui::ScrollArea::vertical().id_salt(("kit-scroll", salt)).auto_shrink(false).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        self.section(ui, t, salt);
                        ui.add_space(tk::GUTTER * 2.0);
                    });
                });
                if i > 0 {
                    kit::vline(ui.painter(), r.left(), r.y_range(), t.line_strong);
                }
            }
        });
        // Advance the demo meter, slowly, while the bars section is open.
        if self.section == 8 {
            self.progress = (self.progress + ctx.input(|i| i.stable_dt) * 0.02) % 1.0;
            ctx.request_repaint();
        }
    }

    fn label(&self, ui: &mut Ui, t: &Tokens, n: usize, label: &str, aside: &str) {
        SectionLabel::new(n, label).aside(aside).show(ui, t);
    }

    fn pad<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(tk::GUTTER as i8, tk::GAP as i8)).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(tk::GAP, tk::GAP);
            add(ui)
        }).inner
    }

    fn section(&mut self, ui: &mut Ui, t: &Tokens, salt: &str) {
        match self.section {
            0 => self.type_and_icons(ui, t),
            1 => self.colour(ui, t),
            2 => self.lines_and_buttons(ui, t),
            3 => self.controls(ui, t),
            4 => self.settings(ui, t),
            5 => self.hover_list(ui, t, salt),
            6 => self.compact_lists(ui, t, salt),
            7 => self.table_and_tiles(ui, t),
            8 => self.bars(ui, t, salt),
            9 => self.sheets(ui, t, salt),
            10 => self.instance_patterns(ui, t, salt),
            _ => self.flows(ui, t, salt),
        }
    }

    fn type_and_icons(&mut self, ui: &mut Ui, t: &Tokens) {
        self.label(ui, t, 1, "Type", "Melodrama · Switzer · JetBrains Mono");
        Self::pad(ui, |ui| {
            kit::text::display_large(ui, t, "Asteria", tk::SIZE_DISPLAY_LARGE);
            kit::display(ui, t, "Vanilla+ 26.3", tk::SIZE_DISPLAY);
            kit::heading(ui, t, "Find breaking mod");
            kit::heading_small(ui, t, "Sodium 0.6.1");
            kit::body(ui, t, "Body text is Switzer SemiBold 14. Paragraphs wrap to the width they're given and are never justified, so the spacing between words stays even.");
            kit::body_muted(ui, t, "Muted body text for hints and secondary lines.");
            kit::caption(ui, t, "Captions are uppercase, 11px, tracked 2px");
            ui.horizontal(|ui| {
                kit::figure(ui, "93h 09m · 46.6 GB · 1,204", fonts::heading(tk::SIZE_HEADING), t.text_strong);
            });
            ui.horizontal(|ui| {
                kit::figure(ui, "111.1", fonts::heading(tk::SIZE_HEADING), t.text_strong);
                kit::caption(ui, t, "tabular: digits share one width");
            });
            kit::text::raw(ui, "[21:04:13] [Render thread/WARN]: Missing sound for event: minecraft:item.goat_horn.play", t.ochre);
            kit::text::raw(ui, "java.lang.NoClassDefFoundError: me/jellysquid/mods/sodium/client/SodiumClientMod", t.clay);
        });
        self.label(ui, t, 2, "Icons", "Phosphor regular · bold where the icon is the label");
        Self::pad(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(tk::GAP, tk::GAP);
                for (name, glyph) in g::ALL {
                    let r = kit::icon(ui, t, glyph, tk::ICON_SIZE, false);
                    r.on_hover_text(*name);
                }
            });
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
                kit::text::icon_label(ui.painter(), r.center(), g::PLAY, 22.0, t.text_strong);
                let (r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
                kit::text::icon_label(ui.painter(), r.center(), g::STOP, 22.0, t.text_strong);
                kit::caption(ui, t, "play and stop: bold, in ink");
            });
        });
    }

    fn colour(&mut self, ui: &mut Ui, t: &Tokens) {
        self.label(ui, t, 1, "Palette", "one surface · two inks · one accent");
        let sw = |ui: &mut Ui, name: &str, c: egui::Color32, note: &str| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(56.0, 28.0), Sense::hover());
                ui.painter().rect_filled(r, 0.0, c);
                kit::rule::frame(ui.painter(), r, t.line);
                ui.add_space(tk::GAP);
                ui.add_sized(vec2(140.0, 28.0), egui::Label::new(egui::RichText::new(name).font(fonts::text(tk::SIZE_BODY)).color(c.to_opaque())));
                kit::caption(ui, t, note);
            });
        };
        Self::pad(ui, |ui| {
            sw(ui, "background", t.background, "every surface");
            sw(ui, "accent", t.accent, "hover · focus · active · selected");
            sw(ui, "text strong", t.text_strong, "headings, primary text");
            sw(ui, "text muted", t.text_muted, "secondary, captions, icons at rest");
            sw(ui, "line strong", t.line_strong, "frames sections");
            sw(ui, "line", t.line, "divides rows");
            sw(ui, "wash", t.background.lerp_to_gamma(t.text_strong, 0.08), "selection");
        });
        self.label(ui, t, 2, "Clarity hues", "text and small marks only, never state");
        Self::pad(ui, |ui| {
            for (name, c, use_) in [
                ("clay", t.clay, "errors · red groups"),
                ("sienna", t.sienna, "orange groups"),
                ("ochre", t.ochre, "warnings · yellow groups"),
                ("olive", t.olive, "running · installed · green groups"),
                ("teal", t.teal, "cyan groups"),
                ("slate", t.slate, "blue groups"),
                ("plum", t.plum, "magenta groups"),
            ] {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(tk::MARK_SQUARE, 28.0), Sense::hover());
                    ui.painter().rect_filled(Rect::from_center_size(r.center(), vec2(tk::MARK_SQUARE, tk::MARK_SQUARE)), 0.0, c);
                    ui.add_space(tk::UNIT);
                    ui.add_sized(vec2(140.0, 28.0), egui::Label::new(egui::RichText::new(name).font(fonts::text(tk::SIZE_BODY)).color(c)));
                    kit::caption_colored(ui, use_, c);
                });
            }
        });
    }

    fn lines_and_buttons(&mut self, ui: &mut Ui, t: &Tokens) {
        self.label(ui, t, 1, "Section labels", "numbered, over a strong rule");
        SectionLabel::new(2, "Singleplayer").aside("4").show(ui, t);
        SectionLabel::new(3, "Modded servers").aside("2").hue(t.named("blue")).show(ui, t);
        SectionLabel::new(4, "Drop here").aside("dragging").drop_target(true).show(ui, t);
        SectionLabel::new(5, "Click to fold").aside("3").clickable().show(ui, t);
        self.label(ui, t, 6, "Buttons", "outlined · one filled per screen");
        Self::pad(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                Button::new("play").icon(g::PLAY).filled().show(ui, t);
                Button::new("edit").min_width(120.0).show(ui, t);
                Button::new("folder").icon(g::FOLDER).show(ui, t);
                Button::new("disabled").enabled(false).show(ui, t);
                Button::new("keyboard hot").hot(true).show(ui, t);
                Button::new("").icon(g::ARROW_UP).show(ui, t);
                Button::new("").icon(g::ARROW_DOWN).show(ui, t);
            });
            ui.horizontal(|ui| {
                kit::button_row(ui, t, &[Button::new("folder"), Button::new("check updates"), Button::new("roll back")]);
            });
            ui.horizontal(|ui| {
                let mut v = self.loader;
                kit::segmented(ui, t, &mut v, &[(0, "vanilla"), (1, "fabric"), (2, "quilt"), (3, "forge"), (4, "neoforge")]);
                self.loader = v;
            });
        });
    }

    fn controls(&mut self, ui: &mut Ui, t: &Tokens) {
        self.label(ui, t, 1, "Switches and checkboxes", "square, accent when on");
        Self::pad(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, label) in ["show snapshots", "auto backups", "disabled", "keyboard hot"].iter().enumerate() {
                    let r = kit::switch(ui, t, self.switches[i], i != 2, i == 3, label);
                    if r.clicked() {
                        self.switches[i] = !self.switches[i];
                    }
                    kit::caption(ui, t, label);
                    ui.add_space(tk::GUTTER);
                }
            });
            for (i, label) in ["Sodium", "Iris Shaders", "Lithium"].iter().enumerate() {
                if kit::checkbox(ui, t, self.checks[i], true, false, label).clicked() {
                    self.checks[i] = !self.checks[i];
                }
            }
        });
        self.label(ui, t, 2, "Fields", "1px frame · 2px accent caret · no well");
        Self::pad(ui, |ui| {
            kit::Field::new(&mut self.name).label("name").width(420.0).show(ui, t);
            kit::Field::new(&mut self.jvm).hint("-XX:+UseZGC").width(420.0).show(ui, t);
            kit::Field::new(&mut self.password).label("password").password(true).focus(false).width(420.0).show(ui, t);
            kit::query(ui, t, &mut self.filter, "Filter instances", tk::SIZE_SEARCH, false);
            kit::caption(ui, t, "notes");
            kit::field::notes(ui, t, &mut self.notes, "Anything worth remembering about this instance", 4);
        });
        self.label(ui, t, 3, "Slider and meter", "ruler of ticks · mark for the default");
        Self::pad(ui, |ui| {
            let mem = self.memory;
            kit::Slider::new(&mut self.memory, 1024.0, 32768.0).step(512.0).label("memory").value_text(format!("{} MiB", mem as i64)).mark(4096.0).width(420.0).show(ui, t);
            let fov = self.fov;
            kit::Slider::new(&mut self.fov, 30.0, 110.0).step(1.0).label("field of view").value_text(format!("{fov}")).mark(70.0).hot(self.key_row == Some(1)).width(420.0).show(ui, t);
            kit::caption(ui, t, "downloading libraries · 62%");
            kit::meter(ui, t, kit::Fill::Part(0.62), 420.0, &[]);
            kit::caption(ui, t, "resolving dependencies");
            kit::meter(ui, t, kit::Fill::Unknown, 420.0, &[]);
        });
    }

    fn settings(&mut self, ui: &mut Ui, t: &Tokens) {
        kit::setting_group(ui, t, 1, "Java", "", |ui| {
            let mem = self.memory;
            SettingRow::new("Memory").caption("per instance unless set there").show(ui, t, |ui| {
                kit::Slider::new(&mut self.memory, 1024.0, 32768.0).step(512.0).value_text(format!("{} MiB", mem as i64)).mark(4096.0).width(ui.available_width().min(tk::SETTING_CONTROL_WIDTH)).show(ui, t)
            });
            SettingRow::new("JVM arguments").caption("added after Asteria's own").show(ui, t, |ui| kit::Field::new(&mut self.jvm).hint("none").width(ui.available_width().min(tk::SETTING_CONTROL_WIDTH)).show(ui, t));
        });
        kit::setting_group(ui, t, 2, "Versions", "", |ui| {
            SettingRow::new("Show snapshots").caption("in the version picker").show(ui, t, |ui| {
                if kit::switch(ui, t, self.switches[0], true, false, "show snapshots").clicked() {
                    self.switches[0] = !self.switches[0];
                }
            });
        });
        kit::setting_group(ui, t, 3, "Sodium", "sodium-options.json · 2 unsaved changes", |ui| {
            SettingRow::new("quality.weather_quality").dense().unsaved(true).description("Controls the quality of rain and snow effects. Fancy draws more particles.").show(ui, t, |ui| {
                let mut v = self.loader.min(2);
                kit::segmented(ui, t, &mut v, &[(0, "fast"), (1, "fancy"), (2, "default")]);
                self.loader = v;
            });
            SettingRow::new("performance.use_block_face_culling").dense().description("If enabled, only the sides of blocks facing the camera are rendered. Default: true").show(ui, t, |ui| {
                if kit::switch(ui, t, self.switches[1], true, false, "culling").clicked() {
                    self.switches[1] = !self.switches[1];
                }
            });
            let fov = self.fov;
            SettingRow::new("fov").dense().unsaved(self.fov != 70.0).description("Range 30–110. Default: 70").show(ui, t, |ui| {
                kit::Slider::new(&mut self.fov, 30.0, 110.0).step(1.0).value_text(format!("{fov}")).mark(70.0).width(ui.available_width().min(tk::SETTING_CONTROL_WIDTH)).show(ui, t)
            });
            SettingRow::new("key.inventory").dense().hot(true).description("Waiting for a key, shown as a keyboard-hot row.").show(ui, t, |ui| {
                kit::caption_colored(ui, "press a key · esc cancels", t.accent);
            });
        });
    }

    fn instances() -> [(&'static str, &'static str, &'static str, Option<&'static str>); 6] {
        [
            ("Vanilla+", "Fabric 0.19.3 · 1.21.11 / played 93h · last today / 46.6 GB", "grass", Some("blue")),
            ("26.3", "Vanilla · 26.3 / played 2h · last yesterday", "diamond", Some("blue")),
            ("Epic Modpack 1", "NeoForge 21.1.77 · 1.21.1 / played 12h · last 3 days ago", "creeper", Some("green")),
            ("COBBLEVERSE - Pokemon Adventure [Cobblemon]", "Fabric 0.16.14 · 1.21.1 / never played", "netherstar", Some("green")),
            ("Skyblock", "Forge 47.3.0 · 1.20.1 / played 40h", "dirt", None),
            ("Old world", "Vanilla · 1.12.2 / played 300h · last a year ago", "planks", None),
        ]
    }

    fn hover_list(&mut self, ui: &mut Ui, t: &Tokens, salt: &str) {
        let rows = Self::instances();
        let groups = [("Singleplayer", "blue", 0..2), ("Modded servers", "green", 2..4), ("Ungrouped", "", 4..6)];
        for (gi, (name, hue, range)) in groups.into_iter().enumerate() {
            SectionLabel::new(gi + 1, name).aside(&range.len().to_string()).hue(t.named(hue)).show(ui, t);
            let mut list = HoverList::begin(ui, Variant::Full);
            for i in range {
                let (title, desc, icon, hue) = rows[i];
                let mut row = HoverRow::new((salt, "inst", i), title)
                    .number(i + 1)
                    .sense(Sense::click_and_drag())
                    .description(desc)
                    .image(crate::icons::source(icon), 1.0)
                    .hue(hue.and_then(|h| t.named(h)))
                    .selected(self.selected == i)
                    .hot(self.key_row == Some(i));
                if i == 0 {
                    row = row.badge("running", t.ok());
                }
                if list.row(ui, t, row).clicked() {
                    self.selected = i;
                }
            }
            list.end(ui);
        }
    }

    fn compact_lists(&mut self, ui: &mut Ui, t: &Tokens, salt: &str) {
        self.label(ui, t, 1, "Mods", "72 of 72 on");
        let mods = [("Sodium", "0.6.1 · 1.2 MB", "sodium-fabric-0.6.1+mc1.21.jar", "magitech"), ("Iris Shaders", "1.8.0 · 2.9 MB", "iris-fabric-1.8.0.jar", "enderpearl"), ("Lithium", "0.14.3 · 700 KB", "lithium-fabric-0.14.3.jar", "iron"), ("Mod Menu (off)", "13.0.0 · 71 KB", "modmenu-13.0.0.jar.disabled", "gold")];
        let mut list = HoverList::begin(ui, Variant::Compact);
        for (i, (name, desc, file, icon)) in mods.iter().enumerate() {
            let row = HoverRow::new((salt, "mod", i), name).description(file).lead(crate::icons::source(icon)).badge(desc, t.text_muted).dim(i == 3).hot(self.key_row == Some(i)).selected(self.selected == i);
            if list.row(ui, t, row).clicked() {
                self.selected = i;
            }
        }
        list.end(ui);
        self.label(ui, t, 2, "Configs tree", "quiet: accent and nothing else");
        let tree = [("Sodium", 0, "3"), ("sodium-options.json", 1, ""), ("sodium-mixins.properties", 1, ""), ("Iris", 0, "1"), ("iris.properties", 1, ""), ("Other files", 0, "12")];
        let mut list = HoverList::begin(ui, Variant::Quiet);
        for (i, (name, depth, aside)) in tree.iter().enumerate() {
            let row = HoverRow::new((salt, "tree", i), name).depth(*depth).aside(aside).selected(i == 1).hot(self.key_row == Some(i));
            list.row(ui, t, row);
        }
        list.end(ui);
        self.label(ui, t, 3, "Instance list", "beside the page: the description goes under the title");
        let insts = [(None, "26.3", "vanilla · 26.3", "stone", false), (Some((1, "Singleplayer", t.slate)), "Vanilla+", "fabric · 1.21.11 · 93h 9m", "grass", true), (None, "Creative builds", "vanilla · 1.21.4 · 3h 20m", "brick", false)];
        ui.allocate_ui(vec2(tk::LIST_RAIL_MAX, 0.0), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut list = HoverList::begin(ui, Variant::Compact).indent(tk::GUTTER + tk::HOVER_SHIFT_COMPACT);
            for (i, (group, name, desc, icon, running)) in insts.iter().enumerate() {
                if let Some((n, g, hue)) = group {
                    kit::SectionLabel::new(*n, g).aside("2").hue(Some(*hue)).clickable().show(ui, t);
                }
                let hue = (i > 0).then_some(t.slate);
                let mut row = HoverRow::new((salt, "inst", i), name).description(desc).image(crate::icons::source(icon), 1.0).hue(hue).hot(self.key_row == Some(i)).selected(self.selected == i);
                if *running {
                    row = row.badge("running", t.ok());
                }
                if list.row(ui, t, row).clicked() {
                    self.selected = i;
                }
            }
            list.end(ui);
        });
    }

    fn table_and_tiles(&mut self, ui: &mut Ui, t: &Tokens) {
        self.label(ui, t, 1, "Backups", "quiet table · no hover");
        let cols = [Column::fill("taken"), Column::fixed("kind", 140.0), Column::fixed("size", 120.0).right()];
        kit::table::header(ui, t, &cols);
        let rows = [("18 Nov 2026 21:04", "auto", "182.4 MB"), ("17 Nov 2026 09:12", "by hand", "180.1 MB"), ("12 Nov 2026 18:40", "before restore", "176.0 MB"), ("2 Nov 2026 11:03", "auto", "91.7 MB")];
        for (i, (taken, kind, size)) in rows.iter().enumerate() {
            let when = if i == 0 { Cell::Text(taken.to_string()) } else { Cell::Muted(taken.to_string()) };
            let cells = [when, Cell::Caption(kind.to_string(), None), Cell::Figure(size.to_string())];
            if kit::table::row(ui, t, &cols, &cells, self.table_sel == Some(i)).clicked() {
                self.table_sel = Some(i);
            }
        }
        self.label(ui, t, 2, "Crash evidence", "statuses in clarity hues");
        let cols = [Column::fill("mod"), Column::fixed("confidence", 160.0)];
        kit::table::header(ui, t, &cols);
        for (m, c, hue) in [("Sodium", "certain", Some(t.clay)), ("Iris Shaders", "likely", Some(t.ochre)), ("Lithium", "possible", None)] {
            kit::table::row(ui, t, &cols, &[Cell::Text(m.into()), Cell::Caption(c.into(), hue)], false);
        }
        self.label(ui, t, 3, "Tiles", "shared hairlines · marker on hover");
        let icons = ["grass", "creeper", "diamond", "tnt", "fox", "bee", "chicken", "enderman"];
        let out = kit::Tiles { count: icons.len(), columns: 4, aspect: 16.0 / 10.0, selected: self.tile_sel, hot: self.key_row }.show(ui, t, |ui, i, r| {
            let s = r.height() * 0.6;
            egui::Image::new(crate::icons::source(icons[i])).paint_at(ui, Rect::from_center_size(r.center(), vec2(s, s)));
            format!("2026-11-1{} 21:0{} · vanilla+", i % 9, i)
        });
        if let Some(i) = out.clicked {
            self.tile_sel = Some(i);
            if i != self.picture.1 {
                self.picture = (Some(self.picture.1), i, self.picture.2 + 1);
            }
        }
        self.label(ui, t, 4, "Big picture", "click a tile: 700ms crossfade");
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 240.0), Sense::hover());
        let (prev, cur, n) = self.picture;
        let square = Rect::from_center_size(r.center(), vec2(200.0, 200.0));
        kit::tiles::crossfade(ui, square, egui::Id::new(("picture", n)), prev.map(|p| crate::icons::source(icons[p])), crate::icons::source(icons[cur]));
    }

    fn bars(&mut self, ui: &mut Ui, t: &Tokens, salt: &str) {
        self.label(ui, t, 1, "Top bar", "hairline cells · 3px marker");
        kit::bar(ui, t, |ui| {
            kit::mark_cell(ui, t, "Asteria");
            kit::BarCell::new("instances").number(1).open(true).show(ui, t);
            kit::BarCell::new("accounts").number(2).hot(self.key_row == Some(1)).show(ui, t);
            kit::BarCell::new("settings").number(3).show(ui, t);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                kit::BarCell::new("star_ges · microsoft").right().show(ui, t);
                kit::BarCell::new("vanilla+").status("running", t.ok()).right().show(ui, t);
            });
        });
        ui.add_space(tk::GAP);
        kit::bar(ui, t, |ui| {
            kit::mark_cell(ui, t, "Asteria");
            kit::BarCell::new("instances").number(1).show(ui, t);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                kit::BarCell::new("no account").status("sign in", t.warning()).right().show(ui, t);
            });
        });
        self.label(ui, t, 2, "Bottom bar", "key hints · task meter · notices");
        let bottom = |ui: &mut Ui, notice: Option<(kit::NoticeKind, &str, egui::Id)>, progress: Option<f32>| -> kit::sheet::NoticeOut {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), tk::BAR_HEIGHT), Sense::hover());
            kit::hline(ui.painter(), rect.x_range(), rect.top() + 0.5, t.line_strong);
            let task_w = if progress.is_some() { 360.0 } else { 0.0 };
            let hints = Rect::from_min_max(rect.min, egui::pos2(rect.right() - task_w, rect.bottom()));
            let mut out = kit::sheet::NoticeOut::default();
            if let Some((kind, text, id)) = notice {
                out = kit::notice(ui, t, hints, id, kind, text);
            } else {
                ui.scope_builder(UiBuilder::new().max_rect(hints.shrink2(vec2(tk::GUTTER, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                    ui.spacing_mut().item_spacing.x = tk::GUTTER;
                    for (k, l) in [("?", "keys"), ("enter", "open"), ("p", "play"), ("n", "new"), ("i", "import"), ("/", "filter")] {
                        kit::key_hint(ui, t, k, l);
                    }
                });
            }
            if let Some(f) = progress {
                let task = Rect::from_min_max(egui::pos2(hints.right(), rect.top()), rect.max);
                kit::vline(ui.painter(), task.left(), task.y_range(), t.line_strong);
                ui.scope_builder(UiBuilder::new().max_rect(task.shrink2(vec2(tk::GAP, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                    kit::caption(ui, t, "libraries");
                    ui.add_space(tk::GAP);
                    let w = ui.available_width();
                    ui.vertical(|ui| {
                        ui.add_space((tk::BAR_HEIGHT - kit::slider::METER_TOTAL_HEIGHT) / 2.0 + 2.0);
                        kit::meter(ui, t, kit::Fill::Part(f), w, &[]);
                    });
                });
            }
            out
        };
        bottom(ui, None, Some(self.progress));
        ui.add_space(tk::GAP);
        // An info notice holds the bar for its few seconds, then the hints come back.
        let age = (ui.input(|i| i.time) % 8.0) as f32;
        let info = kit::NoticeKind::Info.alive(age).then(|| (kit::NoticeKind::Info, "Created Vanilla+", egui::Id::new((salt, "n-info", (ui.input(|i| i.time) / 8.0) as u32))));
        bottom(ui, info, None);
        ui.add_space(tk::GAP);
        if let Some((kind, n)) = self.notice {
            let out = bottom(ui, Some((kind, "Couldn't start the game: Java 21 is needed for 1.21.11, and the download failed.\nconnection reset by peer", egui::Id::new((salt, "n-err", n)))), None);
            if out.dismissed {
                self.notice = None;
            }
        } else {
            Self::pad(ui, |ui| {
                if Button::new("show the error again").show(ui, t).clicked() {
                    self.notice = Some((kit::NoticeKind::Error, rand_id()));
                }
            });
        }
    }

    fn sheets(&mut self, ui: &mut Ui, t: &Tokens, salt: &str) {
        self.label(ui, t, 1, "Strip", "a confirmation takes over the foot of a pane");
        Self::pad(ui, |ui| {
            kit::body_muted(ui, t, "Destructive actions ask in a strip: a sentence, then the action filled and cancel. Enter and Esc answer it.");
            if Button::new("delete world").show(ui, t).clicked() {
                self.strip_open = rand_id();
            }
        });
        if self.strip_open != 0 {
            let id = egui::Id::new((salt, "strip", self.strip_open));
            if kit::strip(ui, t, id, "delete world", "Delete New World? A snapshot is kept in backups.", "delete").is_some() {
                self.strip_open = 0;
            }
        }
        self.label(ui, t, 2, "Sheet", "takes over the content area · content rises 24px");
        Self::pad(ui, |ui| {
            if Button::new("open a sheet").show(ui, t).clicked() {
                self.sheet_open = rand_id();
            }
        });
        let (area, _) = ui.allocate_exact_size(vec2(ui.available_width(), 340.0), Sense::hover());
        kit::rule::frame(ui.painter(), area, t.line);
        kit::Tiles { count: 3, columns: 3, aspect: 2.0, selected: None, hot: None }.show(&mut ui.new_child(UiBuilder::new().max_rect(area.shrink(1.0))), t, |_, i, _| format!("under the sheet {i}"));
        if self.sheet_open != 0 {
            let id = egui::Id::new((salt, "sheet", self.sheet_open));
            let close = kit::sheet(ui, t, area, id, "01 / new instance", "New instance", |ui| {
                kit::body_muted(ui, t, "Dialogs become sheets over the content area. The bars stay.");
                ui.add_space(tk::GAP);
                kit::Field::new(&mut self.name).label("name").width(360.0).show(ui, t);
                ui.add_space(tk::GAP);
                ui.horizontal(|ui| {
                    let create = Button::new("create").filled().show(ui, t).clicked();
                    let cancel = Button::new("cancel").show(ui, t).clicked();
                    create || cancel
                })
                .inner
            });
            if close {
                self.sheet_open = 0;
            }
        }
    }

    fn instance_patterns(&mut self, ui: &mut Ui, t: &Tokens, salt: &str) {
        self.label(ui, t, 1, "Instance", "fabric · 1.21.11");
        Self::pad(ui, |ui| {
            // Buttons first, from the right, then the name in the room left.
            ui.allocate_ui_with_layout(vec2(ui.available_width(), tk::SIZE_DISPLAY * 1.2), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                Button::new("folder").show(ui, t);
                Button::new("edit").show(ui, t);
                Button::new("play").icon(g::PLAY).filled().show(ui, t);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    kit::display(ui, t, "Vanilla+", tk::SIZE_DISPLAY);
                });
            });
        });
        let group = Some(t.slate);
        kit::spec_strip(ui, t, &[
            Spec::new("minecraft", "1.21.11"),
            Spec::new("loader", "Fabric 0.19.3"),
            Spec::new("group", "Singleplayer").link().hue(group),
            Spec::new("share set", "Singleplayer").link(),
            Spec::new("world set", "—").link(),
            Spec::new("played", "93h 9m"),
            Spec::new("on disk", "measuring"),
        ]);
        // Section tabs with the marker, then sub-tabs.
        kit::bar(ui, t, |ui| {
            for (i, s) in ["overview", "content", "configs", "worlds", "screenshots", "logs"].iter().enumerate() {
                kit::BarCell::new(s).number(i + 1).open(i == 1).show(ui, t);
            }
        });
        Self::pad(ui, |ui| {
            ui.horizontal(|ui| {
                let mut v = 0;
                kit::segmented(ui, t, &mut v, &[(0, "mods"), (1, "resource packs"), (2, "shader packs")]);
            });
        });
        // The search strip under the spec strip.
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), tk::BUTTON_HEIGHT + tk::GAP * 2.0), Sense::hover());
        ui.scope_builder(UiBuilder::new().max_rect(r.shrink2(vec2(tk::GUTTER, 0.0))).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = tk::GAP;
            Button::new("stop").show(ui, t);
            Button::new("answer").filled().show(ui, t);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                kit::caption_colored(ui, "search in progress · step 3 of ~7", t.text_strong);
            });
        });
        kit::hline(ui.painter(), r.x_range(), r.top() + 0.5, t.line_strong);
        kit::hline(ui.painter(), r.x_range(), r.bottom() - 0.5, t.line_strong);
        self.label(ui, t, 2, "Empty and loading", "hold still: the frame stays");
        Self::pad(ui, |ui| {
            if let Some(r) = kit::empty_state(ui, t, "No worlds yet", "Play this instance once, or drop a world folder or a .zip of one here.", Some("open saves folder")) {
                let _ = r;
            }
            kit::loading(ui, t, "reading worlds");
        });
        let _ = salt;
    }

    fn flows(&mut self, ui: &mut Ui, t: &Tokens, _salt: &str) {
        self.label(ui, t, 1, "Find breaking mod", "a sheet");
        Self::pad(ui, |ui| {
            kit::heading(ui, t, "Find breaking mod");
            kit::body(ui, t, "Asteria turns half of the suspects off, you play until the problem shows or doesn't, and it halves again. Seven steps at most for 72 mods.");
            kit::bracketed(ui, t, "before you start", |ui| {
                kit::body(ui, t, "Worlds you open with mods missing can lose what those mods added. Asteria snapshots each world first, but play a test world if you can.");
            });
        });
        kit::setting_group(ui, t, 2, "Step 3 of ~7", "9 suspects on", |ui| {
            for (i, (title, desc)) in [("Problem occurred", "The suspects are among the 9 mods that were on."), ("Problem did not occur", "The suspects are among the 8 mods that were off.")].iter().enumerate() {
                if kit::choice_row(ui, t, title, desc, self.answer == Some(i), self.key_row == Some(i)).clicked() {
                    self.answer = Some(i);
                }
            }
        });
        Self::pad(ui, |ui| {
            ui.horizontal(|ui| {
                Button::new("answer").filled().enabled(self.answer.is_some()).show(ui, t);
                Button::new("stop the search").show(ui, t);
            });
        });
        self.label(ui, t, 3, "Microsoft sign-in", "the one big figure");
        Self::pad(ui, |ui| {
            kit::caption(ui, t, "go to microsoft.com/link and enter");
            kit::text::display_large(ui, t, "AB12-CD34", tk::SIZE_DISPLAY_LARGE * 0.75);
            kit::button_row(ui, t, &[Button::new("copy").icon(g::COPY), Button::new("open").icon(g::GLOBE)]);
            kit::caption(ui, t, "waiting for microsoft · expires in 14 min");
        });
    }
}

/// A fresh id for each opening of a demo sheet, strip or notice.
fn rand_id() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.subsec_nanos() | 1)
}
