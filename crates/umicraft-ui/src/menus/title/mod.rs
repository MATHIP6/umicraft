// SPDX-License-Identifier: MIT OR Apache-2.0

//! The title screen (ADR-0056).
//!
//! [`classic`] is the vanilla screen (MCP `GuiMainMenu`).

mod classic;

pub use classic::{
    title_layout, TitleInfo, TitleLayout, TitleOutcome, TitleScreen, LOGO_TEXT, SPLASHES,
};

/// Which title screen the player sees (ADR-0056).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TitleStyle {
    /// Umicraft's own screen: logo, wordmark, Play and Options.
    #[default]
    Modern,
    /// The vanilla 1.8 screen.
    Classic,
}

impl TitleStyle {
    /// The other style, for the toggle of the options.
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Modern => Self::Classic,
            Self::Classic => Self::Modern,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TitleStyle;

    #[test]
    fn the_modern_screen_is_the_default_and_toggles_to_classic() {
        assert_eq!(TitleStyle::default(), TitleStyle::Modern);
        assert_eq!(TitleStyle::Modern.toggled(), TitleStyle::Classic);
        assert_eq!(TitleStyle::Classic.toggled(), TitleStyle::Modern);
    }
}
