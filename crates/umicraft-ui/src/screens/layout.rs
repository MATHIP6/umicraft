// SPDX-License-Identifier: MIT OR Apache-2.0

//! Slot-grid helpers shared by the screens.

use super::SlotSpec;

/// Distance between two neighbouring slots, in unscaled pixels.
pub(super) const STRIDE: u32 = 18;
/// Vertical distance between the backpack's first row and the hotbar row.
const HOTBAR_GAP: u32 = 58;

/// A `cols` x `rows` grid of consecutive window slots, filled row by row.
pub(super) fn grid_slots(
    first_index: usize,
    cols: u32,
    rows: u32,
    x: u32,
    y: u32,
) -> Vec<SlotSpec> {
    (0..rows)
        .flat_map(|row| (0..cols).map(move |col| (col, row)))
        .enumerate()
        .map(|(offset, (col, row))| SlotSpec {
            index: first_index + offset,
            x: x + col * STRIDE,
            y: y + row * STRIDE,
        })
        .collect()
}

/// The 27 backpack and 9 hotbar slots, starting at window index `first_index`.
///
/// Every vanilla container puts the backpack at x = 8 and the hotbar 58
/// pixels below its first row, so only the backpack's `y` varies.
pub(super) fn player_inventory_slots(first_index: usize, backpack_y: u32) -> Vec<SlotSpec> {
    let mut slots = grid_slots(first_index, 9, 3, 8, backpack_y);
    slots.extend(grid_slots(
        first_index + 27,
        9,
        1,
        8,
        backpack_y + HOTBAR_GAP,
    ));
    slots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_row_major() {
        let slots = grid_slots(1, 3, 2, 30, 17);
        assert_eq!(slots.len(), 6);
        assert_eq!(
            slots[0],
            SlotSpec {
                index: 1,
                x: 30,
                y: 17
            }
        );
        assert_eq!(
            slots[3],
            SlotSpec {
                index: 4,
                x: 30,
                y: 35
            }
        );
    }

    #[test]
    fn player_inventory_has_backpack_then_hotbar() {
        let slots = player_inventory_slots(5, 84);
        assert_eq!(slots.len(), 36);
        assert_eq!(
            slots[0],
            SlotSpec {
                index: 5,
                x: 8,
                y: 84
            }
        );
        assert_eq!(
            slots[35],
            SlotSpec {
                index: 40,
                x: 8 + 8 * 18,
                y: 142
            }
        );
    }
}
