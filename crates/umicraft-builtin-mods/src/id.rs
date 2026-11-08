// SPDX-License-Identifier: MIT OR Apache-2.0
//! Identity of a mod and the category it is listed under.

use std::fmt;

/// The stable identifier of a built-in mod.
///
/// It is the key of the mod in `options.toml` (`[mods.<id>]`), so it must never
/// change once released.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModId(&'static str);

impl ModId {
    /// Wraps an identifier. Use lowercase `snake_case`, e.g. `"toggle_sprint"`.
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    /// The identifier as written in the config file.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for ModId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// The group a mod is listed under in the mods menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModCategory {
    /// Information drawn on the HUD.
    Hud,
    /// Changes how the player controls or plays.
    Gameplay,
    /// Changes how the game looks.
    Visual,
}

impl ModCategory {
    /// Every category, in the order the menu lists them.
    pub const ALL: [Self; 3] = [Self::Hud, Self::Gameplay, Self::Visual];

    /// The language key of the category name.
    #[must_use]
    pub const fn lang_key(self) -> &'static str {
        match self {
            Self::Hud => "umicraft.mods.category.hud",
            Self::Gameplay => "umicraft.mods.category.gameplay",
            Self::Visual => "umicraft.mods.category.visual",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_displays_as_its_text() {
        assert_eq!(ModId::new("fps").to_string(), "fps");
    }

    #[test]
    fn every_category_has_a_distinct_key() {
        let keys: Vec<_> = ModCategory::ALL.iter().map(|c| c.lang_key()).collect();
        assert!(keys
            .iter()
            .all(|k| k.starts_with("umicraft.mods.category.")));
        assert_eq!(keys.len(), 3);
        assert!(keys[0] != keys[1] && keys[1] != keys[2]);
    }
}
