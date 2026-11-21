//! Asteria's design system: StarShell's language in egui. `tokens` names every value,
//! `system` follows the desktop (light/dark, reduced motion, fonts), `motion` eases, `fonts`
//! finds the type faces, `glyphs` names Phosphor icons, `style` dresses egui's stock widgets
//! from the tokens, and `kit` is the widget kit.

pub mod fonts;
pub mod glyphs;
pub mod kit;
pub mod motion;
pub mod style;
pub mod system;
pub mod tokens;

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
