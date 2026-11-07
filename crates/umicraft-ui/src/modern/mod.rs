// SPDX-License-Identifier: MIT OR Apache-2.0

//! Reusable widgets for Umicraft's own screens (ADR-0056).
//!
//! The look follows the PvP clients: translucent dark surfaces over the
//! panorama, rounded corners, a sans-serif face and a short hover fade. The
//! vanilla screens never use these widgets.
//!
//! Every length of [`theme`] is in GUI units; callers multiply by the GUI
//! scale (physical pixels per unit) when they build rectangles and pass the
//! same scale to the widgets. Text uses the brand atlas, which the caller
//! passes like the game atlas.
//!
//! The UI keeps no clock: a screen keeps one [`HoverFade`] per button,
//! advances it once per 20 TPS tick with the `hovered` flag the button
//! returned, and passes `fade.level(partial_tick)` to the button.

mod button;
mod panel;
mod text;
pub mod theme;

pub use button::{HoverFade, IconButton, ModernButton};
pub use panel::{panel, vignette};
pub use text::{brand_text_style, draw_text, measure_text, Align};

#[cfg(test)]
mod tests;
