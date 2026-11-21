use egui::{Align, Layout, RichText, Sense, Ui, vec2};

use super::{App, Confirm, ConfirmAction, Modal, ToastKind, config, content, logs, new_instance, screenshots, servers, version, worlds};
use crate::design::kit::{self, Button, HoverList, HoverRow, SectionLabel, SettingRow, Spec, Variant};
use crate::design::motion::Motion;
use crate::design::{glyphs as g, tokens as tk};
use super::nav::{Nav, Zone};
use crate::icons;
use crate::instance::{self, ContentKind, Instance};
use crate::shares::Kind;

/// A view of an instance's page (a leaf: sections group them). `[`/`]` step through every
/// leaf in this order, section by section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstTab {
    /// Components and notes.
    Overview,
    Mods,
    ResourcePacks,
    ShaderPacks,
    Config,
    Worlds,
    Servers,
    Screenshots,
    Log,
    LogHistory,
}

/// The page's sections, each a run of leaves; a section with more than one has sub-tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Overview,
    Content,
    Configs,
    Worlds,
    Screenshots,
    Logs,
}

impl Section {
    pub const ALL: [(Section, &'static str); 6] = [
        (Section::Overview, "overview"),
        (Section::Content, "mods & packs"),
        (Section::Configs, "configs"),
        (Section::Worlds, "worlds"),
        (Section::Screenshots, "screenshots"),
        (Section::Logs, "logs"),
    ];

    /// Its leaves with their sub-tab labels, in `[`/`]` order.
    pub fn leaves(self) -> &'static [(InstTab, &'static str)] {
        match self {
            Section::Overview => &[(InstTab::Overview, "overview")],
            Section::Content => &[(InstTab::Mods, "mods"), (InstTab::ResourcePacks, "resource packs"), (InstTab::ShaderPacks, "shader packs")],
            Section::Configs => &[(InstTab::Config, "configs")],
            Section::Worlds => &[(InstTab::Worlds, "worlds"), (InstTab::Servers, "servers")],
            Section::Screenshots => &[(InstTab::Screenshots, "screenshots")],
            Section::Logs => &[(InstTab::Log, "live"), (InstTab::LogHistory, "history")],
        }
    }

    pub fn of(tab: InstTab) -> Section {
        Self::ALL.iter().map(|(s, _)| *s).find(|s| s.leaves().iter().any(|(l, _)| *l == tab)).unwrap()
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|(s, _)| *s == self).unwrap()
    }
}

impl InstTab {
    /// Every leaf, section by section.
    pub fn all() -> impl Iterator<Item = InstTab> {
        Section::ALL.into_iter().flat_map(|(s, _)| s.leaves().iter().map(|(l, _)| *l))
    }

    pub fn content_kind(self) -> Option<ContentKind> {
        match self {
            InstTab::Mods => Some(ContentKind::Mods),
            InstTab::ResourcePacks => Some(ContentKind::ResourcePacks),
            InstTab::ShaderPacks => Some(ContentKind::ShaderPacks),
            _ => None,
        }
    }

    /// Next/previous leaf, wrapping.
    pub fn step(self, delta: isize) -> InstTab {
        let all: Vec<InstTab> = Self::all().collect();
        let n = all.len() as isize;
        let i = all.iter().position(|t| *t == self).unwrap() as isize;
        all[((i + delta).rem_euclid(n)) as usize]
    }
}

/// Drag-and-drop payload: the id of the instance being dragged.
struct DragInst(String);

/// A line in the instance list.
pub enum ListRow {
    Group { name: String, count: usize, collapsed: bool },
    Instance(usize),
}

enum RowAction {
    Select(String),
    Launch(String),
    Edit(String),
    Icon(String),
    Folder(String),
    Export(String),
    SetGroup(String, String),
    Delete(String),
    ToggleGroup(String),
    ManageGroups(Option<String>),
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    if app.instances.is_empty() {
        return empty(app, ui);
    }
    // The list on the left, the selected instance's page on the right.
    let t = app.look.tokens.clone();
    egui::Panel::left("instance-list")
        .exact_size((ui.available_width() * tk::LIST_RAIL_SHARE).clamp(tk::LIST_RAIL_MIN, tk::LIST_RAIL_MAX))
        .resizable(false)
        .frame(egui::Frame::new().fill(t.background))
        .show(ui, |ui| {
            let r = ui.max_rect();
            kit::vline(ui.painter(), r.right(), r.y_range(), t.line_strong);
            filter_line(app, ui);
            for a in list(app, ui) {
                apply(app, &ctx, a);
            }
        });
    match app.selected_instance().cloned() {
        Some(inst) => detail(app, ui, &inst),
        None => {
            egui::Frame::new().inner_margin(egui::Margin::same(tk::GUTTER as i8)).show(ui, |ui| {
                kit::empty_state(ui, &t, "No instance selected", "Pick one on the left.", None);
            });
        }
    }
}

/// No instances yet: the wordmark large, what an instance is, and the two ways to get one.
fn empty(app: &mut App, ui: &mut Ui) {
    let t = app.look.tokens.clone();
    egui::Frame::new().inner_margin(egui::Margin::same(tk::GUTTER as i8)).show(ui, |ui| {
        ui.add_space((ui.available_height() * 0.2).max(tk::GUTTER));
        kit::text::display_large(ui, &t, "Asteria", tk::SIZE_DISPLAY_LARGE.min(ui.available_width() / 4.0));
        ui.add_space(tk::GUTTER);
        kit::caption(ui, &t, "an instance is a self-contained minecraft install");
        ui.add_space(tk::GUTTER);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = tk::GAP;
            if Button::new("new instance").filled().show(ui, &t).clicked() {
                app.modal = Some(Modal::NewInstance(Box::new(new_instance::NewInstance::new(&app.cfg))));
            }
            if Button::new("import").show(ui, &t).clicked() {
                app.modal = Some(Modal::NewInstance(Box::new(new_instance::NewInstance::import(&app.cfg, String::new()))));
            }
        });
    });
}

