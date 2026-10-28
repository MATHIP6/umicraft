// SPDX-License-Identifier: MIT OR Apache-2.0

//! The M4 widgets: button, text field, scrollable list and panel.
//!
//! A widget is an ordinary function that draws through [`crate::Ui::draw`],
//! reports its state with [`crate::Ui::response`] and returns the same
//! [`crate::UiResponse`]. Widgets own no renderer resources and never touch a
//! window, a file or the network, so the same calls run in the windowed path
//! and in the headless acceptance driver.
//!
//! The functions are re-exported here for a short call path:
//!
//! ```
//! use umicraft_ui::widgets::{button, list, panel, text_field, ListState, TextFieldState};
//! ```
//!
//! The frozen names and semantics come from `docs/decisions/0006-m4-ui-foundation.md`
//! (Appendix B). The button and the text field take a [`crate::FontAtlas`]
//! because the UI core does not own a font; the panel returns its inset
//! content rectangle. Both refinements are documented on the functions and do
//! not change what the widgets mean.

pub(crate) mod style;
mod util;

pub mod button;
pub mod death;
pub mod inventory;
pub mod item_overlay;
pub mod list;
pub mod panel;
pub mod slot;
pub mod text_field;

pub use button::button;
pub use death::{death_layout, death_screen, DeathAction, DeathLayout, BUTTON_DELAY_TICKS};
pub use inventory::{
    container_layout, gui_scale, player_inventory_layout, ContainerLayout, InventoryLayout,
};
pub use item_overlay::{draw_item, durability_bar, DurabilityBar};
pub use list::{list, ListState, ROW_HEIGHT, SCROLLBAR_WIDTH};
pub use panel::{panel, PANEL_INSET};
pub use slot::{slot, SlotItem};
pub use text_field::{text_field, TextFieldState};
