// SPDX-License-Identifier: MIT OR Apache-2.0

//! The anvil, after `ContainerRepair` / `GuiRepair`.
//!
//! Only the static layout and the name field background are here; the text
//! field, the cost line and `MC|ItemName` come with the anvil input handling.

use super::decoration::Decoration;
use super::layout::player_inventory_slots;
use super::{ScreenSpec, SlotSpec};

/// Builds the anvil screen: two inputs, the result, backpack, hotbar.
///
/// The name field is drawn at (59, 20) from `v = 166` (enabled) or `v = 182`
/// (no item in the first slot, so the field is disabled).
pub(super) fn spec() -> ScreenSpec {
    let mut slots: Vec<SlotSpec> = [(27, 47), (76, 47), (134, 47)]
        .iter()
        .enumerate()
        .map(|(index, &(x, y))| SlotSpec { index, x, y })
        .collect();
    slots.extend(player_inventory_slots(3, 84));
    ScreenSpec::single("container/anvil", (176, 166), slots)
}

/// The name field and the "no result" cross for the current slot contents.
pub(super) fn decorations(has_input: bool, has_second: bool, has_result: bool) -> Vec<Decoration> {
    let field_v = if has_input { 166 } else { 182 };
    let mut decorations = vec![Decoration::full((0, field_v), (110, 16), (59, 20))];
    if (has_input || has_second) && !has_result {
        decorations.push(Decoration::full((176, 0), (28, 21), (99, 45)));
    }
    decorations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_container_repair_coordinates() {
        let slots = spec().slots;
        assert_eq!(slots.len(), 39);
        assert_eq!((slots[0].x, slots[0].y), (27, 47));
        assert_eq!((slots[1].x, slots[1].y), (76, 47));
        assert_eq!((slots[2].x, slots[2].y), (134, 47));
        assert_eq!((slots[3].x, slots[3].y), (8, 84));
    }

    #[test]
    fn the_name_field_dims_without_an_input_item() {
        assert_eq!(decorations(true, false, true)[0].src, (0, 166));
        assert_eq!(decorations(false, false, false)[0].src, (0, 182));
    }

    #[test]
    fn the_cross_shows_for_an_input_without_a_result() {
        assert_eq!(decorations(true, false, false).len(), 2);
        assert_eq!(decorations(false, true, false).len(), 2);
        assert_eq!(decorations(true, true, true).len(), 1);
        assert_eq!(decorations(false, false, false).len(), 1);
    }
}
