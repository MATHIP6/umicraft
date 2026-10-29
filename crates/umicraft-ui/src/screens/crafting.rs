// SPDX-License-Identifier: MIT OR Apache-2.0

//! The crafting table, after `ContainerWorkbench`.

use super::layout::{grid_slots, player_inventory_slots};
use super::{ScreenSpec, SlotSpec};

/// Builds the workbench screen: result, 3x3 grid, backpack, hotbar.
pub(super) fn spec() -> ScreenSpec {
    let mut slots = vec![SlotSpec {
        index: 0,
        x: 124,
        y: 35,
    }];
    slots.extend(grid_slots(1, 3, 3, 30, 17));
    slots.extend(player_inventory_slots(10, 84));
    ScreenSpec::single("container/crafting_table", (176, 166), slots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_container_workbench_coordinates() {
        let slots = spec().slots;
        assert_eq!(slots.len(), 46);
        assert_eq!((slots[0].x, slots[0].y), (124, 35));
        assert_eq!((slots[1].x, slots[1].y), (30, 17));
        assert_eq!((slots[9].x, slots[9].y), (66, 53));
        assert_eq!(slots[10].index, 10);
        assert_eq!((slots[10].x, slots[10].y), (8, 84));
        assert_eq!((slots[45].x, slots[45].y), (152, 142));
    }
}
