// SPDX-License-Identifier: MIT OR Apache-2.0

//! The armour bar value, read from the armour slots of the player inventory.
//!
//! Vanilla derives it from `InventoryPlayer.armorInventory`
//! (`ForgeHooks`-free `EntityLivingBase.getTotalArmorValue`), not from
//! `Entity Equipment` packets, which the server never sends for the local
//! player.

use umicraft_protocol::packets::play::ItemStack;

use crate::items::{self, ArmorMaterial, ArmorSlot, ItemKind};

/// The damage reduction of a piece (MCP `ItemArmor.ArmorMaterial`, listed as
/// {head, chest, legs, feet}).
#[must_use]
pub fn piece_points(material: ArmorMaterial, slot: ArmorSlot) -> u8 {
    let row = match material {
        ArmorMaterial::Leather => [1, 3, 2, 1],
        ArmorMaterial::Chain => [2, 5, 4, 1],
        ArmorMaterial::Iron => [2, 6, 5, 2],
        ArmorMaterial::Diamond => [3, 8, 6, 3],
        ArmorMaterial::Gold => [2, 5, 3, 1],
    };
    row[slot as usize]
}

/// The armour points of an item, `None` when the item is not armour.
#[must_use]
pub fn armor_points(item_id: i16) -> Option<u8> {
    match items::lookup(i32::from(item_id))?.kind {
        ItemKind::Armor(material, slot) => Some(piece_points(material, slot)),
        _ => None,
    }
}

/// The sum of the armour points of the worn pieces.
#[must_use]
pub fn total_armor<'a>(worn: impl IntoIterator<Item = Option<&'a ItemStack>>) -> u8 {
    worn.into_iter()
        .flatten()
        .filter_map(|stack| armor_points(stack.item_id))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_pieces_keep_the_vanilla_values() {
        assert_eq!(armor_points(298), Some(1));
        assert_eq!(armor_points(303), Some(5));
        assert_eq!(armor_points(308), Some(5));
        assert_eq!(armor_points(313), Some(3));
        assert_eq!(armor_points(314), Some(2));
        assert_eq!(armor_points(297), None);
        assert_eq!(armor_points(-1), None);
    }

    #[test]
    fn the_worn_pieces_add_up() {
        let chest = ItemStack {
            item_id: 311,
            stack_size: 1,
            damage: 0,
            nbt: None,
        };
        let legs = ItemStack {
            item_id: 312,
            ..chest.clone()
        };
        assert_eq!(total_armor([Some(&chest), None, Some(&legs)]), 14);
    }
}
