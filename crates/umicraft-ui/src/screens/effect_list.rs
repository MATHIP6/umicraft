// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where the potion effect list beside the inventory goes (MCP
//! `InventoryEffectRenderer`).
//!
//! While the player has an effect, the survival and creative inventories move
//! right (`updateActivePotionEffects`) and a column of 140 by 32 boxes from
//! `container/inventory.png` hangs 124 GUI pixels left of the window, one per
//! effect: the status icon, the name and the time. Five boxes fit 33 pixels
//! apart; more are squeezed into 132 pixels.

use crate::geom::Rect;

use super::decoration::{Sheet, SpriteRect};

/// How far left of the window the column starts, in GUI pixels.
const COLUMN_OFFSET: i32 = 124;
/// The box behind an effect in `container/inventory.png`.
const BOX_SRC: (u32, u32, u32, u32) = (0, 166, 140, 32);
/// Where the status icon sits in its box, and its edge.
const ICON_OFFSET: (i32, i32) = (6, 7);
const ICON_SIZE: u32 = 18;
/// The first row of the status icon grid; it is 8 icons wide.
const ICON_GRID_V: u32 = 198;
const ICON_GRID_COLUMNS: u32 = 8;
/// Where the name and, 10 pixels lower, the time start in their box
/// (`i + 10 + 18`, `j + 6`).
const TEXT_OFFSET: (i32, i32) = (28, 6);
const DURATION_LINE: i32 = 10;
/// The spacing of the boxes while five or fewer are shown.
const BOX_SPACING: i32 = 33;
/// The height the boxes are squeezed into past five.
const SQUEEZED_HEIGHT: i32 = 132;
/// The most effects that keep the regular spacing.
const UNSQUEEZED_COUNT: usize = 5;

/// The colour of the effect name (`16777215`).
pub const NAME_COLOR: u32 = 0xFF_FF_FF;
/// The colour of the remaining time (`8355711`).
pub const DURATION_COLOR: u32 = 0x7F_7F_7F;

/// `guiLeft` of a window `x_size` GUI pixels wide while effects are shown, for
/// a screen `scaled_width` GUI pixels wide: `160 + (width - xSize - 200) / 2`,
/// with Java's integer division (which rounds towards zero).
///
/// # Example
///
/// ```
/// use umicraft_ui::screens::effect_list::shifted_gui_left;
///
/// // 427 GUI pixels: the 176 pixel inventory moves from 125 to 185.
/// assert_eq!(shifted_gui_left(427, 176), 185);
/// ```
#[must_use]
pub fn shifted_gui_left(scaled_width: i32, x_size: i32) -> i32 {
    160 + (scaled_width - x_size - 200) / 2
}

/// The pieces of one effect box, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectBox {
    /// The box, from the inventory texture.
    pub background: SpriteRect,
    /// The status icon, from the inventory texture, if the effect has one.
    pub icon: Option<SpriteRect>,
    /// The top-left of the name.
    pub name_at: [f32; 2],
    /// The top-left of the remaining time.
    pub duration_at: [f32; 2],
}

/// The boxes of the effects whose status icons are `icons`, in list order,
/// for a window whose top-left corner (`guiLeft`, `guiTop`) is `window` in
/// physical pixels at GUI `scale`.
#[must_use]
pub fn effect_boxes(window: (f32, f32), scale: u32, icons: &[Option<u8>]) -> Vec<EffectBox> {
    let s = scale as f32;
    let spacing = box_spacing(icons.len());
    let at = |x: i32, y: i32| [window.0 + x as f32 * s, window.1 + y as f32 * s];
    icons
        .iter()
        .zip(0..)
        .map(|(icon, row)| {
            let (x, y) = (-COLUMN_OFFSET, row * spacing);
            let sprite = |offset: (i32, i32), src: (u32, u32, u32, u32)| {
                let [left, top] = at(x + offset.0, y + offset.1);
                SpriteRect {
                    src,
                    sheet: Sheet::Inventory,
                    dst: Rect::new(left, top, src.2 as f32 * s, src.3 as f32 * s),
                }
            };
            EffectBox {
                background: sprite((0, 0), BOX_SRC),
                icon: icon.map(|index| sprite(ICON_OFFSET, icon_src(index))),
                name_at: at(x + TEXT_OFFSET.0, y + TEXT_OFFSET.1),
                duration_at: at(x + TEXT_OFFSET.0, y + TEXT_OFFSET.1 + DURATION_LINE),
            }
        })
        .collect()
}

/// The distance between two boxes for `count` effects.
fn box_spacing(count: usize) -> i32 {
    if count > UNSQUEEZED_COUNT {
        SQUEEZED_HEIGHT / i32::try_from(count - 1).unwrap_or(i32::MAX)
    } else {
        BOX_SPACING
    }
}

/// The status icon `index` in the grid of `container/inventory.png`.
fn icon_src(index: u8) -> (u32, u32, u32, u32) {
    let index = u32::from(index);
    (
        index % ICON_GRID_COLUMNS * ICON_SIZE,
        ICON_GRID_V + index / ICON_GRID_COLUMNS * ICON_SIZE,
        ICON_SIZE,
        ICON_SIZE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_moves_sixty_pixels_right_like_the_reference() {
        let centred = |width: i32| (width - 176) / 2;
        assert_eq!(shifted_gui_left(427, 176) - centred(427), 60);
        // Java division rounds towards zero on a narrow screen.
        assert_eq!(shifted_gui_left(320, 176), 160 + (-56 / 2));
        assert_eq!(shifted_gui_left(321, 176), 133);
        assert_eq!(shifted_gui_left(427, 195), 176);
    }

    #[test]
    fn a_box_hangs_left_of_the_window_with_its_icon_and_text() {
        let boxes = effect_boxes((400.0, 100.0), 2, &[Some(12), None]);
        let first = boxes[0];
        assert_eq!(first.background.src, (0, 166, 140, 32));
        assert_eq!(first.background.dst, Rect::new(152.0, 100.0, 280.0, 64.0));
        let icon = first.icon.expect("night vision has an icon");
        // Icon 12 is column 4, row 1 of the grid.
        assert_eq!(icon.src, (72, 216, 18, 18));
        assert_eq!(icon.dst, Rect::new(164.0, 114.0, 36.0, 36.0));
        assert_eq!(first.name_at, [208.0, 112.0]);
        assert_eq!(first.duration_at, [208.0, 132.0]);
        assert!(boxes[1].icon.is_none());
        assert_eq!(boxes[1].background.dst.y, 100.0 + 33.0 * 2.0);
    }

    #[test]
    fn more_than_five_effects_are_squeezed_into_the_same_height() {
        assert_eq!(box_spacing(5), 33);
        assert_eq!(box_spacing(6), 26);
        assert_eq!(box_spacing(9), 16);
        let boxes = effect_boxes((0.0, 0.0), 1, &[None; 6]);
        assert_eq!(boxes[5].background.dst.y, 130.0);
    }
}
