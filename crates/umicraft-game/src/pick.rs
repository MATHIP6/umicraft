// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pick block (the middle mouse button): which item a block gives and how the
//! hotbar changes.
//!
//! MCP `Minecraft.middleClickMouse` asks the block for its item
//! (`Block.getItem`, `Block.getDamageValue`) and `InventoryPlayer.setCurrentItem`
//! selects the hotbar slot that holds it; a creative player who has none gets
//! the item in the first free hotbar slot. The block-to-item table below
//! stands in for the 1.8 item registry until P3.

use umicraft_protocol::packets::play::ItemStack;
use umicraft_world::BlockState;

/// The slots of the main inventory in vanilla order: the hotbar first
/// (MCP `InventoryPlayer.mainInventory`).
pub const MAIN_SLOTS: usize = 36;

/// The number of hotbar slots.
const HOTBAR: usize = 9;

/// Blocks that give another item than the item of their own id
/// (MCP `Block.getItem` overrides): `(block, item)`.
const REDIRECTS: &[(u16, i16)] = &[
    (26, 355),  // bed (BlockBed)
    (55, 331),  // redstone wire (BlockRedstoneWire)
    (59, 295),  // wheat (BlockCrops.getSeed)
    (60, 3),    // farmland (BlockFarmland)
    (62, 61),   // lit furnace (BlockFurnace)
    (63, 323),  // standing sign (BlockSign)
    (64, 324),  // oak door (BlockDoor)
    (68, 323),  // wall sign (BlockSign)
    (71, 330),  // iron door (BlockDoor)
    (75, 76),   // unlit redstone torch (BlockRedstoneTorch)
    (83, 338),  // sugar cane (BlockReed)
    (92, 354),  // cake (BlockCake)
    (93, 356),  // unpowered repeater (BlockRedstoneRepeater)
    (94, 356),  // powered repeater
    (115, 372), // nether wart (BlockNetherWart)
    (117, 379), // brewing stand (BlockBrewingStand)
    (118, 380), // cauldron (BlockCauldron)
    (124, 123), // lit redstone lamp (BlockRedstoneLight)
    (127, 351), // cocoa (BlockCocoa, dye)
    (140, 390), // flower pot (BlockFlowerPot)
    (141, 391), // carrots (BlockCrops)
    (142, 392), // potatoes (BlockCrops)
    (144, 397), // skull (BlockSkull)
    (149, 404), // unpowered comparator (BlockRedstoneComparator)
    (150, 404), // powered comparator
    (193, 427), // spruce door
    (194, 428), // birch door
    (195, 429), // jungle door
    (196, 430), // acacia door
    (197, 431), // dark oak door
];

/// Blocks that give no item (MCP `Block.getItem` returns `null`, or the block
/// has no item block): air, liquids, fire, portals, the spawner, the moving
/// piston.
const NO_ITEM: &[u16] = &[0, 8, 9, 10, 11, 36, 51, 52, 90, 119];

/// Blocks whose damage value is a variant of the item, with the mask
/// `damageDropped` applies to the metadata (MCP `Block.damageDropped`).
const VARIANTS: &[(u16, u8)] = &[
    (1, 0xF),   // stone
    (3, 0xF),   // dirt
    (5, 0xF),   // planks
    (6, 0x7),   // sapling
    (12, 0xF),  // sand
    (17, 0x3),  // log
    (18, 0x3),  // leaves
    (24, 0xF),  // sandstone
    (31, 0xF),  // tall grass
    (35, 0xF),  // wool
    (38, 0xF),  // flower
    (44, 0x7),  // stone slab
    (95, 0xF),  // stained glass
    (97, 0xF),  // monster egg
    (98, 0xF),  // stone brick
    (126, 0x7), // wooden slab
    (139, 0xF), // cobblestone wall
    (159, 0xF), // stained clay
    (160, 0xF), // stained glass pane
    (161, 0x3), // leaves 2
    (162, 0x3), // log 2
    (171, 0xF), // carpet
    (175, 0x7), // double plant
    (179, 0xF), // red sandstone
    (168, 0xF), // prismarine
    (182, 0x7), // red sandstone slab
];