/// The list's head: the filter as a large query line (`/` takes it over) and `new`.
fn filter_line(app: &mut App, ui: &mut Ui) {
    let t = app.look.tokens.clone();
    egui::Frame::new().inner_margin(egui::Margin { left: tk::GUTTER as i8, right: tk::GUTTER as i8, top: tk::GAP as i8, bottom: tk::UNIT as i8 }).show(ui, |ui| {
        ui.allocate_ui_with_layout(vec2(ui.available_width(), tk::BUTTON_HEIGHT + tk::UNIT * 2.0), Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = tk::GAP;
            if Button::new("new").icon(g::PLUS).show(ui, &t).on_hover_text("new or import (n / i)").clicked() {
                app.modal = Some(Modal::NewInstance(Box::new(new_instance::NewInstance::new(&app.cfg))));
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let focus = std::mem::take(&mut app.focus_filter);
                let resp = kit::query(ui, &t, &mut app.filter, "Filter", tk::SIZE_HEADING, focus);
                if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    app.filter.clear();
                    resp.surrender_focus();
                }
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    // Enter in the filter selects the first match.
                    if let Some(&n) = app.visible_instances().first() {
                        app.selected = Some(app.instances[n].id.clone());
                    }
                }
            });
        });
    });
}

/// What a row says when you reach for it: the loader (by kind: its version is in the spec
/// strip), the Minecraft version and the time played, short enough for the narrow list.
fn description(inst: &Instance) -> String {
    let game = format!("{} · {}", inst.loader.kind().label(), inst.mc_version);
    match inst.played_secs {
        0 => game,
        s => format!("{game} · {}", instance::format_duration(s)),
    }
}

fn list(app: &mut App, ui: &mut Ui) -> Vec<RowAction> {
    let t = app.look.tokens.clone();
    let rows = app.list_rows();
    let groups = app.groups.names();
    let tint = |g: &str| app.groups.color(g).and_then(|c| t.named(c));
    let mut buf = std::mem::take(&mut app.group_buf);
    let mut actions = Vec::new();
    let ctx = ui.ctx().clone();
    let scroll = std::mem::take(&mut app.scroll_list);
    let focused = app.focus == Zone::List;
    let dragging = egui::DragAndDrop::payload::<DragInst>(&ctx)
        .and_then(|p| app.instances.iter().find(|i| i.id == p.0).cloned());
    // The group the pointer would drop into (its section label's rule turns accent), as
    // found last frame: the label is drawn before the rows that can be its target.
    let target_id = egui::Id::new("instance-drop-target");
    let last_target: Option<String> = ctx.data(|d| d.get_temp(target_id)).flatten();
    let mut hover_target: Option<String> = None;
    // Treat a row as a drop target for `group`. Rows hit-test on their unshifted rect.
    let drop_on = |r: &egui::Response, group: &str, actions: &mut Vec<RowAction>, hover: &mut Option<String>| {
        let Some(d) = &dragging else { return };
        if r.dnd_hover_payload::<DragInst>().is_some() && d.group != group {
            *hover = Some(group.to_string());
        }
        if let Some(p) = r.dnd_release_payload::<DragInst>()
            && d.group != group
        {
            actions.push(RowAction::SetGroup(p.0.clone(), group.to_string()));
        }
    };
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if rows.is_empty() {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(tk::GUTTER as i8, tk::GAP as i8)).show(ui, |ui| {
                kit::caption(ui, &t, "nothing matches");
            });
        }
        // Titles start a shift in from the gutter, so a hot one lands on it.
        let mut list = HoverList::begin(ui, Variant::Compact).indent(tk::GUTTER + tk::HOVER_SHIFT_COMPACT);
        let mut number = 0;
        for row in &rows {
            match row {
                ListRow::Group { name, count, collapsed } => {
                    number += 1;
                    let aside = if *collapsed { format!("{count} · folded") } else { count.to_string() };
                    let r = SectionLabel::new(number, name)
                        .aside(&aside)
                        .hue(tint(name))
                        .clickable()
                        .drop_target(last_target.as_deref() == Some(name.as_str()))
                        .show(ui, &t)
                        .on_hover_text(if *collapsed { "show" } else { "fold" });
                    drop_on(&r, name, &mut actions, &mut hover_target);
                    let g = name.clone();
                    r.context_menu(|ui| {
                        if ui.button("manage groups…").clicked() {
                            actions.push(RowAction::ManageGroups(Some(g.clone())));
                        }
                    });
                    if r.clicked() {
                        actions.push(RowAction::ToggleGroup(name.clone()));
                    }
                }
                ListRow::Instance(n) => {
                    let inst = &app.instances[*n];
                    let sel = app.selected.as_deref() == Some(inst.id.as_str());
                    let desc = description(inst);
                    let mut row = HoverRow::new(("instance", &inst.id), &inst.name)
                        .description(&desc)
                        .image(icons::source(&inst.icon), 1.0)
                        .hue(tint(&inst.group))
                        .selected(sel)
                        .hot(sel && focused)
                        .sense(Sense::click_and_drag());
                    if app.is_running(&inst.id) {
                        row = row.badge("running", t.ok());
                    }
                    let r = list.row(ui, &t, row);
                    r.dnd_set_drag_payload(DragInst(inst.id.clone()));
                    drop_on(&r, &inst.group, &mut actions, &mut hover_target);
                    if sel && scroll {
                        r.scroll_to_me(None);
                    }
                    if r.clicked() {
                        actions.push(RowAction::Select(inst.id.clone()));
                    }
                    if r.double_clicked() {
                        actions.push(RowAction::Launch(inst.id.clone()));
                    }
                    let id = inst.id.clone();
                    let current = inst.group.clone();
                    r.context_menu(|ui| {
                        actions.push(RowAction::Select(id.clone()));
                        if ui.button("play").clicked() {
                            actions.push(RowAction::Launch(id.clone()));
                        }
                        if ui.button("edit…").clicked() {
                            actions.push(RowAction::Edit(id.clone()));
                        }
                        if ui.button("change icon…").clicked() {
                            actions.push(RowAction::Icon(id.clone()));
                        }
                        if ui.button("open folder").clicked() {
                            actions.push(RowAction::Folder(id.clone()));
                        }
                        if ui.button("export…").clicked() {
                            actions.push(RowAction::Export(id.clone()));
                        }
                        ui.menu_button("move to group", |ui| {
                            for g in &groups {
                                if *g != current && ui.button(g).clicked() {
                                    actions.push(RowAction::SetGroup(id.clone(), g.clone()));
                                }
                            }
                            if !current.is_empty() && ui.button(RichText::new("no group").color(t.text_muted)).clicked() {
                                actions.push(RowAction::SetGroup(id.clone(), String::new()));
                            }
                            if ui.button("manage groups…").clicked() {
                                actions.push(RowAction::ManageGroups(Some(current.clone()).filter(|g| !g.is_empty())));
                            }
                            ui.separator();
                            let r = ui.add(egui::TextEdit::singleline(&mut buf).hint_text("new group").desired_width(160.0));
                            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !buf.trim().is_empty() {
                                actions.push(RowAction::SetGroup(id.clone(), buf.trim().to_string()));
                                buf.clear();
                                ui.close();
                            }
                        });
                        ui.separator();
                        if ui.button(RichText::new("delete…").color(t.error())).clicked() {
                            actions.push(RowAction::Delete(id.clone()));
                        }
                    });
                }
            }
        }
        list.end(ui);

        // A place to drop on to leave a group, while dragging a grouped instance: a framed
        // row, accent while the pointer is over it.
        if let Some(d) = dragging.as_ref().filter(|d| !d.group.is_empty()) {
            ui.add_space(tk::GAP);
            let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), tk::LIST_ROW_HEIGHT_COMPACT), Sense::hover());
            let on = r.dnd_hover_payload::<DragInst>().is_some();
            let rect = rect.shrink2(vec2(tk::GUTTER, 0.0));
            let c = crate::design::motion::color(&ctx, r.id, on, t.line_strong, t.accent);
            kit::rule::frame(ui.painter(), rect, c);
            let ink = if on { t.accent } else { t.text_muted };
            kit::text::paint_caption(ui.painter(), egui::pos2(rect.left() + tk::GAP, rect.center().y), &format!("drop here to leave {}", d.group), ink);
            if let Some(p) = r.dnd_release_payload::<DragInst>() {
                actions.push(RowAction::SetGroup(p.0.clone(), String::new()));
            }
        }
    });
    ctx.data_mut(|d| d.insert_temp(target_id, hover_target.clone()));
    if hover_target != last_target {
        ctx.request_repaint();
    }

    // What's being dragged follows the pointer: its icon and name in a hairline frame.
    if let Some(d) = &dragging {
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        if let Some(pos) = ctx.pointer_interact_pos() {
            egui::Area::new(egui::Id::new("drag-ghost"))
                .order(egui::Order::Tooltip)
                .fixed_pos(pos + egui::vec2(14.0, 10.0))
                .interactable(false)
                .show(&ctx, |ui| {
                    egui::Frame::new().fill(t.background).stroke(egui::Stroke::new(tk::LINE, t.line_strong)).inner_margin(egui::Margin::symmetric(tk::GAP as i8, tk::UNIT as i8)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = tk::UNIT;
                            ui.add(egui::Image::new(icons::source(&d.icon)).corner_radius(0).fit_to_exact_size(egui::vec2(tk::ICON_SIZE, tk::ICON_SIZE)));
                            kit::heading_small(ui, &t, &d.name);
                        });
                    });
                });
        }
    }
    app.group_buf = buf;
    actions
}

