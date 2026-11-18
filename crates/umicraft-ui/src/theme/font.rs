// SPDX-License-Identifier: MIT OR Apache-2.0

//! The font a theme draws all UI text with.

/// Cell sizes of the bundled HD fonts, smallest first.
pub const HD_CELLS: [u32; 3] = [16, 24, 32];

/// Which glyph grid the text of a theme comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontSkin {
    /// The 1.8 `ascii.png` of the active resource packs.
    Game,
    /// The theme's own high-resolution grid, picked by GUI scale.
    Hd,
}

/// The HD cell size to use at `gui_scale`: a glyph is 8 units tall, so the
/// smallest cell with at least `8 * gui_scale` texels stays crisp, and a scale
/// beyond the largest cell uses the largest one.
///
/// ```
/// use umicraft_ui::theme::hd_cell_for_scale;
/// assert_eq!(hd_cell_for_scale(1), 16);
/// assert_eq!(hd_cell_for_scale(3), 24);
/// assert_eq!(hd_cell_for_scale(6), 32);
/// ```
#[must_use]
pub fn hd_cell_for_scale(gui_scale: u32) -> u32 {
    let wanted = 8 * gui_scale;
    HD_CELLS
        .into_iter()
        .find(|cell| *cell >= wanted)
        .unwrap_or(HD_CELLS[HD_CELLS.len() - 1])
}
