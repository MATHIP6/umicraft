// SPDX-License-Identifier: MIT OR Apache-2.0

//! The in-game HUD of spec §6.6 (UI-2): hotbar, health/food/armour rows,
//! experience bar, crosshair, chat, social overlays and the debug block.
//!
//! The HUD is split by responsibility:
//!
//! - [`state`]: [`HudState`] and the view structs the client fills each frame;
//! - [`layout`]: [`HudLayout`], the pure geometry derived from the viewport;
//! - [`gui`]: the sprite-based renderer into the UI [`DrawList`];
//! - [`legacy`]: the flat-colour [`Overlay`] renderer kept for demos and
//!   fallback paths (with `chat_hud` and `social_hud`, which also hold the
//!   pure chat and tab-list rules the GUI renderer reuses).
//!
//! The HUD draws no hotbar items: the UI crate draws them over the hotbar.
//!
//! # Wiring
//!
//! The viewport size must be the one the [`Overlay`] was built with, because
//! the overlay turns the pixel coordinates produced here into clip space:
//!
//! ```
//! use umicraft_render::{draw_hud, HudState, Overlay};
//!
//! let mut overlay = Overlay::new(640, 360);
//! let state = HudState::default();
//! draw_hud(&mut overlay, &state, 640, 360);
//! assert!(!overlay.is_empty());
//! ```
//!
//! [`DrawList`]: umicraft_ui::DrawList
//! [`Overlay`]: crate::Overlay

mod chat_hud;
pub mod gui;
mod layout;
mod legacy;
mod overlay_views;
mod social_hud;
mod state;

pub use gui::{draw_hud_gui, HudSheets};
pub use layout::{selected_index, status_fills, Fill, HudLayout, Rect, StatusRow};
pub use legacy::{draw, draw_debug_only};
pub use overlay_views::{
    plain_text, ActionBarView, BossView, SidebarLineView, SidebarView, TabEntryView, TabListView,
    TabValueStyle, TitleView,
};
pub use state::{
    ChatInputView, ChatLineView, ChatRunView, ChatStyle, DebugF3View, HudState, CHAT_LINES,
    HOTBAR_SLOTS, STATUS_ICONS,
};
