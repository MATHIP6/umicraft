// SPDX-License-Identifier: MIT OR Apache-2.0

//! Client themes (ADR-0078): the look of buttons, sliders and menu backgrounds.
//!
//! A [`Theme`] is plain data: a [`WidgetSkin`] for the controls and a
//! [`BackgroundSkin`] for the screen behind them. The widgets keep all their
//! behaviour (hit testing, focus, click sound) and only ask the theme how to
//! paint. The set of themes is closed, so a skin is an enum, not a trait
//! object: painting stays allocation-free and statically dispatched.
//!
//! [`ThemeId::Vanilla`] is the 1.8 look and the only one that reads the
//! resource packs (`widgets.png`, `options_background.png`). The other themes
//! are original and ignore them.

mod background;
mod flat;
mod font;
mod palette;

pub(crate) use background::fill_gradient;
pub use background::BackgroundSkin;
pub use flat::{ControlState, FlatSkin};
pub use font::{hd_cell_for_scale, FontSkin, HD_CELLS};

use crate::draw::Color;
use palette::{AMETHYST, EMBER};

/// The themes the player can choose from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum ThemeId {
    /// The 1.8 look, textured by the active resource packs.
    #[default]
    Vanilla,
    /// Near-black flat controls with an violet accent.
    Ember,
    /// Beveled purple controls over a dark backdrop.
    Amethyst,
}

impl ThemeId {
    /// Every theme, in the order the theme screen lists them.
    pub const ALL: [Self; 3] = [Self::Vanilla, Self::Ember, Self::Amethyst];

    /// The `umicraft.theme.*` language key of the theme's display name.
    #[must_use]
    pub const fn lang_key(self) -> &'static str {
        match self {
            Self::Vanilla => "umicraft.theme.vanilla",
            Self::Ember => "umicraft.theme.ember",
            Self::Amethyst => "umicraft.theme.amethyst",
        }
    }

    /// The design data of this theme.
    #[must_use]
    pub const fn theme(self) -> Theme {
        match self {
            Self::Vanilla => Theme::VANILLA,
            Self::Ember => Theme {
                widgets: WidgetSkin::Flat(EMBER),
                font: FontSkin::Hd,
                background: BackgroundSkin::Gradient {
                    top: palette::EMBER_BACKDROP_TOP,
                    bottom: palette::EMBER_BACKDROP_BOTTOM,
                    list: palette::EMBER_LIST,
                },
            },
            Self::Amethyst => Theme {
                widgets: WidgetSkin::Flat(AMETHYST),
                font: FontSkin::Hd,
                background: BackgroundSkin::Gradient {
                    top: palette::AMETHYST_BACKDROP,
                    bottom: palette::AMETHYST_BACKDROP,
                    list: palette::AMETHYST_LIST,
                },
            },
        }
    }
}

/// How the controls of a screen are painted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WidgetSkin {
    /// The 1.8 textures, or the grey fallback without a client jar.
    Vanilla,
    /// Flat rectangles with an optional border and bevel.
    Flat(FlatSkin),
}

/// The design data of one theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    /// Buttons and sliders.
    pub widgets: WidgetSkin,
    /// The screen behind the menus and the lists drawn over it.
    pub background: BackgroundSkin,
    /// The glyph grid of every text of the UI.
    pub font: FontSkin,
}

impl Theme {
    /// The 1.8 look, which is what the client drew before themes existed.
    pub const VANILLA: Self = Self {
        widgets: WidgetSkin::Vanilla,
        background: BackgroundSkin::Dirt,
        font: FontSkin::Game,
    };
}

impl Default for Theme {
    fn default() -> Self {
        Self::VANILLA
    }
}

/// An opaque colour from a `0xRRGGBB` value.
#[must_use]
pub(crate) const fn rgb(hex: u32) -> Color {
    [
        ((hex >> 16) & 0xFF) as f32 / 255.0,
        ((hex >> 8) & 0xFF) as f32 / 255.0,
        (hex & 0xFF) as f32 / 255.0,
        1.0,
    ]
}

#[cfg(test)]
mod tests;
