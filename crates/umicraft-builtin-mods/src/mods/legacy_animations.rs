// SPDX-License-Identifier: MIT OR Apache-2.0
//! 1.7 Animations: brings back how the 1.7 client animated the hand and the
//! view (ADR-0079).
//!
//! The mod holds only its switch and one toggle per animation. The client
//! reads them (`ModsRuntime::legacy_animations`) and hands them to the game
//! thread, which runs them (`umicraft_game::legacy`). Nothing the server sees
//! changes.

use crate::id::{ModCategory, ModId};
use crate::meta::ModMeta;
use crate::settings::{SettingDef, SettingKind};
use crate::traits::BuiltinMod;

static META: ModMeta = ModMeta {
    id: ModId::new("legacy_animations"),
    name_key: "umicraft.mods.legacy_animations.name",
    description_key: "umicraft.mods.legacy_animations.desc",
    category: ModCategory::Visual,
    default_enabled: false,
};

const SETTINGS: &[SettingDef] = &[
    SettingDef {
        key: "block_hit",
        label_key: "umicraft.mods.legacy_animations.block_hit",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "equip",
        label_key: "umicraft.mods.legacy_animations.equip",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "sneak_camera",
        label_key: "umicraft.mods.legacy_animations.sneak_camera",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "item_position",
        label_key: "umicraft.mods.legacy_animations.item_position",
        kind: SettingKind::Toggle { default: true },
    },
];

/// The 1.7 animations mod.
pub(super) struct LegacyAnimations;

impl BuiltinMod for LegacyAnimations {
    fn meta(&self) -> &'static ModMeta {
        &META
    }

    fn settings(&self) -> &'static [SettingDef] {
        SETTINGS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::ModSettings;

    #[test]
    fn it_is_a_visual_mod_that_starts_off_with_every_animation_selected() {
        let m = LegacyAnimations;
        assert_eq!(m.meta().category, ModCategory::Visual);
        assert!(!m.meta().default_enabled, "the 1.8 look is the default");
        assert!(m.hud().is_none(), "it draws no widget");
        let settings = ModSettings::defaults(m.settings());
        for key in ["block_hit", "equip", "sneak_camera", "item_position"] {
            assert!(settings.bool(key), "{key}");
        }
    }
}
