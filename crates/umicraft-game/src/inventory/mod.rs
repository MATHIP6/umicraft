// SPDX-License-Identifier: MIT OR Apache-2.0

//! Inventory and container window model (M9, P4; ADR-0016, ADR-0025).
//!
//! The model is pure data — no I/O, no packets. [`WindowManager`] owns the
//! player inventory, the optional open container and the cursor, applies the
//! server's window packets and predicts the player's own clicks
//! ([`click::slot_click`]) with a rollback on rejection ([`transactions`]).
//!
//! ## Slot numbering
//!
//! The player window (id 0): 0 crafting result, 1–4 crafting grid, 5–8 armour
//! (head to feet), 9–35 backpack, 36–44 hotbar. A container window lists its
//! own slots first, then the 27 backpack and the 9 hotbar slots
//! ([`kind::resolve_container_slot`]).

pub mod armor;
pub mod click;
pub mod kind;
mod manager;
pub mod player;
pub mod preview;
pub mod slot;
pub mod space;
pub mod stack;
pub mod tooltip;
pub mod transactions;
pub mod view;

pub use kind::{
    resolve_container_slot, SlotTarget, WindowKind, HOTBAR_LEN, HOTBAR_START, MAIN_INV_LEN,
    MAIN_INV_START, PLAYER_SLOT_COUNT,
};
pub use manager::{ClickCommand, ContainerWindow, WindowManager};
pub use player::PlayerInventory;
pub use preview::{drag_preview, DragPreview};
pub use view::{ContainerView, WindowView};
