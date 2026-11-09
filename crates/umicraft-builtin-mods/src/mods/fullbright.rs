// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fullbright: the world is lit at full strength, so caves and nights are as
//! readable as a sunny day.
//!
//! The mod holds nothing but its switch. The renderer lights the world
//! (`umicraft-render`, `Renderer::set_fullbright`), which the client tells each
//! frame whether the mod is on.

use crate::id::{ModCategory, ModId};
use crate::meta::ModMeta;
use crate::traits::BuiltinMod;

static META: ModMeta = ModMeta {
    id: ModId::new("fullbright"),
    name_key: "umicraft.mods.fullbright.name",
    description_key: "umicraft.mods.fullbright.desc",
    category: ModCategory::Visual,
    default_enabled: false,
};

/// The fullbright mod.
pub(super) struct Fullbright;

impl BuiltinMod for Fullbright {
    fn meta(&self) -> &'static ModMeta {
        &META
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_a_visual_mod_that_starts_off_and_has_no_settings() {
        let m = Fullbright;
        assert_eq!(m.meta().category, ModCategory::Visual);
        assert!(!m.meta().default_enabled, "it changes the look of the game");
        assert!(m.settings().is_empty());
        assert!(m.hud().is_none(), "it draws no widget");
    }
}
