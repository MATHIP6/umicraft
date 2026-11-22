// SPDX-License-Identifier: MIT OR Apache-2.0

//! The enchantments of 1.8 as the anvil needs them (MCP `Enchantment` and
//! its subclasses, `EnumEnchantmentType.canEnchantItem`): the highest level,
//! the weight, which items each applies to and which ones exclude each other.

use crate::items::{self, ArmorSlot, ItemKind, ToolKind};

/// `EnumEnchantmentType`: the items an enchantment applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    /// Every piece of armour.
    Armor,
    /// One armour slot.
    ArmorPiece(ArmorSlot),
    /// Swords.
    Weapon,
    /// Pickaxes, axes and shovels (`ItemTool`).
    Digger,
    /// Bows.
    Bow,
    /// Fishing rods.
    FishingRod,
    /// Anything with a durability.
    Breakable,
}

/// The subclass of an enchantment, for its extra rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    /// `EnchantmentProtection` with its protection type (2 is feather falling).
    Protection(u8),
    /// `EnchantmentDamage`: excludes the other damage enchantments.
    Damage,
    /// `EnchantmentLootBonus`: excludes silk touch.
    LootBonus,
    /// `EnchantmentUntouching`: excludes fortune.
    SilkTouch,
    /// `EnchantmentDigging`: also on shears.
    Digging,
    /// `EnchantmentDurability`: also on any damageable stack.
    Durability,
    /// `EnchantmentThorns`: also on any armour.
    Thorns,
    /// No extra rule.
    Plain,
}

/// One enchantment of the 1.8 table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnchantmentDef {
    /// The id (`effectId`).
    pub id: i16,
    /// `getMaxLevel`.
    pub max_level: i32,
    /// `getWeight`, which prices it on the anvil.
    pub weight: u8,
    target: Target,
    family: Family,
}

const fn def(
    id: i16,
    max_level: i32,
    weight: u8,
    target: Target,
    family: Family,
) -> EnchantmentDef {
    EnchantmentDef {
        id,
        max_level,
        weight,
        target,
        family,
    }
}

/// `Enchantment.enchantmentsList` of 1.8, by id.
const ENCHANTMENTS: [EnchantmentDef; 25] = [
    def(0, 4, 10, Target::Armor, Family::Protection(0)),
    def(1, 4, 5, Target::Armor, Family::Protection(1)),
    def(
        2,
        4,
        5,
        Target::ArmorPiece(ArmorSlot::Boots),
        Family::Protection(2),
    ),
    def(3, 4, 2, Target::Armor, Family::Protection(3)),
    def(4, 4, 5, Target::Armor, Family::Protection(4)),
    def(
        5,
        3,
        2,
        Target::ArmorPiece(ArmorSlot::Helmet),
        Family::Plain,
    ),
    def(
        6,
        1,
        2,
        Target::ArmorPiece(ArmorSlot::Helmet),
        Family::Plain,
    ),
    def(
        7,
        3,
        1,
        Target::ArmorPiece(ArmorSlot::Chestplate),
        Family::Thorns,
    ),
    def(8, 3, 2, Target::ArmorPiece(ArmorSlot::Boots), Family::Plain),
    def(16, 5, 10, Target::Weapon, Family::Damage),
    def(17, 5, 5, Target::Weapon, Family::Damage),
    def(18, 5, 5, Target::Weapon, Family::Damage),
    def(19, 2, 5, Target::Weapon, Family::Plain),
    def(20, 2, 2, Target::Weapon, Family::Plain),
    def(21, 3, 2, Target::Weapon, Family::LootBonus),
    def(32, 5, 10, Target::Digger, Family::Digging),
    def(33, 1, 1, Target::Digger, Family::SilkTouch),
    def(34, 3, 5, Target::Breakable, Family::Durability),
    def(35, 3, 2, Target::Digger, Family::LootBonus),
    def(48, 5, 10, Target::Bow, Family::Plain),
    def(49, 2, 2, Target::Bow, Family::Plain),
    def(50, 1, 2, Target::Bow, Family::Plain),
    def(51, 1, 1, Target::Bow, Family::Plain),
    def(61, 3, 2, Target::FishingRod, Family::LootBonus),
    def(62, 3, 2, Target::FishingRod, Family::Plain),
];

/// Silk touch and fortune, which exclude each other.
const SILK_TOUCH: i16 = 33;
const FORTUNE: i16 = 35;
/// Feather falling's protection type, compatible with the other protections.
const FEATHER_FALLING_TYPE: u8 = 2;

/// The enchantment `id`, `None` for an id 1.8 does not know.
#[must_use]
pub fn lookup(id: i16) -> Option<&'static EnchantmentDef> {
    ENCHANTMENTS.iter().find(|def| def.id == id)
}

