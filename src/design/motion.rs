//! Motion helpers: ease-out curves and eased animations kept in egui's memory. A repaint is
//! requested only while something is moving, so nothing redraws when you aren't doing
//! anything. When the desktop asks for no animations every duration is 0.

use egui::{Context, Id};

use super::tokens;

/// One easing family: OutCubic for colour and controls, OutExpo for movement and imagery.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    /// Colour and controls (180ms OutCubic).
    Fast,
    /// Shifts, reveals, panels, switches (280ms OutExpo).
    Medium,
    /// Stars and inline pictures (350ms OutExpo).
    Imagery,
    /// A big picture changing (700ms OutCubic crossfade).
    ImageFade,
    /// Light ⇄ dark (600ms OutCubic).
    DarkFade,
}

impl Motion {
    pub fn seconds(self) -> f32 {
        match self {
            Motion::Fast => tokens::FAST,
            Motion::Medium => tokens::MEDIUM,
            Motion::Imagery => tokens::IMAGERY,
            Motion::ImageFade => tokens::IMAGE_FADE,
            Motion::DarkFade => tokens::DARK_FADE,
        }
    }

    pub fn ease(self, t: f32) -> f32 {
        match self {
            Motion::Fast | Motion::ImageFade | Motion::DarkFade => ease_out_cubic(t),
            Motion::Medium | Motion::Imagery => ease_out_expo(t),
        }
    }
}

pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_out_expo(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t >= 1.0 { 1.0 } else { 1.0 - 2f32.powf(-10.0 * t) }
}

fn reduced_id() -> Id {
    Id::new("asteria-reduced-motion")
}

/// Set from the desktop's `enable-animations` each frame (see `Look`).
pub fn set_reduced(ctx: &Context, reduced: bool) {
    ctx.data_mut(|d| d.insert_temp(reduced_id(), reduced));
}

pub fn reduced(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(reduced_id())).unwrap_or(false)
}

#[derive(Clone, Copy)]
struct Anim {
    from: f32,
    to: f32,
    /// When the move towards `to` began (egui's input time, seconds).
    start: f64,
    delay: f32,
    duration: f32,
    motion: Motion,
}

/// `value` eased towards `target` over `motion`, starting `delay` seconds after the target
/// changes. The first call for an id starts at the target, so nothing animates in on open.
pub fn animate_delayed(ctx: &Context, id: Id, target: f32, motion: Motion, delay: f32) -> f32 {
    animate_timed(ctx, id, target, motion, motion.seconds(), delay)
}

/// Like `animate_delayed` with `motion`'s curve but its own duration (leaving faster than arriving).
pub fn animate_timed(ctx: &Context, id: Id, target: f32, motion: Motion, seconds: f32, delay: f32) -> f32 {
    let now = ctx.input(|i| i.time);
    let reduced = reduced(ctx);
    let (seconds, delay) = if reduced { (0.0, 0.0) } else { (seconds, delay) };
    let id = id.with("asteria-anim");
    let stored = ctx.data(|d| d.get_temp::<Anim>(id));
    let a = match stored {
        None => {
            let a = Anim { from: target, to: target, start: now, delay: 0.0, duration: 0.0, motion };
            ctx.data_mut(|d| d.insert_temp(id, a));
            a
        }
        // Restart from wherever the value is now, so reversing mid-move doesn't jump.
        Some(a) if a.to != target => {
            let a = Anim { from: value_at(&a, now), to: target, start: now, delay, duration: seconds, motion };
            ctx.data_mut(|d| d.insert_temp(id, a));
            a
        }
        Some(a) => a,
    };
    let v = value_at(&a, now);
    if v != a.to {
        ctx.request_repaint();
    }
    v
}

fn value_at(a: &Anim, now: f64) -> f32 {
    let elapsed = (now - a.start) as f32 - a.delay;
    if a.duration <= 0.0 || elapsed >= a.duration {
        return a.to;
    }
    if elapsed <= 0.0 {
        return a.from;
    }
    a.from + (a.to - a.from) * a.motion.ease(elapsed / a.duration)
}

/// `animate_delayed` with no delay.
pub fn animate(ctx: &Context, id: Id, target: f32, motion: Motion) -> f32 {
    animate_delayed(ctx, id, target, motion, 0.0)
}

/// 0 → 1 while `on`, eased; back down when it turns off.
pub fn toggle(ctx: &Context, id: Id, on: bool, motion: Motion) -> f32 {
    animate(ctx, id, if on { 1.0 } else { 0.0 }, motion)
}

/// A colour moving between `off` and `on` (FAST), for hover and focus.
pub fn color(ctx: &Context, id: Id, on: bool, off_color: egui::Color32, on_color: egui::Color32) -> egui::Color32 {
    let t = toggle(ctx, id.with("colour"), on, Motion::Fast);
    off_color.lerp_to_gamma(on_color, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_start_at_0_and_end_at_1_without_overshoot() {
        for ease in [ease_out_cubic, ease_out_expo] {
            assert_eq!(ease(0.0), 0.0);
            assert_eq!(ease(1.0), 1.0);
            let mut last = 0.0;
            for i in 1..=100 {
                let v = ease(i as f32 / 100.0);
                assert!(v >= last && v <= 1.0, "no bounce or overshoot");
                last = v;
            }
        }
    }

    #[test]
    fn values_ease_to_their_target_and_hold_still_after() {
        let ctx = Context::default();
        let id = Id::new("x");
        let frame = |t: f64, target: f32| {
            let mut out = 0.0;
            crate::design::test_frame(&ctx, t, |ui| out = animate(ui.ctx(), id, target, Motion::Medium));
            out
        };
        assert_eq!(frame(0.0, 0.0), 0.0, "the first value is the target: nothing animates in on open");
        assert_eq!(frame(0.1, 1.0), 0.0, "a new target starts from where the value was");
        let mid = frame(0.2, 1.0);
        assert!(mid > 0.0 && mid < 1.0);
        assert_eq!(frame(0.1 + tokens::MEDIUM as f64, 1.0), 1.0);
        assert_eq!(frame(5.0, 1.0), 1.0);
    }

    #[test]
    fn reduced_motion_jumps_straight_to_the_target() {
        let ctx = Context::default();
        let id = Id::new("y");
        let mut out = 0.0;
        for (t, target) in [(0.0, 0.0), (0.1, 1.0)] {
            crate::design::test_frame(&ctx, t, |ui| {
                set_reduced(ui.ctx(), true);
                out = animate(ui.ctx(), id, target, Motion::Medium);
            });
        }
        assert_eq!(out, 1.0);
    }
}
