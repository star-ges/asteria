//! What the desktop says about how Asteria should look: the colour scheme and whether to
//! animate, from the XDG desktop portal's Settings interface (works on any desktop with a
//! portal; StarShell drives it through dconf), and fontconfig changes.
//!
//! egui's own `system_theme` can't be used: winit never reports a theme on Wayland.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use super::fonts::FontSet;
use super::legacy::Theme;
use super::motion::{self, Motion};
use super::tokens::{self, Palette, Tokens};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";
const COLOR_SCHEME: &str = "color-scheme";
/// GNOME's reduced-motion switch, which the GNOME and GTK portal backends expose.
const INTERFACE: &str = "org.gnome.desktop.interface";
const ANIMATIONS: &str = "enable-animations";

/// The portal's `color-scheme`: 1 is "prefer dark"; 0 (no preference) and 2 (prefer light)
/// are light, the original palette.
fn is_dark(value: &zbus::zvariant::Value) -> Option<bool> {
    match value {
        zbus::zvariant::Value::U32(n) => Some(*n == 1),
        // `Read` (older portals) wraps the value in one more variant.
        zbus::zvariant::Value::Value(inner) => is_dark(inner),
        _ => None,
    }
}

fn as_bool(value: &zbus::zvariant::Value) -> Option<bool> {
    match value {
        zbus::zvariant::Value::Bool(b) => Some(*b),
        zbus::zvariant::Value::Value(inner) => as_bool(inner),
        _ => None,
    }
}

fn settings_proxy(conn: &zbus::blocking::Connection) -> zbus::Result<zbus::blocking::Proxy<'static>> {
    zbus::blocking::Proxy::new(conn, PORTAL, PORTAL_PATH, SETTINGS)
}

fn read(proxy: &zbus::blocking::Proxy, ns: &str, key: &str) -> Option<zbus::zvariant::OwnedValue> {
    let call = |method: &str| proxy.call::<_, _, zbus::zvariant::OwnedValue>(method, &(ns, key)).ok();
    call("ReadOne").or_else(|| call("Read"))
}

/// What the desktop asks for. Missing settings read as light and animated.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Preference {
    pub dark: bool,
    pub reduced_motion: bool,
}

fn read_preference(proxy: &zbus::blocking::Proxy) -> Preference {
    Preference {
        dark: read(proxy, APPEARANCE, COLOR_SCHEME).and_then(|v| is_dark(&v)).unwrap_or(false),
        reduced_motion: read(proxy, INTERFACE, ANIMATIONS).and_then(|v| as_bool(&v)).is_some_and(|on| !on),
    }
}

/// Read once (at start, before the first frame).
pub fn preference() -> Preference {
    zbus::blocking::Connection::session()
        .ok()
        .and_then(|c| settings_proxy(&c).ok().map(|p| read_preference(&p)))
        .unwrap_or_default()
}

#[derive(Default)]
struct Pending {
    preference: Option<Preference>,
    fonts: Option<FontSet>,
}

