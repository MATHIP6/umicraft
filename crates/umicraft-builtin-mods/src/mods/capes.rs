// SPDX-License-Identifier: MIT OR Apache-2.0
//! Capes: shows the capes players own on the third-party cape services
//! (ADR-0081).
//!
//! The mod holds only its switch and its settings: which service supplies the
//! local player's cape and which services are asked for everybody else. The
//! client reads them (`ModsRuntime::capes`), fetches and draws the capes.
//! Nothing reaches the game server.

use crate::id::{ModCategory, ModId};
use crate::meta::ModMeta;
use crate::settings::{SettingDef, SettingKind};
use crate::traits::BuiltinMod;

static META: ModMeta = ModMeta {
    id: ModId::new("capes"),
    name_key: "umicraft.mods.capes.name",
    description_key: "umicraft.mods.capes.desc",
    category: ModCategory::Visual,
    default_enabled: true,
};

/// The sources of the local player's cape. `minecraft` keeps the Mojang cape,
/// so no service is asked; the others are the keys of the provider toggles.
const OWN_SOURCES: &[&str] = &[
    "minecraft",
    "optifine",
    "labymod",
    "minecraftcapes",
    "cosmetica",
];

const SETTINGS: &[SettingDef] = &[
    SettingDef {
        key: "own_source",
        label_key: "umicraft.mods.capes.own_source",
        kind: SettingKind::Choice {
            options: OWN_SOURCES,
            default: 0,
        },
    },
    SettingDef {
        key: "optifine",
        label_key: "umicraft.mods.capes.optifine",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "labymod",
        label_key: "umicraft.mods.capes.labymod",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "minecraftcapes",
        label_key: "umicraft.mods.capes.minecraftcapes",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "cosmetica",
        label_key: "umicraft.mods.capes.cosmetica",
        kind: SettingKind::Toggle { default: true },
    },
];

/// The capes mod.
pub(super) struct Capes;

impl BuiltinMod for Capes {
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
    fn it_is_a_visual_mod_that_starts_on_without_a_widget() {
        let m = Capes;
        assert_eq!(m.meta().category, ModCategory::Visual);
        assert!(m.meta().default_enabled);
        assert!(m.hud().is_none(), "it draws no widget");
    }

    #[test]
    fn it_keeps_the_mojang_cape_and_asks_every_service_by_default() {
        let settings = ModSettings::defaults(Capes.settings());
        assert_eq!(settings.text("own_source"), "minecraft");
        for key in ["optifine", "labymod", "minecraftcapes", "cosmetica"] {
            assert!(settings.bool(key), "{key}");
        }
    }

    #[test]
    fn the_own_source_options_come_in_the_documented_order() {
        assert_eq!(
            OWN_SOURCES,
            [
                "minecraft",
                "optifine",
                "labymod",
                "minecraftcapes",
                "cosmetica"
            ]
        );
        assert!(matches!(
            Capes.settings()[0].kind,
            SettingKind::Choice { default: 0, .. }
        ));
    }
}
