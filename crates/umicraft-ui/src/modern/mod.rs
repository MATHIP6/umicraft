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
pub mod search_field;
mod slider;
mod tabs;
mod text;
pub mod theme;
mod toggle;

pub use button::{HoverFade, ModernButton};
pub use panel::{panel, vignette};
pub use search_field::search_field;
pub use slider::{modern_slider, SliderDrag, SliderRange};
pub use tabs::{tab_at, tab_rects, tabs};
pub use text::{brand_text_style, draw_text, fit_text, measure_text, Align};
pub use toggle::{knob_rect, toggle, TOGGLE_HEIGHT, TOGGLE_WIDTH};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod widget_tests;
