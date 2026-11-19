//! Asteria's design system: StarShell's language in egui. `tokens` names every value,
//! `system` follows the desktop (light/dark, reduced motion, fonts), `motion` eases, `fonts`
//! finds the type faces, `glyphs` names Phosphor icons, and `kit` is the widget kit.
//! `legacy` keeps the old screens drawing until each moves to the kit.

pub mod fonts;
pub mod glyphs;
pub mod kit;
pub mod legacy;
pub mod motion;
pub mod system;
pub mod tokens;

pub use legacy::Theme;
pub use system::Look;

/// Runs one frame for a test: a context with the kit's fonts (bundled and egui's only), at
/// `time` seconds.
#[cfg(test)]
pub fn test_frame(ctx: &egui::Context, time: f64, f: impl FnMut(&mut egui::Ui)) {
    // Unchanged definitions aren't reloaded.
    ctx.set_fonts(fonts::FontSet::default().definitions());
    let input = egui::RawInput { time: Some(time), ..Default::default() };
    let mut out = ctx.run_ui(input, f);
    out.textures_delta.clear();
}
