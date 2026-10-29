// SPDX-License-Identifier: MIT OR Apache-2.0

//! The items whose model is chosen by code rather than by the name table.
//!
//! These are the five `ItemMeshDefinition` registrations of MCP
//! `RenderItem.registerItems` (potion, spawn egg, banner, enchanted book,
//! filled map), plus the in-use models the renderer swaps in for the bow and
//! the fishing rod (`RenderItem.renderItemModelTransform` paths).

/// The `ItemPotion.isSplash` bit of a potion's metadata.
const SPLASH_BIT: u16 = 16384;

const POTION: u16 = 373;
const SPAWN_EGG: u16 = 383;
const FILLED_MAP: u16 = 358;
const ENCHANTED_BOOK: u16 = 403;
const BANNER: u16 = 425;

/// Every model name [`special_model_name`] can return, so the set loads them.
pub(super) const SPECIAL_MODEL_NAMES: [&str; 6] = [
    "bottle_splash",
    "bottle_drinkable",
    "spawn_egg",
    "banner",
    "enchanted_book",
    "filled_map",
];

/// The model of an item chosen by its mesh definition, `None` for items that
/// use the name table.
#[must_use]
pub fn special_model_name(item_id: u16, damage: u16) -> Option<&'static str> {
    match item_id {
        POTION if damage & SPLASH_BIT != 0 => Some("bottle_splash"),
        POTION => Some("bottle_drinkable"),
        SPAWN_EGG => Some("spawn_egg"),
        BANNER => Some("banner"),
        ENCHANTED_BOOK => Some("enchanted_book"),
        FILLED_MAP => Some("filled_map"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn potions_split_on_the_splash_bit() {
        assert_eq!(special_model_name(373, 0), Some("bottle_drinkable"));
        assert_eq!(special_model_name(373, 16384 | 5), Some("bottle_splash"));
        assert_eq!(special_model_name(1, 0), None);
    }

    #[test]
    fn every_special_name_is_listed() {
        for id in [373, 383, 425, 403, 358] {
            let name = special_model_name(id, 0).expect("special");
            assert!(SPECIAL_MODEL_NAMES.contains(&name));
        }
        assert!(SPECIAL_MODEL_NAMES.contains(&"bottle_splash"));
    }
}
