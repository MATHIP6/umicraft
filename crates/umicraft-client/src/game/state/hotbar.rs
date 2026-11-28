// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hotbar: the selected slot, its sync with the server and the presses of
//! the slot, drop and pick-block keys between two ticks.

use umicraft_game::hud::HotbarPopups;
use umicraft_game::item_use::DropKind;

/// The selected hotbar slot and the presses the next tick takes.
#[derive(Default)]
pub(in crate::game) struct Hotbar {
    /// The selected hotbar slot.
    pub(in crate::game) selected: u8,
    /// The slot as the last keyboard stage of a tick left it (MCP
    /// `inventory.currentItem` between two ticks).
    pub(in crate::game) applied: u8,
    /// The slot the server was last told (MCP
    /// `PlayerControllerMP.currentPlayerItem`).
    pub(in crate::game) synced: u8,
    /// The presses of each digit key not taken by a tick yet (MCP
    /// `KeyBinding.pressTime`).
    pub(in crate::game) key_presses: [u8; 9],
    /// The drop key presses since the last tick, oldest first.
    pub(in crate::game) drops: Vec<DropKind>,
    /// The pick-block presses since the last tick.
    pub(in crate::game) picks: u32,
    /// The bounce of the items the server just gave.
    pub(in crate::game) popups: HotbarPopups,
}

impl Hotbar {
    /// Adds the digit key presses of an input update and returns the slot
    /// they will give at the tick, which the player sees at once.
    pub(in crate::game) fn add_key_presses(&mut self, keys: [u8; 9]) -> Option<u8> {
        for (total, new) in self.key_presses.iter_mut().zip(keys) {
            *total = total.saturating_add(new);
        }
        (0..9u8)
            .rev()
            .find(|&slot| self.key_presses[usize::from(slot)] > 0)
    }

    /// Takes one press of every digit key that has one and returns the
    /// highest of them (MCP `Minecraft.runTick`: `for l in 0..9 { if
    /// keyBindsHotbar[l].isPressed() { currentItem = l } }`).
    pub(in crate::game) fn take_key_press(&mut self) -> Option<u8> {
        let mut keyed = None;
        for (slot, presses) in self.key_presses.iter_mut().enumerate() {
            if *presses > 0 {
                *presses -= 1;
                keyed = Some(slot as u8);
            }
        }
        keyed
    }

    /// The slot to tell the server, when it differs from the last one told
    /// (MCP `PlayerControllerMP.syncCurrentPlayItem`); it counts as told.
    pub(in crate::game) fn take_sync(&mut self) -> Option<u8> {
        (self.applied != self.synced).then(|| {
            self.synced = self.applied;
            self.synced
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_highest_pending_key_is_the_slot_the_player_sees() {
        let mut hotbar = Hotbar::default();
        let mut keys = [0; 9];
        keys[2] = 1;
        keys[6] = 1;
        assert_eq!(hotbar.add_key_presses(keys), Some(6));
    }

    #[test]
    fn each_key_gives_up_one_press_per_tick() {
        let mut hotbar = Hotbar::default();
        let mut keys = [0; 9];
        keys[1] = 2;
        keys[4] = 1;
        hotbar.add_key_presses(keys);
        assert_eq!(hotbar.take_key_press(), Some(4));
        assert_eq!(hotbar.take_key_press(), Some(1));
        assert_eq!(hotbar.take_key_press(), None);
    }

    #[test]
    fn the_server_is_told_a_slot_once() {
        let mut hotbar = Hotbar {
            applied: 3,
            ..Hotbar::default()
        };
        assert_eq!(hotbar.take_sync(), Some(3));
        assert_eq!(hotbar.take_sync(), None);
    }
}
