// SPDX-License-Identifier: MIT OR Apache-2.0

//! The player's own 45-slot inventory (window id 0).

use umicraft_protocol::packets::play::ItemStack;

use super::kind::{HOTBAR_LEN, HOTBAR_START, MAIN_INV_LEN, PLAYER_SLOT_COUNT};
use super::stack::{add_count, count, is_stackable, max_stack, same_item};

/// The slot of the player window of a vanilla main-inventory index
/// (`InventoryPlayer.mainInventory`: hotbar 0–8, then the backpack).
#[must_use]
pub fn main_index_to_slot(index: usize) -> usize {
    if index < HOTBAR_LEN {
        HOTBAR_START + index
    } else {
        index
    }
}

/// The 45 slots of the player window: result, 2×2 grid, armour, backpack
/// and hotbar.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInventory {
    slots: [Option<ItemStack>; PLAYER_SLOT_COUNT],
}

impl Default for PlayerInventory {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayerInventory {
    /// Creates an empty player inventory.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
        }
    }

    /// Returns the item in the given slot, if any.
    #[must_use]
    pub fn slot(&self, index: usize) -> Option<&ItemStack> {
        self.slots.get(index).and_then(|s| s.as_ref())
    }

    /// Mutable access to the item in the given slot.
    pub fn slot_mut(&mut self, index: usize) -> Option<&mut ItemStack> {
        self.slots.get_mut(index).and_then(|s| s.as_mut())
    }

    /// Sets the item in the given slot.
    ///
    /// Returns `false` if `index` is out of range.
    pub fn set_slot(&mut self, index: usize, item: Option<ItemStack>) -> bool {
        match self.slots.get_mut(index) {
            Some(slot) => {
                *slot = item;
                true
            }
            None => false,
        }
    }

    /// Returns the 9 hotbar slots.
    #[must_use]
    pub fn hotbar(&self) -> [Option<ItemStack>; HOTBAR_LEN] {
        std::array::from_fn(|i| self.slots[HOTBAR_START + i].clone())
    }

    /// Copies all 45 slots into a fixed-size array.
    #[must_use]
    pub fn all_slots(&self) -> [Option<ItemStack>; PLAYER_SLOT_COUNT] {
        self.slots.clone()
    }

    /// The first empty slot of the main inventory in vanilla order (hotbar
    /// first), as a player window slot (`InventoryPlayer.getFirstEmptyStack`).
    #[must_use]
    pub fn first_empty(&self) -> Option<usize> {
        (0..HOTBAR_LEN + MAIN_INV_LEN)
            .map(main_index_to_slot)
            .find(|&slot| self.slots[slot].is_none())
    }

    /// Adds a stack to the main inventory, merging into matching stacks first
    /// (`InventoryPlayer.addItemStackToInventory`). Returns whatever did not
    /// fit.
    pub fn add_stack(&mut self, mut stack: ItemStack) -> Option<ItemStack> {
        if is_stackable(&stack) {
            for slot in (0..HOTBAR_LEN + MAIN_INV_LEN).map(main_index_to_slot) {
                if count(&stack) == 0 {
                    break;
                }
                let limit = max_stack(&stack);
                let Some(there) = self.slot_mut(slot) else {
                    continue;
                };
                if !same_item(there, &stack) {
                    continue;
                }
                let moved = (limit - count(there)).clamp(0, count(&stack));
                add_count(there, moved);
                add_count(&mut stack, -moved);
            }
        }
        if count(&stack) == 0 {
            return None;
        }
        match self.first_empty() {
            Some(slot) => {
                self.slots[slot] = Some(stack);
                None
            }
            None => Some(stack),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::stack::fixtures::stack;
    use super::*;

    #[test]
    fn the_first_empty_slot_is_in_the_hotbar() {
        let mut inv = PlayerInventory::new();
        assert_eq!(inv.first_empty(), Some(HOTBAR_START));
        inv.set_slot(HOTBAR_START, Some(stack(1, 1)));
        assert_eq!(inv.first_empty(), Some(HOTBAR_START + 1));
    }

    #[test]
    fn add_stack_merges_before_using_an_empty_slot() {
        let mut inv = PlayerInventory::new();
        inv.set_slot(20, Some(stack(1, 60)));
        assert_eq!(inv.add_stack(stack(1, 10)), None);
        assert_eq!(inv.slot(20).map(count), Some(64));
        assert_eq!(inv.slot(HOTBAR_START).map(count), Some(6));
    }

    #[test]
    fn add_stack_returns_the_rest_when_full() {
        let mut inv = PlayerInventory::new();
        for i in 0..36 {
            inv.set_slot(main_index_to_slot(i), Some(stack(2, 64)));
        }
        assert_eq!(inv.add_stack(stack(1, 3)), Some(stack(1, 3)));
    }
}