fn apply(app: &mut App, ctx: &egui::Context, a: RowAction) {
    let find = |app: &App, id: &str| app.instances.iter().find(|i| i.id == id).cloned();
    match a {
        RowAction::Select(id) => {
            app.selected = Some(id);
            app.focus = Zone::List;
        }
        RowAction::Launch(id) => {
            app.selected = Some(id.clone());
            app.launch(ctx, &id, None);
        }
        RowAction::Edit(id) => {
            if let Some(i) = find(app, &id) {
                app.modal = Some(Modal::Edit(Box::new(EditState::from(&i))));
            }
        }
        RowAction::Icon(id) => app.modal = Some(Modal::IconPicker(Box::new(super::icon_picker::IconPicker::new(id)))),
        RowAction::Folder(id) => {
            if let Some(i) = find(app, &id) {
                super::open_path(&i.game_dir());
            }
        }
        RowAction::Export(id) => app.start_export_dialog(ctx, &id),
        RowAction::SetGroup(id, group) => {
            if let Some(i) = app.instances.iter_mut().find(|i| i.id == id) {
                i.group = group;
                if let Err(e) = i.save() {
                    app.toast(ToastKind::Error, format!("Save failed: {e}"));
                }
            }
        }
        RowAction::Delete(id) => app.confirm_delete_instance(&id),
        RowAction::ManageGroups(g) => app.modal = Some(Modal::Groups(Box::new(super::group_manager::GroupManager::new(g)))),
        RowAction::ToggleGroup(name) => {
            let c = &mut app.cfg.collapsed_groups;
            if let Some(p) = c.iter().position(|g| *g == name) {
                c.remove(p);
            } else {
                c.push(name);
            }
            let _ = app.cfg.save();
        }
    }
}