/// Watches the portal (a D-Bus signal) and fontconfig (polled), leaving what changed for the
/// next frame.
fn spawn_watcher(ctx: egui::Context, start: Preference) -> Arc<Mutex<Pending>> {
    let pending = Arc::new(Mutex::new(Pending::default()));

    let out = pending.clone();
    let repaint = ctx.clone();
    std::thread::spawn(move || {
        let mut current = start;
        loop {
            // Each pass follows the portal until the connection ends, then reconnects
            // (the portal may start after Asteria, or restart).
            let _ = (|| -> zbus::Result<()> {
                let conn = zbus::blocking::Connection::session()?;
                let proxy = settings_proxy(&conn)?;
                let signals = proxy.receive_signal("SettingChanged")?;
                let publish = |current: &mut Preference, p: Preference| {
                    if p != *current {
                        *current = p;
                        out.lock().unwrap().preference = Some(p);
                        repaint.request_repaint();
                    }
                };
                publish(&mut current, read_preference(&proxy));
                for msg in signals {
                    let Ok((ns, key, value)) = msg.body().deserialize::<(String, String, zbus::zvariant::OwnedValue)>() else {
                        continue;
                    };
                    let mut p = current;
                    match (ns.as_str(), key.as_str()) {
                        (APPEARANCE, COLOR_SCHEME) => p.dark = is_dark(&value).unwrap_or(p.dark),
                        (INTERFACE, ANIMATIONS) => p.reduced_motion = as_bool(&value).map_or(p.reduced_motion, |on| !on),
                        _ => continue,
                    }
                    publish(&mut current, p);
                }
                Ok(())
            })();
            std::thread::sleep(Duration::from_secs(30));
        }
    });

    let out = pending.clone();
    std::thread::spawn(move || {
        let fonts_conf = dirs::config_dir().unwrap_or_default().join("fontconfig/fonts.conf");
        let mtime = |p: &PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let mut last: Option<SystemTime> = mtime(&fonts_conf);
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let now = mtime(&fonts_conf);
            if now != last {
                last = now;
                out.lock().unwrap().fonts = Some(FontSet::resolve());
                ctx.request_repaint();
            }
        }
    });
    pending
}

/// The look in use: tokens (crossfading between palettes), the old screens' `Theme` made
/// from the same colours, and the fonts.
pub struct Look {
    pub tokens: Tokens,
    pub theme: Theme,
    pub fonts: FontSet,
    preference: Preference,
    darkness: f32,
    watcher: Arc<Mutex<Pending>>,
}

impl Look {
    pub fn new(ctx: &egui::Context) -> Self {
        let preference = preference();
        let fonts = FontSet::resolve();
        fonts.apply(ctx);
        let tokens = Tokens { reduced_motion: preference.reduced_motion, ..Tokens::new(preference.dark) };
        let theme = Theme::from_palette(&tokens.p, preference.dark);
        theme.apply(ctx);
        motion::set_reduced(ctx, preference.reduced_motion);
        let watcher = spawn_watcher(ctx.clone(), preference);
        Self { tokens, theme, fonts, preference, darkness: if preference.dark { 1.0 } else { 0.0 }, watcher }
    }

    /// Takes what the watcher saw and moves the crossfade on. True when the colours changed
    /// this frame.
    pub fn frame(&mut self, ctx: &egui::Context) -> bool {
        let pending = std::mem::take(&mut *self.watcher.lock().unwrap());
        if let Some(fonts) = pending.fonts {
            fonts.apply(ctx);
            self.fonts = fonts;
        }
        if let Some(p) = pending.preference {
            self.preference = p;
        }
        motion::set_reduced(ctx, self.preference.reduced_motion);
        let target = if self.preference.dark { 1.0 } else { 0.0 };
        let darkness = motion::animate(ctx, egui::Id::new("asteria-darkness"), target, Motion::DarkFade);
        if darkness == self.darkness && self.tokens.dark == self.preference.dark && self.tokens.reduced_motion == self.preference.reduced_motion {
            return false;
        }
        self.darkness = darkness;
        self.tokens = Tokens {
            p: Palette::mix(&tokens::LIGHT, &tokens::DARK, darkness),
            dark: self.preference.dark,
            reduced_motion: self.preference.reduced_motion,
        };
        self.theme = Theme::from_palette(&self.tokens.p, self.preference.dark);
        self.theme.apply(ctx);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_portal_settings_read_either_shape() {
        use zbus::zvariant::Value;
        assert_eq!(is_dark(&Value::U32(1)), Some(true));
        assert_eq!(is_dark(&Value::U32(2)), Some(false));
        assert_eq!(is_dark(&Value::U32(0)), Some(false));
        assert_eq!(is_dark(&Value::Value(Box::new(Value::U32(1)))), Some(true));
        assert_eq!(as_bool(&Value::Bool(false)), Some(false));
        assert_eq!(as_bool(&Value::Value(Box::new(Value::Bool(true)))), Some(true));
    }
}
