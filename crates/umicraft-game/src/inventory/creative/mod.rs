// SPDX-License-Identifier: MIT OR Apache-2.0

//! The creative inventory (P4e; MCP `GuiContainerCreative`, `CreativeTabs`).
//!
//! The module is the pure game layer: what each tab lists, the search, the
//! scrollbar, and what every click does. Drawing and input wiring live in the
//! client.
//!
//! ## How a frontend uses it
//!
//! Keep one [`CreativeScreen`] per session and call [`CreativeScreen::reopen`]
//! when the window opens. Feed it mouse and key events together with the
//! [`WindowManager`](super::WindowManager), which stays the only owner of the
//! player's slots and the cursor. Every transition returns the
//! [`CreativeAction`]s to send as `Creative Inventory Action`
//! ([`CreativeAction`] converts into the packet).
//!
//! Window geometry (all relative to the window's top-left corner, 195 by 136,
//! see [`tabs::WINDOW_WIDTH`]): tab buttons [`CreativeTab::hit_origin`], slots
//! [`CreativeSlot::position`], hit test [`CreativeScreen::slot_at`] and
//! [`CreativeTab::at`], scrollbar [`scroll`].
//!
//! ## Tab contents
//!
//! `displayAllReleventItems` is reproduced by the generated [`table`]
//! (`docs/tools/gen_creative_tabs.py`): items in registry (id) order, each
//! expanded through its `getSubItems` / `getSubBlocks`. Rows that need NBT or a
//! computation are expanded at start-up by [`entry`]: enchanted books (one per
//! enchantment at its maximum level on the Tools and Combat tabs; every level on
//! the search tab), banners (base colour only) and the potion variants
//! ([`crate::items::potion_creative_damages`]).

mod action;
mod click;
#[cfg(test)]
mod click_tests;
mod entry;
mod items;
#[cfg(test)]
mod key_tests;
mod keys;
pub mod scroll;
mod search;
mod stack_nbt;
mod state;
mod table;
pub mod tabs;
mod target;

pub use action::{CreativeAction, DROP_SLOT};
pub use click::MouseButton;
pub use items::tab_items;
pub use search::search_items;
pub use state::{CreativeScreen, Resolver};
pub use tabs::CreativeTab;
pub use target::{slot_at, visible_slots, CreativeSlot};
