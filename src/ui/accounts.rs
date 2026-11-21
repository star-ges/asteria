use std::collections::HashMap;
use std::path::{Path, PathBuf};

use std::time::Instant;

use egui::{Ui, vec2};

use super::content::{padded, pane_head};
use super::nav::{Nav, Zone};
use super::skin_view::{self, Model};
use super::{App, Confirm, ConfirmAction, Modal, Toast, ToastKind};
use crate::auth::{self, Account, Accounts, DeviceCode, MsaAccount};
use crate::skins::{self, Change, Loaded, Look, Profile};
use crate::design::glyphs;
use crate::design::kit::{self, Button, HoverList, HoverRow, Variant};
use crate::design::tokens::{self as tk, Tokens};
use crate::keymap::Action;
use crate::task::Promise;

#[derive(Default)]
pub struct AccountsState {
    begin_p: Option<Promise<DeviceCode>>,
    /// The sign-in code and when it came (it expires).
    code: Option<(DeviceCode, Instant)>,
    finish_p: Option<Promise<MsaAccount>>,
    offline_name: String,
    selected: Option<String>,
    /// Set by the `m` key.
    pub start_microsoft: bool,
    /// Set by the upload-skin key.
    pub start_upload: bool,
    /// Skins by account UUID.
    skins: HashMap<String, SkinSlot>,
    /// A skin file chosen but not uploaded yet, shown in the preview.
    pending: Option<PendingSkin>,
    pick_p: Option<Promise<Option<PathBuf>>>,
    /// The preview's model and the look it was built from.
    model: Option<(String, Model)>,
    /// The preview's (yaw, pitch) away from its starting view.
    turn: (f32, f32),
}

#[derive(Default)]
struct SkinSlot {
    shown: Option<Result<Shown, String>>,
    /// A load or change in flight, and whether it's an upload.
    busy: Option<(Promise<Loaded>, bool)>,
}

struct Shown {
    profile: Option<Profile>,
    look: Look,
}

struct PendingSkin {
    uuid: String,
    path: PathBuf,
    png: Vec<u8>,
    img: image::RgbaImage,
    slim: bool,
}

impl AccountsState {
    pub fn poll(&mut self, accounts: &mut Accounts, toasts: &mut Vec<Toast>) {
        let mut toast = |kind, text: String| toasts.push(Toast::new(kind, text));
        if let Some(p) = &self.begin_p
            && let Some(r) = p.take()
        {
            self.begin_p = None;
            match r {
                Ok(code) => self.code = Some((code, Instant::now())),
                Err(e) => toast(ToastKind::Error, format!("{e:#}")),
            }
        }
        if let Some(p) = &self.finish_p
            && let Some(r) = p.take()
        {
            self.finish_p = None;
            self.code = None;
            match r {
                Ok(acct) => {
                    let name = acct.name.clone();
                    let uuid = acct.uuid.clone();
                    // A fresh sign-in may come with a different skin.
                    self.skins.remove(&uuid);
                    accounts.upsert(Account::Msa(acct));
                    accounts.active = Some(uuid);
                    let _ = accounts.save();
                    toast(ToastKind::Info, format!("Signed in as {name}"));
                }
                Err(e) => toast(ToastKind::Error, format!("{e:#}")),
            }
        }
        for (uuid, slot) in &mut self.skins {
            let Some((p, upload)) = &slot.busy else { continue };
            let Some(r) = p.take() else { continue };
            let upload = *upload;
            slot.busy = None;
            match r {
                Ok(l) => {
                    if let Some(a) = l.account
                        && accounts.refresh(Account::Msa(a))
                    {
                        let _ = accounts.save();
                    }
                    slot.shown = Some(Ok(Shown { profile: l.profile, look: l.look }));
                    if upload && self.pending.as_ref().is_some_and(|p| &p.uuid == uuid) {
                        self.pending = None;
                        toast(ToastKind::Info, "Skin uploaded".into());
                    }
                }
                Err(e) => {
                    let msg = format!("{e:#}");
                    if slot.shown.is_none() {
                        slot.shown = Some(Err(msg.clone()));
                    }
                    toast(ToastKind::Error, msg);
                }
            }
        }
        if let Some(p) = &self.pick_p
            && let Some(r) = p.take()
        {
            self.pick_p = None;
            if let Ok(Some(path)) = r
                && let Some(uuid) = self.selected.clone().or(accounts.active.clone())
                && let Err(e) = self.preview_file(&uuid, &path)
            {
                toast(ToastKind::Error, format!("{e:#}"));
            }
        }
    }

