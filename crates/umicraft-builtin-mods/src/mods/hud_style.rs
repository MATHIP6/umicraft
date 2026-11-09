// SPDX-License-Identifier: MIT OR Apache-2.0
//! The options every HUD mod shares: what is behind the text and how the text is
//! shadowed. A mod lists them with [`hud_settings!`], after its own. Their language
//! keys are `umicraft.mods.hud.*`, once for all the mods.

use crate::settings::{SettingDef, SettingKind};

pub(crate) const BACKGROUND: SettingDef = SettingDef {
    key: "background",
    label_key: "umicraft.mods.hud.background",
    kind: SettingKind::Toggle { default: true },
};
pub(crate) const BACKGROUND_COLOR: SettingDef = SettingDef {
    key: "background_color",
    label_key: "umicraft.mods.hud.background_color",
    kind: SettingKind::Color { default: "#000000" },
};
pub(crate) const BACKGROUND_OPACITY: SettingDef = SettingDef {
    key: "background_opacity",
    label_key: "umicraft.mods.hud.background_opacity",
    kind: SettingKind::Slider {
        min: 0.0,
        max: 1.0,
        step: 0.05,
        default: 0.35,
    },
};
pub(crate) const CORNER_RADIUS: SettingDef = SettingDef {
    key: "corner_radius",
    label_key: "umicraft.mods.hud.corner_radius",
    kind: SettingKind::Slider {
        min: 0.0,
        max: 6.0,
        step: 0.5,
        default: 2.0,
    },
};
pub(crate) const PADDING: SettingDef = SettingDef {
    key: "padding",
    label_key: "umicraft.mods.hud.padding",
    kind: SettingKind::Slider {
        min: 0.0,
        max: 8.0,
        step: 0.5,
        default: 2.0,
    },
};
pub(crate) const SHADOW: SettingDef = SettingDef {
    key: "shadow",
    label_key: "umicraft.mods.hud.shadow",
    kind: SettingKind::Choice {
        options: &["vanilla", "custom", "off"],
        default: 0,
    },
};
pub(crate) const SHADOW_COLOR: SettingDef = SettingDef {
    key: "shadow_color",
    label_key: "umicraft.mods.hud.shadow_color",
    kind: SettingKind::Color { default: "#000000" },
};

/// The keys of the shared options, for the tests of the mods.
#[cfg(test)]
pub(crate) const KEYS: [&str; 7] = [
    "background",
    "background_color",
    "background_opacity",
    "corner_radius",
    "padding",
    "shadow",
    "shadow_color",
];

/// A settings list: the options of the mod, then the shared HUD ones.
macro_rules! hud_settings {
    ($($own:expr),* $(,)?) => {
        &[
            $($own,)*
            $crate::mods::hud_style::BACKGROUND,
            $crate::mods::hud_style::BACKGROUND_COLOR,
            $crate::mods::hud_style::BACKGROUND_OPACITY,
            $crate::mods::hud_style::CORNER_RADIUS,
            $crate::mods::hud_style::PADDING,
            $crate::mods::hud_style::SHADOW,
            $crate::mods::hud_style::SHADOW_COLOR,
        ]
    };
}
pub(crate) use hud_settings;
