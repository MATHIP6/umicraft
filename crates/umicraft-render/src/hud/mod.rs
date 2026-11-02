// SPDX-License-Identifier: MIT OR Apache-2.0

//! The in-game HUD of spec §6.6 (UI-2): hotbar, health/food/armour rows,
//! experience bar, crosshair, chat, social overlays and the debug block.
//!
//! The HUD is split by responsibility:
//!
//! - [`state`]: [`HudState`] and the view structs the client fills each frame;
//! - [`gui`]: the sprite-based renderer into the UI [`DrawList`].
//!
//! The flat-colour `Overlay` HUD of earlier milestones was deleted in P9-h
//! (ADR-0036): without the GUI atlas the same renderer draws the parts that
//! need no sprite sheet (chat, titles, sidebar, F3) with the built-in font.
//!
//! # Wiring
//!
//! ```
//! use umicraft_render::{draw_hud_gui, HudSheets, HudState};
//! use umicraft_ui::{DrawList, FontAtlas};
//!
//! let mut list = DrawList::new();
//! let state = HudState::default();
//! draw_hud_gui(&mut list, &state, (640, 360), &FontAtlas::fallback(), &HudSheets::default());
//! ```
//!
//! [`DrawList`]: umicraft_ui::DrawList

pub mod gui;
mod overlay_views;
mod state;

pub use gui::{chat_run_at, draw_hud_gui, HudSheets};
pub use overlay_views::{
    plain_text, ActionBarView, BossView, SidebarLineView, SidebarView, TabEntryView, TabListView,
    TabValueStyle, TitleView,
};
pub use state::{
    ChatInputView, ChatLineView, ChatRunView, ChatStyle, DebugF3View, HudState, HOTBAR_SLOTS,
};
