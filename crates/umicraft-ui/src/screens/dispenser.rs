// SPDX-License-Identifier: MIT OR Apache-2.0

//! Dispensers and droppers, after `ContainerDispenser`.

use super::layout::{grid_slots, player_inventory_slots};
use super::ScreenSpec;

/// Builds the 3x3 dispenser screen (also used for droppers).
pub(super) fn spec() -> ScreenSpec {
    let mut slots = grid_slots(0, 3, 3, 62, 17);
    slots.extend(player_inventory_slots(9, 84));
    ScreenSpec::single("container/dispenser", (176, 166), slots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_container_dispenser_coordinates() {
        let slots = spec().slots;
        assert_eq!(slots.len(), 45);
        assert_eq!((slots[0].x, slots[0].y), (62, 17));
        assert_eq!((slots[8].x, slots[8].y), (98, 53));
        assert_eq!((slots[9].x, slots[9].y), (8, 84));
        assert_eq!((slots[44].x, slots[44].y), (152, 142));
    }
}
