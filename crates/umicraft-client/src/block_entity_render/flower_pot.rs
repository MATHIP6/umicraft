// SPDX-License-Identifier: MIT OR Apache-2.0

//! The plant in a flower pot (MCP `BlockFlowerPot`, `TileEntityFlowerPot`).
//!
//! The pot is a block model and stays in the chunk mesh as the empty pot. The
//! server tells what it holds with an Update Block Entity (type 5) whose NBT
//! has `Item` (an item name, or the numeric id in the packet) and `Data`.
//! `BlockFlowerPot.getActualState` maps them to the `contents` blockstate
//! property; [`contents_name`] is that map. The plant itself is appended to
//! the dynamic terrain mesh by [`push_pot_plants`], see
//! `umicraft_mesh::push_flower_pot_plant`.

use umicraft_game::nbt::Nbt;
use umicraft_game::server::block_entities::EntityTable;
use umicraft_mesh::{
    push_flower_pot_plant, BlockEntityKind, BlockEntityRef, Mesh, ModelMeshContext,
};

/// The `Update Block Entity` action of a flower pot.
const FLOWER_POT_ACTION: u8 = 5;

/// Item (block) ids that can sit in a pot.
const SAPLING: i32 = 6;
const TALL_GRASS: i32 = 31;
const DEAD_BUSH: i32 = 32;
const YELLOW_FLOWER: i32 = 37;
const RED_FLOWER: i32 = 38;
const BROWN_MUSHROOM: i32 = 39;
const RED_MUSHROOM: i32 = 40;
const CACTUS: i32 = 81;

/// The `contents` property for the stack `item`/`data` (`getActualState`),
/// `None` for the empty pot and for anything a pot cannot show.
///
/// A red flower out of range is a poppy and a sapling out of range an oak
/// (`EnumFlowerType.getType`, `EnumType.byMetadata`).
#[must_use]
pub(super) fn contents_name(item: i32, data: i32) -> Option<&'static str> {
    const FLOWERS: [&str; 9] = [
        "rose",
        "blue_orchid",
        "allium",
        "houstonia",
        "red_tulip",
        "orange_tulip",
        "white_tulip",
        "pink_tulip",
        "oxeye_daisy",
    ];
    const SAPLINGS: [&str; 6] = [
        "oak_sapling",
        "spruce_sapling",
        "birch_sapling",
        "jungle_sapling",
        "acacia_sapling",
        "dark_oak_sapling",
    ];
    let index = |len: usize| usize::try_from(data).ok().filter(|&i| i < len).unwrap_or(0);
    match item {
        SAPLING => Some(SAPLINGS[index(SAPLINGS.len())]),
        TALL_GRASS => match data {
            0 => Some("dead_bush"),
            2 => Some("fern"),
            _ => None,
        },
        YELLOW_FLOWER => Some("dandelion"),
        RED_FLOWER => Some(FLOWERS[index(FLOWERS.len())]),
        RED_MUSHROOM => Some("mushroom_red"),
        BROWN_MUSHROOM => Some("mushroom_brown"),
        DEAD_BUSH => Some("dead_bush"),
        CACTUS => Some("cactus"),
        _ => None,
    }
}

/// The id of an item named by `Item.getByNameOrId`: a registry name with or
/// without the `minecraft:` prefix, or a number as text. Only the blocks a
/// pot can hold are known.
fn item_id_by_name(name: &str) -> Option<i32> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    match name {
        "sapling" => Some(SAPLING),
        "tallgrass" => Some(TALL_GRASS),
        "deadbush" => Some(DEAD_BUSH),
        "yellow_flower" => Some(YELLOW_FLOWER),
        "red_flower" => Some(RED_FLOWER),
        "brown_mushroom" => Some(BROWN_MUSHROOM),
        "red_mushroom" => Some(RED_MUSHROOM),
        "cactus" => Some(CACTUS),
        number => number.parse().ok(),
    }
}

/// What the NBT of a pot holds (`TileEntityFlowerPot.readFromNBT`: a string
/// `Item` is a name, anything else is read as the numeric id). `None` for
/// malformed NBT, an empty pot or an unknown item.
fn pot_contents(nbt: &[u8]) -> Option<&'static str> {
    let root = Nbt::parse(nbt).ok()?;
    let item = match root.get("Item")? {
        Nbt::String(name) => item_id_by_name(name)?,
        other => other.as_int()?,
    };
    let data = root.get("Data").and_then(Nbt::as_int).unwrap_or(0);
    contents_name(item, data)
}