impl EnchantmentDef {
    /// `canApply`: whether the enchantment fits the item `item_id`.
    /// `damageable` is `ItemStack.isItemStackDamageable` (a durability and no
    /// `Unbreakable` tag), which unbreaking also accepts.
    #[must_use]
    pub fn can_apply(&self, item_id: i16, damageable: bool) -> bool {
        let Some(item) = items::lookup(i32::from(item_id)) else {
            return false;
        };
        let extra = match (self.family, item.kind) {
            (Family::Damage, ItemKind::Tool(ToolKind::Axe, _)) => true,
            (Family::Digging | Family::SilkTouch, ItemKind::Tool(ToolKind::Shears, _)) => true,
            (Family::Thorns, ItemKind::Armor(..)) => true,
            (Family::Durability, _) => damageable,
            _ => false,
        };
        extra || self.target_fits(item)
    }

    /// `EnumEnchantmentType.canEnchantItem`.
    fn target_fits(&self, item: &items::ItemDef) -> bool {
        match (self.target, item.kind) {
            (Target::Breakable, _) => item.is_damageable(),
            (Target::Armor, ItemKind::Armor(..)) => true,
            (Target::ArmorPiece(slot), ItemKind::Armor(_, worn)) => slot == worn,
            (_, ItemKind::Armor(..)) => false,
            (Target::Weapon, ItemKind::Tool(ToolKind::Sword, _)) => true,
            (
                Target::Digger,
                ItemKind::Tool(ToolKind::Pickaxe | ToolKind::Axe | ToolKind::Shovel, _),
            ) => true,
            (Target::Bow, _) => item.name == "bow",
            (Target::FishingRod, _) => item.name == "fishing_rod",
            _ => false,
        }
    }

    /// `canApplyTogether(other)`, asked of the enchantment being added.
    #[must_use]
    pub fn can_apply_together(&self, other: &Self) -> bool {
        match (self.family, other.family) {
            (Family::Protection(mine), Family::Protection(theirs)) => {
                mine != theirs && (mine == FEATHER_FALLING_TYPE || theirs == FEATHER_FALLING_TYPE)
            }
            (Family::Damage, Family::Damage) => false,
            (Family::LootBonus, _) => self.id != other.id && other.id != SILK_TOUCH,
            (Family::SilkTouch, _) => self.id != other.id && other.id != FORTUNE,
            _ => self.id != other.id,
        }
    }

    /// The levels one level of it costs on the anvil (`updateRepairOutput`,
    /// the switch on `getWeight`: 1 is 8, 2 is 4, 5 is 2, 10 is 1), halved
    /// (at least 1) when it comes from a book.
    #[must_use]
    pub fn anvil_price(&self, from_book: bool) -> i32 {
        let price = match self.weight {
            1 => 8,
            2 => 4,
            5 => 2,
            10 => 1,
            _ => 0,
        };
        if from_book {
            (price / 2).max(1)
        } else {
            price
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIAMOND_SWORD: i16 = 276;
    const DIAMOND_AXE: i16 = 279;
    const DIAMOND_BOOTS: i16 = 313;
    const DIAMOND_HELMET: i16 = 310;
    const SHEARS: i16 = 359;
    const BOW: i16 = 261;

    fn ench(id: i16) -> &'static EnchantmentDef {
        lookup(id).expect("a 1.8 enchantment")
    }

    #[test]
    fn enchantments_fit_their_items() {
        assert!(ench(16).can_apply(DIAMOND_SWORD, true));
        assert!(ench(16).can_apply(DIAMOND_AXE, true), "sharpness on an axe");
        assert!(!ench(16).can_apply(BOW, true));
        assert!(ench(2).can_apply(DIAMOND_BOOTS, true));
        assert!(!ench(2).can_apply(DIAMOND_HELMET, true));
        assert!(
            ench(7).can_apply(DIAMOND_BOOTS, true),
            "thorns on any armour"
        );
        assert!(ench(32).can_apply(SHEARS, true));
        assert!(ench(48).can_apply(BOW, true));
        assert!(ench(34).can_apply(BOW, true));
    }

    #[test]
    fn exclusive_enchantments_refuse_each_other() {
        assert!(!ench(16).can_apply_together(ench(17)));
        assert!(!ench(0).can_apply_together(ench(1)));
        assert!(
            ench(0).can_apply_together(ench(2)),
            "feather falling goes with protection"
        );
        assert!(!ench(33).can_apply_together(ench(35)));
        assert!(!ench(35).can_apply_together(ench(33)));
        assert!(ench(16).can_apply_together(ench(34)));
        assert!(!ench(34).can_apply_together(ench(34)));
    }

    #[test]
    fn the_anvil_price_follows_the_weight() {
        assert_eq!(ench(16).anvil_price(false), 1);
        assert_eq!(ench(7).anvil_price(false), 8);
        assert_eq!(ench(7).anvil_price(true), 4);
        assert_eq!(ench(16).anvil_price(true), 1);
    }
}
