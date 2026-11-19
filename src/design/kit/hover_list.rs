//! The hover list, StarShell's signature, for anything you choose from. Rows are split by
//! hairlines and muted at rest. While a row is hot (hovered, or keyboard-current: exactly the
//! same response) its title turns accent and shifts left, an uppercase description slides in
//! to its right, a few stars fade in *behind* the text and lines, and its picture, if it has
//! one, shows inline at the row's right end.
//!
//! - `Variant::Full`: 104px rows, Melodrama 64 titles, five stars.
//! - `Variant::Compact`: 56px rows, Switzer ExtraBold 20, three smaller stars.
//! - `Variant::Quiet`: 32px rows for long navigators (the configs tree): the title turns accent
//!   and that's all; no shift, description or stars.
//!
//! The selection (the item a page is about) is a faint wash of the text colour with the title
//! in `text_strong`, never an accent tint, so "accent" always means "the one you're reaching for".

use egui::{Color32, ImageSource, Rect, Response, Sense, Shape, Ui, layers::ShapeIdx, vec2};

use super::stars::Scatter;
use super::{rule, text};
use crate::design::fonts;
use crate::design::motion::{self, Motion};
use crate::design::tokens::{self as tk, Tokens};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Full,
    Compact,
    Quiet,
}

impl Variant {
    pub fn height(self) -> f32 {
        match self {
            Variant::Full => tk::LIST_ROW_HEIGHT,
            Variant::Compact => tk::LIST_ROW_HEIGHT_COMPACT,
            Variant::Quiet => tk::LIST_ROW_HEIGHT_QUIET,
        }
    }

    fn indent(self) -> f32 {
        match self {
            Variant::Full => tk::LIST_INDENT,
            Variant::Compact => tk::LIST_INDENT_COMPACT,
            Variant::Quiet => tk::LIST_INDENT_QUIET,
        }
    }

    fn shift(self) -> f32 {
        match self {
            Variant::Full => tk::HOVER_SHIFT,
            Variant::Compact => tk::HOVER_SHIFT_COMPACT,
            Variant::Quiet => 0.0,
        }
    }

    fn title_font(self, display_size: f32) -> egui::FontId {
        match self {
            Variant::Full => fonts::display(display_size),
            Variant::Compact => fonts::heading(tk::SIZE_LIST_COMPACT),
            Variant::Quiet => fonts::text(tk::SIZE_BODY),
        }
    }

    fn description(self) -> (f32, f32) {
        match self {
            Variant::Full => (tk::LIST_DESCRIPTION_GAP, tk::LIST_DESCRIPTION_WIDTH),
            _ => (tk::LIST_DESCRIPTION_GAP_COMPACT, tk::LIST_DESCRIPTION_WIDTH_COMPACT),
        }
    }

    fn preview_height(self) -> f32 {
        match self {
            Variant::Full => tk::LIST_PREVIEW_HEIGHT,
            _ => tk::LIST_PREVIEW_HEIGHT_COMPACT,
        }
    }
}

/// One list: call `row` for each item between `begin` and `end`. The stars of every row are
/// painted in one layer under the whole list.
pub struct HoverList {
    variant: Variant,
    imagery: ShapeIdx,
    shapes: Vec<Shape>,
    /// Melodrama size for full rows (one step down in narrow columns).
    display_size: f32,
}

