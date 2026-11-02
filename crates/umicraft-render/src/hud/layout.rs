// SPDX-License-Identifier: MIT OR Apache-2.0

//! The HUD geometry: [`HudLayout`] derives every rectangle of the vanilla-style
//! HUD from the viewport size, independent of how it is drawn.
//!
//! The layout is derived from the viewport size in *physical* pixels with the
//! same integer scale convention as the client debug panel,
//! `scale = (height / 360).floor().clamp(1, 3)`.

use super::state::{HOTBAR_SLOTS, STATUS_ICONS};

/// Viewport height that maps to a scale of 1.
const REFERENCE_HEIGHT: f32 = 360.0;
/// Smallest HUD scale.
const MIN_SCALE: f32 = 1.0;
/// Largest HUD scale.
const MAX_SCALE: f32 = 3.0;

/// Edge of one hotbar slot, in unscaled pixels.
const SLOT_SIZE: f32 = 20.0;
/// Height of a status icon, in unscaled pixels.
const ICON_HEIGHT: f32 = 9.0;
/// Height of the experience bar, in unscaled pixels.
const XP_HEIGHT: f32 = 6.0;
/// Vertical gap between two stacked bottom elements, in unscaled pixels.
const STACK_GAP: f32 = 1.0;
/// Distance between the hotbar and the bottom of the viewport, unscaled.
const BOTTOM_MARGIN: f32 = 4.0;
/// Height of one chat or debug line, in unscaled pixels.
const LINE_HEIGHT: f32 = 8.0;
/// Left edge of the chat block, in pixels.
const CHAT_LEFT: f32 = 4.0;
/// Left edge of the debug panel, in pixels.
pub(super) const DEBUG_LEFT: f32 = 4.0;
/// Top edge of the debug panel, in pixels.
pub(super) const DEBUG_TOP: f32 = 4.0;
/// Top-left corner of the debug text, in pixels.
const DEBUG_TEXT_X: f32 = 10.0;
/// Top-left corner of the debug text, in pixels.
const DEBUG_TEXT_Y: f32 = 10.0;
/// Horizontal padding of the debug panel, in pixels.
pub(super) const DEBUG_PADDING_X: f32 = 20.0;
/// Vertical padding of the debug panel, in pixels.
pub(super) const DEBUG_PADDING_Y: f32 = 12.0;

/// A screen-space rectangle in physical pixels, with a top-left origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge, in pixels from the left of the viewport.
    pub x: f32,
    /// Top edge, in pixels from the top of the viewport.
    pub y: f32,
    /// Width, in pixels.
    pub width: f32,
    /// Height, in pixels.
    pub height: f32,
}

impl Rect {
    /// Creates a rectangle from its top-left corner and its size.
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The right edge, in pixels.
    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    /// The bottom edge, in pixels.
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// True when `other` lies completely inside this rectangle.
    #[must_use]
    pub fn contains(&self, other: &Rect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }
}

/// The three status rows of the HUD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusRow {
    /// Hearts, the row just above the experience bar.
    Health,
    /// Food, the row above the health row.
    Food,
    /// Armour, the row above the food row; only drawn when the value is
    /// non-zero.
    Armor,
}

/// How much of a status icon is filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// The icon is empty.
    Empty,
    /// The left half of the icon is filled.
    Half,
    /// The icon is completely filled.
    Full,
}

/// The HUD geometry for one viewport size.
///
/// The layout is a pure function of the viewport size, so it can be computed
/// and tested without a GPU. All rectangles are in physical pixels with a
/// top-left origin, which is the coordinate system [`Overlay`] expects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HudLayout {
    width: f32,
    height: f32,
    scale: f32,
}

