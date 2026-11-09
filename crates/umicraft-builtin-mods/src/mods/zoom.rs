// SPDX-License-Identifier: MIT OR Apache-2.0
//! Zoom: hold a key (C by default) to magnify the view, with a glide in and out.
//!
//! The mod holds its settings only. The window reads them each frame
//! (`ModsRuntime::zoom`) and moves the field of view of the camera.

use crate::id::{ModCategory, ModId};
use crate::meta::ModMeta;
use crate::settings::{SettingDef, SettingKind};
use crate::traits::BuiltinMod;

static META: ModMeta = ModMeta {
    id: ModId::new("zoom"),
    name_key: "umicraft.mods.zoom.name",
    description_key: "umicraft.mods.zoom.desc",
    category: ModCategory::Visual,
    default_enabled: false,
};

const SETTINGS: &[SettingDef] = &[
    SettingDef {
        key: "factor",
        label_key: "umicraft.mods.zoom.factor",
        kind: SettingKind::Slider {
            min: 1.5,
            max: 10.0,
            step: 0.5,
            default: 4.0,
        },
    },
    SettingDef {
        key: "smooth",
        label_key: "umicraft.mods.zoom.smooth",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "scroll",
        label_key: "umicraft.mods.zoom.scroll",
        kind: SettingKind::Toggle { default: true },
    },
    SettingDef {
        key: "slow_mouse",
        label_key: "umicraft.mods.zoom.slow_mouse",
        kind: SettingKind::Toggle { default: true },
    },
];

/// The zoom mod.
pub(super) struct Zoom;

impl BuiltinMod for Zoom {
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
    use crate::settings::{ModSettings, SettingValue};

    #[test]
    fn it_is_a_visual_mod_that_starts_off_with_a_smooth_fourfold_zoom() {
        assert_eq!(Zoom.meta().category, ModCategory::Visual);
        assert!(!Zoom.meta().default_enabled);
        assert!(Zoom.hud().is_none(), "it draws no widget");
        let settings = ModSettings::defaults(Zoom.settings());
        assert_eq!(settings.number("factor"), 4.0);
        assert!(settings.bool("smooth") && settings.bool("scroll") && settings.bool("slow_mouse"));
    }

    #[test]
    fn a_hand_written_factor_is_repaired() {
        let mut settings = ModSettings::defaults(Zoom.settings());
        settings
            .set("factor", &SettingValue::Number(500.0))
            .expect("the key exists");
        assert_eq!(settings.number("factor"), 10.0, "clamped");
    }
}
