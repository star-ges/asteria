//! Stars: StarShell's four- and five-pointed star, and the scatter of them that fades in
//! behind a hover-list row while it's hot. The only "space" decoration in Asteria.

use egui::{Color32, Mesh, Pos2, Rect, Shape, Stroke, pos2, vec2};

use crate::design::tokens as tk;

/// The star's outline: `points` tips at `radius`, notches at `radius * inner`, the first tip
/// straight up, turned by `rotation` radians.
pub fn star_points(center: Pos2, radius: f32, points: usize, inner: f32, rotation: f32) -> Vec<Pos2> {
    (0..points * 2)
        .map(|i| {
            let r = if i % 2 == 0 { radius } else { radius * inner };
            let a = std::f32::consts::PI * i as f32 / points as f32 - std::f32::consts::FRAC_PI_2 + rotation;
            pos2(center.x + r * a.cos(), center.y + r * a.sin())
        })
        .collect()
}

/// A star's shapes, filled or outlined, pushed onto `out`. The fill is a fan from the centre (a star isn't convex,
/// which egui's polygon fill needs) with an antialiased outline over its edge.
pub fn paint_star(out: &mut Vec<Shape>, center: Pos2, radius: f32, points: usize, inner: f32, rotation: f32, color: Color32, outlined: bool) {
    let pts = star_points(center, radius, points, inner, rotation);
    if !outlined {
        let mut mesh = Mesh::default();
        mesh.colored_vertex(center, color);
        for p in &pts {
            mesh.colored_vertex(*p, color);
        }
        let n = pts.len() as u32;
        for i in 0..n {
            mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
        }
        out.push(Shape::mesh(mesh));
    }
    let width = if outlined { tk::LINE } else { 0.5 };
    out.push(Shape::closed_line(pts, Stroke::new(width, color)));
}

/// Deterministic pseudo-random in [0, 1) for piece `i`, channel `k`: the same row always
/// scatters the same way, so hovering back and forth doesn't reshuffle.
pub fn rand(seed: u64, i: usize, k: usize) -> f32 {
    let v = (((seed % 100_000) as f64 + 1.0) * 12.9898 + (i as f64 + 1.0) * 78.233 + (k as f64 + 1.0) * 37.719).sin() * 43758.5453;
    (v - v.floor()) as f32
}

fn between(lo: f32, hi: f32, t: f32) -> f32 {
    lo + (hi - lo) * t
}

/// Where the stars may go around a row.
pub struct Scatter {
    pub row: Rect,
    /// The text's horizontal extent: stars keep clear of it.
    pub text_left: f32,
    pub text_right: f32,
    pub left_limit: f32,
    pub right_limit: f32,
    pub seed: u64,
    pub compact: bool,
    /// What the list is drawn within (a scroll area's view): a star it would cut is left out,
    /// rather than showing only its tips.
    pub clip: Rect,
}

impl Scatter {
    pub fn count(&self) -> usize {
        if self.compact { tk::STAR_COUNT_COMPACT } else { tk::STAR_COUNT }
    }

    /// Piece `i`'s square: alternate sides, starting on the right; on the right if the left
    /// margin is too narrow. They may reach past the row's top and bottom.
    pub fn place(&self, i: usize) -> Rect {
        let (min, max) = if self.compact { (tk::STAR_SIZE_MIN_COMPACT, tk::STAR_SIZE_MAX_COMPACT) } else { (tk::STAR_SIZE_MIN, tk::STAR_SIZE_MAX) };
        let size = between(min, max, rand(self.seed, i, 0)).round();
        let pad = tk::UNIT;
        let left_room = self.text_left - self.left_limit - pad * 2.0 - size;
        let right = i % 2 == 0 || left_room <= pad;
        let (lo, hi) = if right { (self.text_right + tk::GUTTER, self.right_limit - size - pad) } else { (self.left_limit + pad, self.text_left - size - pad) };
        let x = between(lo, hi.max(lo), rand(self.seed, i, 1));
        let h = self.row.height();
        let y = self.row.top() + between(-0.45 * h, 1.45 * h - size, rand(self.seed, i, 2));
        Rect::from_min_size(pos2(x, y), vec2(size, size))
    }

    /// Every piece's shapes; `shown(i)` is its 0–1 progress (fade, scale from 0.92, drift up 18px).
    pub fn paint(&self, out: &mut Vec<Shape>, accent: Color32, ink: Color32, shown: impl Fn(usize) -> f32) {
        for i in 0..self.count() {
            let s = shown(i);
            if s <= 0.0 {
                continue;
            }
            let r = self.place(i);
            if r.right() > self.right_limit || r.left() < self.left_limit {
                continue;
            }
            // Drifting in, it starts lower down.
            if !self.clip.contains_rect(r.with_max_y(r.max.y + tk::IMAGERY_DRIFT)) {
                continue;
            }
            let five = rand(self.seed, i, 3) >= 0.7;
            let points = if five { 5 } else { 4 };
            let inner = if five { tk::STAR_INNER_RATIO * 2.0 } else { tk::STAR_INNER_RATIO };
            let outlined = rand(self.seed, i, 4) < 0.35;
            let color = if rand(self.seed, i, 5) < 0.6 { accent } else { ink };
            let rotation = if five { rand(self.seed, i, 6) * 72f32.to_radians() } else { 0.0 };
            let scale = tk::IMAGERY_START_SCALE + (1.0 - tk::IMAGERY_START_SCALE) * s;
            let center = r.center() + vec2(0.0, tk::IMAGERY_DRIFT * (1.0 - s));
            paint_star(out, center, r.width() / 2.0 * scale, points, inner, rotation, tk::alpha_of(color, s * color.a() as f32 / 255.0), outlined);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row at a scroll area's edge shows whole stars or none: never a star cut down to its tips.
    #[test]
    fn stars_a_scroll_area_would_cut_are_left_out() {
        let row = Rect::from_min_size(pos2(0.0, 100.0), vec2(1000.0, 56.0));
        for seed in 0..200u64 {
            let clip = Rect::from_min_max(pos2(0.0, 100.0), pos2(1000.0, 400.0));
            let s = Scatter { row, text_left: 40.0, text_right: 200.0, left_limit: 0.0, right_limit: 1000.0, seed, compact: true, clip };
            let mut out = Vec::new();
            s.paint(&mut out, Color32::RED, Color32::BLACK, |_| 1.0);
            for shape in &out {
                let b = shape.visual_bounding_rect();
                assert!(clip.expand(1.0).contains_rect(b), "seed {seed}: a star at {b:?} is cut by {clip:?}");
            }
        }
    }
}
