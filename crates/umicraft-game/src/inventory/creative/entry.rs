// SPDX-License-Identifier: MIT OR Apache-2.0

//! One row of the generated tab table and its expansion into item stacks.

use umicraft_protocol::packets::play::ItemStack;

use super::stack_nbt;
use crate::items::potion_creative_damages;

/// Item id of the potion (`Items.potionitem`).
const POTION_ID: i16 = 373;
/// Item id of the banner (`Items.banner`).
const BANNER_ID: i16 = 425;
/// Item id of the enchanted book (`Items.enchanted_book`).
const ENCHANTED_BOOK_ID: i16 = 403;

/// What `displayAllReleventItems` adds at one position of a tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Entry {
    /// A plain stack of one item with a damage value.
    Stack(i16, i16),
    /// An enchanted book holding one enchantment (`Items.enchanted_book.getEnchantedItemStack`).
    Book {
        /// `Enchantment.effectId`.
        enchantment: i16,
        /// The stored level.
        level: i16,
    },
    /// A banner of one base colour; the value is the dye damage (`EnumDyeColor.getDyeDamage`).
    Banner(i16),
    /// Every potion `ItemPotion.getSubItems` lists (see [`potion_creative_damages`]).
    Potions,
}

impl Entry {
    /// Appends the stacks this entry stands for to `out`.
    pub(super) fn expand(self, out: &mut Vec<ItemStack>) {
        match self {
            Self::Stack(id, damage) => out.push(single(id, damage, None)),
            Self::Book { enchantment, level } => out.push(single(
                ENCHANTED_BOOK_ID,
                0,
                Some(stack_nbt::stored_enchantment(enchantment, level)),
            )),
            Self::Banner(dye) => out.push(single(
                BANNER_ID,
                dye,
                Some(stack_nbt::banner_base(i32::from(dye))),
            )),
            Self::Potions => out.extend(
                potion_creative_damages()
                    .into_iter()
                    .map(|damage| single(POTION_ID, damage, None)),
            ),
        }
    }
}

fn single(item_id: i16, damage: i16, nbt: Option<Vec<u8>>) -> ItemStack {
    ItemStack {
        item_id,
        stack_size: 1,
        damage,
        nbt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stack_entry_is_one_plain_item() {
        let mut out = Vec::new();
        Entry::Stack(35, 4).expand(&mut out);
        assert_eq!(out, vec![single(35, 4, None)]);
    }

    #[test]
    fn potions_expand_to_the_water_bottle_and_the_variants() {
        let mut out = Vec::new();
        Entry::Potions.expand(&mut out);
        assert_eq!(out[0], single(POTION_ID, 0, None));
        assert_eq!(out.len(), 63);
        assert!(out.iter().all(|stack| stack.nbt.is_none()));
    }

    #[test]
    fn books_and_banners_carry_nbt() {
        let mut out = Vec::new();
        Entry::Book {
            enchantment: 16,
            level: 5,
        }
        .expand(&mut out);
        Entry::Banner(15).expand(&mut out);
        assert!(out.iter().all(|stack| stack.nbt.is_some()));
        assert_eq!((out[0].item_id, out[1].item_id), (403, 425));
    }
}
