// SPDX-License-Identifier: MIT OR Apache-2.0

//! The opening animation of the mods menu: the dark sheet and the blur of the
//! game behind fade in and the logo of the landing grows. Every text, button
//! and the panel are there from the first frame, so the menu is usable at once.

use crate::menus::title::modern::Stage;

/// Ticks the opening animation lasts (0.6 s at 20 TPS): the end of its last
/// stage.
pub(super) const OPEN_TICKS: u8 = 12;
/// The dark sheet comes in.
pub(super) const SHEET: Stage = Stage::new(0.0, 8.0);
/// The blur of the game behind fades in, a little slower than the sheet.
pub(super) const BLUR: Stage = Stage::new(0.0, 12.0);
/// The logo grows, rises and fades in.
pub(super) const LOGO: Stage = Stage::new(0.0, 10.0);
/// How far the logo rises while it grows, in GUI units.
pub(super) const LOGO_RISE: f32 = 8.0;
/// Size of the logo when it starts to grow, as a fraction of its final size.
pub(super) const LOGO_START_SCALE: f32 = 0.8;
