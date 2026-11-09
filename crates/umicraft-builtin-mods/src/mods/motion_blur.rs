// SPDX-License-Identifier: MIT OR Apache-2.0
//! Motion blur: the picture smears along the way the camera turns or moves.
//!
//! The mod holds only its settings. The picture is blurred by the renderer
//! (`umicraft-render`, `motion_blur`), which the client tells each frame what the
//! settings are. The three versions are `v1`, the classic trail of the previous
//! pictures, `v2`, a blur along the motion of each pixel, and `v3`, the same with
//! an adaptive number of taps that respects edges.

use crate::id::{ModCategory, ModId};
use crate::meta::ModMeta;
use crate::settings::{SettingDef, SettingKind};
use crate::traits::BuiltinMod;

static META: ModMeta = ModMeta {
    id: ModId::new("motion_blur"),
    name_key: "umicraft.mods.motion_blur.name",
    description_key: "umicraft.mods.motion_blur.desc",
    category: ModCategory::Visual,
    default_enabled: false,
};

const SETTINGS: &[SettingDef] = &[
    SettingDef {
        key: "version",
        label_key: "umicraft.mods.motion_blur.version",
        kind: SettingKind::Choice {
            options: &["v1", "v2", "v3"],
            default: 2,
        },
    },
    SettingDef {
        key: "strength",
        label_key: "umicraft.mods.motion_blur.strength",
        kind: SettingKind::Slider {
            min: 0.1,
            max: 1.0,
            step: 0.05,
            default: 0.5,
        },
    },
];

/// The motion blur mod.
pub(super) struct MotionBlur;

impl BuiltinMod for MotionBlur {
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
    fn it_is_a_visual_mod_that_starts_off() {
        let m = MotionBlur;
        assert_eq!(m.meta().category, ModCategory::Visual);
        assert!(
            !m.meta().default_enabled,
            "a blur is for the player to choose"
        );
        assert!(m.hud().is_none(), "it draws no widget");
    }

    #[test]
    fn it_starts_on_the_best_version_at_half_strength() {
        let settings = ModSettings::defaults(MotionBlur.settings());
        assert_eq!(settings.text("version"), "v3");
        assert_eq!(settings.number("strength"), 0.5);
    }

    #[test]
    fn a_hand_written_version_or_strength_is_repaired() {
        let mut settings = ModSettings::defaults(MotionBlur.settings());
        settings
            .set("version", &SettingValue::Text("v9".into()))
            .expect("the key exists");
        assert_eq!(settings.text("version"), "v3", "an unknown version");
        settings
            .set("strength", &SettingValue::Number(40.0))
            .expect("the key exists");
        assert_eq!(settings.number("strength"), 1.0, "clamped");
        settings
            .set("strength", &SettingValue::Number(f32::NAN))
            .expect("the key exists");
        assert_eq!(settings.number("strength"), 0.5, "not a number");
    }
}
