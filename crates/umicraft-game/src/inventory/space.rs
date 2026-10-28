// SPDX-License-Identifier: MIT OR Apache-2.0

//! The whole window seen as one indexable list of slots.
//!
//! The click logic of MCP `Container.slotClick` addresses slots by their
//! window index. [`ClickSpace`] resolves that index onto the container's own
//! slots or the player inventory and applies the [`SlotKind`] rules, so the
//! click code never cares where a slot is stored.

use umicraft_protocol::packets::play::ItemStack;

use super::kind::{resolve_container_slot, SlotTarget, HOTBAR_START, PLAYER_SLOT_COUNT};
use super::player::PlayerInventory;
use super::slot::SlotKind;
use super::stack::{add_count, count, normalize};

/// Mutable view of a window for one click.
pub struct ClickSpace<'a> {
    inv: &'a mut PlayerInventory,
    own: &'a mut [Option<ItemStack>],
    kinds: &'a [SlotKind],
    player_window: bool,
}

impl<'a> ClickSpace<'a> {
    /// A view of the player window (id 0).
    pub fn player_window(inv: &'a mut PlayerInventory, kinds: &'a [SlotKind]) -> Self {
        Self {
            inv,
            own: &mut [],
            kinds,
            player_window: true,
        }
    }

    /// A view of a container window.
    pub fn container_window(
        inv: &'a mut PlayerInventory,
        own: &'a mut [Option<ItemStack>],
        kinds: &'a [SlotKind],
    ) -> Self {
        Self {
            inv,
            own,
            kinds,
            player_window: false,
        }
    }

    /// Number of slots of the window.
    #[must_use]
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    /// Whether the window has no slot.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    /// The behaviour of a slot (`Normal` out of range).
    #[must_use]
    pub fn kind(&self, index: usize) -> SlotKind {
        self.kinds.get(index).copied().unwrap_or(SlotKind::Normal)
    }

    fn target(&self, index: usize) -> Option<SlotTarget> {
        if index >= self.kinds.len() {
            None
        } else if self.player_window {
            (index < PLAYER_SLOT_COUNT).then_some(SlotTarget::Player(index))
        } else {
            resolve_container_slot(index, self.own.len())
        }
    }

    /// Whether the slot belongs to the player inventory
    /// (`slot.inventory == inventoryplayer`).
    #[must_use]
    pub fn is_player_slot(&self, index: usize) -> bool {
        matches!(self.target(index), Some(SlotTarget::Player(_)))
    }

    /// The stack in a slot.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&ItemStack> {
        match self.target(index)? {
            SlotTarget::Container(i) => self.own.get(i)?.as_ref(),
            SlotTarget::Player(i) => self.inv.slot(i),
        }
    }

    /// Mutable access to the stack in a slot.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut ItemStack> {
        match self.target(index)? {
            SlotTarget::Container(i) => self.own.get_mut(i)?.as_mut(),
            SlotTarget::Player(i) => self.inv.slot_mut(i),
        }
    }

    /// Replaces the stack in a slot (an empty stack clears it).
    pub fn set(&mut self, index: usize, stack: Option<ItemStack>) {
        let stack = normalize(stack);
        match self.target(index) {
            Some(SlotTarget::Container(i)) => {
                if let Some(slot) = self.own.get_mut(i) {
                    *slot = stack;
                }
            }
            Some(SlotTarget::Player(i)) => {
                self.inv.set_slot(i, stack);
            }
            None => {}
        }
    }

    /// Removes `amount` items from a slot (`Slot.decrStackSize`), clearing
    /// the slot when it runs empty.
    pub fn decrease(&mut self, index: usize, amount: i32) -> Option<ItemStack> {
        let stack = self.get_mut(index)?;
        let taken = super::stack::split(stack, amount);
        let empty = count(stack) == 0;
        if empty {
            self.set(index, None);
        }
        normalize(Some(taken))
    }

    /// Adds `delta` to the size of a slot's stack.
    pub fn grow(&mut self, index: usize, delta: i32) {
        if let Some(stack) = self.get_mut(index) {
            add_count(stack, delta);
        }
    }

    /// `Slot.onPickupFromSlot`: taking a crafting result consumes one item of
    /// each ingredient. The result is cleared; the server sends the next one.
    pub fn picked_up(&mut self, index: usize) {
        if let SlotKind::CraftingResult { grid_len } = self.kind(index) {
            for slot in index + 1..=index + usize::from(grid_len) {
                self.decrease(slot, 1);
            }
            self.set(index, None);
        }
    }

    /// The hotbar slot (player window index) of a number key.
    #[must_use]
    pub fn hotbar_slot(key: usize) -> usize {
        HOTBAR_START + key
    }

    /// The player inventory behind the window.
    pub fn inventory(&mut self) -> &mut PlayerInventory {
        self.inv
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::super::kind::WindowKind;
    use super::super::slot::{container_window_kinds, player_window_kinds};
    use super::*;

    /// Owner of the storage a [`ClickSpace`] borrows, for tests.
    pub struct TestWindow {
        pub inv: PlayerInventory,
        pub own: Vec<Option<ItemStack>>,
        pub kinds: Vec<SlotKind>,
        pub player: bool,
    }

    impl TestWindow {
        pub fn player() -> Self {
            Self {
                inv: PlayerInventory::new(),
                own: Vec::new(),
                kinds: player_window_kinds(),
                player: true,
            }
        }

        pub fn container(kind: WindowKind, own: usize) -> Self {
            Self {
                inv: PlayerInventory::new(),
                own: vec![None; own],
                kinds: container_window_kinds(kind, own),
                player: false,
            }
        }

        pub fn space(&mut self) -> ClickSpace<'_> {
            if self.player {
                ClickSpace::player_window(&mut self.inv, &self.kinds)
            } else {
                ClickSpace::container_window(&mut self.inv, &mut self.own, &self.kinds)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::kind::WindowKind;
    use super::super::stack::fixtures::stack;
    use super::fixtures::TestWindow;
    use super::*;

    #[test]
    fn container_indices_reach_both_storages() {
        let mut w = TestWindow::container(WindowKind::Chest, 27);
        {
            let mut space = w.space();
            space.set(0, Some(stack(1, 3)));
            space.set(27, Some(stack(2, 4)));
            assert!(!space.is_player_slot(0));
            assert!(space.is_player_slot(27));
            assert_eq!(space.len(), 63);
        }
        assert_eq!(w.own[0], Some(stack(1, 3)));
        assert_eq!(w.inv.slot(9), Some(&stack(2, 4)));
    }

    #[test]
    fn decrease_clears_an_emptied_slot() {
        let mut w = TestWindow::player();
        let mut space = w.space();
        space.set(10, Some(stack(1, 2)));
        assert_eq!(space.decrease(10, 2), Some(stack(1, 2)));
        assert!(space.get(10).is_none());
    }

    #[test]
    fn taking_the_crafting_result_consumes_one_ingredient_each() {
        let mut w = TestWindow::player();
        let mut space = w.space();
        space.set(0, Some(stack(5, 4)));
        space.set(1, Some(stack(17, 1)));
        space.set(2, Some(stack(17, 3)));
        space.picked_up(0);
        assert!(space.get(1).is_none());
        assert_eq!(space.get(2).map(count), Some(2));
        assert!(space.get(0).is_none());
    }
}