impl HudLayout {
    /// Computes the layout for a `width`×`height` viewport in physical pixels.
    ///
    /// The scale follows the client debug panel convention,
    /// `(height / 360).floor().clamp(1, 3)`.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let width = width.max(1) as f32;
        let height = height.max(1) as f32;
        let scale = (height / REFERENCE_HEIGHT)
            .floor()
            .clamp(MIN_SCALE, MAX_SCALE);
        Self {
            width,
            height,
            scale,
        }
    }

    /// The viewport width, in pixels.
    #[must_use]
    pub fn width(&self) -> f32 {
        self.width
    }

    /// The viewport height, in pixels.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.height
    }

    /// The integer UI scale derived from the viewport height.
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The height of one chat or debug line, in pixels.
    #[must_use]
    pub fn line_height(&self) -> f32 {
        LINE_HEIGHT * self.scale
    }

    /// The whole hotbar, centred at the bottom of the viewport.
    #[must_use]
    pub fn hotbar_rect(&self) -> Rect {
        let size = SLOT_SIZE * self.scale;
        let width = HOTBAR_SLOTS as f32 * size;
        Rect {
            x: (self.width - width) * 0.5,
            y: self.height - size - BOTTOM_MARGIN * self.scale,
            width,
            height: size,
        }
    }

    /// The rectangle of hotbar slot `index`; out-of-range indices are clamped.
    #[must_use]
    pub fn slot_rect(&self, index: usize) -> Rect {
        let hotbar = self.hotbar_rect();
        let size = SLOT_SIZE * self.scale;
        let index = index.min(HOTBAR_SLOTS - 1);
        Rect {
            x: hotbar.x + index as f32 * size,
            y: hotbar.y,
            width: size,
            height: size,
        }
    }

    /// The experience bar, directly above the hotbar.
    #[must_use]
    pub fn xp_bar_rect(&self) -> Rect {
        let hotbar = self.hotbar_rect();
        Rect {
            x: hotbar.x,
            y: hotbar.y - STACK_GAP * self.scale - XP_HEIGHT * self.scale,
            width: hotbar.width,
            height: XP_HEIGHT * self.scale,
        }
    }

    /// The whole status row; its width is the hotbar width, so the ten icons
    /// exactly fill the hotbar.
    #[must_use]
    pub fn status_row_rect(&self, row: StatusRow) -> Rect {
        let hotbar = self.hotbar_rect();
        let height = ICON_HEIGHT * self.scale;
        let gap = STACK_GAP * self.scale;
        let health_top = self.xp_bar_rect().y - gap - height;
        let food_top = health_top - gap - height;
        let armor_top = food_top - gap - height;
        let y = match row {
            StatusRow::Health => health_top,
            StatusRow::Food => food_top,
            StatusRow::Armor => armor_top,
        };
        Rect {
            x: hotbar.x,
            y,
            width: hotbar.width,
            height,
        }
    }

    /// The rectangle of one icon of a status row; out-of-range indices are
    /// clamped.
    #[must_use]
    pub fn status_icon_rect(&self, row: StatusRow, index: usize) -> Rect {
        let row_rect = self.status_row_rect(row);
        let index = index.min(STATUS_ICONS - 1);
        let width = row_rect.width / STATUS_ICONS as f32;
        Rect {
            x: row_rect.x + index as f32 * width,
            y: row_rect.y,
            width,
            height: row_rect.height,
        }
    }

    /// The top edge of the bottom UI stack (hotbar, experience bar, status
    /// rows).
    ///
    /// `armor_visible` tells whether the armour row is drawn; the caller
    /// passes the state so the chat and the debug block stay clear of the
    /// elements that are actually on screen.
    #[must_use]
    pub fn content_top(&self, armor_visible: bool) -> f32 {
        let mut top = self.xp_bar_rect().y;
        top = top.min(self.status_row_rect(StatusRow::Health).y);
        top = top.min(self.status_row_rect(StatusRow::Food).y);
        if armor_visible {
            top = top.min(self.status_row_rect(StatusRow::Armor).y);
        }
        top
    }

    /// The band occupied by chat line `index`.
    ///
    /// `bottom` is the top edge of the bottom UI stack and `visible` the
    /// number of chat lines drawn; index zero is the oldest line. The width
    /// is the space available for the line, so a caller can clamp its text.
    #[must_use]
    pub fn chat_line_rect(&self, bottom: f32, index: usize, visible: usize) -> Rect {
        let visible = visible.max(1);
        let index = index.min(visible - 1);
        let line_height = self.line_height();
        Rect {
            x: CHAT_LEFT,
            y: bottom - (visible - index) as f32 * line_height,
            width: (self.width - CHAT_LEFT).max(0.0),
            height: line_height,
        }
    }

    /// The background of the debug panel for `lines` lines of `text_width`
    /// pixels.
    #[must_use]
    pub fn debug_panel_rect(&self, text_width: f32, lines: usize) -> Rect {
        Rect {
            x: DEBUG_LEFT,
            y: DEBUG_TOP,
            width: text_width + DEBUG_PADDING_X,
            height: lines as f32 * self.line_height() + DEBUG_PADDING_Y,
        }
    }

    /// The top-left corner of debug line `index`.
    #[must_use]
    pub fn debug_text_origin(&self, index: usize) -> (f32, f32) {
        (
            DEBUG_TEXT_X,
            DEBUG_TEXT_Y + index as f32 * self.line_height(),
        )
    }

    /// The two rectangles of the crosshair: the horizontal and the vertical
    /// arm, centred on the viewport.
    #[must_use]
    pub fn crosshair_rects(&self) -> (Rect, Rect) {
        let arm = 5.0 * self.scale;
        let thickness = self.scale.max(1.0);
        let center_x = self.width * 0.5;
        let center_y = self.height * 0.5;
        (
            Rect {
                x: center_x - arm,
                y: center_y - thickness * 0.5,
                width: arm * 2.0,
                height: thickness,
            },
            Rect {
                x: center_x - thickness * 0.5,
                y: center_y - arm,
                width: thickness,
                height: arm * 2.0,
            },
        )
    }
}