/// The double-slab blocks and the single slab they give.
const DOUBLE_SLABS: &[(u16, i16)] = &[(43, 44), (125, 126), (181, 182)];

/// The item a block gives when picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickedItem {
    /// The item id.
    pub id: i16,
    /// The damage value (the variant of the block).
    pub damage: i16,
    /// Whether the item has variants, so that the damage value takes part in
    /// the search (MCP `Item.getHasSubtypes`).
    pub has_subtypes: bool,
}

/// The item that picking `block` gives, `None` for a block without item.
///
/// MCP `Minecraft.middleClickMouse` lines 2478-2500.
#[must_use]
pub fn picked_item(block: BlockState) -> Option<PickedItem> {
    let id = block.id();
    if NO_ITEM.contains(&id) {
        return None;
    }
    let meta = block.metadata();
    let item = |id: i16, mask: Option<u8>| PickedItem {
        id,
        damage: mask.map_or(0, |mask| i16::from(meta & mask)),
        has_subtypes: mask.is_some(),
    };
    if let Some(&(_, single)) = DOUBLE_SLABS.iter().find(|(double, _)| *double == id) {
        return Some(item(single, Some(0x7)));
    }
    let mask = VARIANTS
        .iter()
        .find(|(variant, _)| *variant == id)
        .map(|&(_, mask)| mask);
    let item_id = REDIRECTS
        .iter()
        .find(|(from, _)| *from == id)
        .map_or(i16::try_from(id).ok()?, |&(_, item)| item);
    Some(item(item_id, mask))
}

/// What picking changes in the inventory.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Pick {
    /// The hotbar slot to select, if it changes.
    pub select: Option<u8>,
    /// The slots whose content changes, as `(vanilla index, stack)`.
    pub changed: Vec<(usize, ItemStack)>,
}

