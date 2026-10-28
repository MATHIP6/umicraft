// SPDX-License-Identifier: MIT OR Apache-2.0

//! The player inventory (window 0), after vanilla `ContainerPlayer`.

use super::layout::{grid_slots, player_inventory_slots, STRIDE};
use super::{ScreenSpec, SlotSpec};

/// Builds the 45-slot player window: result, 2x2 grid, armour, backpack, hotbar.
pub(super) fn spec() -> ScreenSpec {
    let mut slots = vec![SlotSpec {
        index: 0,
        x: 144,
        y: 36,
    }];
    slots.extend(grid_slots(1, 2, 2, 88, 26));
    slots.extend((0..4).map(|i| SlotSpec {
        index: 5 + i as usize,
        x: 8,
        y: 8 + i * STRIDE,
    }));
    slots.extend(player_inventory_slots(9, 84));
    ScreenSpec::single("container/inventory", (176, 166), slots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_45_slots_in_window_order() {
        let spec = spec();
        assert_eq!(spec.slots.len(), 45);
        assert!(spec.slots.iter().enumerate().all(|(i, s)| s.index == i));
    }

    #[test]
    fn matches_container_player_coordinates() {
        let slots = spec().slots;
        assert_eq!((slots[0].x, slots[0].y), (144, 36));
        assert_eq!((slots[1].x, slots[1].y), (88, 26));
        assert_eq!((slots[4].x, slots[4].y), (106, 44));
        assert_eq!((slots[5].x, slots[5].y), (8, 8));
        assert_eq!((slots[8].x, slots[8].y), (8, 62));
        assert_eq!((slots[9].x, slots[9].y), (8, 84));
        assert_eq!((slots[35].x, slots[35].y), (152, 120));
        assert_eq!((slots[36].x, slots[36].y), (8, 142));
    }
}
