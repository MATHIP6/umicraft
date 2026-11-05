// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the statistics screen shows, as plain data the caller builds from
//! the `Statistics` packet.
//!
//! The screen knows no statistic ids: the client maps ids to rows (general
//! counters, per-item and per-block counters, per-mob kills) and the screen
//! lays them out, sorts them and draws them.

use crate::widgets::SlotItem;

/// One line of the general list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatRow {
    /// The translation key of the label (`stat.walkOneCm`).
    pub key: String,
    /// The formatted value (`1.2 km`).
    pub value: String,
}

impl StatRow {
    /// A row from a label key and its value.
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// One counter of an item or block row: the value to sort by and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatCell {
    /// The counter.
    pub raw: i32,
    /// The formatted counter.
    pub text: String,
}

/// One row of the Blocks or Items table.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectRow {
    /// The item id: the tie-break of every sort (`Item.getIdFromItem`).
    pub id: i32,
    /// The icon of the item.
    pub icon: SlotItem,
    /// The item name, for the hover tooltip.
    pub name: String,
    /// The three counters in column order; `None` is a statistic the item
    /// does not have and shows as `-`.
    pub cells: [Option<StatCell>; 3],
}

/// One row of the Mobs list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MobRow {
    /// The entity name of the registry (`Zombie`), which names the language
    /// key `entity.Zombie.name`.
    pub entity: String,
    /// How many of them the player killed.
    pub kills: i32,
    /// How many times one of them killed the player.
    pub killed_by: i32,
}

/// Everything the four tabs show.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatsData {
    /// The general counters, in vanilla order.
    pub general: Vec<StatRow>,
    /// The blocks table (crafted, used, mined), by item id.
    pub blocks: Vec<ObjectRow>,
    /// The items table (depleted, crafted, used), by item id.
    pub items: Vec<ObjectRow>,
    /// The mobs list.
    pub mobs: Vec<MobRow>,
}
