//! The running game's output, and past logs and crash reports, with what a crash
//! analysis ([`crate::crash`]) makes of them. Logs are for reading, so they're quiet: raw lines
//! in JetBrains Mono (an approved exception) in a 1px frame, the level as a caption in a narrow
//! column, errors in clay and warnings in ochre.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use egui::{Color32, Rect, Ui, vec2};

use super::content::{self, padded};
use super::nav::{Nav, Zone};
use super::{App, ToastKind};
use crate::crash::{Analysis, Confidence};
use crate::design::fonts;
use crate::design::kit;
use crate::design::tokens::{self as tk, Tokens};
use crate::instance::Instance;
use crate::keymap::Action;
use crate::launch::{self, LogLevel, LogLine};
use crate::task::Promise;

/// Where the log view should go: a line (in the unfiltered log) to scroll to and mark.
#[derive(Clone, Copy, Default)]
pub struct Mark {
    pub line: Option<usize>,
    /// Scroll there this frame (once, after a click or key).
    pub jump: bool,
}

/// A level's caption and its text colour.
fn level_style(t: &Tokens, level: LogLevel) -> (&'static str, Color32, Color32) {
    match level {
        LogLevel::Launcher => ("asteria", t.text_strong, t.text_strong),
        LogLevel::Info => ("", t.text_muted, t.text_strong),
        LogLevel::Warn => ("warn", t.warning(), t.warning()),
        LogLevel::Error => ("error", t.error(), t.error()),
        LogLevel::Debug => ("", t.text_muted, t.text_muted),
    }
}

/// Log lines in a 1px frame filling the rest of the pane (accent while it has the keyboard),
/// virtualised: only the rows on screen are laid out. Each row: the level as a caption in a
/// narrow column, then the line in JetBrains Mono, clipped. `scroll` comes from the keyboard;
/// `mark` is an index into `lines`, washed and marked at its left edge.
pub fn render_lines(ui: &mut Ui, t: &Tokens, lines: &[&LogLine], stick_to_bottom: bool, scroll: Option<Nav>, focused: bool, mark: Mark) {
    let row_h = tk::LOG_LINE_HEIGHT;
    let frame_c = if focused { t.accent } else { t.line_strong };
    let outer = ui.available_rect_before_wrap();
    let outer = Rect::from_min_max(egui::pos2(outer.left() + tk::GUTTER, outer.top()), egui::pos2(outer.right() - tk::GUTTER, outer.bottom() - tk::GAP));
    ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(outer.shrink(tk::LINE)).layout(egui::Layout::top_down(egui::Align::Min)));
    let page = outer.height() * 0.9;
    let mut area = egui::ScrollArea::vertical().id_salt("log-lines").auto_shrink(false).stick_to_bottom(stick_to_bottom);
    if let (Some(l), true) = (mark.line, mark.jump) {
        // A few lines of context above the marked one.
        area = area.vertical_scroll_offset((l.saturating_sub(4)) as f32 * row_h);
    }
    area.show_rows(&mut inner, row_h, lines.len(), |ui, range| {
        if let Some(d) = scroll.and_then(|n| n.scroll_delta(row_h, page)) {
            ui.scroll_with_delta(vec2(0.0, d));
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        let font = fonts::raw(tk::SIZE_MONO);
        for (i, line) in lines[range.clone()].iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), row_h), egui::Sense::hover());
            let p = ui.painter_at(rect);
            if mark.line == Some(range.start + i) {
                p.rect_filled(rect, 0.0, t.wash(0.08));
                p.rect_filled(Rect::from_min_size(rect.min, vec2(tk::MARKER, rect.height())), 0.0, t.accent);
            }
            let (cap, cap_c, text_c) = level_style(t, line.level);
            if !cap.is_empty() {
                kit::text::paint_caption(&p, egui::pos2(rect.left() + tk::GAP, rect.center().y), cap, cap_c);
            }
            let x = rect.left() + tk::GAP + tk::LOG_LEVEL_WIDTH;
            let mut job = egui::text::LayoutJob::simple_singleline(line.text.replace('\t', "    "), font.clone(), text_c);
            job.wrap = egui::text::TextWrapping::truncate_at_width((rect.right() - tk::GAP - x).max(0.0));
            let g = p.layout_job(job);
            p.galley(egui::pos2(x, rect.center().y - g.size().y / 2.0), g, text_c);
        }
    });
    kit::rule::frame(ui.painter(), outer, frame_c);
}

fn filtered<'a>(lines: impl Iterator<Item = &'a LogLine>, filter: &str) -> Vec<&'a LogLine> {
    let f = filter.trim().to_lowercase();
    lines.filter(|l| f.is_empty() || l.text.to_lowercase().contains(&f)).collect()
}

