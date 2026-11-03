// SPDX-License-Identifier: MIT OR Apache-2.0

//! The clickable parts of the container screens and the state their sprites
//! depend on.

use super::decoration::Decoration;

/// A button or option row of a container screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hotspot {
    /// One of the three enchantment rows of the enchanting table.
    Enchant(u8),
    /// A beacon power: `tier` is the row (0 to 3) and `slot` the position in
    /// it (see [`super::beacon::effect_of`]).
    BeaconPower {
        /// Row of the button, 0 to 3.
        tier: u8,
        /// Position inside the row.
        slot: u8,
    },
    /// The beacon "done" button.
    BeaconConfirm,
    /// The beacon "cancel" button.
    BeaconCancel,
    /// The villager arrow towards the previous offer.
    TradePrevious,
    /// The villager arrow towards the next offer.
    TradeNext,
}

/// The rectangle of a hotspot in unscaled texture pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotspotSpec {
    /// What the rectangle is.
    pub hotspot: Hotspot,
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// What the dynamic sprites of a screen depend on.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScreenState<'a> {
    /// The window properties the server sent.
    pub properties: &'a [i16],
    /// Whether each of the first three own slots holds a stack.
    pub filled: [bool; 3],
    /// The stack size in the second own slot (the lapis of the enchanting
    /// table).
    pub lapis: u8,
    /// The experience level of the player.
    pub xp_level: i32,
    /// Whether the player is in creative mode (enchanting is free).
    pub creative: bool,
    /// The hotspot under the pointer.
    pub hover: Option<Hotspot>,
    /// Whether the villager has an offer before the selected one.
    pub trade_previous: bool,
    /// Whether the villager has an offer after the selected one.
    pub trade_next: bool,
}

/// A line of text drawn over a screen, in unscaled GUI pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    /// The text.
    pub text: String,
    /// Right edge of the text.
    pub right: u32,
    /// Top edge of the text.
    pub y: u32,
    /// The colour as `0xRRGGBB`.
    pub color: u32,
}

/// The sprites and labels of a screen for a [`ScreenState`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dynamic {
    /// The sprites, drawn over the background.
    pub decorations: Vec<Decoration>,
    /// The texts.
    pub labels: Vec<Label>,
}

impl Dynamic {
    /// Only sprites.
    pub(super) fn sprites(decorations: Vec<Decoration>) -> Self {
        Self {
            decorations,
            labels: Vec::new(),
        }
    }
}