fn detail(app: &mut App, ui: &mut Ui, inst: &Instance) {
    // The spec strip, the search banner and the section tabs share their hairlines: no
    // spacing between them (the header sets it to 0 before the strip).
    let spacing = ui.spacing().item_spacing.y;
    header(app, ui, inst);
    // The search banner, when there is one, shares the spec strip's line and the tabs' line.
    super::mod_sets::banner(app, ui, inst);
    sections(app, ui);
    ui.spacing_mut().item_spacing.y = spacing;
    // Panes with heads and hover lists run to the page's edges (their rules and rows reach the
    // lines); the overview sits on the gutter, unless the version picker's panes take it over.
    let side = if app.inst_tab == InstTab::Overview && !app.version.picker_open() { tk::GUTTER } else { 0.0 };
    let fade = view_fade(ui, app.inst_tab);
    egui::Frame::new().inner_margin(egui::Margin { left: side as i8, right: side as i8, top: tk::GAP as i8, bottom: tk::GAP as i8 }).show(ui, |ui| {
        ui.multiply_opacity(fade);
        // Files dropped now go into this tab's folder: its pane is what the drop hint frames.
        app.drop_rect = Some(ui.available_rect_before_wrap().expand2(vec2(side, tk::GAP)));

        match app.inst_tab {
            InstTab::Overview => overview(app, ui, inst),
            InstTab::Log => logs::live(app, ui, inst),
            InstTab::Config => config::show(app, ui, inst),
            InstTab::Worlds => worlds::show(app, ui, inst),
            InstTab::Servers => servers::show(app, ui, inst),
            InstTab::Screenshots => screenshots::show(app, ui, inst),
            InstTab::LogHistory => logs::history(app, ui, inst),
            tab => content::show(app, ui, inst, tab.content_kind().unwrap()),
        }
    });
}

/// 0 → 1 as a newly opened view (a section or a sub-tab) fades in, so switching doesn't jolt.
/// The first view shown, and a different instance on the same view, are there at once.
fn view_fade(ui: &Ui, tab: InstTab) -> f32 {
    let key = egui::Id::new("asteria-view");
    let (last, n) = ui.ctx().data(|d| d.get_temp::<(InstTab, u64)>(key)).unwrap_or((tab, 0));
    let n = if last == tab { n } else { n + 1 };
    ui.ctx().data_mut(|d| d.insert_temp(key, (tab, n)));
    if n == 0 {
        return 1.0;
    }
    kit::sheet::entrance_with(ui, key.with(n), Motion::Fast)
}

/// The section tabs (hairline cells, the open one marked), then the open section's sub-tabs
/// where it has more than one view. A section opens on the view it was last left on.
fn sections(app: &mut App, ui: &mut Ui) {
    let t = app.look.tokens.clone();
    let open = Section::of(app.inst_tab);
    app.section_leaf[open.index()] = Some(app.inst_tab);
    // Numbered cells with full padding if they fit, else tighter, else without numbers.
    let cells = |numbers: bool, pad: f32| -> Vec<kit::BarCell<'static>> {
        Section::ALL
            .iter()
            .enumerate()
            .map(|(i, (s, label))| {
                let c = kit::BarCell::new(label).open(*s == open).padding(pad);
                if numbers { c.number(i + 1) } else { c }
            })
            .collect()
    };
    let room = ui.available_width();
    let fits = |cs: &[kit::BarCell]| cs.iter().map(|c| c.width(ui, &t)).sum::<f32>() <= room;
    let choice = [(true, tk::BAR_CELL_PADDING), (true, tk::GAP), (false, tk::GAP), (false, tk::UNIT)]
        .into_iter()
        .map(|(n, p)| cells(n, p))
        .find(|cs| fits(cs))
        .unwrap_or_else(|| cells(false, tk::UNIT));
    let clicked = kit::bar(ui, &t, |ui| choice.into_iter().enumerate().find_map(|(i, c)| c.show(ui, &t).clicked().then_some(i)));
    if let Some(i) = clicked {
        let s = Section::ALL[i].0;
        if s != open {
            app.inst_tab = app.section_leaf[i].unwrap_or(s.leaves()[0].0);
        }
    }
    let leaves = open.leaves();
    if leaves.len() > 1 {
        egui::Frame::new().inner_margin(egui::Margin { left: tk::GUTTER as i8, right: tk::GUTTER as i8, top: tk::GAP as i8, bottom: 0 }).show(ui, |ui| {
            ui.horizontal(|ui| {
                kit::segmented(ui, &t, &mut app.inst_tab, leaves);
            });
        });
    }
}

/// The instance page's head: a caption line (the section number, the loader and version, the
/// icon), the name large with play, edit and folder, then the spec strip.
fn header(app: &mut App, ui: &mut Ui, inst: &Instance) {
    let t = app.look.tokens.clone();
    let disk = app.sizes.get(inst, &app.rt, ui.ctx());
    let hue = app.groups.color(&inst.group).and_then(|c| t.named(c));
    let width = ui.available_width();
    egui::Frame::new().inner_margin(egui::Margin { left: tk::GUTTER as i8, right: tk::GUTTER as i8, top: tk::GAP as i8, bottom: tk::GAP as i8 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = tk::GAP;
            kit::caption(ui, &t, "01 / instance");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if caption_link(ui, &t, "change icon").clicked() {
                    app.modal = Some(Modal::IconPicker(Box::new(super::icon_picker::IconPicker::new(inst.id.clone()))));
                }
                kit::caption(ui, &t, &format!("{} · {}", inst.loader.display(), inst.mc_version));
            });
        });
        ui.add_space(tk::UNIT);
        // Buttons first, from the right, then the name in the room left.
        let size = if width < 900.0 { tk::SIZE_DISPLAY_SMALL } else { tk::SIZE_DISPLAY };
        ui.allocate_ui_with_layout(vec2(ui.available_width(), size), Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = tk::GAP;
            if Button::new("folder").show(ui, &t).clicked() {
                super::open_path(&inst.game_dir());
            }
            if Button::new("edit").show(ui, &t).clicked() {
                app.modal = Some(Modal::Edit(Box::new(EditState::from(inst))));
            }
            if app.is_running(&inst.id) {
                if Button::new("stop").icon(g::STOP).filled().show(ui, &t).clicked() {
                    app.kill(&inst.id);
                }
            } else if Button::new("play").icon(g::PLAY).filled().show(ui, &t).clicked() {
                app.launch(ui.ctx(), &inst.id, None);
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                kit::display(ui, &t, &inst.name, size);
            });
        });
    });
    let none = "—";
    let played = instance::format_duration(inst.played_secs);
    let last = kit::text::ago(inst.last_played);
    let size = disk.map(kit::text::size_label).unwrap_or_else(|| "measuring".into());
    let group = if inst.group.is_empty() { none } else { &inst.group };
    let share = if inst.share_set.is_empty() { none } else { &inst.share_set };
    let worlds = if inst.world_set.is_empty() { none } else { &inst.world_set };
    // The group and the sets are set in edit, so those cells lead there.
    let cells = [
        Spec::new("minecraft", &inst.mc_version),
        Spec::new("group", group).link().hue(hue.filter(|_| !inst.group.is_empty())),
        Spec::new("share set", share).link(),
        Spec::new("world set", worlds).link(),
        Spec::new("played", &played),
        Spec::new("last played", &last),
        Spec::new("on disk", &size),
    ];
    // As many cells as fit, about 110px each; the first ones matter most.
    let fit = ((width / 110.0) as usize).clamp(3, cells.len());
    ui.spacing_mut().item_spacing.y = 0.0;
    if kit::spec_strip(ui, &t, &cells[..fit]).is_some() {
        app.modal = Some(Modal::Edit(Box::new(EditState::from(inst))));
    }
}