/// A headline as a sentence: its first letter capitalised.
fn sentence(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// The filter as a query line, on the gutter, with room above and below.
fn filter_line(ui: &mut Ui, t: &Tokens, value: &mut String, focus: bool) {
    ui.add_space(tk::GAP);
    padded(ui, |ui| kit::query(ui, t, value, "filter", tk::SIZE_HEADING, focus));
    ui.add_space(tk::GAP);
}

/// A game that crashed, for the notice in its log tab.
#[derive(Clone)]
pub struct Notice {
    /// The crash report, or the log the analysis read.
    pub path: PathBuf,
    pub headline: String,
}

pub fn live(app: &mut App, ui: &mut Ui, inst: &Instance) {
    let t = app.look.tokens.clone();
    ui.spacing_mut().item_spacing.y = 0.0;
    let why = app.logs.key.take() == Some(Action::Why);
    // A crash holds the top of the tab until it's looked at or dismissed.
    if let Some(n) = app.logs.notices.get(&inst.id).cloned() {
        let clicked = kit::banner(ui, &t, "game crashed", Some(t.error()), &sentence(&n.headline), &[kit::Button::new("see why"), kit::Button::new("dismiss")]);
        if why || clicked == Some(0) {
            app.logs.show_analysis(&n.path);
            app.inst_tab = super::instances::InstTab::LogHistory;
            app.focus = Zone::Side;
        }
        if clicked == Some(1) {
            app.logs.notices.remove(&inst.id);
        }
    }
    let Some(session) = app.sessions.get(&inst.id) else {
        content::pane_head(ui, &t, 1, "live", "not running", &[], 0.0);
        padded(ui, |ui| kit::empty_state(ui, &t, "Not running", "Press play to see the game's output here. Earlier runs are in history.", None));
        return;
    };
    let log = session.log.clone();
    let lines = log.lock().unwrap().lines.clone();
    let kb = app.nav_for(Zone::Main);
    let focused = app.focus == Zone::Main;
    let aside = if lines.len() == 1 { "1 line".to_string() } else { format!("{} lines", lines.len()) };
    let (clicked, _) = content::pane_head(ui, &t, 1, "live", &aside, &[kit::Button::new("copy"), kit::Button::new("clear")], 0.0);
    let copy = kb == Some(Nav::Copy) || clicked == Some(0);
    if clicked == Some(1) {
        log.lock().unwrap().lines.clear();
    }
    filter_line(ui, &t, &mut app.logs.live_filter, kb == Some(Nav::Search));
    let shown = filtered(lines.iter(), &app.logs.live_filter);
    if copy {
        ui.ctx().copy_text(shown.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n"));
        app.toast(ToastKind::Info, "Log copied");
    }
    render_lines(ui, &t, &shown, true, kb, focused, Mark::default());
}

// -- history --------------------------------------------------------------------

#[derive(Clone)]
struct LogFile {
    path: PathBuf,
    name: String,
    modified: Option<SystemTime>,
    size: u64,
    crash: bool,
}

/// A log read from disk, with an analysis if it shows a crash.
struct Loaded {
    lines: Vec<LogLine>,
    analysis: Option<Analysis>,
}

/// The analysis panel's own state.
#[derive(Default)]
struct Panel {
    /// The keyboard drives the panel rather than the log under it.
    focused: bool,
    selected: usize,
    /// Every suspect, not just the first few.
    all: bool,
    hidden: bool,
}

#[derive(Default)]
pub struct LogsState {
    pub live_filter: String,
    files: Option<(String, Vec<LogFile>, Instant)>,
    selected: Option<PathBuf>,
    loaded: Option<(PathBuf, Result<Loaded, String>)>,
    load_p: Option<(PathBuf, Promise<Loaded>)>,
    filter: String,
    panel: Panel,
    mark: Mark,
    /// sha1 → Modrinth project id (None: not on Modrinth), for "modrinth" buttons.
    pages: HashMap<String, Option<String>>,
    page_p: Option<(String, Promise<HashMap<String, crate::content_meta::Project>>)>,
    /// An instance action for these tabs (`why`), taken by whichever is drawn.
    pub key: Option<Action>,
    /// Instance id → its last game's crash.
    pub notices: HashMap<String, Notice>,
    /// Games that just ended, being checked for a crash: (instance id, exit, result).
    crash_p: Vec<(String, Result<i32, String>, Promise<Option<Notice>>)>,
}

impl LogsState {
    /// Open the history on `path`, the analysis in front.
    pub fn show_analysis(&mut self, path: &Path) {
        self.selected = Some(path.to_path_buf());
        self.filter.clear();
        self.panel = Panel { focused: true, ..Default::default() };
        self.mark = Mark::default();
        // Re-read: the file may be newer than what's loaded.
        self.loaded = None;
        self.load_p = None;
        self.files = None;
    }

    /// The analysis of the log on screen, if it shows a crash.
    pub fn analysis(&self) -> Option<&Analysis> {
        match &self.loaded {
            Some((p, Ok(l))) if Some(p) == self.selected.as_ref() => l.analysis.as_ref().filter(|a| a.crashed),
            _ => None,
        }
    }

    /// Whether the keyboard is on the analysis panel (in log history's right pane).
    pub fn panel_focused(&self) -> bool {
        self.panel.focused && !self.panel.hidden && self.analysis().is_some()
    }

    /// Check a game that just ended for a crash: a crash report written since it
    /// started, or else a log that shows one. A game the user stopped isn't checked.
    pub fn watch_exit(&mut self, rt: &tokio::runtime::Handle, ctx: &egui::Context, inst: &Instance, default_memory_mb: u32, started: SystemTime, exit: Result<i32, String>, lines: Vec<String>) {
        if exit.as_ref().is_err_and(|e| e == "killed by signal") {
            return;
        }
        let (id, inst, failed) = (inst.id.clone(), inst.clone(), exit != Ok(0));
        let p = Promise::spawn(rt, ctx, async move {
            Ok(tokio::task::spawn_blocking(move || {
                let (path, text) = match crate::crash::crash_report_since(&inst, started) {
                    Some(p) => {
                        let text = String::from_utf8_lossy(&crate::fsutil::read_capped(std::fs::File::open(&p).ok()?).ok()?).into_owned();
                        (p, text)
                    }
                    None if failed => (inst.logs_dir().join("latest.log"), lines.join("\n")),
                    None => return None,
                };
                let a = crate::crash::analyze(&inst, default_memory_mb, &text);
                (a.crashed || failed).then(|| Notice { path, headline: if a.crashed { a.headline() } else { "see the log".into() } })
            })
            .await?)
        });
        self.crash_p.push((id, exit, p));
    }
}

/// Poll the crash checks of games that ended; toast what they found.
pub fn poll_crashes(app: &mut App) {
    let (done, running): (Vec<_>, Vec<_>) = std::mem::take(&mut app.logs.crash_p).into_iter().partition(|(.., p)| p.ready());
    app.logs.crash_p = running;
    for (id, exit, p) in done {
        match p.take().and_then(|r| r.ok()).flatten() {
            Some(n) => {
                let name = app.instances.iter().find(|i| i.id == id).map(|i| i.name.clone()).unwrap_or_default();
                app.toast(ToastKind::Error, format!("{name} crashed: {}. The log tab says why", n.headline));
                app.logs.notices.insert(id, n);
            }
            None => match exit {
                Ok(0) => {}
                Ok(c) => app.toast(ToastKind::Error, format!("Minecraft exited with code {c}; see the log")),
                Err(e) => app.toast(ToastKind::Error, format!("Minecraft {e}")),
            },
        }
    }
}

fn list_files(inst: &Instance) -> Vec<LogFile> {
    let mut out = Vec::new();
    for (dir, crash) in [(inst.logs_dir(), false), (inst.crash_reports_dir(), true)] {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let ok = if crash { name.ends_with(".txt") } else { name.ends_with(".log") || name.ends_with(".log.gz") };
            let Ok(meta) = e.metadata() else { continue };
            if ok && meta.is_file() {
                out.push(LogFile { path: e.path(), name, modified: meta.modified().ok(), size: meta.len(), crash });
            }
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

fn read_log(path: &Path) -> anyhow::Result<(String, bool)> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.read_to_end(&mut bytes)?;
    if path.extension().is_some_and(|e| e == "gz") {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&bytes[..]).read_to_end(&mut out)?;
        bytes = out;
    }
    let crash = path.parent().and_then(|p| p.file_name()).is_some_and(|n| n == "crash-reports");
    Ok((String::from_utf8_lossy(&bytes).into_owned(), crash))
}

/// A crash report's line: the exception and its stack are errors, the rest (the report's
/// details) plain, so the level column marks what went wrong rather than every line.
fn crash_level(line: &str) -> LogLevel {
    let t = line.trim_start();
    let thrown = t.split([':', ' ']).next().is_some_and(|w| w.contains('.') && (w.ends_with("Exception") || w.ends_with("Error")));
    if t.starts_with("at ") || t.starts_with("Caused by:") || thrown || line.starts_with("Description:") { LogLevel::Error } else { launch::classify(line, false) }
}

/// Read a log and, if it shows a crash, analyse it (reads the instance's jars).
fn load(path: &Path, inst: &Instance, default_memory_mb: u32) -> anyhow::Result<Loaded> {
    let (text, crash) = read_log(path)?;
    let lines = text.lines().map(|l| LogLine { level: if crash { crash_level(l) } else { launch::classify(l, false) }, text: l.to_string() }).collect();
    let analysis = (crash || crate::crash::looks_crashed(&text)).then(|| crate::crash::analyze(inst, default_memory_mb, &text));
    Ok(Loaded { lines, analysis })
}

fn modified_label(m: Option<SystemTime>) -> String {
    m.map(|m| kit::text::ago(Some(chrono::DateTime::<chrono::Utc>::from(m)))).unwrap_or_default()
}

pub fn history(app: &mut App, ui: &mut Ui, inst: &Instance) {
    let t = app.look.tokens.clone();
    let mem = app.cfg.default_memory_mb;
    // Mods can't be switched while a job has them, as the game starts, or while a search for the breaking mod does.
    let mods_busy = super::mod_sets::busy(app, inst).by_hand();
    let why = app.logs.key.take() == Some(Action::Why);
    let s = &mut app.logs;
    let fresh = s.files.as_ref().is_some_and(|(id, _, at)| *id == inst.id && at.elapsed() < Duration::from_secs(3));
    if !fresh {
        if s.files.as_ref().is_some_and(|(id, _, _)| *id != inst.id) {
            s.selected = None;
            s.loaded = None;
            s.panel = Panel::default();
            s.mark = Mark::default();
        }
        s.files = Some((inst.id.clone(), list_files(inst), Instant::now()));
    }
    let files = s.files.as_ref().unwrap().1.clone();
    let list_nav = app.nav_for(Zone::Main);
    let view_nav = app.nav_for(Zone::Side);
    let focus = app.focus;
    let s = &mut app.logs;
    ui.spacing_mut().item_spacing.y = 0.0;
    if files.is_empty() {
        content::pane_head(ui, &t, 1, "history", "", &[], 0.0);
        padded(ui, |ui| kit::empty_state(ui, &t, "No logs yet", "Minecraft writes logs and crash reports here after it runs.", None));
        return;
    }
    if s.selected.is_none() {
        s.selected = files.first().map(|f| f.path.clone());
    }
    if let Some((path, p)) = &s.load_p
        && let Some(r) = p.take()
    {
        s.loaded = Some((path.clone(), r.map_err(|e| format!("{e:#}"))));
        s.load_p = None;
    }
    let mut scroll = false;
    if let Some(n) = list_nav.filter(|n| n.is_move()) {
        let pos = files.iter().position(|f| Some(&f.path) == s.selected.as_ref());
        if let Some(p) = n.step(files.len(), pos) {
            s.selected = Some(files[p].path.clone());
            scroll = true;
        }
    }
    let sel = s.selected.clone().unwrap();
    let have = s.loaded.as_ref().is_some_and(|(p, _)| *p == sel) || s.load_p.as_ref().is_some_and(|(p, _)| *p == sel);
    if !have {
        let (path, i) = (sel.clone(), inst.clone());
        s.load_p = Some((sel.clone(), Promise::spawn(&app.rt, ui.ctx(), async move { tokio::task::spawn_blocking(move || load(&path, &i, mem)).await? })));
        s.mark = Mark::default();
        s.panel.selected = 0;
        s.panel.all = false;
    }
    // `why` turns the keyboard to the analysis and back, showing it if hidden.
    let has_analysis = s.analysis().is_some();
    if why && has_analysis {
        if s.panel.hidden || !s.panel.focused || focus != Zone::Side {
            s.panel.hidden = false;
            s.panel.focused = true;
            app.focus = Zone::Side;
        } else {
            s.panel.focused = false;
        }
    }
    let focus = app.focus;
    let mut out = Out::default();
    if ui.available_width() < tk::CONTENT_TWO_PANES {
        // Too narrow for both: one pane at a time, the one with the keyboard (h/l still switch).
        let mut side = focus == Zone::Side;
        padded(ui, |ui| ui.horizontal(|ui| kit::segmented(ui, &t, &mut side, &[(false, "files"), (true, "log")])));
        ui.add_space(tk::GAP);
        if side {
            reader(app, ui, &files, &sel, view_nav, mods_busy, &mut out);
            out.to_side |= focus != Zone::Side;
        } else {
            file_list(ui, &t, &mut app.logs, &files, &sel, focus == Zone::Main, scroll, &mut out);
            out.to_main |= focus == Zone::Side;
        }
    } else {
        let list_w = (ui.available_width() * tk::LOG_FILES_SHARE).clamp(tk::LOG_FILES_MIN, tk::LOG_FILES_MAX);
        let (mut left, mut right) = content::panes(ui, &t, list_w);
        file_list(&mut left, &t, &mut app.logs, &files, &sel, focus == Zone::Main, scroll, &mut out);
        reader(app, &mut right, &files, &sel, view_nav, mods_busy, &mut out);
    }
    let key = |n: Nav| list_nav == Some(n);
    if key(Nav::Activate) {
        super::open_url(&sel.display().to_string());
    }
    if key(Nav::Folder) {
        super::open_path(sel.parent().unwrap());
    }
    if out.to_main {
        app.focus = Zone::Main;
    }
    if out.to_side {
        app.focus = Zone::Side;
    }
    if let Some(p) = out.panel {
        let a = app.logs.analysis().cloned();
        if let Some(a) = a {
            act(app, ui.ctx(), inst, &a, p);
        }
    }
}

/// What the history's panes ask of the app after drawing.
#[derive(Default)]
struct Out {
    to_main: bool,
    to_side: bool,
    head_h: f32,
    panel: Option<PanelOut>,
}

/// The logs and crash reports, newest first, as a compact hover list.
#[allow(clippy::too_many_arguments)]
fn file_list(ui: &mut Ui, t: &Tokens, s: &mut LogsState, files: &[LogFile], sel: &Path, focused: bool, scroll: bool, out: &mut Out) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let (_, h) = content::pane_head(ui, t, 1, "files", &files.len().to_string(), &[], 0.0);
    out.head_h = h;
    egui::ScrollArea::vertical().id_salt("log-files").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut list = kit::HoverList::begin(ui, kit::Variant::Compact).indent(tk::GUTTER + tk::HOVER_SHIFT_COMPACT);
        for f in files {
            let on = f.path == sel;
            let desc = format!("{} · {}", modified_label(f.modified), kit::text::size_label(f.size));
            let mut row = kit::HoverRow::new(("log-file", &f.path), &f.name).description(&desc).selected(on).hot(on && focused);
            if f.crash {
                row = row.badge("crash", t.error());
            }
            let r = list.row(ui, t, row);
            if on && scroll {
                r.scroll_to_me(None);
            }
            if r.clicked() {
                s.selected = Some(f.path.clone());
                out.to_main = true;
            }
            if r.double_clicked() {
                super::open_url(&f.path.display().to_string());
            }
        }
        list.end(ui);
    });
}

