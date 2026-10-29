// SPDX-License-Identifier: MIT OR Apache-2.0

//! The scroll position of the item grid and the scrollbar maths (MCP
//! `GuiContainerCreative` and `ContainerCreative.scrollTo`).
//!
//! The grid shows [`ROWS`] rows of [`COLUMNS`] cells. The position is a float
//! from 0 (top) to 1 (bottom); vanilla converts it to a first visible row
//! with [`Scroll::first_row`].

/// Cells per grid row.
pub const COLUMNS: usize = 9;
/// Visible grid rows.
pub const ROWS: usize = 5;
/// Visible grid cells; a list with more items needs the scrollbar.
pub const GRID_CELLS: usize = COLUMNS * ROWS;

/// Left edge of the scrollbar track, relative to the window.
pub const SCROLLBAR_X: i32 = 175;
/// Top edge of the scrollbar track, relative to the window.
pub const SCROLLBAR_Y: i32 = 18;
/// Width of the area that starts a scrollbar drag.
pub const SCROLLBAR_WIDTH: i32 = 14;
/// Height of the track.
pub const SCROLLBAR_HEIGHT: i32 = 112;
/// Height of the handle in `tabs.png` (12 wide).
pub const HANDLE_HEIGHT: i32 = 15;
/// The handle's `u` in `tabs.png` when scrolling is possible; add 12 when it is not.
pub const HANDLE_U_ACTIVE: i32 = 232;

/// Whether `items` entries need more than one page of the grid.
#[must_use]
pub fn needs_scrollbar(items: usize) -> bool {
    items > GRID_CELLS
}

/// The scroll position of the grid and whether its handle is being dragged.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Scroll {
    position: f32,
    dragging: bool,
}

impl Scroll {
    /// A grid scrolled to the top.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The position, 0 (top) to 1 (bottom).
    #[must_use]
    pub fn position(&self) -> f32 {
        self.position
    }

    /// Back to the top, as when a tab is selected or the search changes.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// `ContainerCreative.scrollTo`: the first visible row of a list of
    /// `items` entries.
    ///
    /// Short lists give a negative row count, which rounds to row 0.
    #[must_use]
    pub fn first_row(&self, items: usize) -> usize {
        let hidden_rows = (items + COLUMNS - 1) as i64 / COLUMNS as i64 - ROWS as i64;
        let row = (f64::from(self.position * hidden_rows as f32) + 0.5) as i64;
        row.max(0) as usize
    }

    /// Applies a mouse wheel step (`handleMouseInput`): `delta` is the sign
    /// of the wheel movement, positive scrolling up.
    ///
    /// One step moves `1 / (items / 9 - 5)` of the track. Without a scrollbar
    /// the wheel does nothing. With 46 to 53 items that divisor is zero and
    /// the position jumps to an end, as in vanilla.
    pub fn wheel(&mut self, delta: i32, items: usize) {
        if delta == 0 || !needs_scrollbar(items) {
            return;
        }
        let steps = (items / COLUMNS) as f64 - ROWS as f64;
        let moved = f64::from(self.position) - f64::from(delta.signum()) / steps;
        self.position = (moved as f32).clamp(0.0, 1.0);
    }

    /// Starts dragging when the press landed on the track and a scrollbar is
    /// needed (`drawScreen`, `isScrolling = needsScrollBars()`).
    pub fn begin_drag(&mut self, items: usize) {
        self.dragging = needs_scrollbar(items);
    }

    /// Ends the drag when the mouse button is released.
    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    /// Whether the handle is being dragged.
    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// Moves the handle with the mouse while dragging; `y` is the mouse
    /// height relative to the top of the track ([`SCROLLBAR_Y`]).
    ///
    /// Vanilla centres the handle (7.5) on a 97 pixel travel, although it
    /// draws it over 95 pixels ([`Scroll::handle_offset`]).
    pub fn drag_to(&mut self, y: f32) {
        if self.dragging {
            let travel = (SCROLLBAR_HEIGHT - HANDLE_HEIGHT) as f32;
            self.position = ((y - 7.5) / travel).clamp(0.0, 1.0);
        }
    }

    /// The vertical offset of the handle from the top of the track.
    #[must_use]
    pub fn handle_offset(&self) -> i32 {
        ((SCROLLBAR_HEIGHT - 17) as f32 * self.position) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scrolled(position: f32) -> Scroll {
        Scroll {
            position,
            dragging: false,
        }
    }

    #[test]
    fn a_list_that_fits_needs_no_scrollbar() {
        assert!(!needs_scrollbar(45));
        assert!(needs_scrollbar(46));
    }

    #[test]
    fn first_row_rounds_the_position_over_the_hidden_rows() {
        // 158 items = 18 rows, 13 hidden.
        assert_eq!(scrolled(0.0).first_row(158), 0);
        assert_eq!(scrolled(1.0).first_row(158), 13);
        assert_eq!(scrolled(0.5).first_row(158), 7);
        assert_eq!(scrolled(0.03).first_row(158), 0);
        assert_eq!(scrolled(0.04).first_row(158), 1);
    }

    #[test]
    fn short_lists_always_start_at_row_zero() {
        assert_eq!(scrolled(1.0).first_row(12), 0);
        assert_eq!(scrolled(1.0).first_row(0), 0);
    }

    #[test]
    fn the_wheel_moves_one_row_and_clamps() {
        let mut scroll = Scroll::new();
        // 158 items: 158 / 9 - 5 = 12 steps.
        scroll.wheel(-3, 158);
        assert!((scroll.position() - 1.0 / 12.0).abs() < 1e-6);
        scroll.wheel(1, 158);
        assert!(scroll.position().abs() < 1e-6);
        scroll.wheel(1, 158);
        assert_eq!(scroll.position(), 0.0);
        for _ in 0..20 {
            scroll.wheel(-1, 158);
        }
        assert_eq!(scroll.position(), 1.0);
    }

    #[test]
    fn the_wheel_is_ignored_without_a_scrollbar() {
        let mut scroll = Scroll::new();
        scroll.wheel(-1, 45);
        assert_eq!(scroll.position(), 0.0);
    }

    #[test]
    fn one_page_over_jumps_to_the_ends_like_vanilla() {
        let mut scroll = Scroll::new();
        scroll.wheel(-1, 50);
        assert_eq!(scroll.position(), 1.0);
        scroll.wheel(1, 50);
        assert_eq!(scroll.position(), 0.0);
    }

    #[test]
    fn dragging_maps_the_mouse_height_to_the_position() {
        let mut scroll = Scroll::new();
        scroll.drag_to(50.0);
        assert_eq!(scroll.position(), 0.0, "not dragging yet");
        scroll.begin_drag(100);
        scroll.drag_to(7.5 + 97.0 / 2.0);
        assert!((scroll.position() - 0.5).abs() < 1e-6);
        scroll.drag_to(-30.0);
        assert_eq!(scroll.position(), 0.0);
        scroll.drag_to(500.0);
        assert_eq!(scroll.position(), 1.0);
        scroll.end_drag();
        assert!(!scroll.is_dragging());
    }

    #[test]
    fn a_drag_cannot_start_when_everything_fits() {
        let mut scroll = Scroll::new();
        scroll.begin_drag(20);
        assert!(!scroll.is_dragging());
    }

    #[test]
    fn the_handle_travels_ninety_five_pixels() {
        assert_eq!(scrolled(0.0).handle_offset(), 0);
        assert_eq!(scrolled(1.0).handle_offset(), 95);
        assert_eq!(scrolled(0.5).handle_offset(), 47);
    }
}