/// A caption that leads somewhere: muted, accent when hovered.
fn caption_link(ui: &mut Ui, t: &tk::Tokens, text: &str) -> egui::Response {
    let g = kit::text::caption_galley(ui, text, t.text_muted);
    let (rect, resp) = ui.allocate_exact_size(g.size(), Sense::click());
    let c = crate::design::motion::color(ui.ctx(), resp.id, resp.hovered() || resp.has_focus(), t.text_muted, t.accent);
    kit::text::paint_caption(ui.painter(), rect.left_center(), text, c);
    let label = text.to_string();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, true, &label));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Overview: the components (and changing them) on the left, the notes on the right, with a
/// hairline between. The version picker takes over the whole section while it's open.
fn overview(app: &mut App, ui: &mut Ui, inst: &Instance) {
    if app.version.picker_open() {
        return version::show(app, ui, inst);
    }
    let t = app.look.tokens.clone();
    let area = ui.available_rect_before_wrap();
    // Too narrow for two columns: the components, then the notes under them.
    if area.width() < tk::OVERVIEW_TWO_COLUMNS {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            version::show(app, ui, inst);
            ui.add_space(tk::GUTTER);
            kit::rule(ui, &t, false);
            ui.add_space(tk::GUTTER);
            notes(app, ui, inst, false);
        });
        return;
    }
    let left_w = ((area.width() - tk::GUTTER * 2.0) * 0.5).min(tk::OVERVIEW_COLUMN_MAX).floor();
    let line_x = area.left() + left_w + tk::GUTTER;
    let left = egui::Rect::from_min_max(area.min, egui::pos2(area.left() + left_w, area.bottom()));
    let right = egui::Rect::from_min_max(egui::pos2(line_x + tk::GUTTER, area.top()), area.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(left).layout(Layout::top_down(Align::Min)), |ui| version::show(app, ui, inst));
    kit::vline(ui.painter(), line_x, area.y_range(), t.line);
    ui.scope_builder(egui::UiBuilder::new().max_rect(right).layout(Layout::top_down(Align::Min)), |ui| notes(app, ui, inst, true));
    ui.allocate_rect(area, Sense::hover());
}

/// The instance's notes, saved as they're typed. The side zone: `enter` writes. `fill`: down
/// to the foot of the column, else a few lines (under the components, in a scrolling page).
fn notes(app: &mut App, ui: &mut Ui, inst: &Instance, fill: bool) {
    let t = app.look.tokens.clone();
    let write = app.nav_for(Zone::Side) == Some(Nav::Activate);
    let focused = app.focus == Zone::Side;
    kit::caption(ui, &t, "notes");
    ui.add_space(tk::UNIT);
    let Some(i) = app.instances.iter_mut().find(|i| i.id == inst.id) else { return };
    let r = kit::field::notes_sized(ui, &t, &mut i.notes, "Anything worth remembering: seeds, coordinates, server addresses, what you changed…", 6, if fill { ui.available_height() } else { 0.0 });
    // The pane has the keyboard: its frame says so, as the field's own focus does.
    if focused && !r.has_focus() {
        kit::rule::frame(ui.painter(), r.rect, t.accent);
    }
    if write {
        r.request_focus();
    }
    if r.clicked() || r.has_focus() {
        app.focus = Zone::Side;
    }
    if r.changed()
        && let Err(e) = i.save()
    {
        app.toast(ToastKind::Error, format!("Couldn't save notes: {e}"));
    }
}

// ---------------------------------------------------------------------------

pub struct EditState {
    pub id: String,
    name: String,
    group: String,
    /// The share set and the world set, in `Kind::ALL` order.
    sets: [SetField; 2],
    memory_mb: u32,
    custom_memory: bool,
    jvm_args: String,
    java_path: String,
    wrapper_command: String,
    custom_window: bool,
    window: (u32, u32),
}

/// A share set or world set field in the edit dialog.
struct SetField {
    kind: Kind,
    value: String,
    /// The set when the dialog opened, and what leaving it would copy.
    old: String,
    leave_size: Option<crate::task::Promise<u64>>,
    leave_bytes: Option<u64>,
}

impl SetField {
    fn new(kind: Kind, i: &Instance) -> Self {
        Self { kind, value: kind.set_of(i).to_string(), old: kind.set_of(i).to_string(), leave_size: None, leave_bytes: None }
    }
}

