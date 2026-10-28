// SPDX-License-Identifier: MIT OR Apache-2.0

//! Chest-like containers of 1 to 6 rows, after `ContainerChest` / `GuiChest`.

use super::decoration::Decoration;
use super::layout::{grid_slots, player_inventory_slots, STRIDE};
use super::ScreenSpec;

/// Width of every chest texture.
const WIDTH: u32 = 176;
/// Height of the bottom (player inventory) part of the texture.
const BOTTOM_HEIGHT: u32 = 96;
/// Row of the texture where the bottom part starts.
const BOTTOM_SRC_Y: u32 = 126;

/// Builds a chest screen; `rows` is clamped to 1..=6.
///
/// `generic_54.png` holds the six-row chest. Vanilla (`GuiChest`) draws the
/// top `rows * 18 + 17` pixels and then the 96 pixel player part from
/// `v = 126`, so the background is two pieces. The pieces cover one row less than
/// `ySize` (114 + rows * 18): vanilla leaves that bottom line undrawn.
pub(super) fn spec(rows: u8) -> ScreenSpec {
    let rows = u32::from(rows.clamp(1, 6));
    let top = rows * STRIDE + 17;
    // Vanilla: 103 + (rows - 4) * 18, rewritten to stay unsigned.
    let backpack_y = 31 + rows * STRIDE;
    let mut slots = grid_slots(0, 9, rows, 8, 18);
    slots.extend(player_inventory_slots(rows as usize * 9, backpack_y));
    ScreenSpec {
        texture: "container/generic_54",
        size: (WIDTH, 114 + rows * STRIDE),
        background: vec![
            Decoration::full((0, 0), (WIDTH, top), (0, 0)),
            Decoration::full((0, BOTTOM_SRC_Y), (WIDTH, BOTTOM_HEIGHT), (0, top)),
        ],
        slots,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_rows_match_container_chest() {
        let spec = spec(3);
        assert_eq!(spec.size, (176, 168));
        assert_eq!(spec.slots.len(), 27 + 36);
        let at = |i: usize| (spec.slots[i].x, spec.slots[i].y);
        assert_eq!(at(0), (8, 18));
        assert_eq!(at(26), (152, 54));
        assert_eq!(spec.slots[27].index, 27);
        assert_eq!(at(27), (8, 85));
        assert_eq!(at(54), (8, 143));
    }

    #[test]
    fn six_rows_match_container_chest() {
        let spec = spec(6);
        assert_eq!(spec.size, (176, 222));
        assert_eq!(spec.slots.len(), 54 + 36);
        assert_eq!((spec.slots[54].x, spec.slots[54].y), (8, 139));
        assert_eq!((spec.slots[89].x, spec.slots[89].y), (152, 197));
    }

    #[test]
    fn background_pieces_cover_the_whole_height() {
        for rows in 1..=6 {
            let spec = spec(rows);
            let [top, bottom] = spec.background[..] else {
                panic!("expected two pieces");
            };
            assert_eq!(top.size.1 + bottom.size.1 + 1, spec.size.1);
            assert_eq!(bottom.dst.1, top.size.1);
        }
    }

    #[test]
    fn out_of_range_rows_are_clamped() {
        assert_eq!(spec(0).slots.len(), 9 + 36);
        assert_eq!(spec(9).slots.len(), 54 + 36);
    }
}
