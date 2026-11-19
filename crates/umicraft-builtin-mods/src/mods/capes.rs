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

/// The sources of the local player's cape. `all` asks every enabled service
/// in the order used for the other players; `minecraft` keeps the Mojang cape,
/// so no service is asked; the others are the keys of the provider toggles.
/// The services come in lookup order (OptiFine last, ADR-0081).
const OWN_SOURCES: &[&str] = &[
    "all",
    "labymod",
    "minecraftcapes",
    "cosmetica",
    "optifine",
    "minecraft",
];

/// One toggle per service, in lookup order.
const fn provider_toggle(key: &'static str, label_key: &'static str) -> SettingDef {
    SettingDef {
        key,
        label_key,
        kind: SettingKind::Toggle { default: true },
    }
}

const SETTINGS: &[SettingDef] = &[
    SettingDef {
        key: "own_source",
        label_key: "umicraft.mods.capes.own_source",
        kind: SettingKind::Choice {
            options: OWN_SOURCES,
            default: 0,
        },
    },
    provider_toggle("labymod", "umicraft.mods.capes.labymod"),
    provider_toggle("minecraftcapes", "umicraft.mods.capes.minecraftcapes"),
    provider_toggle("cosmetica", "umicraft.mods.capes.cosmetica"),
    provider_toggle("optifine", "umicraft.mods.capes.optifine"),
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
    fn it_asks_every_service_for_everybody_by_default() {
        let settings = ModSettings::defaults(Capes.settings());
        assert_eq!(settings.text("own_source"), "all");
        for key in ["optifine", "labymod", "minecraftcapes", "cosmetica"] {
            assert!(settings.bool(key), "{key}");
        }
    }

    #[test]
    fn the_own_source_options_come_in_the_documented_order() {
        assert_eq!(
            OWN_SOURCES,
            [
                "all",
                "labymod",
                "minecraftcapes",
                "cosmetica",
                "optifine",
                "minecraft"
            ]
        );
        assert!(matches!(
            Capes.settings()[0].kind,
            SettingKind::Choice { default: 0, .. }
        ));
    }
}
