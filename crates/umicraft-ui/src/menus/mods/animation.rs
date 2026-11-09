// SPDX-License-Identifier: MIT OR Apache-2.0

//! The opening animation of the mods menu: the dark sheet and the blur of the
//! game behind fade in. The panel comes in with the motion of its own (see
//! [`motion`](super::motion)).

use crate::menus::title::modern::Stage;

/// Ticks the opening animation lasts (0.6 s at 20 TPS): the end of its last
/// stage.
pub(super) const OPEN_TICKS: u8 = 12;
/// The dark sheet comes in.
pub(super) const SHEET: Stage = Stage::new(0.0, 8.0);
/// The blur of the game behind fades in, a little slower than the sheet.
pub(super) const BLUR: Stage = Stage::new(0.0, 12.0);
