// SPDX-License-Identifier: MIT OR Apache-2.0

//! Hoppers, after `ContainerHopper` / `GuiHopper`.

use super::layout::{grid_slots, player_inventory_slots};
use super::ScreenSpec;

/// Builds the five-slot hopper screen (176x133).
pub(super) fn spec() -> ScreenSpec {
    let mut slots = grid_slots(0, 5, 1, 44, 20);
    slots.extend(player_inventory_slots(5, 51));
    ScreenSpec::single("container/hopper", (176, 133), slots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_container_hopper_coordinates() {
        let slots = spec().slots;
        assert_eq!(slots.len(), 41);
        assert_eq!((slots[0].x, slots[0].y), (44, 20));
        assert_eq!((slots[4].x, slots[4].y), (116, 20));
        assert_eq!((slots[5].x, slots[5].y), (8, 51));
        assert_eq!((slots[40].x, slots[40].y), (152, 109));
    }
}