impl EditState {
    pub fn from(i: &Instance) -> Self {
        Self {
            id: i.id.clone(),
            name: i.name.clone(),
            group: i.group.clone(),
            sets: Kind::ALL.map(|k| SetField::new(k, i)),
            memory_mb: i.memory_mb.unwrap_or(4096),
            custom_memory: i.memory_mb.is_some(),
            jvm_args: i.jvm_args.clone(),
            java_path: i.java_path.clone(),
            wrapper_command: i.wrapper_command.clone(),
            custom_window: i.window.is_some(),
            window: i.window.unwrap_or((1280, 720)),
        }
    }
}

pub enum EditOutcome {
    Keep,
    Close,
    Delete,
}

pub fn edit(app: &mut App, s: &mut EditState, ui: &mut Ui) -> EditOutcome {
    let t = app.look.tokens.clone();
    let activate = app.modal_nav() == Some(Nav::Activate);
    ui.set_max_width(ui.available_width().min(tk::SHEET_WIDTH));
    ui.spacing_mut().item_spacing.y = 0.0;
    kit::heading(ui, &t, "Edit instance");
    ui.add_space(tk::GUTTER);

    let groups = app.groups.names();
    let sets = Kind::ALL.map(|k| app.all_sets(k));
    let foot = tk::LINE + tk::GAP + tk::BUTTON_HEIGHT + tk::GUTTER;
    let control = |ui: &Ui| ui.available_width().min(tk::SETTING_CONTROL_WIDTH);
    egui::ScrollArea::vertical().id_salt("edit-instance").max_height((ui.available_height() - foot).max(tk::SETTING_ROW_HEIGHT * 3.0)).auto_shrink([false, true]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        kit::setting_group(ui, &t, 1, "Instance", "", |ui| {
            SettingRow::new("Name").show(ui, &t, |ui| {
                let w = control(ui);
                kit::Field::new(&mut s.name).hint("Instance name").width(w).show(ui, &t);
            });
            SettingRow::new("Group").show(ui, &t, |ui| {
                let w = control(ui);
                new_instance::group_field(ui, &t, egui::Id::new("edit-group"), &mut s.group, &groups, "no group", "No group", w);
            });
        });
        kit::setting_group(ui, &t, 2, "Sharing", "", |ui| {
            SettingRow::new("Share set").description("Instances in the same share set share resource packs, shader packs and schematics.").show(ui, &t, |ui| {
                let w = control(ui);
                new_instance::group_field(ui, &t, egui::Id::new("edit-share-set"), &mut s.sets[0].value, &sets[0], "not shared", "Not shared", w);
            });
            SettingRow::new("World set").description("Instances in the same world set share their worlds, and play the same Minecraft version and loader.").show(ui, &t, |ui| {
                let w = control(ui);
                new_instance::group_field(ui, &t, egui::Id::new("edit-world-set"), &mut s.sets[1].value, &sets[1], "not shared", "Not shared", w);
            });
        });
        // What changing a set does to the files, before saving.
        let warnings: Vec<_> = s.sets.iter_mut().filter_map(|f| share_warning(app, &s.id, f, ui.ctx())).collect();
        if !warnings.is_empty() {
            content::padded(ui, |ui| {
                ui.set_max_width(ui.available_width().min(tk::SETTING_DESCRIPTION_WIDTH));
                for (caption, text, color) in warnings {
                    ui.add_space(tk::GAP);
                    kit::bracketed(ui, &t, caption, |ui| kit::text::body_colored(ui, &text, color.unwrap_or(t.text_strong)));
                }
                ui.add_space(tk::GAP);
            });
            kit::rule(ui, &t, true);
        }
        let default_mem = app.cfg.default_memory_mb;
        kit::setting_group(ui, &t, 3, "Game", "", |ui| {
            let caption = if s.custom_memory { format!("default {default_mem} MiB") } else { format!("default · {default_mem} MiB") };
            SettingRow::new("Memory").caption(&caption).show(ui, &t, |ui| {
                // The slider (when on) then its switch, in reading order.
                let slider_w = (control(ui) - tk::SWITCH_WIDTH - tk::GAP).max(tk::GUTTER * 4.0);
                let total = if s.custom_memory { slider_w + tk::GAP + tk::SWITCH_WIDTH } else { tk::SWITCH_WIDTH };
                kit::in_order(ui, total, |ui| {
                    if s.custom_memory {
                        let mut v = s.memory_mb as f64;
                        kit::Slider::new(&mut v, 1024.0, 32768.0).step(512.0).value_text(format!("{} MiB", s.memory_mb)).mark(default_mem as f64).width(slider_w).show(ui, &t);
                        s.memory_mb = v as u32;
                    }
                    if kit::switch(ui, &t, s.custom_memory, true, false, "own memory").clicked() {
                        s.custom_memory = !s.custom_memory;
                    }
                });
            });
            let caption = if s.custom_window { "width × height" } else { "game default" };
            SettingRow::new("Window").caption(caption).show(ui, &t, |ui| {
                // Width × height, then the switch, in reading order.
                let x_w = kit::text::caption_galley(ui, "×", t.text_muted).size().x;
                let w = ((control(ui) - tk::SWITCH_WIDTH - x_w - tk::GAP * 3.0) / 2.0).max(tk::GUTTER * 2.0);
                let total = if s.custom_window { w * 2.0 + x_w + tk::GAP * 3.0 + tk::SWITCH_WIDTH } else { tk::SWITCH_WIDTH };
                kit::in_order(ui, total, |ui| {
                    if s.custom_window {
                        config::number(ui, &t, &mut s.window.0, 320..=7680, 1.0, false, w);
                        ui.add(egui::Label::new(kit::text::caption_job("×", t.text_muted)).selectable(false));
                        config::number(ui, &t, &mut s.window.1, 240..=4320, 1.0, false, w);
                    }
                    if kit::switch(ui, &t, s.custom_window, true, false, "own window size").clicked() {
                        s.custom_window = !s.custom_window;
                    }
                });
            });
            SettingRow::new("JVM arguments").caption("added after Asteria's own").show(ui, &t, |ui| {
                let w = control(ui);
                kit::Field::new(&mut s.jvm_args).hint("-XX:+UseZGC").width(w).show(ui, &t);
            });
            SettingRow::new("Java").caption("a java binary").show(ui, &t, |ui| {
                let w = control(ui);
                kit::Field::new(&mut s.java_path).hint("Mojang's runtime for the version").width(w).show(ui, &t);
            });
            let global = match app.cfg.wrapper_command.trim() {
                "" => "None".to_string(),
                g => format!("Default: {g}"),
            };
            SettingRow::new("Wrapper").caption("like prime-run or gamemoderun").show(ui, &t, |ui| {
                let w = control(ui);
                kit::Field::new(&mut s.wrapper_command).hint(&global).width(w).show(ui, &t);
            });
        });
    });

    ui.add_space(tk::GUTTER);
    let mut out = EditOutcome::Keep;
    let k = |a| app.keymap.label(a);
    let hints = [(k(crate::keymap::Action::Activate), "save")];
    let clicked = content::sheet_foot(ui, &t, &hints, &[Button::new("save").filled(), Button::new("cancel"), Button::new("delete").icon(g::TRASH)]);
    match clicked {
        Some(1) => out = EditOutcome::Close,
        Some(2) => out = EditOutcome::Delete,
        _ => {}
    }
    let save = clicked == Some(0) || activate || ui.input(|i| i.key_pressed(egui::Key::Enter));
    let new: [String; 2] = [0, 1].map(|n| s.sets[n].value.trim().to_string());
    let changed: Vec<Kind> = Kind::ALL.into_iter().zip(&new).filter(|(k, v)| app.instances.iter().any(|i| i.id == s.id && k.set_of(i) != v.as_str())).map(|(k, _)| k).collect();
    if save && let Err(e) = crate::launch::split_args(&s.jvm_args) {
        app.toast(ToastKind::Error, format!("JVM arguments: {e}"));
    } else if save && let Err(e) = crate::launch::split_args(&s.wrapper_command) {
        app.toast(ToastKind::Error, format!("wrapper command: {e}"));
    } else if save && let Some(e) = set_change_error(app, s, &new, &changed) {
        app.toast(ToastKind::Error, e);
    } else if save {
        if let Some(i) = app.instances.iter_mut().find(|i| i.id == s.id) {
            if !s.name.trim().is_empty() {
                i.name = s.name.trim().to_string();
            }
            i.group = s.group.trim().to_string();
            i.memory_mb = s.custom_memory.then_some(s.memory_mb);
            i.window = s.custom_window.then_some(s.window);
            i.jvm_args = s.jvm_args.trim().to_string();
            i.java_path = s.java_path.trim().to_string();
            i.wrapper_command = s.wrapper_command.trim().to_string();
            [i.share_set, i.world_set] = new;
            if let Err(e) = i.save() {
                app.toast(ToastKind::Error, format!("Save failed: {e}"));
            } else if !changed.is_empty() {
                let i = i.clone();
                app.start_sharing(ui.ctx(), vec![i]);
            }
        }
        out = EditOutcome::Close;
    }
    out
}

