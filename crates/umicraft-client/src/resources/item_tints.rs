// SPDX-License-Identifier: MIT OR Apache-2.0

//! The tint of each layer of an item model (P3).
//!
//! A generated item model has up to five layers and each quad carries its
//! layer as tint index; the colour comes from `Item.getColorFromItemStack`
//! (leather armour, potions, spawn eggs, firework charges) or from
//! `Block.getRenderColor` (grass and foliage block items). Everything that
//! depends only on the item id and damage is a pure table in
//! `umicraft_game::items::tint`; leather and firework charges read a colour
//! from the NBT, which the icon key carries as `nbt_color`.

use umicraft_assets::BiomeColormaps;
use umicraft_game::item_nbt::ItemDisplay;
use umicraft_game::items::tint::{firework_charge_tint, layer_tints as table_tints};
use umicraft_game::items::{self, ArmorMaterial, ItemKind};
use umicraft_mesh::tint::WHITE;

/// The colour of undyed leather armour (MCP `ItemArmor.getColor`).
const DEFAULT_LEATHER_COLOR: u32 = 0x00A0_6540;
/// Foliage colours of the leaf, vine and lily pad items (MCP
/// `ColorizerFoliage.getFoliageColorBasic` / `Pine` / `Birch`,
/// `BlockLilyPad.getRenderColor`).
const FOLIAGE_BASIC: u32 = 4_764_952;
const FOLIAGE_PINE: u32 = 6_396_257;
const FOLIAGE_BIRCH: u32 = 8_431_445;
const LILY_PAD_COLOR: u32 = 7_455_580;

/// Climate sample of the grass colour of an item (MCP
/// `ColorizerGrass.getGrassColor(0.5, 1.0)`).
const GRASS_TEMPERATURE: f32 = 0.5;
const GRASS_RAINFALL: f32 = 1.0;

const GRASS: i16 = 2;
const LEAVES: i16 = 18;
const TALL_GRASS: i16 = 31;
const VINE: i16 = 106;
const LILY_PAD: i16 = 111;
const LEAVES_2: i16 = 161;
const DOUBLE_PLANT: i16 = 175;
const FIREWORK_CHARGE: i16 = 402;

/// The grass colour of the grass block items, sampled from the colormap.
#[must_use]
pub fn item_grass_color(colormaps: &BiomeColormaps) -> u32 {
    let [r, g, b] = colormaps.grass.sample(GRASS_TEMPERATURE, GRASS_RAINFALL);
    u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
}

/// The colour an item reads from its NBT, the part of its appearance the
/// icon key cannot derive from the id and the damage: the dye of leather
/// armour (undyed leather has a default) and the colour of a firework
/// charge.
#[must_use]
pub fn nbt_color(item_id: i16, nbt: Option<&[u8]>) -> Option<u32> {
    if item_id == FIREWORK_CHARGE {
        return firework_charge_tint(nbt);
    }
    let leather = matches!(
        items::lookup(i32::from(item_id)).map(|def| def.kind),
        Some(ItemKind::Armor(ArmorMaterial::Leather, _))
    );
    leather.then(|| {
        nbt.and_then(|bytes| ItemDisplay::from_nbt(bytes).leather_color)
            .unwrap_or(DEFAULT_LEATHER_COLOR)
    })
}

/// The tints of layers 0 and 1, white where the item has none.
///
/// `color` is [`nbt_color`] of the stack and `grass` the grass colour of the
/// block items (`None` without the colormap).
#[must_use]
pub fn layer_tints(item_id: i16, damage: i16, color: Option<u32>, grass: Option<u32>) -> [u32; 2] {
    if item_id == FIREWORK_CHARGE {
        // `ItemFireworkCharge`: only the overlay layer is coloured.
        return [WHITE, color.unwrap_or(WHITE)];
    }
    if let Some(color) = color {
        // Leather: the base layer is dyed, the overlay is not.
        return [color, WHITE];
    }
    if let Some(tints) = table_tints(item_id, damage) {
        return tints;
    }
    [
        block_item_tint(item_id, damage, grass).unwrap_or(WHITE),
        WHITE,
    ]
}

/// The tint of the block items that the reference colours
/// (`Block.getRenderColor` through `ItemColored` and `ItemLeaves`).
fn block_item_tint(item_id: i16, damage: i16, grass: Option<u32>) -> Option<u32> {
    match item_id {
        GRASS => grass,
        TALL_GRASS if damage != 0 => grass,
        DOUBLE_PLANT if matches!(damage & 7, 2 | 3) => grass,
        LEAVES => Some(match damage & 3 {
            1 => FOLIAGE_PINE,
            2 => FOLIAGE_BIRCH,
            _ => FOLIAGE_BASIC,
        }),
        LEAVES_2 | VINE => Some(FOLIAGE_BASIC),
        LILY_PAD => Some(LILY_PAD_COLOR),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foliage_and_grass_items_are_tinted_like_the_reference() {
        let grass = Some(0x0091_BD59);
        assert_eq!(block_item_tint(2, 0, grass), grass);
        assert_eq!(block_item_tint(31, 0, grass), None, "dead bush stays white");
        assert_eq!(block_item_tint(31, 1, grass), grass);
        assert_eq!(block_item_tint(175, 2, grass), grass);
        assert_eq!(
            block_item_tint(175, 0, grass),
            None,
            "sunflower is untinted"
        );
        assert_eq!(block_item_tint(18, 1, None), Some(FOLIAGE_PINE));
        assert_eq!(block_item_tint(18, 2, None), Some(FOLIAGE_BIRCH));
        assert_eq!(block_item_tint(18, 0, None), Some(FOLIAGE_BASIC));
        assert_eq!(block_item_tint(111, 0, None), Some(LILY_PAD_COLOR));
        assert_eq!(block_item_tint(1, 0, grass), None);
    }

    #[test]
    fn undyed_leather_uses_the_default_colour_and_other_items_none() {
        assert_eq!(nbt_color(298, None), Some(DEFAULT_LEATHER_COLOR));
        assert_eq!(nbt_color(1, None), None);
        assert_eq!(
            layer_tints(298, 0, Some(0x12_3456), None),
            [0x12_3456, WHITE]
        );
    }

    #[test]
    fn firework_charges_tint_the_overlay_layer_only() {
        assert_eq!(nbt_color(402, None), Some(0x008A_8A8A));
        assert_eq!(
            layer_tints(402, 0, Some(0x00AB_CDEF), None),
            [WHITE, 0x00AB_CDEF]
        );
    }

    #[test]
    fn potions_and_eggs_come_from_the_tables_and_plain_items_are_white() {
        let water = layer_tints(373, 0, None, None);
        assert_eq!(water, [0x0038_5DC6, 0x00FF_FFFF]);
        assert_eq!(layer_tints(276, 0, None, None), [WHITE, WHITE]);
        assert_eq!(
            layer_tints(2, 0, None, Some(0x0091_BD59)),
            [0x0091_BD59, WHITE]
        );
    }
}
