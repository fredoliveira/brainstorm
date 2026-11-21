//! Every size and colour in the overlays, in one place to tweak. Sizes are in points.
//!
//! The overlays sit on top of a game, so they have one dark look of their own instead of
//! following the system's light or dark mode. The windows' sizes are in `window.rs` and
//! `settings_window.rs`, with the theme's colours.

/// The overlays' metrics: one layout, uniformly scaled.
#[derive(Clone, Copy, PartialEq)]
pub struct Layout {
    pub scale: f32,
}

impl Layout {
    pub fn new(compact: bool) -> Self {
        Self { scale: if compact { 0.8 } else { 1. } }
    }

    fn scaled(&self, points: f32) -> f32 {
        (points * self.scale).round()
    }
    pub fn width(&self) -> f32 {
        self.scaled(300.)
    }
    pub fn header(&self) -> f32 {
        self.scaled(34.)
    }
    pub fn padding(&self) -> f32 {
        self.scaled(6.)
    }
    /// A row of the draft panel: art, name, type and win rate.
    pub fn draft_row(&self) -> f32 {
        self.scaled(44.)
    }
    /// A row of the deck panel: art, name, cost and type, copies left. Shorter than a draft
    /// row, since a deck has more cards than a pack.
    pub fn deck_row(&self) -> f32 {
        self.scaled(36.)
    }
    /// A panel's height with `rows` rows of height `row`.
    pub fn height(&self, rows: usize, row: f32) -> f32 {
        self.header() + rows as f32 * row + self.padding()
    }
}

pub const RADIUS: f32 = 10.;
pub const HEADER_TEXT: f32 = 11.;
pub const PADDING_X: f32 = 10.;
/// Between the collapse button that ends a header and the panel's right side.
pub const HEADER_END: f32 = 6.;

// 0xRRGGBBAA
pub const PANEL: u32 = 0x12141AF0;
pub const BORDER: u32 = 0xFFFFFF1F;
pub const TEXT: u32 = 0xFFFFFFFF;
pub const MUTED: u32 = 0xFFFFFF99;
pub const HOVER: u32 = 0xFFFFFF14;
pub const PILL: u32 = 0xFFFFFF12;
pub const PILL_HOVER: u32 = 0xFFFFFF24;
pub const NOTE: u32 = 0xFF9F40FF;
pub const THUMB: u32 = 0xFFFFFF0F;

// Rarity, on the bar at a draft row's left edge.
pub const MYTHIC: u32 = 0xED6B29FF;
pub const RARE: u32 = 0xD9B352FF;
pub const UNCOMMON: u32 = 0x9EB8CCFF;
pub const COMMON: u32 = 0xFFFFFF2E;

// A win rate against the set's mean, from standouts down.
pub const GRADES: [(f64, u32); 4] = [(1.25, 0x59E680FF), (0.4, 0x9EDB8FFF), (-0.4, 0xFFFFFFE6), (-1.25, 0xF2B373FF)];
pub const GRADE_WORST: u32 = 0xF2736BFF;

/// A mana colour's dot.
pub fn pip(color: char) -> u32 {
    match color {
        'W' => 0xF7F0CCFF,
        'U' => 0x408FE6FF,
        'B' => 0x6B5C75FF,
        'R' => 0xE04D3DFF,
        'G' => 0x40A861FF,
        _ => 0x9AA0A6FF,
    }
}

// ── Placement ────────────────────────────────────────────────────────────────
// A panel hangs from a point near the top-right corner of Arena's window.

/// In from Arena's right edge.
pub const ANCHOR_INSET: f64 = 8.;
/// Down from Arena's top edge, as a share of the window's height: just below the draft
/// screen's header row at any window size.
pub const ANCHOR_DROP: f64 = 0.16;

/// A collapsed panel: a tab flush against Arena's right edge, at the height the panel hangs
/// from. Not scaled with the panels, so it stays easy to hit.
pub const TAB_WIDTH: f32 = 14.;
pub const TAB_HEIGHT: f32 = 56.;
pub const TAB_RADIUS: f32 = 7.;

/// The card shown beside a row under the pointer.
pub const PREVIEW_WIDTH: f64 = 280.;
pub const PREVIEW_HEIGHT: f64 = 390.;
pub const PREVIEW_GAP: f64 = 8.;
pub const PREVIEW_RADIUS: f32 = 13.;
