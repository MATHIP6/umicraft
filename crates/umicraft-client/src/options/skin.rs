// SPDX-License-Identifier: MIT OR Apache-2.0

//! The skin customisation options (`GuiCustomizeSkin`).
//!
//! The seven model-part toggles of MCP `EnumPlayerModelParts` are sent to the
//! server in the skin-parts byte of `Client Settings`. The "main hand" option
//! belongs to protocol 1.9; the 1.8 packet has no field for it, so it does
//! not exist here (ADR-0043).

use serde::{Deserialize, Serialize};

/// One outer model layer of the player skin (`EnumPlayerModelParts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinPart {
    /// The cape.
    Cape,
    /// The jacket layer of the body.
    Jacket,
    /// The left sleeve layer.
    LeftSleeve,
    /// The right sleeve layer.
    RightSleeve,
    /// The left pants layer.
    LeftPantsLeg,
    /// The right pants layer.
    RightPantsLeg,
    /// The hat layer of the head.
    Hat,
}

impl SkinPart {
    /// The parts in the order of `EnumPlayerModelParts.values()`, which is
    /// the order of the buttons of the page.
    pub const ALL: [Self; 7] = [
        Self::Cape,
        Self::Jacket,
        Self::LeftSleeve,
        Self::RightSleeve,
        Self::LeftPantsLeg,
        Self::RightPantsLeg,
        Self::Hat,
    ];

    /// The part id of `EnumPlayerModelParts` (`0` cape ... `6` hat).
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// The bit of the part in the skin-parts byte (`1 << id`).
    #[must_use]
    pub const fn mask(self) -> u8 {
        1 << self.id()
    }

    /// The language key of the button label (`options.modelPart.<name>`).
    #[must_use]
    pub const fn lang_key(self) -> &'static str {
        match self {
            Self::Cape => "options.modelPart.cape",
            Self::Jacket => "options.modelPart.jacket",
            Self::LeftSleeve => "options.modelPart.left_sleeve",
            Self::RightSleeve => "options.modelPart.right_sleeve",
            Self::LeftPantsLeg => "options.modelPart.left_pants_leg",
            Self::RightPantsLeg => "options.modelPart.right_pants_leg",
            Self::Hat => "options.modelPart.hat",
        }
    }
}

/// The skin customisation options; every layer is shown by default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkinOptions {
    /// Show the cape.
    pub cape: bool,
    /// Show the jacket layer.
    pub jacket: bool,
    /// Show the left sleeve layer.
    pub left_sleeve: bool,
    /// Show the right sleeve layer.
    pub right_sleeve: bool,
    /// Show the left pants layer.
    pub left_pants_leg: bool,
    /// Show the right pants layer.
    pub right_pants_leg: bool,
    /// Show the hat layer.
    pub hat: bool,
}

impl Default for SkinOptions {
    fn default() -> Self {
        Self {
            cape: true,
            jacket: true,
            left_sleeve: true,
            right_sleeve: true,
            left_pants_leg: true,
            right_pants_leg: true,
            hat: true,
        }
    }
}

impl SkinOptions {
    /// Whether `part` is shown.
    #[must_use]
    pub fn is_enabled(&self, part: SkinPart) -> bool {
        *self.flag(part)
    }

    /// Flips `part` (`GameSettings.switchModelPartEnabled`).
    pub fn toggle(&mut self, part: SkinPart) {
        let flag = self.flag_mut(part);
        *flag = !*flag;
    }

    /// The skin-parts byte of `Client Settings`: the OR of the masks of the
    /// shown parts (`GameSettings.sendSettingsToServer`).
    #[must_use]
    pub fn parts_mask(&self) -> u8 {
        SkinPart::ALL
            .into_iter()
            .filter(|part| self.is_enabled(*part))
            .fold(0, |mask, part| mask | part.mask())
    }

    fn flag(&self, part: SkinPart) -> &bool {
        match part {
            SkinPart::Cape => &self.cape,
            SkinPart::Jacket => &self.jacket,
            SkinPart::LeftSleeve => &self.left_sleeve,
            SkinPart::RightSleeve => &self.right_sleeve,
            SkinPart::LeftPantsLeg => &self.left_pants_leg,
            SkinPart::RightPantsLeg => &self.right_pants_leg,
            SkinPart::Hat => &self.hat,
        }
    }

    fn flag_mut(&mut self, part: SkinPart) -> &mut bool {
        match part {
            SkinPart::Cape => &mut self.cape,
            SkinPart::Jacket => &mut self.jacket,
            SkinPart::LeftSleeve => &mut self.left_sleeve,
            SkinPart::RightSleeve => &mut self.right_sleeve,
            SkinPart::LeftPantsLeg => &mut self.left_pants_leg,
            SkinPart::RightPantsLeg => &mut self.right_pants_leg,
            SkinPart::Hat => &mut self.hat,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layer_is_shown_by_default_and_all_seven_bits_are_set() {
        let skin = SkinOptions::default();
        assert!(SkinPart::ALL.into_iter().all(|part| skin.is_enabled(part)));
        assert_eq!(skin.parts_mask(), 0b0111_1111);
    }

    #[test]
    fn each_part_owns_the_bit_of_enum_player_model_parts() {
        // `EnumPlayerModelParts`: partMask = 1 << partId.
        let expected = [
            (SkinPart::Cape, 0x01),
            (SkinPart::Jacket, 0x02),
            (SkinPart::LeftSleeve, 0x04),
            (SkinPart::RightSleeve, 0x08),
            (SkinPart::LeftPantsLeg, 0x10),
            (SkinPart::RightPantsLeg, 0x20),
            (SkinPart::Hat, 0x40),
        ];
        for (part, mask) in expected {
            assert_eq!(part.mask(), mask, "{part:?}");
            let mut skin = SkinOptions::default();
            skin.toggle(part);
            assert!(!skin.is_enabled(part));
            assert_eq!(skin.parts_mask(), 0b0111_1111 & !mask, "{part:?}");
        }
    }

    #[test]
    fn only_the_shown_parts_are_in_the_byte() {
        let skin = SkinOptions {
            cape: false,
            jacket: false,
            left_sleeve: false,
            right_sleeve: false,
            left_pants_leg: false,
            right_pants_leg: false,
            hat: true,
        };
        assert_eq!(skin.parts_mask(), 0x40);
        let nothing = SkinOptions { hat: false, ..skin };
        assert_eq!(nothing.parts_mask(), 0);
    }

    #[test]
    fn toggling_twice_restores_the_part() {
        let mut skin = SkinOptions::default();
        skin.toggle(SkinPart::Hat);
        skin.toggle(SkinPart::Hat);
        assert_eq!(skin, SkinOptions::default());
    }

    #[test]
    fn the_labels_use_the_model_part_keys() {
        for part in SkinPart::ALL {
            assert!(part.lang_key().starts_with("options.modelPart."));
        }
    }
}