    pub fn turn_by(&mut self, steps: f32) {
        self.turn.0 += steps * 15f32.to_radians();
    }

    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn cancel_pending(&mut self) {
        self.pending = None;
    }

    /// An account was removed: nothing of it stays selected or pending.
    pub fn forget(&mut self, uuid: &str) {
        if self.selected.as_deref() == Some(uuid) {
            self.selected = None;
        }
        if self.pending.as_ref().is_some_and(|p| p.uuid == uuid) {
            self.pending = None;
        }
        self.skins.remove(uuid);
    }

    /// Show a skin file in the preview, ready to upload.
    fn preview_file(&mut self, uuid: &str, path: &Path) -> anyhow::Result<()> {
        let (png, img) = skins::read_skin_file(path)?;
        let slim = match self.skins.get(uuid).and_then(|s| s.shown.as_ref()) {
            Some(Ok(s)) => s.look.slim,
            _ => false,
        };
        self.pending = Some(PendingSkin { uuid: uuid.to_string(), path: path.to_path_buf(), png, img, slim });
        Ok(())
    }
}

/// The account whose skin is shown: the selected one, else the active one.
fn current(app: &App) -> Option<Account> {
    let find = |u: &str| app.accounts.accounts.iter().find(|a| a.uuid() == u).cloned();
    app.acct.selected.as_deref().and_then(find).or_else(|| app.accounts.active().cloned())
}

pub fn current_is_microsoft(app: &App) -> bool {
    matches!(current(app), Some(Account::Msa(_)))
}

/// Load an account's skin, or change it.
pub fn skin_change(app: &mut App, ctx: &egui::Context, uuid: &str, change: Change) {
    let Some(acct) = app.accounts.accounts.iter().find(|a| a.uuid() == uuid).cloned() else { return };
    let upload = matches!(change, Change::Upload { .. });
    let p = match acct {
        Account::Msa(a) => {
            let cid = app.cfg.client_id();
            Promise::spawn(&app.rt, ctx, async move { skins::apply(&cid, &a, change).await })
        }
        Account::Offline { uuid, .. } => Promise::spawn(&app.rt, ctx, async move { skins::default_look(&uuid).await }),
    };
    app.acct.skins.entry(uuid.to_string()).or_default().busy = Some((p, upload));
}

/// A PNG dropped on the accounts page becomes the pending skin.
pub fn drop_file(app: &mut App, path: &Path) -> bool {
    let Some(Account::Msa(a)) = current(app) else { return false };
    if let Err(e) = app.acct.preview_file(&a.uuid, path) {
        app.toast(ToastKind::Error, format!("{e:#}"));
    }
    true
}

/// Both pane heads are as tall as one with buttons, whether or not they have any now, so their
/// rules are one line.
const HEAD_WITH_BUTTONS: f32 = tk::BUTTON_HEIGHT + tk::GAP * 2.0;

pub fn show(app: &mut App, ui: &mut Ui) {
    let t = app.look.tokens.clone();
    let kb = app.nav_for(Zone::Main);
    ui.spacing_mut().item_spacing.y = 0.0;
    let area = ui.available_rect_before_wrap();
    if area.width() < tk::CONTENT_TWO_PANES {
        // Too narrow for two panes: the accounts, then the skin, in one column.
        egui::ScrollArea::vertical().id_salt("accounts-narrow").auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            account_list(app, ui, &t, kb, false);
            skin_pane(app, ui, &t, kb, HEAD_WITH_BUTTONS);
        });
        return;
    }
    let left_w = (area.width() * tk::ACCOUNTS_SHARE).floor();
    let (mut left, mut right) = super::content::panes(ui, &t, left_w);
    // The skin's head as tall as the accounts' (whose buttons go under its caption when narrow).
    let head_h = account_list(app, &mut left, &t, kb, true);
    egui::ScrollArea::vertical().id_salt("skin").auto_shrink(false).show(&mut right, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        skin_pane(app, ui, &t, kb, head_h);
    });
}

