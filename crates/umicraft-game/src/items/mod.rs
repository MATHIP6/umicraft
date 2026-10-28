// SPDX-License-Identifier: MIT OR Apache-2.0

//! The 1.8 item registry (P3).
//!
//! One [`ItemDef`] per numeric item id: registry name, stack and durability
//! limits and an [`ItemKind`] with the tool tier, armour material or food
//! values. The table is generated from MCP `Item.registerItems` by
//! `docs/tools/gen_item_data.py` (see `docs/sources.md`), so the client no
//! longer needs the hand-kept id lists of the M9 and P2 code.
//!
//! ```
//! use umicraft_game::items::{lookup, ItemKind};
//!
//! let sword = lookup(276).expect("diamond sword");
//! assert_eq!(sword.name, "diamond_sword");
//! assert_eq!(sword.max_damage, 1561);
//! assert!(matches!(sword.kind, ItemKind::Tool(..)));
//! ```

mod egg_table;
pub mod kind;
mod potion_color;
mod table;
pub mod tint;

pub use kind::{ArmorMaterial, ArmorSlot, FoodValues, ItemKind, Tier, ToolKind};

/// The static description of one item id.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemDef {
    /// The numeric item id (the block id for a block item).
    pub id: u16,
    /// The registry name without the `minecraft:` prefix.
    pub name: &'static str,
    /// The maximum stack size (`Item.getItemStackLimit`).
    pub max_stack: u8,
    /// The durability (`Item.getMaxDamage`), 0 for an item without one.
    pub max_damage: u16,
    /// The behavioural class.
    pub kind: ItemKind,
}

impl ItemDef {
    /// Whether the item has a durability bar (`Item.isDamageable`).
    pub fn is_damageable(&self) -> bool {
        self.max_damage > 0
    }
}

/// Looks up an item by its numeric id, `None` for an unknown or negative id.
///
/// The table is sorted by id, so this is a binary search without allocation.
pub fn lookup(id: i32) -> Option<&'static ItemDef> {
    let id = u16::try_from(id).ok()?;
    let index = table::ITEMS.binary_search_by_key(&id, |def| def.id).ok()?;
    table::ITEMS.get(index)
}

/// Every registered item, sorted by id.
pub fn all() -> &'static [ItemDef] {
    table::ITEMS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_sorted() {
        assert!(all().windows(2).all(|pair| pair[0].id < pair[1].id));
    }

    #[test]
    fn unknown_and_negative_ids_have_no_definition() {
        assert!(lookup(-1).is_none());
        assert!(lookup(30_000).is_none());
    }

    #[test]
    fn tools_and_armour_are_damageable_and_unstackable() {
        for def in all() {
            if matches!(def.kind, ItemKind::Tool(..) | ItemKind::Armor(..)) {
                assert!(def.is_damageable(), "{}", def.name);
                assert_eq!(def.max_stack, 1, "{}", def.name);
            }
        }
    }

    #[test]
    fn known_items_match_the_reference() {
        let stone = lookup(1).expect("stone");
        assert_eq!(
            (stone.name, stone.max_stack, stone.kind),
            ("stone", 64, ItemKind::Block)
        );
        let apple = lookup(322).expect("golden apple");
        assert_eq!(
            apple.kind,
            ItemKind::Food(FoodValues {
                hunger: 4,
                saturation: 1.2,
                always_edible: true
            })
        );
        assert_eq!(lookup(261).expect("bow").max_damage, 384);
    }
}
