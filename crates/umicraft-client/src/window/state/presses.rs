// SPDX-License-Identifier: MIT OR Apache-2.0

//! The presses of the frame that the next input update carries to the game
//! thread.

use umicraft_game::item_use::DropKind;

/// The presses not handed to the game thread yet.
pub(in crate::window) struct PendingPresses {
    pub(in crate::window) select_slot: Option<u8>,
    /// The digit key presses not sent yet (MCP `KeyBinding.pressTime`).
    pub(in crate::window) hotbar_keys: [u8; 9],
    pub(in crate::window) break_block: bool,
    /// The use button went down since the last input update.
    pub(in crate::window) place_block: bool,
    /// The pick block binding went down since the last input update.
    pub(in crate::window) pick_block: bool,
    /// The drop key was pressed since the last input update.
    pub(in crate::window) drop_item: Option<DropKind>,
    /// The perspective key was pressed since the last input update.
    pub(in crate::window) toggle_perspective: bool,
    /// The screenshot binding was pressed; the next frame is saved.
    pub(in crate::window) screenshot: bool,
    /// The player pressed "Respawn" and waits for the server.
    pub(in crate::window) respawn: bool,
}
