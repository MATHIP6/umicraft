// SPDX-License-Identifier: MIT OR Apache-2.0

//! The colours of the built-in themes, sampled from the owner's mock-ups
//! (`/tmp/tes/menu-1.png` for Ember, `menu-2.png` for Amethyst).

use super::{rgb, FlatSkin};
use crate::draw::Color;

/// Ember: flat near-black controls, one light border, orange accent.
pub(super) const EMBER: FlatSkin = FlatSkin {
    fill: rgb(0x161518),
    hover: rgb(0x242327),
    disabled: rgb(0x161518),
    track: rgb(0x161518),
    border: Some(rgb(0x222122)),
    bevel: None,
    accent: rgb(0xFF7329),
    slider_fill: Some(rgb(0x32211A)),
    text: rgb(0xE5E5E8),
    text_hover: rgb(0xFFFFFF),
    text_disabled: rgb(0x75757D),
    text_shadow: false,
    handle_texels: 6.0,
};

/// Amethyst: purple controls with a dark border and a darker bottom bevel.
pub(super) const AMETHYST: FlatSkin = FlatSkin {
    fill: rgb(0x6E3176),
    hover: rgb(0x853E8E),
    disabled: rgb(0x221E1C),
    track: rgb(0x221E1C),
    border: Some(rgb(0x090604)),
    bevel: Some(rgb(0x4E2352)),
    accent: rgb(0x6E3176),
    slider_fill: None,
    text: rgb(0xF0EAED),
    text_hover: rgb(0xFFFFFF),
    text_disabled: rgb(0x7A7573),
    text_shadow: true,
    handle_texels: 6.0,
};

pub(super) const EMBER_BACKDROP_TOP: Color = rgb(0x111011);
pub(super) const EMBER_BACKDROP_BOTTOM: Color = rgb(0x190E0A);
pub(super) const EMBER_LIST: Color = [0.0, 0.0, 0.0, 0.35];
pub(super) const AMETHYST_BACKDROP: Color = rgb(0x161413);
pub(super) const AMETHYST_LIST: Color = [0.0, 0.0, 0.0, 0.35];