/// Applies `InventoryPlayer.setCurrentItem(item, damage, hasSubtypes, creative)`
/// to the main inventory (vanilla order: the hotbar first).
///
/// `selected` is the selected hotbar slot. Without creative mode the pick only
/// selects a hotbar slot that already holds the item. In creative mode it
/// takes the first free hotbar slot, and fills it with the item (or with the
/// stack found in the main inventory, which is swapped out).
#[must_use]
pub fn pick_block(
    slots: &[Option<ItemStack>; MAIN_SLOTS],
    selected: u8,
    picked: PickedItem,
    creative: bool,
) -> Pick {
    let matches = |stack: &Option<ItemStack>, with_damage: bool| {
        stack.as_ref().is_some_and(|stack| {
            stack.item_id == picked.id && (!with_damage || stack.damage == picked.damage)
        })
    };
    let found = slots
        .iter()
        .position(|stack| matches(stack, picked.has_subtypes));
    if let Some(found) = found.filter(|&found| found < HOTBAR) {
        return Pick {
            select: Some(found as u8),
            changed: Vec::new(),
        };
    }
    if !creative {
        return Pick::default();
    }
    let free = slots.iter().position(Option::is_none);
    let target = free
        .filter(|&free| free < HOTBAR)
        .unwrap_or(usize::from(selected));
    let mut pick = Pick {
        select: (target != usize::from(selected)).then_some(target as u8),
        changed: Vec::new(),
    };
    let existing = slots.iter().position(|stack| matches(stack, true));
    let size = existing
        .and_then(|index| slots[index].as_ref())
        .map_or(1, |stack| stack.stack_size);
    if let Some(index) = existing {
        let displaced = slots[target].clone().unwrap_or(ItemStack::EMPTY);
        pick.changed.push((index, displaced));
    }
    pick.changed.push((
        target,
        ItemStack {
            item_id: picked.id,
            stack_size: size,
            damage: picked.damage,
            nbt: None,
        },
    ));
    pick
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(id: i16, size: i8, damage: i16) -> Option<ItemStack> {
        Some(ItemStack {
            item_id: id,
            stack_size: size,
            damage,
            nbt: None,
        })
    }

    fn empty_slots() -> [Option<ItemStack>; MAIN_SLOTS] {
        std::array::from_fn(|_| None)
    }

    #[test]
    fn a_block_gives_the_item_of_its_id() {
        let stone = picked_item(BlockState::new(4, 0)).unwrap();
        assert_eq!((stone.id, stone.damage, stone.has_subtypes), (4, 0, false));
    }

    #[test]
    fn variants_keep_their_metadata() {
        let wool = picked_item(BlockState::new(35, 14)).unwrap();
        assert_eq!((wool.id, wool.damage, wool.has_subtypes), (35, 14, true));
        // A log keeps the wood type, not the orientation bits.
        let log = picked_item(BlockState::new(17, 0b1001)).unwrap();
        assert_eq!(log.damage, 1);
    }

    #[test]
    fn blocks_may_give_another_item() {
        assert_eq!(picked_item(BlockState::new(55, 3)).unwrap().id, 331);
        assert_eq!(picked_item(BlockState::new(64, 0)).unwrap().id, 324);
        assert_eq!(picked_item(BlockState::new(43, 2)).unwrap().id, 44);
        assert_eq!(picked_item(BlockState::new(43, 2)).unwrap().damage, 2);
    }

    #[test]
    fn air_and_liquids_give_nothing() {
        for id in [0, 8, 9, 10, 11, 51, 90] {
            assert_eq!(picked_item(BlockState::new(id, 0)), None, "{id}");
        }
    }

    fn picked(id: i16, damage: i16, has_subtypes: bool) -> PickedItem {
        PickedItem {
            id,
            damage,
            has_subtypes,
        }
    }

    #[test]
    fn the_hotbar_slot_that_holds_the_item_is_selected() {
        let mut slots = empty_slots();
        slots[5] = stack(4, 10, 0);
        let pick = pick_block(&slots, 0, picked(4, 0, false), false);
        assert_eq!(pick.select, Some(5));
        assert!(pick.changed.is_empty());
    }

    #[test]
    fn the_variant_must_match_when_the_item_has_subtypes() {
        let mut slots = empty_slots();
        slots[2] = stack(35, 1, 3);
        slots[6] = stack(35, 1, 14);
        let pick = pick_block(&slots, 0, picked(35, 14, true), false);
        assert_eq!(pick.select, Some(6));
    }

    #[test]
    fn survival_does_not_fetch_from_the_main_inventory() {
        let mut slots = empty_slots();
        slots[20] = stack(4, 10, 0);
        assert_eq!(
            pick_block(&slots, 0, picked(4, 0, false), false),
            Pick::default()
        );
    }

    #[test]
    fn creative_fills_the_first_free_hotbar_slot() {
        let mut slots = empty_slots();
        slots[0] = stack(1, 1, 0);
        slots[1] = stack(1, 1, 0);
        let pick = pick_block(&slots, 0, picked(4, 0, false), true);
        assert_eq!(pick.select, Some(2));
        assert_eq!(pick.changed, vec![(2, stack(4, 1, 0).unwrap())]);
    }

    #[test]
    fn creative_swaps_a_stack_of_the_main_inventory_in() {
        let mut slots = empty_slots();
        slots[0] = stack(1, 1, 0);
        slots[12] = stack(4, 33, 0);
        let pick = pick_block(&slots, 0, picked(4, 0, false), true);
        // The stack of slot 12 moves to the free slot 1; slot 12 gets the
        // (empty) previous content of that slot.
        assert_eq!(pick.select, Some(1));
        assert_eq!(
            pick.changed,
            vec![(12, ItemStack::EMPTY), (1, stack(4, 33, 0).unwrap())]
        );
    }

    #[test]
    fn creative_replaces_the_selected_slot_when_the_hotbar_is_full() {
        let slots: [Option<ItemStack>; MAIN_SLOTS] =
            std::array::from_fn(|index| (index < HOTBAR).then(|| stack(1, 1, 0)).flatten());
        let pick = pick_block(&slots, 3, picked(4, 0, false), true);
        assert_eq!(pick.select, None);
        assert_eq!(pick.changed, vec![(3, stack(4, 1, 0).unwrap())]);
    }
}