pub struct HoverRow<'a> {
    pub id: egui::Id,
    pub title: &'a str,
    pub description: &'a str,
    /// The row's index caption (`01`) at the left, if any.
    pub number: Option<usize>,
    /// A picture shown inline at the right end while hot (content: an icon, a pack's logo).
    pub image: Option<(ImageSource<'a>, f32)>,
    /// A picture at the left of the title, always shown (a mod's icon in a compact list).
    pub lead: Option<ImageSource<'a>>,
    /// A status that stays visible at the right end (`RUNNING`, in a clarity hue or muted).
    pub badge: Option<(&'a str, Color32)>,
    /// A group's clarity hue: a 3px tick at the row's left edge.
    pub hue: Option<Color32>,
    /// Keyboard-current, from the pane's `Nav`.
    pub hot: bool,
    pub selected: bool,
    /// Muted even when hot (a disabled mod): the response stays, the title is fainter.
    pub dim: bool,
    /// Quiet rows: how far in (a tree's depth) and a count or aside at the right.
    pub depth: usize,
    pub aside: &'a str,
    pub sense: Sense,
}

impl<'a> HoverRow<'a> {
    pub fn new(id: impl std::hash::Hash + std::fmt::Debug, title: &'a str) -> Self {
        Self {
            id: egui::Id::new(id),
            title,
            description: "",
            number: None,
            image: None,
            lead: None,
            badge: None,
            hue: None,
            hot: false,
            selected: false,
            dim: false,
            depth: 0,
            aside: "",
            sense: Sense::click(),
        }
    }

    pub fn description(mut self, d: &'a str) -> Self {
        self.description = d;
        self
    }

    pub fn number(mut self, n: usize) -> Self {
        self.number = Some(n);
        self
    }

    /// `aspect` is width / height.
    pub fn image(mut self, src: impl Into<ImageSource<'a>>, aspect: f32) -> Self {
        self.image = Some((src.into(), aspect));
        self
    }

    pub fn lead(mut self, src: impl Into<ImageSource<'a>>) -> Self {
        self.lead = Some(src.into());
        self
    }

    pub fn badge(mut self, text: &'a str, color: Color32) -> Self {
        self.badge = Some((text, color));
        self
    }

    pub fn hue(mut self, hue: Option<Color32>) -> Self {
        self.hue = hue;
        self
    }

    pub fn hot(mut self, on: bool) -> Self {
        self.hot = on;
        self
    }

    pub fn selected(mut self, on: bool) -> Self {
        self.selected = on;
        self
    }

    pub fn dim(mut self, on: bool) -> Self {
        self.dim = on;
        self
    }

    pub fn depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    pub fn aside(mut self, aside: &'a str) -> Self {
        self.aside = aside;
        self
    }

    /// E.g. `Sense::click_and_drag()` for rows that can be dragged between groups.
    pub fn sense(mut self, sense: Sense) -> Self {
        self.sense = sense;
        self
    }
}

impl HoverList {
    pub fn begin(ui: &mut Ui, variant: Variant) -> Self {
        let imagery = ui.painter().add(Shape::Noop);
        // One step down when the title column is narrow.
        let display_size = if ui.available_width() - tk::LIST_INDENT - tk::LIST_DESCRIPTION_WIDTH < 480.0 { tk::SIZE_DISPLAY_SMALL } else { tk::SIZE_DISPLAY };
        Self { variant, imagery, shapes: Vec::new(), display_size }
    }

    pub fn row(&mut self, ui: &mut Ui, t: &Tokens, row: HoverRow) -> Response {
        let v = self.variant;
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), v.height()), row.sense);
        if !ui.is_rect_visible(rect.expand(tk::STAR_SIZE_MAX)) {
            return resp;
        }
        let ctx = ui.ctx().clone();
        let hot = resp.hovered() || resp.has_focus() || row.hot;
        let id = row.id;
        let shift = motion::toggle(&ctx, id.with("shift"), hot, Motion::Medium) * v.shift();
        let rest = if row.selected { t.text_strong } else { t.text_muted };
        let rest = if row.dim { tk::alpha_of(t.text_muted, 0.6) } else { rest };
        let hot_c = if row.dim { tk::alpha_of(t.accent, 0.7) } else { t.accent };
        let title_c = motion::color(&ctx, id.with("title"), hot, rest, hot_c);
        let reveal = if v == Variant::Quiet {
            0.0
        } else if hot {
            motion::animate_delayed(&ctx, id.with("reveal"), 1.0, Motion::Fast, tk::DESCRIPTION_DELAY)
        } else {
            motion::animate_timed(&ctx, id.with("reveal"), 0.0, Motion::Fast, tk::FAST, 0.0)
        };

        let p = ui.painter();
        if row.selected {
            p.rect_filled(rect, 0.0, t.wash(0.06));
        }
        let cy = rect.center().y;
        if let Some(n) = row.number {
            let c = motion::color(&ctx, id.with("number"), hot, t.text_muted, t.accent);
            text::paint_caption(p, egui::pos2(rect.left() + tk::GUTTER, cy), &text::number(n), c);
        }
        if let Some(hue) = row.hue {
            let h = (v.height() * 0.4).round();
            p.rect_filled(Rect::from_min_size(egui::pos2(rect.left(), cy - h / 2.0), vec2(tk::MARKER, h)), 0.0, hue);
        }

        // The right end: badge (at rest), picture (while hot), quiet aside.
        let right_edge = rect.right() - tk::GUTTER;
        let mut end_x = right_edge;
        let preview = row.image.as_ref().map(|(_, aspect)| {
            let h = v.preview_height();
            vec2((h * aspect).round(), h)
        });
        if let Some(size) = preview {
            end_x = end_x.min(right_edge - size.x);
        }
        let mut badge_g = None;
        if let Some((b, color)) = row.badge {
            let g = text::caption_galley(ui, b, Color32::PLACEHOLDER);
            end_x = end_x.min(right_edge - g.size().x);
            badge_g = Some((g, color));
        }
        if v == Variant::Quiet && !row.aside.is_empty() {
            let g = text::caption_galley(ui, row.aside, t.text_muted);
            p.galley(egui::pos2(right_edge - g.size().x, cy - g.size().y / 2.0), g.clone(), t.text_muted);
            end_x = end_x.min(right_edge - g.size().x);
        }

        // Title (and lead picture), shifted.
        let mut x = rect.left() + v.indent() + row.depth as f32 * tk::GAP * 1.5 - shift;
        if let Some(src) = row.lead.clone() {
            let s = (v.height() * 0.57).round();
            let r = Rect::from_min_size(egui::pos2(x, cy - s / 2.0), vec2(s, s));
            let mut img = egui::Image::new(src).corner_radius(0);
            if row.dim {
                img = img.tint(Color32::from_white_alpha(110));
            }
            img.paint_at(ui, r);
            x += s + tk::GAP;
        }
        let (desc_gap, desc_w) = v.description();
        let has_desc = !row.description.is_empty() && v != Variant::Quiet;
        // The title keeps room for at least a compact description beside it; the description
        // takes what's left, up to its full width.
        let reserve = if has_desc { desc_gap + desc_w.min(tk::LIST_DESCRIPTION_WIDTH_COMPACT) } else { 0.0 };
        let room = (end_x - tk::GUTTER - x - reserve).max(40.0);
        let mut job = egui::text::LayoutJob::simple_singleline(row.title.to_string(), v.title_font(self.display_size), title_c);
        job.wrap.max_width = room;
        job.wrap.max_rows = 1;
        let title_g = p.layout_job(job);
        let title_r = Rect::from_min_size(egui::pos2(x, cy - title_g.size().y / 2.0), title_g.size());
        p.galley(title_r.min, title_g, title_c);

        let desc_w = desc_w.min(end_x - tk::GUTTER - title_r.right() - desc_gap).max(tk::LIST_DESCRIPTION_WIDTH_COMPACT.min(desc_w));
        if has_desc && reveal > 0.0 {
            let c = tk::alpha_of(t.text_strong, reveal);
            let job = text::caption_wrapped(row.description, c, desc_w, if v == Variant::Full { tk::LIST_DESCRIPTION_LINES } else { 2 });
            let g = p.layout_job(job);
            let dx = title_r.right() + desc_gap + (1.0 - reveal) * tk::DESCRIPTION_OFFSET;
            p.galley(egui::pos2(dx, cy - g.size().y / 2.0), g, c);
        }
        if let (Some(size), Some((src, _))) = (preview, row.image.clone())
            && reveal > 0.0
        {
            let r = Rect::from_min_size(egui::pos2(right_edge - size.x + (1.0 - reveal) * tk::DESCRIPTION_OFFSET, cy - size.y / 2.0), size);
            egui::Image::new(src).corner_radius(0).tint(Color32::from_white_alpha((reveal * 255.0) as u8)).paint_at(ui, r);
        }
        if let Some((g, color)) = badge_g {
            let a = if preview.is_some() { 1.0 - reveal } else { 1.0 };
            if a > 0.0 {
                let c = tk::alpha_of(color, a);
                ui.painter().galley(egui::pos2(right_edge - g.size().x, cy - g.size().y / 2.0), g, c);
            }
        }
        rule::hline(ui.painter(), rect.x_range(), rect.bottom() - 0.5, t.line);

        // Stars, behind everything in the list.
        if v != Variant::Quiet && !t.reduced_motion {
            let scatter = Scatter {
                row: rect,
                text_left: title_r.left(),
                text_right: if has_desc { title_r.right() + desc_gap + desc_w } else { title_r.right() },
                left_limit: rect.left() + if row.number.is_some() { tk::GUTTER * 2.0 } else { 0.0 },
                right_limit: if preview.is_some() || badge_g_present(&row) { end_x - tk::GAP } else { rect.right() },
                seed: id.value(),
                compact: v == Variant::Compact,
            };
            let n = scatter.count();
            let shown: Vec<f32> = (0..n)
                .map(|i| {
                    let pid = id.with(("star", i));
                    if hot {
                        motion::animate_delayed(&ctx, pid, 1.0, Motion::Imagery, i as f32 * tk::IMAGERY_STAGGER)
                    } else {
                        // Leaving is quicker and unstaggered, so the next row reads as the new focus.
                        motion::animate_timed(&ctx, pid, 0.0, Motion::Imagery, tk::IMAGERY / 2.0, 0.0)
                    }
                })
                .collect();
            scatter.paint(&mut self.shapes, t.accent, t.text_strong, |i| shown[i]);
        }

        let mut resp = resp;
        if row.sense.senses_click() {
            resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
        let (title, selected) = (row.title.to_string(), row.selected);
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &title));
        resp
    }

    pub fn end(self, ui: &mut Ui) {
        ui.painter().set(self.imagery, Shape::Vec(self.shapes));
    }
}

fn badge_g_present(row: &HoverRow) -> bool {
    row.badge.is_some()
}