/// Why the edit dialog can't save these set changes, if it can't.
fn set_change_error(app: &App, s: &EditState, new: &[String; 2], changed: &[Kind]) -> Option<String> {
    for &kind in changed {
        let set = &new[kind as usize];
        let old = &s.sets[kind as usize].old;
        if let Some(e) = (!set.is_empty()).then(|| crate::shares::check_set_name(set).err()).flatten() {
            return Some(format!("{e}"));
        }
        if let Some(why) = app.instance_busy(&s.id) {
            return Some(format!("Can't change the {} while {why}", kind.noun()));
        }
        if let Some(why) = [old, set].into_iter().find_map(|set| app.set_busy(kind, set, &s.id)) {
            return Some(format!("Can't change the {} while {why}", kind.noun()));
        }
        if kind == Kind::Worlds
            && let Some(inst) = app.instances.iter().find(|i| i.id == s.id)
            && let Some(why) = crate::world_sets::mismatch(inst, set, &app.instances)
        {
            return Some(format!("{why}. Change this instance's version to match first."));
        }
    }
    None
}

/// The instances in set `set` of this kind, and whether each is only still linked in
/// (it left on paper, but the leave hasn't finished).
pub fn set_members<'a>(kind: Kind, set: &'a str, instances: &'a [Instance], links: &'a std::collections::BTreeMap<String, Vec<String>>) -> impl Iterator<Item = (&'a Instance, bool)> {
    let linked = links.get(set);
    instances.iter().filter_map(move |i| match (!set.is_empty() && kind.set_of(i) == set, !set.is_empty() && linked.is_some_and(|l| l.contains(&i.id))) {
        (true, _) => Some((i, false)),
        (false, true) => Some((i, true)),
        (false, false) => None,
    })
}