/// The hotbar slot to highlight, clamped to the nine existing slots.
///
/// A `selected_slot` above eight (a malformed packet, or a key press that
/// raced the inventory update) selects the last slot instead of panicking.
#[must_use]
pub fn selected_index(selected_slot: u8) -> usize {
    (selected_slot as usize).min(HOTBAR_SLOTS - 1)
}

/// The fill state of every icon of a status row.
///
/// The value is measured in hit points and two points fill one icon, so
/// nineteen points give nine full icons and one half icon. Values outside
/// `0..=20` and non-finite values are clamped away.
#[must_use]
pub fn status_fills(value: f32) -> [Fill; STATUS_ICONS] {
    let points = clamped(value, (STATUS_ICONS * 2) as f32).round() as usize;
    let mut fills = [Fill::Empty; STATUS_ICONS];
    for (index, fill) in fills.iter_mut().enumerate() {
        let first = index * 2;
        *fill = if points >= first + 2 {
            Fill::Full
        } else if points > first {
            Fill::Half
        } else {
            Fill::Empty
        };
    }
    fills
}

/// Clamps `value` into `0.0..=max`, mapping non-finite input to zero.
pub(super) fn clamped(value: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, max)
    } else {
        0.0
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> HudLayout {
        HudLayout::new(1280, 720)
    }

    #[test]
    fn scale_follows_the_client_convention() {
        assert!((HudLayout::new(640, 360).scale() - 1.0).abs() < f32::EPSILON);
        assert!((HudLayout::new(1280, 720).scale() - 2.0).abs() < f32::EPSILON);
        assert!((HudLayout::new(1920, 1080).scale() - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn scale_stays_within_bounds_for_extreme_viewports() {
        for height in [1_u32, 2, 16, 100, 359, 360, 719, 720, 1080, 2160, 4320] {
            let scale = HudLayout::new(800, height).scale();
            assert!(
                (1.0..=3.0).contains(&scale),
                "height {height} produced scale {scale}"
            );
        }
    }

    #[test]
    fn health_icons_exactly_fill_the_hotbar_width() {
        let layout = layout();
        let hotbar = layout.hotbar_rect();
        let row = layout.status_row_rect(StatusRow::Health);
        assert!((row.width - hotbar.width).abs() < f32::EPSILON);
        assert!((row.x - hotbar.x).abs() < f32::EPSILON);
        let first = layout.status_icon_rect(StatusRow::Health, 0);
        let last = layout.status_icon_rect(StatusRow::Health, STATUS_ICONS - 1);
        assert!((first.x - hotbar.x).abs() < f32::EPSILON);
        assert!((last.x + last.width - hotbar.right()).abs() < f32::EPSILON);
    }

    #[test]
    fn food_row_is_aligned_with_the_health_row() {
        let layout = layout();
        let health = layout.status_row_rect(StatusRow::Health);
        let food = layout.status_row_rect(StatusRow::Food);
        assert!((health.x - food.x).abs() < f32::EPSILON);
        assert!((health.width - food.width).abs() < f32::EPSILON);
        assert!(food.y < health.y, "the food row sits above the health row");
    }

    #[test]
    fn every_slot_is_inside_the_hotbar() {
        let layout = layout();
        let hotbar = layout.hotbar_rect();
        for index in 0..HOTBAR_SLOTS {
            let slot = layout.slot_rect(index);
            assert!(hotbar.contains(&slot), "slot {index} escaped the hotbar");
        }
    }

    #[test]
    fn out_of_range_selected_slot_is_clamped() {
        assert_eq!(selected_index(0), 0);
        assert_eq!(selected_index(8), 8);
        assert_eq!(selected_index(9), 8);
        assert_eq!(selected_index(u8::MAX), HOTBAR_SLOTS - 1);
    }

    #[test]
    fn half_health_gives_nine_full_icons_and_one_half() {
        let fills = status_fills(19.0);
        assert!(fills[..9].iter().all(|fill| *fill == Fill::Full));
        assert_eq!(fills[9], Fill::Half);
        assert!(status_fills(20.0).iter().all(|fill| *fill == Fill::Full));
        assert!(status_fills(0.0).iter().all(|fill| *fill == Fill::Empty));
        assert!(status_fills(-5.0).iter().all(|fill| *fill == Fill::Empty));
        assert!(status_fills(100.0).iter().all(|fill| *fill == Fill::Full));
        assert!(status_fills(f32::NAN)
            .iter()
            .all(|fill| *fill == Fill::Empty));
    }
}
