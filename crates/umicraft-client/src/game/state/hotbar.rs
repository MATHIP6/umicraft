// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hotbar: the selected slot, its sync with the server and the presses of
//! the slot, drop and pick-block keys between two ticks.

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
}
