// SPDX-License-Identifier: MIT OR Apache-2.0

//! Immutable snapshots of the window state for the render thread.

use umicraft_protocol::packets::play::ItemStack;

use super::kind::{WindowKind, PLAYER_SLOT_COUNT};

/// An immutable snapshot of one container.
#[derive(Debug, Clone, PartialEq)]
pub struct ContainerView {
    /// The window id assigned by the server.
    pub window_id: u8,
    /// The kind of container.
    pub kind: WindowKind,
    /// The title as JSON chat text.
    pub title: String,
    /// The container's own slots.
    pub slots: Vec<Option<ItemStack>>,
    /// The properties (furnace progress, enchantment levels, …).
    pub properties: Vec<i16>,
}

/// An immutable snapshot of the full window state.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowView {
    /// All 45 player inventory slots.
    pub player_slots: [Option<ItemStack>; PLAYER_SLOT_COUNT],
    /// The open container, if any.
    pub container: Option<ContainerView>,
    /// The item held by the cursor.
    pub cursor: Option<ItemStack>,
    /// The currently selected hotbar slot (0–8).
    pub selected_slot: u8,
}
