// SPDX-License-Identifier: MIT OR Apache-2.0

//! The look of the mods menu: the colours and sizes of its Mods button, its
//! close cross and the sheet behind it. Where things go is in
//! [`layout`](super::layout), how they move in [`animation`](super::animation).

use crate::draw::Color;

/// Thickness of the thin rim of the Mods button, in GUI units.
pub(super) const RIM_WIDTH: f32 = 0.5;
/// The rim at rest: a faint white line.
pub(super) const RIM: Color = [1.0, 1.0, 1.0, 0.14];
/// The rim when the pointer is over the button: the accent colour.
pub(super) const RIM_HOVER: Color = [0.36, 0.55, 1.0, 0.70];
/// Edge of the module grid icon, in GUI units: two squares and a gap.
pub(super) const ICON_SIZE: f32 = 7.0;
/// Space between the icon and the label.
pub(super) const ICON_GAP: f32 = 5.0;
/// Space between the capitals of the label, as a share of their size.
pub(super) const LABEL_TRACKING: f32 = 0.25;
/// The label of the close button, as on the title screen.
pub(super) const CLOSE_CROSS: &str = "\u{d7}";

/// A light dark sheet over the blurred game, so the menu reads well on bright
/// worlds.
pub(super) const BACKDROP: Color = [0.0, 0.0, 0.0, 0.25];