/// What changing a set will do to the files, before saving: a caption, the paragraph and
/// its colour (clay when the change can't be made).
fn share_warning(app: &App, id: &str, f: &mut SetField, ctx: &egui::Context) -> Option<(&'static str, String, Option<egui::Color32>)> {
    let t = &app.look.tokens;
    let new = f.value.trim();
    if new == f.old {
        return None;
    }
    let old = f.old.clone();
    let what = match f.kind {
        Kind::Packs => "resource packs, shader packs and schematics",
        Kind::Worlds => "worlds",
    };
    let mismatch = (f.kind == Kind::Worlds).then(|| app.instances.iter().find(|i| i.id == id)).flatten().and_then(|i| crate::world_sets::mismatch(i, new, &app.instances));
    Some(if let Some(why) = mismatch {
        ("can't join that world set", format!("{why}."), Some(t.error()))
    } else if new.is_empty() {
        if f.leave_size.is_none() && f.leave_bytes.is_none() {
            let (kind, dir) = (f.kind, crate::shares::set_dir(f.kind, &old));
            f.leave_size = Some(crate::task::Promise::spawn(&app.rt, ctx, async move {
                Ok(tokio::task::spawn_blocking(move || kind.folders().iter().map(|f| crate::fsutil::tree_size(&dir.join(f))).sum()).await?)
            }));
        }
        if let Some(Ok(n)) = f.leave_size.as_ref().and_then(|p| p.take()) {
            f.leave_bytes = Some(n);
        }
        let size = f.leave_bytes.map(kit::text::size_label).unwrap_or_else(|| "measuring".into());
        (
            if f.kind == Kind::Packs { "leaving the share set" } else { "leaving the world set" },
            format!("Leaving “{old}” gives this instance its own copy of the set's {what} ({size}). Free on btrfs or xfs; on other filesystems it takes that much space again."),
            None,
        )
    } else if old.is_empty() {
        let more = match f.kind {
            Kind::Packs => "",
            Kind::Worlds => " Before each launch, worlds last played with mods this instance lacks are snapshotted first.",
        };
        (if f.kind == Kind::Packs { "joining a share set" } else { "joining a world set" }, format!("This instance's {what} move into “{new}”, shared with its other instances. Name clashes keep both.{more}"), None)
    } else {
        (if f.kind == Kind::Packs { "changing share set" } else { "changing world set" }, format!("This instance's {what} stay with “{old}”; it uses “{new}”'s instead."), None)
    })
}

impl App {
    /// Why the instance can't be deleted right now, if it can't: its game or
    /// downloads would carry on in a folder that no longer exists.
    pub fn instance_busy(&self, id: &str) -> Option<&'static str> {
        if self.is_running(id) {
            Some("it's running")
        } else if self.content.busy_with(id) {
            Some("content is installing")
        } else if self.exports.iter().any(|(.., i)| i == id) {
            Some("it's exporting")
        } else if self.worlds.busy_with(id) {
            Some("a world is being copied")
        } else if self.sharing.is_some() {
            Some("shared folders are moving")
        } else {
            None
        }
    }

    /// Why set `set`'s folders can't change right now: another instance in it is
    /// exporting them or installing into them, or (for worlds) duplicating or playing
    /// one, which a leave would copy halfway. `changing` is the instance asking.
    pub fn set_busy(&self, kind: Kind, set: &str, changing: &str) -> Option<String> {
        set_members(kind, set, &self.instances, self.set_links(kind)).find_map(|(i, _)| {
            if self.exports.iter().any(|(.., id)| *id == i.id) {
                Some(format!("{} is exporting", i.name))
            } else if self.content.busy_with(&i.id) {
                Some(format!("{} is installing content", i.name))
            } else if kind == Kind::Worlds && self.worlds.busy_with(&i.id) {
                Some(format!("{} is copying a world", i.name))
            } else if kind == Kind::Worlds && i.id != changing && self.is_running(&i.id) {
                Some(format!("{} is running", i.name))
            } else {
                None
            }
        })
    }

    pub fn confirm_delete_instance(&mut self, id: &str) {
        let name = self.instances.iter().find(|i| i.id == id).map(|i| i.name.clone()).unwrap_or_default();
        if let Some(why) = self.instance_busy(id) {
            self.toast(ToastKind::Error, format!("Can't delete {name} while {why}"));
            return;
        }
        self.modal = Some(Modal::Confirm(Confirm {
            title: "delete instance".into(),
            body: format!("Permanently delete “{name}” and all of its worlds, mods and settings, and their backups?"),
            button: "delete".into(),
            action: ConfirmAction::DeleteInstance(id.to_string()),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loaders::Loader;

    #[test]
    fn every_view_is_in_one_section_and_brackets_reach_them_all() {
        let all: Vec<InstTab> = InstTab::all().collect();
        assert_eq!(all.len(), 10);
        for tab in &all {
            assert_eq!(all.iter().filter(|t| *t == tab).count(), 1, "{tab:?} listed once");
            assert!(Section::of(*tab).leaves().iter().any(|(l, _)| l == tab));
        }
        // `]` walks every view section by section and wraps; `[` walks back.
        let mut tab = InstTab::Overview;
        let mut seen = vec![tab];
        for _ in 1..all.len() {
            tab = tab.step(1);
            seen.push(tab);
        }
        assert_eq!(seen, all);
        assert_eq!(tab.step(1), InstTab::Overview);
        assert_eq!(InstTab::Overview.step(-1), InstTab::LogHistory);
        let sections: Vec<Section> = seen.iter().map(|t| Section::of(*t)).collect();
        assert!(sections.windows(2).all(|w| w[0].index() <= w[1].index()), "sections stay in order");
    }

    #[test]
    fn set_members_count_instances_still_linked_in() {
        let inst = |id: &str, set: &str| {
            let mut i = Instance::new(id, "1.21.1", Loader::Vanilla);
            i.id = id.into();
            i.share_set = set.into();
            i
        };
        // b left "packs" on paper, but its leave hasn't finished: its folders still link in.
        let all = [inst("a", "packs"), inst("b", ""), inst("c", "other"), inst("d", "")];
        let links = [("packs".to_string(), vec!["a".to_string(), "b".to_string()])].into();
        let got: Vec<(&str, bool)> = set_members(Kind::Packs, "packs", &all, &links).map(|(i, leaving)| (i.id.as_str(), leaving)).collect();
        assert_eq!(got, [("a", false), ("b", true)]);
        // So an export or install in b holds up changes to the set, as one in a does.
        assert_eq!(set_members(Kind::Packs, "other", &all, &links).count(), 1);
        assert_eq!(set_members(Kind::Packs, "", &all, &links).count(), 0);
    }
}
