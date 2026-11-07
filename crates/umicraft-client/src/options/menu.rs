// SPDX-License-Identifier: MIT OR Apache-2.0

//! The menu options: which title screen the player sees (ADR-0056).
//!
//! Stored as `[menu] title_style = "modern" | "classic"` in `options.toml`.

use serde::{Deserialize, Serialize};
use umicraft_ui::menus::TitleStyle;

/// The saved form of [`TitleStyle`]. A value this version does not know
/// reads as the default (`Modern`), so a file written by a newer version
/// still loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TitleStyleSetting {
    // `serde(other)` must be the last variant.
    /// The vanilla 1.8 title screen.
    Classic,
    /// Umicraft's own title screen.
    #[default]
    #[serde(other)]
    Modern,
}

impl From<TitleStyleSetting> for TitleStyle {
    fn from(setting: TitleStyleSetting) -> Self {
        match setting {
            TitleStyleSetting::Modern => Self::Modern,
            TitleStyleSetting::Classic => Self::Classic,
        }
    }
}

impl From<TitleStyle> for TitleStyleSetting {
    fn from(style: TitleStyle) -> Self {
        match style {
            TitleStyle::Modern => Self::Modern,
            TitleStyle::Classic => Self::Classic,
        }
    }
}

/// The menu options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MenuOptions {
    /// The title screen the client starts with.
    pub title_style: TitleStyleSetting,
}

impl MenuOptions {
    /// The title screen to show.
    #[must_use]
    pub fn style(self) -> TitleStyle {
        self.title_style.into()
    }

    /// Switches to the other title screen, for the toggle of the options.
    pub fn toggle_title_style(&mut self) {
        self.title_style = self.style().toggled().into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_the_modern_screen() {
        assert_eq!(MenuOptions::default().style(), TitleStyle::Modern);
    }

    #[test]
    fn the_style_converts_both_ways() {
        for style in [TitleStyle::Modern, TitleStyle::Classic] {
            assert_eq!(TitleStyle::from(TitleStyleSetting::from(style)), style);
        }
    }

    #[test]
    fn toggling_flips_the_style() {
        let mut options = MenuOptions::default();
        options.toggle_title_style();
        assert_eq!(options.style(), TitleStyle::Classic);
        options.toggle_title_style();
        assert_eq!(options.style(), TitleStyle::Modern);
    }

    #[test]
    fn the_style_round_trips_through_toml() {
        let options = MenuOptions {
            title_style: TitleStyleSetting::Classic,
        };
        let text = toml::to_string(&options).expect("serializes");
        assert_eq!(text.trim(), "title_style = \"classic\"");
        assert_eq!(toml::from_str::<MenuOptions>(&text), Ok(options));
    }

    #[test]
    fn a_missing_value_gives_the_default() {
        assert_eq!(toml::from_str("").ok(), Some(MenuOptions::default()));
    }

    #[test]
    fn a_garbage_value_gives_the_default() {
        let options: MenuOptions =
            toml::from_str("title_style = \"holographic\"").expect("unknown names are tolerated");
        assert_eq!(options, MenuOptions::default());
    }
}
