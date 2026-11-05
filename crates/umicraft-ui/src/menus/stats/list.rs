// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `GuiSlot` frame the four statistics lists share: the geometry, the
//! dirt chrome with its scrollbar, and the small dark hover label.
//!
//! Vanilla builds every list as `GuiSlot(width, height, 32, height - 64,
//! slotHeight)` with the default 220 unit row width, so the first row column
//! starts at `width / 2 - 108` and the scrollbar at `width / 2 + 124`.

use crate::draw::Color;
use crate::geom::Rect;
use crate::menus::background::{draw_list_background, draw_list_overlay};
use crate::menus::common::{draw_scrollbar, wheel_scrolled_by, Gui, TextPlacement, WHITE};
use crate::text::FontAtlas;
use crate::Ui;

/// Top edge of the lists in GUI units.
pub(super) const LIST_TOP: f32 = 32.0;
/// Space between the bottom of the lists and the bottom of the screen.
const LIST_BOTTOM_MARGIN: f32 = 64.0;
/// Distance of the first row column from the centre line.
pub(super) const ROW_LEFT: f32 = 108.0;
/// Distance of the scrollbar from the centre line.
const SCROLLBAR_OFFSET: f32 = 124.0;
/// The offset of the first row below the list top (`top + 4`).
const ROW_START: f32 = 4.0;
/// Hover label background (`0xC0000000`).
const LABEL_BACKGROUND: Color = [0.0, 0.0, 0.0, 0.75];

/// The geometry of one list for one frame, in GUI units.
#[derive(Debug, Clone, Copy)]
pub(super) struct ListFrame {
    /// Row height (`GuiSlot.slotHeight`).
    pub slot_height: f32,
    /// Space above the first row for a column header.
    pub header: f32,
    /// How many rows the list has.
    pub rows: usize,
}

impl ListFrame {
    /// The bottom edge of the lists.
    pub(super) fn bottom(gui: &Gui) -> f32 {
        gui.height - LIST_BOTTOM_MARGIN
    }

    /// The list rectangle in physical pixels.
    pub(super) fn view(gui: &Gui) -> Rect {
        let top = LIST_TOP;
        gui.rect(0.0, top, gui.width, Self::bottom(gui) - top)
    }

    /// `GuiSlot.getContentHeight`.
    pub(super) fn content_height(&self) -> f32 {
        self.rows as f32 * self.slot_height + self.header
    }

    /// The left edge of the row column.
    pub(super) fn left(gui: &Gui) -> f32 {
        gui.cx() - ROW_LEFT
    }

    /// The top of row `index` when the list is scrolled by `offset`.
    pub(super) fn row_top(&self, index: usize, offset: f32) -> f32 {
        LIST_TOP + ROW_START - offset.floor() + self.header + index as f32 * self.slot_height
    }

    /// The top of the header strip.
    pub(super) fn header_top(offset: f32) -> f32 {
        LIST_TOP + ROW_START - offset.floor()
    }

    /// The row under GUI point `point`, if any (`getSlotIndexFromScreenCoords`).
    pub(super) fn row_at(&self, point: [f32; 2], offset: f32) -> Option<usize> {
        let k = point[1] - LIST_TOP - self.header + offset.floor() - ROW_START;
        if k < 0.0 {
            return None;
        }
        let index = (k / self.slot_height).floor() as usize;
        (index < self.rows).then_some(index)
    }

    /// Applies the wheel and returns the clamped offset.
    pub(super) fn scrolled(&self, ui: &Ui, gui: &Gui, offset: f32) -> f32 {
        let step = self.slot_height / 2.0;
        wheel_scrolled_by(
            ui,
            gui,
            Self::view(gui),
            self.content_height(),
            offset,
            step,
        )
    }

    /// Draws the dirt container, runs `rows` clipped to it, then paints the
    /// bands over the overflow and the scrollbar (`GuiSlot.drawScreen`).
    pub(super) fn draw(&self, ui: &mut Ui, gui: &Gui, offset: f32, rows: impl FnOnce(&mut Ui)) {
        let view = Self::view(gui);
        draw_list_background(ui, view, offset.floor() * gui.scale);
        ui.push_clip(view);
        rows(ui);
        ui.pop_clip();
        draw_list_overlay(ui, view);
        let bar = [
            gui.cx() + SCROLLBAR_OFFSET,
            LIST_TOP,
            Self::bottom(gui) - LIST_TOP,
        ];
        draw_scrollbar(ui, gui, bar, self.content_height(), offset);
    }
}

/// Draws the small dark label vanilla shows next to the pointer
/// (`drawGradientRect` of `0xC0000000` plus white text). `pointer` is in GUI
/// units.
pub(super) fn hover_label(
    ui: &mut Ui,
    atlas: &FontAtlas,
    gui: &Gui,
    pointer: [f32; 2],
    text: &str,
) {
    let width = atlas.measure(text, gui.scale) / gui.scale;
    let (x, y) = (pointer[0] + 12.0, pointer[1] - 12.0);
    let background = gui.rect(x - 3.0, y - 3.0, width + 6.0, 14.0);
    ui.draw().rect(background, LABEL_BACKGROUND);
    gui.text(ui, atlas, text, TextPlacement::left([x, y], WHITE));
}
