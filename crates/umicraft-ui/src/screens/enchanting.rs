// SPDX-License-Identifier: MIT OR Apache-2.0

//! The enchanting table, after `ContainerEnchantment` / `GuiEnchantment`.
//!
//! Only the static layout is here. The three option rows, the lapis cost icons
//! and the animated book are not drawn yet (see `docs/parity.md`).

use super::layout::player_inventory_slots;
use super::{ScreenSpec, SlotSpec};

/// Builds the enchanting table screen: item, lapis, backpack, hotbar.
pub(super) fn spec() -> ScreenSpec {
    let mut slots = vec![
        SlotSpec {
            index: 0,
            x: 15,
            y: 47,
        },
        SlotSpec {
            index: 1,
            x: 35,
            y: 47,
        },
    ];
    slots.extend(player_inventory_slots(2, 84));
    ScreenSpec::single("container/enchanting_table", (176, 166), slots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_container_enchantment_coordinates() {
        let slots = spec().slots;
        assert_eq!(slots.len(), 38);
        assert_eq!((slots[0].x, slots[0].y), (15, 47));
        assert_eq!((slots[1].x, slots[1].y), (35, 47));
        assert_eq!((slots[2].x, slots[2].y), (8, 84));
        assert_eq!((slots[37].x, slots[37].y), (152, 142));
    }
}
