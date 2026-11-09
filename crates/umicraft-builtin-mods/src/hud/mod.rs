// SPDX-License-Identifier: MIT OR Apache-2.0
//! What a HUD mod draws ([`HudContent`]) and where it goes ([`HudPlacement`]).
//!
//! The placement maths ([`HudPlacement::rect`], [`HudPlacement::from_rect`],
//! [`snap`]) lives here and not in the UI so the HUD editor stays a thin layer
//! over tested functions.

mod content;
mod placement;
pub mod snap;
mod style;

pub use content::{HudContent, HudLine};
pub use placement::{Anchor, HudPlacement, HudRect, MAX_SCALE, MIN_SCALE};
pub use style::{HudStyle, Shadow};
