// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the mods menu shows and what it reports.
//!
//! `umicraft-ui` knows nothing of the built-in mods crate: the client converts
//! its registry into these plain types (texts already translated) and applies
//! the [`ModsOutcome`] the menu answers with.

use super::hud_layer::{HudEditorOutcome, HudEditorView};

/// The value of a setting as the menu edits it.
#[derive(Debug, Clone, PartialEq)]
pub enum ModValue {
    /// A toggle.
    Bool(bool),
    /// A slider.
    Number(f32),
    /// The key of a choice, or a `#rrggbb` color.
    Text(String),
}

/// How a setting is edited, with its current state.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingControl {
    /// An on/off switch.
    Toggle(bool),
    /// A number in `min..=max`, moved in `step`s.
    Slider {
        /// Smallest value.
        min: f32,
        /// Largest value.
        max: f32,
        /// Distance between two values.
        step: f32,
        /// Current value.
        value: f32,
    },
    /// One of a few options.
    Choice {
        /// `(key, translated label)` of each option, in display order.
        options: Vec<(String, String)>,
        /// Index of the current option in `options`.
        selected: usize,
    },
    /// A `#rrggbb` color.
    Color(String),
}

/// One setting of a mod.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingView {
    /// Key of the setting within its mod; sent back in [`ModsOutcome::Set`].
    pub key: String,
    /// Translated label.
    pub label: String,
    /// How it is edited.
    pub control: SettingControl,
}

/// One mod, as a card of the menu.
#[derive(Debug, Clone, PartialEq)]
pub struct ModCardView {
    /// Identifier sent back in the outcomes.
    pub id: String,
    /// Translated name; the search looks at it.
    pub name: String,
    /// Translated one-line description.
    pub description: String,
    /// Index in [`ModsView::categories`].
    pub category: usize,
    /// Whether the mod is on.
    pub enabled: bool,
    /// The options of the mod, in display order.
    pub settings: Vec<SettingView>,
}

/// Everything the menu lists.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModsView {
    /// Translated category names, in tab order.
    pub categories: Vec<String>,
    /// The mods, in the order of their category then of their registration.
    pub cards: Vec<ModCardView>,
    /// The widgets of the enabled HUD mods, which the menu lets the player move.
    pub hud: HudEditorView,
}

/// What the player did on the mods menu. Moving between its pages is the menu's
/// own business and never leaves it.
#[derive(Debug, Clone, PartialEq)]
pub enum ModsOutcome {
    /// The landing page was closed with its cross.
    Close,
    /// A mod was switched on or off.
    Toggle {
        /// The mod.
        id: String,
        /// The state asked for.
        enabled: bool,
    },
    /// A setting of a mod was changed.
    Set {
        /// The mod.
        id: String,
        /// The setting.
        key: String,
        /// The value asked for; the registry makes it valid.
        value: ModValue,
    },
    /// A HUD widget was placed or reset.
    Hud(HudEditorOutcome),
}
