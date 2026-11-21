//! The widget kit: StarShell's components in egui. Every part takes the tokens as `t`, draws
//! with them only, and registers AccessKit info (role and label), so custom-painted parts stay
//! visible to screen readers. Hot (hovered or keyboard-current, from the pane's `Nav`) always
//! gets the same response.

pub mod bar;
pub mod button;
pub mod field;
pub mod hover_list;
pub mod row;
pub mod rule;
pub mod setting;
pub mod sheet;
pub mod slider;
pub mod stars;
pub mod switch;
pub mod table;
pub mod text;
pub mod tiles;

pub use bar::{BarCell, bar, mark_cell};
pub use button::{Button, button_row, menu, segmented};
pub use field::{Field, query};
pub use hover_list::{HoverList, HoverRow, Variant};
pub use row::QuietRow;
pub use rule::{SectionLabel, hline, rule, vline};
pub use setting::{SettingGroup, SettingRow, choice_row, setting_group};
pub use sheet::{NoticeKind, banner, Spec, bracketed, empty_state, loading, notice, sheet, spec_strip, strip};
pub use slider::{Fill, Slider, meter};
pub use switch::{checkbox, switch};
pub use table::{Cell, Column};
pub use text::{body, body_muted, caption, caption_colored, display, figure, heading, heading_small, icon, key_hint};
pub use tiles::Tiles;