/// The selected log: its head, the filter, the crash analysis when it shows a crash, then its lines.
#[allow(clippy::too_many_arguments)]
fn reader(app: &mut App, ui: &mut Ui, files: &[LogFile], sel: &Path, kb: Option<Nav>, mods_busy: Option<&str>, out: &mut Out) {
    let t = app.look.tokens.clone();
    let focus = app.focus;
    let s = &mut app.logs;
    ui.spacing_mut().item_spacing.y = 0.0;
    let panel_keys = s.panel_focused() && focus == Zone::Side;
    let (panel_nav, view_nav) = if panel_keys { (kb, None) } else { (None, kb) };
    let key = |n: Nav| view_nav == Some(n);
    let file = files.iter().find(|f| f.path == sel);
    let name = file.map_or_else(String::new, |f| f.name.clone());
    let why_button = s.analysis().is_some() && s.panel.hidden;
    let mut buttons = Vec::new();
    if why_button {
        buttons.push(kit::Button::new("why it crashed"));
    }
    let first = buttons.len();
    buttons.extend([kit::Button::new("open"), kit::Button::new("folder"), kit::Button::new("copy")]);
    let aside = if file.is_some_and(|f| f.crash) { "crash report" } else { "" };
    let (clicked, _) = content::pane_head(ui, &t, 2, &name, aside, &buttons, out.head_h);
    let pick = |i: usize| clicked == Some(first + i);
    if why_button && clicked == Some(0) {
        s.panel.hidden = false;
    }
    if pick(0) || key(Nav::Activate) {
        super::open_url(&sel.display().to_string());
    }
    if pick(1) || key(Nav::Folder) {
        super::open_path(sel.parent().unwrap());
    }
    let copy = pick(2) || key(Nav::Copy);
    filter_line(ui, &t, &mut s.filter, key(Nav::Search));

    if let Some(a) = s.analysis().cloned().filter(|_| !s.panel.hidden) {
        // The selected suspect's Modrinth page, looked up once by hash.
        let sel_sha = a.suspects.get(s.panel.selected).and_then(|x| x.sha1.clone());
        if let Some((sha, p)) = &s.page_p
            && let Some(r) = p.take()
        {
            let found = r.ok().and_then(|mut m| m.remove(sha)).map(|p| p.id);
            s.pages.insert(sha.clone(), found);
            s.page_p = None;
        }
        if let Some(sha) = sel_sha.filter(|h| !s.pages.contains_key(h))
            && s.page_p.is_none()
        {
            let h = sha.clone();
            s.page_p = Some((sha, Promise::spawn(&app.rt, ui.ctx(), crate::content_meta::lookup(vec![h]))));
        }
        let o = panel(ui, &t, &a, &mut s.panel, panel_nav, panel_keys, &s.pages, mods_busy);
        if o.click {
            out.to_side = true;
            s.panel.focused = true;
        }
        if let Some(line) = o.jump {
            s.filter.clear();
            s.mark = Mark { line: Some(line), jump: true };
        }
        out.panel = Some(o);
        ui.add_space(tk::GAP);
    }
    let mark = s.mark;
    s.mark.jump = false;
    match &s.loaded {
        Some((p, Ok(l))) if *p == sel => {
            let shown = filtered(l.lines.iter(), &s.filter);
            if copy {
                ui.ctx().copy_text(shown.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n"));
            }
            // A mark is a line of the whole log: only meaningful unfiltered.
            let mark = if s.filter.trim().is_empty() { mark } else { Mark::default() };
            let r = ui.available_rect_before_wrap();
            render_lines(ui, &t, &shown, false, view_nav, focus == Zone::Side && !panel_keys, mark);
            if ui.input(|i| i.pointer.primary_clicked()) && ui.rect_contains_pointer(r) {
                s.panel.focused = false;
                out.to_side = true;
            }
        }
        Some((p, Err(e))) if *p == sel => {
            padded(ui, |ui| kit::text::body_colored(ui, e, t.error()));
        }
        _ => {
            padded(ui, |ui| kit::loading(ui, &t, "reading the log"));
        }
    }
    if copy {
        app.toast(ToastKind::Info, "Log copied");
    }
}

/// What the panel asked for this frame.
#[derive(Default)]
struct PanelOut {
    jump: Option<usize>,
    toggle: Option<usize>,
    page: Option<String>,
    copy: bool,
    bisect: bool,
    /// Clicked: the panel takes the keyboard.
    click: bool,
}

fn act(app: &mut App, ctx: &egui::Context, inst: &Instance, a: &Analysis, out: PanelOut) {
    if out.copy {
        ctx.copy_text(a.summary());
        app.toast(ToastKind::Info, "Crash summary copied");
    }
    if let Some(id) = out.page {
        super::open_url(&format!("https://modrinth.com/project/{id}"));
    }
    if out.bisect {
        start_bisect(app, inst, a);
    }
    if let Some(i) = out.toggle
        && let Some(s) = a.suspects.get(i)
        && let Some(file) = &s.file
    {
        // Checked again here, on the same check the mods tab uses.
        if let Some(why) = super::mod_sets::busy(app, inst).by_hand() {
            app.toast(ToastKind::Error, format!("Can't switch {} while {why}", s.name));
            return;
        }
        let Some(m) = current_file(inst, file) else {
            app.toast(ToastKind::Error, format!("{} isn't in the mods folder any more", s.name));
            return;
        };
        match m.toggle() {
            Ok(()) => {
                app.content.invalidate();
                let what = if m.enabled { "Disabled" } else { "Enabled" };
                let next = if m.enabled { "; play to see if the crash is gone" } else { "" };
                app.toast(ToastKind::Info, format!("{what} {}{next}", s.name));
            }
            Err(e) => app.toast(ToastKind::Error, format!("{e:#}")),
        }
    }
}

/// Hook for the bisect: start finding the breaking mod among `a`'s suspects first.
/// Search for the breaking mod, trying the analysis's suspects first, strongest first.
fn start_bisect(app: &mut App, inst: &Instance, a: &Analysis) {
    let first = a.suspects.iter().filter_map(|s| s.file.as_ref()?.file_name()).map(|n| n.to_string_lossy().into_owned()).collect();
    super::mod_sets::open_bisect(app, &inst.id, first);
}

/// The mod file the analysis named, as it is now (it may have been enabled or disabled since).
fn current_file(inst: &Instance, file: &Path) -> Option<crate::instance::ModFile> {
    let base = file.file_name()?.to_string_lossy().trim_end_matches(".disabled").to_string();
    inst.mods().into_iter().find(|m| m.name.trim_end_matches(".disabled") == base)
}

/// Whether the mod file is enabled now: Some(true/false), None if it's gone.
fn enabled_now(file: &Path) -> Option<bool> {
    let name = file.file_name()?.to_string_lossy().trim_end_matches(".disabled").to_string();
    let on = file.with_file_name(&name);
    if on.exists() {
        Some(true)
    } else if file.with_file_name(format!("{name}.disabled")).exists() {
        Some(false)
    } else {
        None
    }
}

fn confidence_color(t: &Tokens, c: Confidence) -> Color32 {
    match c {
        Confidence::Certain => t.error(),
        Confidence::Likely => t.warning(),
        Confidence::Possible => t.text_muted,
    }
}

/// The analysis: a caption (how sure, what Minecraft says), the headline in large type (the
/// mod's name in Melodrama when it names one), the actions joined, causes that aren't mods as
/// bracketed paragraphs, then the suspects as a quiet table; the selected suspect's evidence
/// follows it as quoted lines. Scrolls within at most half the pane.
#[allow(clippy::too_many_arguments)]
fn panel(ui: &mut Ui, t: &Tokens, a: &Analysis, st: &mut Panel, kb: Option<Nav>, focused: bool, pages: &HashMap<String, Option<String>>, mods_busy: Option<&str>) -> PanelOut {
    let mut out = PanelOut::default();
    let n = a.suspects.len();
    let shown = if st.all { n } else { n.min(tk::CRASH_SUSPECTS_SHOWN) };
    if st.selected >= n.max(1) {
        st.selected = n.saturating_sub(1);
    }
    let mut scroll_sel = false;
    if let Some(nav) = kb {
        if let Some(p) = Some(nav).filter(|m| m.is_move()).and_then(|m| m.step(n, Some(st.selected))) {
            st.selected = p;
            st.all |= p >= tk::CRASH_SUSPECTS_SHOWN;
            scroll_sel = true;
        }
        match nav {
            Nav::Activate => out.jump = a.suspects.get(st.selected).and_then(|s| s.evidence.first()).map(|e| e.line),
            Nav::Toggle if mods_busy.is_none() => out.toggle = Some(st.selected).filter(|&i| a.suspects.get(i).is_some_and(|s| s.file.is_some())),
            Nav::Copy => out.copy = true,
            _ => {}
        }
    }
    let top = a.suspects.first();
    let certain = a.suspects.iter().filter(|s| s.confidence == Confidence::Certain).count();
    // The headline names one mod: its name, large, under how sure that is.
    let one = top.filter(|s| (s.confidence == Confidence::Certain && certain == 1) || (certain == 0 && a.causes.is_empty() && s.confidence == Confidence::Likely));
    let tone = match (top.map(|s| s.confidence), a.causes.is_empty()) {
        (Some(Confidence::Certain), _) | (_, false) => t.error(),
        (Some(Confidence::Likely), _) => t.warning(),
        _ => t.text_muted,
    };
    let max_h = (ui.available_height() * tk::CRASH_PANEL_SHARE).max(tk::TABLE_ROW_HEIGHT * 6.0);
    let selected = a.suspects.get(st.selected);
    let now = selected.and_then(|s| s.file.as_deref()).map(enabled_now);
    egui::ScrollArea::vertical().id_salt("crash-analysis").max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        padded(ui, |ui| {
            let mut cap = vec![match one {
                Some(s) => format!("{} cause", s.confidence.label()),
                None => "crash analysis".to_string(),
            }];
            if let Some(d) = &a.description {
                cap.push(format!("minecraft says: {d}"));
            }
            match n {
                0 => {}
                1 => cap.push("1 suspect".into()),
                _ => cap.push(format!("{n} suspects")),
            }
            let mut job = kit::text::caption_job(&cap[0], tone);
            if cap.len() > 1 {
                job.append(&kit::text::caps(&format!(" · {}", cap[1..].join(" · "))), 0.0, egui::TextFormat { font_id: fonts::text(tk::SIZE_CAPTION), color: t.text_muted, extra_letter_spacing: tk::CAPTION_TRACKING, ..Default::default() });
            }
            job.wrap = egui::text::TextWrapping::truncate_at_width(ui.available_width());
            ui.add(egui::Label::new(job).selectable(false));
            ui.add_space(tk::UNIT);
            match one {
                Some(s) => kit::display(ui, t, &s.label(), tk::SIZE_DISPLAY_SMALL),
                None => kit::heading(ui, t, &sentence(&a.headline())),
            };
            if n == 0 && a.causes.is_empty() {
                ui.add_space(tk::UNIT);
                kit::body_muted(ui, t, "Nothing in this log points at a mod. Finding it by elimination is the way: find breaking mod turns mods off half at a time.");
            }
            ui.add_space(tk::GAP);
            // The actions: the selected suspect's first, then the analysis's.
            let mut buttons = Vec::new();
            let mut ids = Vec::new();
            if let (Some(s), Some(now)) = (selected.filter(|s| s.file.is_some()), now) {
                let label = if now == Some(false) { format!("enable {}", s.name) } else { format!("disable {}", s.name) };
                ids.push((PanelAct::Toggle, label));
            }
            if selected.and_then(|s| s.sha1.as_ref()).and_then(|h| pages.get(h)).is_some_and(Option::is_some) {
                ids.push((PanelAct::Page, "modrinth page".into()));
            }
            if a.crashed {
                ids.push((PanelAct::Bisect, "find breaking mod".into()));
            }
            ids.push((PanelAct::Copy, "copy summary".into()));
            ids.push((PanelAct::Hide, "hide".into()));
            for (act, label) in &ids {
                let on = *act != PanelAct::Toggle || (mods_busy.is_none() && now.flatten().is_some());
                buttons.push(kit::Button::new(label).enabled(on));
            }
            ui.horizontal_wrapped(|ui| {
                if let Some(i) = kit::button_row(ui, t, &buttons) {
                    match ids[i].0 {
                        PanelAct::Toggle => out.toggle = Some(st.selected),
                        PanelAct::Page => out.page = selected.and_then(|s| s.sha1.as_ref()).and_then(|h| pages.get(h).cloned().flatten()),
                        PanelAct::Bisect => out.bisect = true,
                        PanelAct::Copy => out.copy = true,
                        PanelAct::Hide => st.hidden = true,
                    }
                }
            });
            // Why the switch can't be used now, said rather than hidden.
            if ids.first().is_some_and(|(a, _)| *a == PanelAct::Toggle) {
                let why = match (mods_busy, now.flatten()) {
                    (Some(why), _) => Some(format!("mods can't be switched while {why}")),
                    (_, None) => Some("it's not in the mods folder any more".to_string()),
                    _ => None,
                };
                if let Some(why) = why {
                    ui.add_space(tk::UNIT);
                    kit::caption(ui, t, &why);
                }
            }
            let headline = a.headline();
            for c in &a.causes {
                ui.add_space(tk::GUTTER);
                // The headline may already name it.
                let cap = if c.title == headline { "cause".to_string() } else { format!("cause · {}", c.title) };
                kit::bracketed(ui, t, &cap, |ui| {
                    kit::body(ui, t, &c.advice);
                    for e in &c.evidence {
                        if evidence_row(ui, t, e) {
                            out.jump = Some(e.line);
                        }
                    }
                });
            }
            ui.add_space(tk::GUTTER);
        });
        if n == 0 {
            return;
        }
        let cols = [kit::Column::fixed("how sure", tk::CRASH_CONFIDENCE_WIDTH), kit::Column::fill("mod"), kit::Column::fill("why")];
        kit::table::header(ui, t, &cols);
        for (i, s) in a.suspects.iter().enumerate().take(shown) {
            let on = i == st.selected;
            let cells = [kit::Cell::Caption(s.confidence.label().into(), Some(confidence_color(t, s.confidence))), kit::Cell::Text(s.label()), kit::Cell::Muted(s.reason.clone())];
            let r = kit::table::row(ui, t, &cols, &cells, on);
            if on && focused {
                kit::table::mark(ui, t, r.rect);
            }
            if on && scroll_sel {
                r.scroll_to_me(None);
            }
            if r.clicked() {
                st.selected = i;
                out.click = true;
            }
            if on {
                ui.add_space(tk::GAP);
                padded(ui, |ui| {
                    kit::bracketed(ui, t, "evidence", |ui| {
                        for e in &s.evidence {
                            if evidence_row(ui, t, e) {
                                out.jump = Some(e.line);
                            }
                        }
                        if s.evidence.is_empty() {
                            kit::body_muted(ui, t, "From the jar itself, not the log.");
                        }
                    });
                });
                ui.add_space(tk::GAP);
                kit::rule(ui, t, false);
            }
        }
        if n > shown {
            let more = format!("{} more, less likely", n - shown);
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), tk::TABLE_ROW_HEIGHT), egui::Sense::hover());
            let g = kit::text::caption_galley(ui, &more, t.text_muted);
            let lr = Rect::from_min_size(egui::pos2(rect.left() + tk::GUTTER, rect.center().y - g.size().y / 2.0), g.size());
            if content::caption_link(ui, t, lr, "more-suspects", &more).clicked() {
                st.all = true;
            }
        }
    });
    kit::rule(ui, t, true);
    out
}