/// Appends the plant of every flower pot of `entries` that `data` knows.
pub(crate) fn push_pot_plants<'a>(
    mesh: &mut Mesh,
    entries: impl Iterator<Item = &'a BlockEntityRef>,
    data: &EntityTable,
    context: &ModelMeshContext,
) {
    for entry in
        entries.filter(|e| BlockEntityKind::of(e.state) == Some(BlockEntityKind::FlowerPot))
    {
        let [x, y, z] = entry.position;
        let Some(update) = data
            .get(&(x, y, z))
            .filter(|u| u.action == FLOWER_POT_ACTION)
        else {
            continue;
        };
        if let Some(contents) = update.nbt.as_deref().and_then(pot_contents) {
            push_flower_pot_plant(mesh, contents, entry.position, entry.light, context);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A compound with an optional string/int `Item` and an int `Data`.
    fn nbt(item: Option<Nbt>, data: i32) -> Vec<u8> {
        let mut out = vec![10, 0, 0];
        match item {
            Some(Nbt::String(name)) => {
                out.extend([8, 0, 4]);
                out.extend(b"Item");
                out.extend((name.len() as u16).to_be_bytes());
                out.extend(name.as_bytes());
            }
            Some(Nbt::Int(id)) => {
                out.extend([3, 0, 4]);
                out.extend(b"Item");
                out.extend(id.to_be_bytes());
            }
            _ => {}
        }
        out.extend([3, 0, 4]);
        out.extend(b"Data");
        out.extend(data.to_be_bytes());
        out.push(0);
        out
    }

    #[test]
    fn red_flowers_follow_their_data_and_default_to_the_poppy() {
        assert_eq!(contents_name(RED_FLOWER, 0), Some("rose"));
        assert_eq!(contents_name(RED_FLOWER, 8), Some("oxeye_daisy"));
        assert_eq!(contents_name(RED_FLOWER, 9), Some("rose"));
        assert_eq!(contents_name(RED_FLOWER, -1), Some("rose"));
    }

    #[test]
    fn saplings_grass_and_the_single_plants_map_to_their_contents() {
        assert_eq!(contents_name(SAPLING, 5), Some("dark_oak_sapling"));
        assert_eq!(contents_name(SAPLING, 12), Some("oak_sapling"));
        assert_eq!(contents_name(TALL_GRASS, 2), Some("fern"));
        assert_eq!(contents_name(TALL_GRASS, 0), Some("dead_bush"));
        assert_eq!(contents_name(TALL_GRASS, 1), None);
        assert_eq!(contents_name(YELLOW_FLOWER, 0), Some("dandelion"));
        assert_eq!(contents_name(CACTUS, 0), Some("cactus"));
        assert_eq!(contents_name(RED_MUSHROOM, 0), Some("mushroom_red"));
        assert_eq!(contents_name(1, 0), None);
    }

    #[test]
    fn the_item_is_read_as_a_name_with_or_without_prefix_or_as_an_id() {
        let named = nbt(Some(Nbt::String("minecraft:red_flower".into())), 4);
        assert_eq!(pot_contents(&named), Some("red_tulip"));
        let bare = nbt(Some(Nbt::String("cactus".into())), 0);
        assert_eq!(pot_contents(&bare), Some("cactus"));
        let numeric_text = nbt(Some(Nbt::String("37".into())), 0);
        assert_eq!(pot_contents(&numeric_text), Some("dandelion"));
        let id = nbt(Some(Nbt::Int(38)), 1);
        assert_eq!(pot_contents(&id), Some("blue_orchid"));
    }

    #[test]
    fn an_empty_unknown_or_malformed_pot_holds_nothing_and_never_panics() {
        assert_eq!(pot_contents(&nbt(None, 0)), None);
        assert_eq!(pot_contents(&nbt(Some(Nbt::Int(0)), 0)), None);
        assert_eq!(
            pot_contents(&nbt(Some(Nbt::String("stone".into())), 0)),
            None
        );
        let full = nbt(Some(Nbt::Int(38)), 1);
        for len in 0..full.len() {
            assert_eq!(pot_contents(&full[..len]), None, "{len}");
        }
        assert_eq!(pot_contents(&[0xFF; 9]), None);
    }
}
