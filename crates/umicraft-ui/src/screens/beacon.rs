// SPDX-License-Identifier: MIT OR Apache-2.0

//! The beacon, after `ContainerBeacon` / `GuiBeacon`.
//!
//! Only the window and its slots are here; the power buttons and `MC|Beacon`
//! are not implemented yet (see `docs/parity.md`).

use super::layout::{grid_slots, STRIDE};
use super::{ScreenSpec, SlotSpec};

/// Left edge of the backpack, wider than the usual 176 pixel window.
const BACKPACK_X: u32 = 36;
/// Top edge of the backpack.
const BACKPACK_Y: u32 = 137;

/// Builds the beacon screen (230x219): the payment slot, backpack, hotbar.
pub(super) fn spec() -> ScreenSpec {
    let mut slots = vec![SlotSpec {
        index: 0,
        x: 136,
        y: 110,
    }];
    slots.extend(grid_slots(1, 9, 3, BACKPACK_X, BACKPACK_Y));
    slots.extend(grid_slots(
        28,
        9,
        1,
        BACKPACK_X,
        BACKPACK_Y + 3 * STRIDE + 4,
    ));
    ScreenSpec::single("container/beacon", (230, 219), slots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_container_beacon_coordinates() {
        let slots = spec().slots;
        assert_eq!(slots.len(), 37);
        assert_eq!((slots[0].x, slots[0].y), (136, 110));
        assert_eq!((slots[1].x, slots[1].y), (36, 137));
        assert_eq!((slots[27].x, slots[27].y), (180, 173));
        assert_eq!((slots[28].x, slots[28].y), (36, 195));
        assert_eq!((slots[36].x, slots[36].y), (180, 195));
    }
}
