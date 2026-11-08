// SPDX-License-Identifier: MIT OR Apache-2.0

//! Unlocalised item names (MCP `Item.getUnlocalizedName(ItemStack)`).
//!
//! The 1.8 language file is keyed by these names plus `.name`:
//! `item.swordDiamond.name`, `tile.cloth.red.name`, `tile.stone.granite.name`.
//! A name is the item's base name (generated into `name_table.rs`) and, for an
//! item with subtypes, a suffix chosen by the damage value. The suffix tables
//! below copy the `getUnlocalizedName()` of the block and item enums of the
//! reference (`BlockStone.EnumType`, `EnumDyeColor`, ...); an out-of-range
//! damage falls back the way each reference class does.

use super::name_table::{BASE, ENTITIES};
use super::ItemDef;

/// `BlockStone.EnumType`.
const STONE: &[&str] = &[
    "stone",
    "granite",
    "graniteSmooth",
    "diorite",
    "dioriteSmooth",
    "andesite",
    "andesiteSmooth",
];
/// `BlockDirt.DirtType`.
const DIRT: &[&str] = &["default", "coarse", "podzol"];
/// `BlockPlanks.EnumType`, which sapling, log and leaves share.
const WOOD: &[&str] = &["oak", "spruce", "birch", "jungle", "acacia", "big_oak"];
/// `BlockSand.EnumType`.
const SAND: &[&str] = &["default", "red"];
/// `ItemMultiTexture` of the sponge: bit 0 selects wet.
const SPONGE: &[&str] = &["dry", "wet"];
/// `BlockSandStone.EnumType` and `BlockRedSandstone.EnumType`.
const SANDSTONE: &[&str] = &["default", "chiseled", "smooth"];
/// `BlockSilverfish.EnumType`.
const SILVERFISH: &[&str] = &[
    "stone",
    "cobble",
    "brick",
    "mossybrick",
    "crackedbrick",
    "chiseledbrick",
];
/// `BlockStoneBrick.EnumType`.
const STONE_BRICK: &[&str] = &["default", "mossy", "cracked", "chiseled"];
/// `BlockWall.EnumType`.
const WALL: &[&str] = &["normal", "mossy"];
/// The names array of the quartz block item.
const QUARTZ: &[&str] = &["default", "chiseled", "lines"];
/// The names array of the anvil item.
const ANVIL: &[&str] = &["intact", "slightlyDamaged", "veryDamaged"];
/// `BlockPrismarine.EnumType`.
const PRISMARINE: &[&str] = &["rough", "bricks", "dark"];
/// `ItemColored.setSubtypeNames` of the tall grass.
const TALL_GRASS: &[&str] = &["shrub", "grass", "fern"];
/// `BlockFlower.EnumFlowerType` of the red flower block.
const RED_FLOWER: &[&str] = &[
    "poppy",
    "blueOrchid",
    "allium",
    "houstonia",
    "tulipRed",
    "tulipOrange",
    "tulipWhite",
    "tulipPink",
    "oxeyeDaisy",
];
/// `BlockDoublePlant.EnumPlantType`.
const DOUBLE_PLANT: &[&str] = &["sunflower", "syringa", "grass", "fern", "rose", "paeonia"];
/// `BlockStoneSlab.EnumType`.
const STONE_SLAB: &[&str] = &[
    "stone",
    "sand",
    "wood",
    "cobble",
    "brick",
    "smoothStoneBrick",
    "netherBrick",
    "quartz",
];
/// `BlockStoneSlabNew.EnumType`.
const STONE_SLAB_NEW: &[&str] = &["red_sandstone"];
/// `ItemFishFood.FishType`.
const FISH: &[&str] = &["cod", "salmon", "clownfish", "pufferfish"];
/// `ItemSkull.skullTypes`.
const SKULL: &[&str] = &["skeleton", "wither", "zombie", "char", "creeper"];
/// `EnumDyeColor.getUnlocalizedName`, indexed by block metadata (wool order).
const COLOURS: &[&str] = &[
    "white",
    "orange",
    "magenta",
    "lightBlue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "silver",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

/// The suffix at `index`, or at 0 when the damage is outside the table
/// (`EnumType.byMetadata` and `ItemMultiTexture` both fall back to the first).
fn entry(names: &[&'static str], index: i16) -> &'static str {
    usize::try_from(index)
        .ok()
        .and_then(|i| names.get(i))
        .unwrap_or(&names[0])
}

/// The dye colour of a dye or banner damage value (`EnumDyeColor.byDyeDamage`):
/// dye damage counts from black, block metadata from white.
fn dye_colour(damage: i16) -> &'static str {
    let damage = usize::try_from(damage)
        .ok()
        .filter(|d| *d < COLOURS.len())
        .unwrap_or(0);
    COLOURS[COLOURS.len() - 1 - damage]
}

/// The unlocalised name of the base item, without any subtype.
fn base_name(def: &ItemDef) -> String {
    match BASE.binary_search_by_key(&def.id, |(id, _)| *id) {
        Ok(index) => BASE[index].1.to_owned(),
        Err(_) => format!("item.{}", def.name),
    }
}

/// The subtype suffix of the item at `damage`, `None` when the item has no
/// subtype names or the reference leaves the base name for that damage.
fn suffix(def: &ItemDef, damage: i16) -> Option<String> {
    let name = match def.name {
        "stone" => entry(STONE, damage),
        "dirt" => entry(DIRT, damage),
        "planks" | "sapling" | "log" | "leaves" => {
            let wood = if matches!(def.name, "log" | "leaves") {
                damage & 3
            } else {
                damage
            };
            entry(WOOD, wood)
        }
        "log2" | "leaves2" => entry(WOOD, 4 + (damage & 3)),
        "wooden_slab" => entry(WOOD, damage),
        "sand" => entry(SAND, damage),
        "sponge" => entry(SPONGE, damage & 1),
        "sandstone" | "red_sandstone" => entry(SANDSTONE, damage),
        "monster_egg" => entry(SILVERFISH, damage),
        "stonebrick" => entry(STONE_BRICK, damage),
        "cobblestone_wall" => entry(WALL, damage),
        "quartz_block" => entry(QUARTZ, damage),
        "anvil" => entry(ANVIL, damage),
        "prismarine" => entry(PRISMARINE, damage),
        "wool" | "carpet" | "stained_hardened_clay" | "stained_glass" | "stained_glass_pane" => {
            entry(COLOURS, damage)
        }
        "tallgrass" => {
            return TALL_GRASS
                .get(usize::try_from(damage).ok()?)
                .map(|s| (*s).to_owned())
        }
        "yellow_flower" => "dandelion",
        "red_flower" => entry(RED_FLOWER, damage),
        "double_plant" => entry(DOUBLE_PLANT, damage),
        "stone_slab" => entry(STONE_SLAB, damage),
        "stone_slab2" => entry(STONE_SLAB_NEW, damage),
        "dye" | "banner" => dye_colour(damage),
        "skull" => entry(SKULL, damage),
        "fish" => return Some(format!("{}.raw", entry(FISH, damage))),
        "cooked_fish" => {
            let kind = entry(FISH, damage);
            let cooked = matches!(kind, "cod" | "salmon");
            return Some(format!("{kind}.{}", if cooked { "cooked" } else { "raw" }));
        }
        _ => return None,
    };
    Some(name.to_owned())
}

/// The unlocalised name of a stack of `def` with `damage`, such as
/// `tile.cloth.red` or `item.swordDiamond`; append `.name` for the language
/// key of its display name.
///
/// Coal is the one item whose subtype replaces the whole name
/// (`item.charcoal`), as in `ItemCoal`; an empty potion is `item.emptyPotion`.
///
/// ```
/// use umicraft_game::items::{lookup, unlocalized_name};
///
/// let wool = lookup(35).expect("wool");
/// assert_eq!(unlocalized_name(wool, 14), "tile.cloth.red");
/// ```
pub fn unlocalized_name(def: &ItemDef, damage: i16) -> String {
    match def.name {
        "coal" if damage == 1 => return "item.charcoal".to_owned(),
        "potion" if damage == 0 => return "item.emptyPotion".to_owned(),
        _ => {}
    }
    let base = base_name(def);
    match suffix(def, damage) {
        Some(suffix) => format!("{base}.{suffix}"),
        None => base,
    }
}

/// The unlocalised name of the entity a spawn egg of `damage` spawns, such as
/// `Creeper` (`EntityList.getStringFromID`), `None` for an unknown id.
pub fn spawn_egg_entity(damage: i16) -> Option<&'static str> {
    let id = u8::try_from(damage).ok()?;
    let index = ENTITIES.binary_search_by_key(&id, |(id, _)| *id).ok()?;
    Some(ENTITIES[index].1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::{all, lookup};

    fn name(id: i32, damage: i16) -> String {
        unlocalized_name(lookup(id).expect("registered item"), damage)
    }

    #[test]
    fn every_item_has_a_generated_base_name() {
        for def in all() {
            assert!(
                BASE.binary_search_by_key(&def.id, |(id, _)| *id).is_ok(),
                "{}",
                def.name
            );
        }
    }

    #[test]
    fn plain_items_use_their_legacy_name() {
        assert_eq!(name(276, 0), "item.swordDiamond");
        assert_eq!(name(14, 0), "tile.oreGold");
    }

    #[test]
    fn block_subtypes_follow_the_metadata() {
        assert_eq!(name(1, 1), "tile.stone.granite");
        assert_eq!(name(1, 6), "tile.stone.andesiteSmooth");
        assert_eq!(name(35, 0), "tile.cloth.white");
        assert_eq!(name(35, 3), "tile.cloth.lightBlue");
        assert_eq!(name(5, 5), "tile.wood.big_oak");
        assert_eq!(name(162, 1), "tile.log.big_oak");
        assert_eq!(name(18, 5), "tile.leaves.spruce");
        assert_eq!(name(44, 7), "tile.stoneSlab.quartz");
        assert_eq!(name(145, 1), "tile.anvil.slightlyDamaged");
    }

    #[test]
    fn an_out_of_range_damage_falls_back_like_the_reference() {
        assert_eq!(name(1, 99), "tile.stone.stone");
        assert_eq!(name(35, -1), "tile.cloth.white");
        assert_eq!(name(155, 4), "tile.quartzBlock.default");
        assert_eq!(name(31, 7), "tile.tallgrass");
    }

    #[test]
    fn dye_counts_from_black_and_blocks_from_white() {
        assert_eq!(name(351, 0), "item.dyePowder.black");
        assert_eq!(name(351, 15), "item.dyePowder.white");
        assert_eq!(name(351, 4), "item.dyePowder.blue");
        assert_eq!(name(425, 1), "item.banner.red");
    }

    #[test]
    fn items_with_a_subtype_name_of_their_own() {
        assert_eq!(name(263, 1), "item.charcoal");
        assert_eq!(name(373, 0), "item.emptyPotion");
        assert_eq!(name(349, 3), "item.fish.pufferfish.raw");
        assert_eq!(name(350, 1), "item.fish.salmon.cooked");
        assert_eq!(name(397, 1), "item.skull.wither");
    }

    #[test]
    fn spawn_eggs_know_their_entity() {
        assert_eq!(spawn_egg_entity(50), Some("Creeper"));
        assert_eq!(spawn_egg_entity(0), None);
        assert_eq!(spawn_egg_entity(-3), None);
    }
}
