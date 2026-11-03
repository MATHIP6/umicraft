// SPDX-License-Identifier: MIT OR Apache-2.0

//! The villager trading screen, after `ContainerMerchant` / `GuiMerchant`.
//!
//! Only the static layout is here; the offer list, the arrows and `MC|TrSel`
//! are not implemented yet (see `docs/parity.md`).

use super::layout::player_inventory_slots;
use super::{ScreenSpec, SlotSpec};

/// Builds the villager screen: two buy slots, the result, backpack, hotbar.
pub(super) fn spec() -> ScreenSpec {
    let mut slots: Vec<SlotSpec> = [(36, 53), (62, 53), (120, 53)]
        .iter()
        .enumerate()
        .map(|(index, &(x, y))| SlotSpec { index, x, y })
        .collect();
    slots.extend(player_inventory_slots(3, 84));
    ScreenSpec::single("container/villager", (176, 166), slots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_container_merchant_coordinates() {
        let slots = spec().slots;
        assert_eq!(slots.len(), 39);
        assert_eq!((slots[0].x, slots[0].y), (36, 53));
        assert_eq!((slots[1].x, slots[1].y), (62, 53));
        assert_eq!((slots[2].x, slots[2].y), (120, 53));
        assert_eq!((slots[3].x, slots[3].y), (8, 84));
    }
}
