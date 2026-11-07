// SPDX-License-Identifier: MIT OR Apache-2.0

//! The title screen (ADR-0056).
//!
//! [`classic`] is the vanilla screen (MCP `GuiMainMenu`).

mod classic;

pub use classic::{
    title_layout, TitleInfo, TitleLayout, TitleOutcome, TitleScreen, LOGO_TEXT, SPLASHES,
};
