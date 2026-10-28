// SPDX-License-Identifier: MIT OR Apache-2.0

//! The behavioural class of an item: block, tool, armour, food or plain.
//!
//! The variants mirror the MCP item classes (`ItemBlock`, `ItemTool` and its
//! subclasses, `ItemArmor`, `ItemFood`) reduced to what a client needs for
//! the dig speed, the armour bar and the eating rules.

/// A tool material (MCP `Item.ToolMaterial`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// `ToolMaterial.WOOD`.
    Wood,
    /// `ToolMaterial.STONE`.
    Stone,
    /// `ToolMaterial.IRON`.
    Iron,
    /// `ToolMaterial.EMERALD`, the diamond tools.
    Diamond,
    /// `ToolMaterial.GOLD`.
    Gold,
}

/// The kind of tool (MCP `ItemSword`, `ItemPickaxe`, `ItemAxe`, `ItemSpade`,
/// `ItemHoe`, `ItemShears`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    /// `ItemSword`.
    Sword,
    /// `ItemPickaxe`.
    Pickaxe,
    /// `ItemAxe`.
    Axe,
    /// `ItemSpade`.
    Shovel,
    /// `ItemHoe`.
    Hoe,
    /// `ItemShears`.
    Shears,
}

/// An armour material (MCP `ItemArmor.ArmorMaterial`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmorMaterial {
    /// `ArmorMaterial.LEATHER`.
    Leather,
    /// `ArmorMaterial.CHAIN`.
    Chain,
    /// `ArmorMaterial.IRON`.
    Iron,
    /// `ArmorMaterial.GOLD`.
    Gold,
    /// `ArmorMaterial.DIAMOND`.
    Diamond,
}

/// The worn position of an armour piece (MCP `ItemArmor.armorType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmorSlot {
    /// Armour type 0.
    Helmet,
    /// Armour type 1.
    Chestplate,
    /// Armour type 2.
    Leggings,
    /// Armour type 3.
    Boots,
}

/// Food values (MCP `ItemFood` constructor and `setAlwaysEdible`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FoodValues {
    /// Hunger points restored (`healAmount`).
    pub hunger: u8,
    /// Saturation modifier (`saturationModifier`).
    pub saturation: f32,
    /// Whether the item can be eaten with a full food bar.
    pub always_edible: bool,
}

/// What an item is, beyond its id and stack limits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ItemKind {
    /// An item that places a block (`ItemBlock` and its subclasses).
    Block,
    /// A tool with its material.
    Tool(ToolKind, Tier),
    /// A piece of armour.
    Armor(ArmorMaterial, ArmorSlot),
    /// Food.
    Food(FoodValues),
    /// Anything else.
    Plain,
}
