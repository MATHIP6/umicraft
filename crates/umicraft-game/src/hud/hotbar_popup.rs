// SPDX-License-Identifier: MIT OR Apache-2.0

//! The pop-up bounce of a hotbar item that was just picked up (MCP
//! `ItemStack.animationsToGo`, `NetHandlerPlayClient.handleSetSlot`,
//! `InventoryPlayer.decrementAnimations`).

use crate::inventory::{HOTBAR_LEN, HOTBAR_START};

/// Ticks the bounce lasts (`handleSetSlot`: `animationsToGo = 5`).
const POPUP_TICKS: u8 = 5;

/// The bounce countdown of each hotbar slot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HotbarPopups {
    remaining: [u8; HOTBAR_LEN],
}

impl HotbarPopups {
    /// A tracker with no bounce running.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            remaining: [0; HOTBAR_LEN],
        }
    }

    /// A `Set Slot` of the player window (`handleSetSlot`): a hotbar slot
    /// whose stack is new or bigger than the one it replaces bounces. Other
    /// slots and an emptied slot do nothing.
    ///
    /// `old_count` is the count the slot held, `None` when it was empty;
    /// `new_count` is the count the server sets, `None` when it clears the slot.
    pub fn on_set_slot(&mut self, slot: usize, old_count: Option<i8>, new_count: Option<i8>) {
        let Some(index) = slot.checked_sub(HOTBAR_START).filter(|i| *i < HOTBAR_LEN) else {
            return;
        };
        let Some(new) = new_count else { return };
        if old_count.is_none_or(|old| old < new) {
            self.remaining[index] = POPUP_TICKS;
        }
    }

    /// One game tick (`decrementAnimations`). Returns whether a bounce was
    /// running, so the caller redraws the HUD.
    pub fn tick(&mut self) -> bool {
        let running = self.remaining.iter().any(|ticks| *ticks > 0);
        for ticks in &mut self.remaining {
            *ticks = ticks.saturating_sub(1);
        }
        running
    }

    /// The ticks left of every slot; `0` is no bounce.
    #[must_use]
    pub const fn remaining(&self) -> [u8; HOTBAR_LEN] {
        self.remaining
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_or_bigger_stack_bounces_for_five_ticks() {
        let mut popups = HotbarPopups::new();
        popups.on_set_slot(HOTBAR_START + 2, None, Some(1));
        popups.on_set_slot(HOTBAR_START + 3, Some(4), Some(5));
        let mut expected = [0; HOTBAR_LEN];
        expected[2] = 5;
        expected[3] = 5;
        assert_eq!(popups.remaining(), expected);
        for _ in 0..5 {
            assert!(popups.tick());
        }
        assert_eq!(popups.remaining(), [0; HOTBAR_LEN]);
        assert!(!popups.tick());
    }

    #[test]
    fn a_smaller_stack_an_emptied_slot_and_other_slots_do_not_bounce() {
        let mut popups = HotbarPopups::new();
        popups.on_set_slot(HOTBAR_START, Some(5), Some(5));
        popups.on_set_slot(HOTBAR_START, Some(5), Some(2));
        popups.on_set_slot(HOTBAR_START, Some(5), None);
        popups.on_set_slot(HOTBAR_START - 1, None, Some(1));
        popups.on_set_slot(HOTBAR_START + HOTBAR_LEN, None, Some(1));
        assert_eq!(popups.remaining(), [0; HOTBAR_LEN]);
    }
}
