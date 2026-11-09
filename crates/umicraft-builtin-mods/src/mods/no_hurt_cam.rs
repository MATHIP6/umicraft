// SPDX-License-Identifier: MIT OR Apache-2.0
//! No Hurt Cam: the view no longer tilts when the player takes damage.
//!
//! The mod holds only its switch. The client reads it each frame
//! (`ModsRuntime::no_hurt_cam`) and leaves the hurt roll out of the camera.

use crate::id::{ModCategory, ModId};
use crate::meta::ModMeta;
use crate::traits::BuiltinMod;

static META: ModMeta = ModMeta {
    id: ModId::new("no_hurt_cam"),
    name_key: "umicraft.mods.no_hurt_cam.name",
    description_key: "umicraft.mods.no_hurt_cam.desc",
    category: ModCategory::Visual,
    default_enabled: false,
};

/// The no hurt cam mod.
pub(super) struct NoHurtCam;

impl BuiltinMod for NoHurtCam {
    fn meta(&self) -> &'static ModMeta {
        &META
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_a_visual_mod_that_starts_off_and_has_no_settings() {
        assert_eq!(NoHurtCam.meta().category, ModCategory::Visual);
        assert!(!NoHurtCam.meta().default_enabled);
        assert!(NoHurtCam.settings().is_empty());
        assert!(NoHurtCam.hud().is_none());
    }
}