/// The accounts as a full hover list under a pane head, then adding one (Microsoft's sign-in,
/// an offline name). `fill`: the list takes the pane's height, leaving the add area at its foot.
/// Returns the head's height.
fn account_list(app: &mut App, ui: &mut Ui, t: &Tokens, kb: Option<Nav>, fill: bool) -> f32 {
    let ctx = ui.ctx().clone();
    ui.spacing_mut().item_spacing.y = 0.0;
    let active = app.accounts.active.clone();
    let uuids: Vec<String> = app.accounts.accounts.iter().map(|a| a.uuid().to_string()).collect();
    let current = app.acct.selected.clone().filter(|u| uuids.contains(u)).or(active.clone());
    let pos = current.as_ref().and_then(|u| uuids.iter().position(|x| x == u));
    let mut scroll = false;
    if let Some(p) = kb.filter(|n| n.is_move()).and_then(|n| n.step(uuids.len(), pos)) {
        app.acct.selected = Some(uuids[p].clone());
        scroll = true;
    }
    let current = app.acct.selected.clone().filter(|u| uuids.contains(u)).or(active.clone());
    // Enter uploads a pending skin rather than switching accounts.
    let pending = app.acct.pending.is_some();
    let key = |n: Nav| kb == Some(n) && !(n == Nav::Activate && pending);
    let focus_offline = key(Nav::Add);
    let mut set_active = None;
    let mut remove = None;

    let n = uuids.len();
    let aside = if n == 1 { "1 account".to_string() } else { format!("{n} accounts") };
    let other = current.is_some() && current != active;
    let buttons: Vec<Button> = if current.is_some() { vec![Button::new("make active").enabled(other), Button::new("remove")] } else { Vec::new() };
    let (clicked, head_h) = pane_head(ui, t, 1, "accounts", &aside, &buttons, HEAD_WITH_BUTTONS);
    if let Some(sel) = &current {
        if (clicked == Some(0) || key(Nav::Activate)) && other {
            set_active = Some(sel.clone());
        }
        if clicked == Some(1) || key(Nav::Delete) {
            remove = Some(sel.clone());
        }
    }

    let add_h = if app.acct.code.is_some() { tk::ACCOUNTS_ADD_HEIGHT_CODE } else { tk::ACCOUNTS_ADD_HEIGHT };
    let list_h = if fill { (ui.available_height() - add_h).max(tk::LIST_ROW_HEIGHT) } else { f32::INFINITY };
    let focused = app.focus == Zone::Main;
    egui::ScrollArea::vertical().id_salt("accounts").max_height(list_h).auto_shrink([false, !fill]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if app.accounts.accounts.is_empty() {
            padded(ui, |ui| kit::empty_state(ui, t, "No accounts yet", "Sign in with Microsoft to play online, or add an offline name for singleplayer and offline-mode servers.", None));
            return;
        }
        // The full rows keep room for a description beside the name; a narrow pane gets the
        // compact ones (the description under the name), as the instance list has.
        let full = ui.available_width() >= tk::ACCOUNTS_FULL_ROWS_MIN;
        let mut list = if full { HoverList::begin(ui, Variant::Full).indent(tk::GUTTER + tk::HOVER_SHIFT) } else { HoverList::begin(ui, Variant::Compact).indent(tk::GUTTER + tk::HOVER_SHIFT_COMPACT) };
        for a in &app.accounts.accounts {
            let is_active = active.as_deref() == Some(a.uuid());
            let on = current.as_deref() == Some(a.uuid());
            let desc = if is_active { format!("{} · playing as this one", a.kind()) } else { a.kind().to_string() };
            let mut row = HoverRow::new(("account", a.uuid()), a.name()).description(&desc).selected(on).hot(on && focused);
            if is_active {
                row = row.badge("active", t.ok());
            }
            let r = list.row(ui, t, row);
            if on && scroll {
                r.scroll_to_me(None);
            }
            if r.clicked() {
                app.acct.selected = Some(a.uuid().to_string());
            }
            if r.double_clicked() && !is_active {
                set_active = Some(a.uuid().to_string());
            }
        }
        list.end(ui);
    });
    if let Some(uuid) = set_active {
        app.accounts.active = Some(uuid);
        let _ = app.accounts.save();
    }
    if let Some(uuid) = remove
        && let Some(a) = app.accounts.accounts.iter().find(|a| a.uuid() == uuid)
    {
        let body = match a {
            Account::Msa(_) => format!("Sign {} out of Asteria? You can sign in again any time.", a.name()),
            Account::Offline { .. } => format!("Remove the offline account {}? You can add it again any time.", a.name()),
        };
        app.modal = Some(Modal::Confirm(Confirm { title: "remove account".into(), body, button: "remove".into(), action: ConfirmAction::RemoveAccount(uuid) }));
    }

    // Adding an account, at the pane's foot.
    pane_head(ui, t, 2, "add an account", "", &[], 0.0);
    ui.add_space(tk::GAP);
    let client_id = app.cfg.client_id();
    padded(ui, |ui| {
        if let Some((code, at)) = app.acct.code.clone() {
            // The one big figure: the code to type at Microsoft's page.
            kit::caption(ui, t, &format!("go to {} and enter", code.verification_uri.trim_start_matches("https://").trim_start_matches("www.")));
            ui.add_space(tk::UNIT);
            let size = if ui.available_width() >= tk::ACCOUNTS_CODE_ROOM { tk::SIZE_DISPLAY_LARGE * 0.75 } else { tk::SIZE_DISPLAY };
            kit::text::display_large(ui, t, &code.user_code, size);
            ui.add_space(tk::GAP);
            match kit::button_row(ui, t, &[Button::new("copy code").icon(glyphs::COPY), Button::new("open page").icon(glyphs::GLOBE)]) {
                Some(0) => {
                    ui.ctx().copy_text(code.user_code.clone());
                    app.toast(ToastKind::Info, "Copied the code");
                }
                Some(_) => {
                    ui.ctx().copy_text(code.user_code.clone());
                    super::open_url(&code.verification_uri);
                }
                None => {}
            }
            ui.add_space(tk::GAP);
            let left = code.expires_in.saturating_sub(at.elapsed().as_secs()).div_ceil(60);
            kit::caption(ui, t, &format!("waiting for microsoft · expires in {left} min"));
            ui.ctx().request_repaint_after(std::time::Duration::from_secs(20));
        } else if app.acct.begin_p.is_some() {
            kit::caption(ui, t, "asking microsoft for a code");
        } else if Button::new("sign in with microsoft").icon(glyphs::SIGN_IN).show(ui, t).clicked() || std::mem::take(&mut app.acct.start_microsoft) {
            let cid = client_id.clone();
            app.acct.begin_p = Some(Promise::spawn(&app.rt, &ctx, async move { auth::begin_device_code(&cid).await }));
        }
        ui.add_space(tk::GAP);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = tk::GAP;
            let name = app.acct.offline_name.trim().to_string();
            let valid = (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            let add_w = Button::new("add offline").width(ui, t);
            let w = (ui.available_width() - add_w - tk::GAP).min(tk::SETTING_CONTROL_WIDTH * 1.4);
            let r = kit::Field::new(&mut app.acct.offline_name).label("offline").hint("Username").width(w).focus(focus_offline).show(ui, t);
            let submit = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if Button::new("add offline").enabled(valid).show(ui, t).clicked() || (submit && valid) {
                let acct = Account::offline(&name);
                app.accounts.active = Some(acct.uuid().to_string());
                app.accounts.upsert(acct);
                let _ = app.accounts.save();
                app.acct.offline_name.clear();
                app.toast(ToastKind::Info, format!("Added offline account {name}"));
            }
        });
        ui.add_space(tk::UNIT);
        kit::caption(ui, t, "3 to 16 letters, digits or _ · singleplayer and offline-mode servers");
        ui.add_space(tk::GUTTER);
    });
    // Once we have a code, start polling for the token.
    if let Some((code, _)) = app.acct.code.clone()
        && app.acct.finish_p.is_none()
    {
        app.acct.finish_p = Some(Promise::spawn(&app.rt, &ctx, async move { auth::finish_device_code(&client_id, code).await }));
    }
    head_h
}