#[derive(Clone, Copy, PartialEq)]
enum PanelAct {
    Toggle,
    Page,
    Bisect,
    Copy,
    Hide,
}

/// A quoted log line (JetBrains Mono) with its number; clicking shows it in the log. Returns whether clicked.
fn evidence_row(ui: &mut Ui, t: &Tokens, e: &crate::crash::Evidence) -> bool {
    let font = fonts::raw(tk::SIZE_MONO);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), tk::LOG_LINE_HEIGHT), egui::Sense::click());
    let hot = resp.hovered() || resp.has_focus();
    let num_c = crate::design::motion::color(ui.ctx(), resp.id.with("n"), hot, t.text_muted, t.accent);
    let p = ui.painter_at(rect);
    let num = p.layout_no_wrap(format!("{:>6}", e.line + 1), font.clone(), num_c);
    let nw = num.size().x;
    p.galley(egui::pos2(rect.left(), rect.center().y - num.size().y / 2.0), num, num_c);
    let x = rect.left() + nw + tk::GAP;
    let mut job = egui::text::LayoutJob::simple_singleline(e.text.replace('\t', "    "), font, t.text_strong);
    job.wrap = egui::text::TextWrapping::truncate_at_width((rect.right() - x).max(0.0));
    let g = p.layout_job(job);
    p.galley(egui::pos2(x, rect.center().y - g.size().y / 2.0), g, t.text_strong);
    let text = format!("line {}: {}", e.line + 1, e.text);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, true, &text));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("show in the log").clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In a crash report only what went wrong is marked an error: the exception, its stack and
    /// its causes, not every detail line, so the level column says something.
    #[test]
    fn crash_reports_mark_only_the_exception_and_its_stack() {
        let error = |l: &str| crash_level(l) == LogLevel::Error;
        assert!(error("java.lang.OutOfMemoryError: Java heap space"));
        assert!(error("\tat net.minecraft.server.MinecraftServer.run(MinecraftServer.java:100)"));
        assert!(error("Caused by: java.lang.NullPointerException"));
        assert!(error("Description: Exception in server tick loop"));
        assert!(!error("---- Minecraft Crash Report ----"));
        assert!(!error("\tMinecraft Version: 1.21.11"));
        assert!(!error("\tMemory: 19639896 bytes (18 MiB)"));
        assert!(!error("A detailed walkthrough of the error, its code path and all known details is as follows:"));
    }
}
