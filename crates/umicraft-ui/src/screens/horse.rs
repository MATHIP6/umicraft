// SPDX-License-Identifier: MIT OR Apache-2.0

//! The horse inventory, after `ContainerHorseInventory` /
//! `GuiScreenHorseInventory`.

use super::decoration::Decoration;
use super::layout::{grid_slots, player_inventory_slots};
use super::{ScreenSpec, SlotSpec};

/// Height of the window; the extra sprites sit below it in the texture.
const HEIGHT: u32 = 166;
/// Number of chest slot columns of a chested horse and rows (donkey, mule).
const CHEST_COLUMNS: u32 = 5;
/// Rows of the chest.
const CHEST_ROWS: u32 = 3;

/// Builds the horse screen for a window with `own_slots` slots of its own:
/// saddle, armour, and 15 chest slots when the horse carries a chest.
///
/// `GuiScreenHorseInventory` draws the chest backdrop for a chested horse and
/// the armour slot frame when the horse can wear armour. The packet only
/// announces the slot count, so a chest is inferred from a count above 2; the
/// armour frame is always drawn because the type of the horse (donkeys and
/// mules cannot wear armour) is not part of the window.
pub(super) fn spec(own_slots: usize) -> ScreenSpec {
    let chested = own_slots > 2;
    let mut slots = vec![
        SlotSpec {
            index: 0,
            x: 8,
            y: 18,
        },
        SlotSpec {
            index: 1,
            x: 8,
            y: 36,
        },
    ];
    let mut background = vec![Decoration::full((0, 0), (176, HEIGHT), (0, 0))];
    if chested {
        slots.extend(grid_slots(2, CHEST_COLUMNS, CHEST_ROWS, 80, 18));
        background.push(Decoration::full((0, HEIGHT), (90, 54), (79, 17)));
    }
    background.push(Decoration::full((0, HEIGHT + 54), (18, 18), (7, 35)));
    let player_first = slots.len();
    // Vanilla puts the backpack at y = 102 + (3 - 4) * 18 = 84.
    slots.extend(player_inventory_slots(player_first, 84));
    ScreenSpec {
        texture: "container/horse",
        size: (176, HEIGHT),
        background,
        slots,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_horse_has_saddle_and_armour_slots() {
        let spec = spec(2);
        assert_eq!(spec.slots.len(), 2 + 36);
        assert_eq!((spec.slots[0].x, spec.slots[0].y), (8, 18));
        assert_eq!((spec.slots[1].x, spec.slots[1].y), (8, 36));
        assert_eq!((spec.slots[2].x, spec.slots[2].y), (8, 84));
        assert_eq!(spec.background.len(), 2);
    }

    #[test]
    fn a_chested_horse_adds_fifteen_slots() {
        let spec = spec(17);
        assert_eq!(spec.slots.len(), 17 + 36);
        assert_eq!((spec.slots[2].x, spec.slots[2].y), (80, 18));
        assert_eq!((spec.slots[16].x, spec.slots[16].y), (152, 54));
        assert_eq!((spec.slots[17].x, spec.slots[17].y), (8, 84));
        assert_eq!(spec.background.len(), 3);
    }
}