/// The current account's skin: the turning model framed by hairlines, its arm model, upload
/// and reset, and its capes.
fn skin_pane(app: &mut App, ui: &mut Ui, t: &Tokens, kb: Option<Nav>, head_h: f32) {
    let ctx = ui.ctx().clone();
    let key = |n: Nav| kb == Some(n);
    let Some(acct) = current(app) else {
        pane_head(ui, t, 3, "skin", "", &[], head_h);
        padded(ui, |ui| kit::loading(ui, t, "add an account to see its skin"));
        return;
    };
    let uuid = acct.uuid().to_string();
    let msa = matches!(acct, Account::Msa(_));

    let slot = app.acct.skins.entry(uuid.clone()).or_default();
    if slot.shown.is_none() && slot.busy.is_none() {
        skin_change(app, &ctx, &uuid, Change::None);
    }
    let slot = &app.acct.skins[&uuid];
    let busy = slot.busy.as_ref().map(|(_, upload)| *upload);
    let shown = match &slot.shown {
        Some(Ok(s)) => Some(s),
        _ => None,
    };
    let error = match &slot.shown {
        Some(Err(e)) => Some(e.clone()),
        _ => None,
    };
    let profile = shown.and_then(|s| s.profile.clone());
    let pending = app.acct.pending.as_ref().filter(|p| p.uuid == uuid);
    let has_pending = pending.is_some();

    // What the preview draws: a pending file, else the account's skin.
    let look = match (pending, shown) {
        (Some(p), s) => Some((format!("local:{}:{}", p.path.display(), p.slim), &p.img, p.slim, s.and_then(|s| s.look.cape.as_ref()))),
        (None, Some(s)) => Some((s.look.key.clone(), &s.look.skin, s.look.slim, s.look.cape.as_ref())),
        (None, None) => None,
    };
    let slim_now = look.as_ref().map(|l| l.2);
    match look {
        Some((k, img, slim, cape)) if app.acct.model.as_ref().is_none_or(|(m, _)| *m != k) => {
            let model = Model::build(img, slim, cape);
            app.acct.model = Some((k, model));
        }
        None => app.acct.model = None,
        _ => {}
    }

    // The head: what's going on, and upload or reset (upload or cancel while a file is previewed).
    let custom = profile.as_ref().and_then(|p| p.skin()).is_some();
    let picking = app.acct.pick_p.is_some();
    let status = match busy {
        Some(true) => "uploading",
        Some(false) => "loading",
        None => "",
    };
    let aside = if status.is_empty() { acct.name().to_string() } else { format!("{} · {status}", acct.name()) };
    let buttons: Vec<Button> = match (msa, has_pending) {
        (false, _) => Vec::new(),
        (true, true) => vec![Button::new("upload").icon(glyphs::UPLOAD).filled().enabled(busy.is_none()), Button::new("cancel")],
        (true, false) => vec![Button::new("upload skin").icon(glyphs::UPLOAD).enabled(busy.is_none() && !picking), Button::new("reset").enabled(custom && busy.is_none())],
    };
    let (clicked, _) = pane_head(ui, t, 3, "skin", &aside, &buttons, head_h);

    ui.add_space(tk::GUTTER);
    let mut retry = false;
    padded(ui, |ui| {
        if let Some(e) = &error {
            kit::bracketed(ui, t, "can't load the skin", |ui| kit::text::body_colored(ui, e, t.error()));
            ui.add_space(tk::GAP);
            retry = Button::new("try again").show(ui, t).clicked();
            ui.add_space(tk::GUTTER);
        }
        if error.is_none() || app.acct.model.is_some() {
            let w = ui.available_width().min(tk::SKIN_VIEW_WIDTH);
            skin_view::show(ui, t, app.acct.model.as_ref().map(|(_, m)| m), &mut app.acct.turn, vec2(w, (w * 1.25).round()));
            ui.add_space(tk::UNIT);
            let k = &app.keymap;
            kit::caption(ui, t, &format!("drag or {} to turn · double-click to face front", k.pair(Action::TurnLeft, Action::TurnRight)));
        }
    });
    if retry {
        skin_change(app, &ctx, &uuid, Change::None);
    }
    ui.add_space(tk::GUTTER);

    if !msa {
        padded(ui, |ui| {
            ui.set_max_width(ui.available_width().min(tk::SETTING_DESCRIPTION_WIDTH));
            kit::body_muted(ui, t, "Offline accounts wear the default skin for their name (Steve or Alex). Changing skins needs a Microsoft account.");
        });
        return;
    }

    let mut upload = clicked == Some(0) && has_pending;
    if clicked == Some(1) && has_pending {
        app.acct.pending = None;
    }
    let open = std::mem::take(&mut app.acct.start_upload);
    if (clicked == Some(0) && !has_pending) || (open && busy.is_none() && !picking) {
        app.acct.pick_p = Some(Promise::spawn(&app.rt, &ctx, async {
            let f = rfd::AsyncFileDialog::new()
                .set_title("Choose a skin (64×64 PNG)")
                .add_filter("PNG", &["png"])
                .set_directory(dirs::download_dir().or_else(dirs::home_dir).unwrap_or_default())
                .pick_file()
                .await;
            Ok(f.map(|f| f.path().to_path_buf()))
        }));
    }
    if clicked == Some(1) && !has_pending {
        app.modal = Some(Modal::Confirm(Confirm {
            title: "reset skin".into(),
            body: format!("Put {} back in the default skin? The current one isn't kept anywhere.", acct.name()),
            button: "reset".into(),
            action: ConfirmAction::ResetSkin(uuid.clone()),
        }));
    }

    // A file being previewed: what it is, and Enter uploads it.
    if let Some(path) = app.acct.pending.as_ref().filter(|p| p.uuid == uuid).map(|p| p.path.clone()) {
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        padded(ui, |ui| {
            kit::bracketed(ui, t, "previewing", |ui| kit::body(ui, t, &format!("{name}, not uploaded yet. Upload it to {}, or cancel to keep the skin you have.", acct.name())));
        });
        ui.add_space(tk::GUTTER);
        upload |= key(Nav::Activate) && busy.is_none();
    }
    if upload
        && busy.is_none()
        && let Some((png, slim)) = app.acct.pending.as_ref().map(|p| (p.png.clone(), p.slim))
    {
        skin_change(app, &ctx, &uuid, Change::Upload { png, slim });
    }

    // The arm model: switches a pending file, or re-sends the current skin.
    let capes = profile.as_ref().map(|p| p.capes.clone()).unwrap_or_default();
    if let Some(mut slim) = slim_now {
        let mut flip = false;
        kit::setting_group(ui, t, 4, "Look", "", |ui| {
            kit::SettingRow::new("Model").caption("arm width").show(ui, t, |ui| {
                ui.add_enabled_ui(busy.is_none(), |ui| flip = kit::segmented(ui, t, &mut slim, &[(false, "classic"), (true, "slim")]));
            });
        });
        if (flip || key(Nav::Toggle)) && busy.is_none() {
            let slim = if flip { slim } else { !slim };
            if let Some(p) = app.acct.pending.as_mut().filter(|p| p.uuid == uuid) {
                p.slim = slim;
            } else if let Some(s) = profile.as_ref().and_then(|p| p.skin()) {
                skin_change(app, &ctx, &uuid, Change::Variant { url: s.url.clone(), slim });
            }
        }
    }

    // Capes: none, or one of those the account owns.
    if capes.is_empty() {
        return;
    }
    let active = profile.as_ref().and_then(|p| p.cape()).map(|c| c.id.clone());
    let mut choices: Vec<(Option<String>, String)> = vec![(None, "No cape".into())];
    choices.extend(capes.iter().map(|c| (Some(c.id.clone()), c.alias.clone().unwrap_or_else(|| "Cape".into()))));
    kit::SectionLabel::new(5, "cape").aside(&format!("{} owned", capes.len())).show(ui, t);
    let mut pick = None;
    let mut list = HoverList::begin(ui, Variant::Compact).indent(tk::GUTTER + tk::HOVER_SHIFT_COMPACT);
    for (id, label) in &choices {
        let on = *id == active;
        let mut row = HoverRow::new(("cape", id), label).description(if id.is_some() { "wear this cape" } else { "wear none" }).selected(on);
        if on {
            row = row.badge("worn", t.ok());
        }
        if list.row(ui, t, row).clicked() && !on {
            pick = Some(id.clone());
        }
    }
    list.end(ui);
    if key(Nav::Cycle) {
        let at = choices.iter().position(|(id, _)| *id == active).unwrap_or(0);
        pick = Some(choices[(at + 1) % choices.len()].0.clone());
    }
    if let Some(id) = pick
        && busy.is_none()
    {
        skin_change(app, &ctx, &uuid, Change::Cape(id));
    }
}
