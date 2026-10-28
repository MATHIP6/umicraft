// SPDX-License-Identifier: MIT OR Apache-2.0

//! The container types a server can open and the window slot numbering.
//!
//! Container windows list their own slots first (0..*N*), then the player's
//! main inventory (*N*..*N*+27) and the hotbar (*N*+27..*N*+36). The player
//! window (id 0) is the identity mapping onto the 45 inventory slots.

/// Number of main inventory slots (excluding hotbar, armour, crafting).
pub const MAIN_INV_LEN: usize = 27;

/// Number of hotbar slots.
pub const HOTBAR_LEN: usize = 9;

/// First main inventory slot index in the player window.
pub const MAIN_INV_START: usize = 9;

/// First hotbar slot index in the player window.
pub const HOTBAR_START: usize = 36;

/// Number of slots in the player window.
pub const PLAYER_SLOT_COUNT: usize = 45;

/// The 1.8 container types the server can open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowKind {
    /// `minecraft:chest` — 27 or 54 slots.
    Chest,
    /// `minecraft:crafting_table` — 10 slots.
    CraftingTable,
    /// `minecraft:furnace` — 3 slots.
    Furnace,
    /// `minecraft:dispenser` — 9 slots.
    Dispenser,
    /// `minecraft:enchanting_table` — 2 slots.
    EnchantingTable,
    /// `minecraft:brewing_stand` — 4 slots.
    BrewingStand,
    /// `minecraft:villager` — 3 slots.
    Villager,
    /// `minecraft:beacon` — 1 slot.
    Beacon,
    /// `minecraft:anvil` — 3 slots.
    Anvil,
    /// `minecraft:hopper` — 5 slots.
    Hopper,
    /// `minecraft:dropper` — 9 slots.
    Dropper,
    /// `EntityHorse` — variable.
    Horse,
}

impl WindowKind {
    /// Parses the window type string sent by the server in `Open Window`.
    ///
    /// Returns `None` for unknown types.
    #[must_use]
    pub fn from_type_string(s: &str) -> Option<Self> {
        Some(match s {
            "minecraft:chest" | "minecraft:container" => Self::Chest,
            "minecraft:crafting_table" => Self::CraftingTable,
            "minecraft:furnace" => Self::Furnace,
            "minecraft:dispenser" => Self::Dispenser,
            "minecraft:enchanting_table" => Self::EnchantingTable,
            "minecraft:brewing_stand" => Self::BrewingStand,
            "minecraft:villager" => Self::Villager,
            "minecraft:beacon" => Self::Beacon,
            "minecraft:anvil" => Self::Anvil,
            "minecraft:hopper" => Self::Hopper,
            "minecraft:dropper" => Self::Dropper,
            "EntityHorse" => Self::Horse,
            _ => return None,
        })
    }
}

/// Where a window-relative slot index maps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotTarget {
    /// A slot in the container portion.
    Container(usize),
    /// A slot in the player inventory (absolute index 0–44).
    Player(usize),
}

/// Resolves a window-relative slot index for a container window.
///
/// Returns `None` if `slot` is out of range.
#[must_use]
pub fn resolve_container_slot(slot: usize, container_size: usize) -> Option<SlotTarget> {
    if slot < container_size {
        Some(SlotTarget::Container(slot))
    } else if slot < container_size + MAIN_INV_LEN {
        Some(SlotTarget::Player(MAIN_INV_START + slot - container_size))
    } else if slot < container_size + MAIN_INV_LEN + HOTBAR_LEN {
        Some(SlotTarget::Player(
            HOTBAR_START + slot - container_size - MAIN_INV_LEN,
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_slot_mapping_for_a_chest() {
        assert_eq!(
            resolve_container_slot(26, 27),
            Some(SlotTarget::Container(26))
        );
        assert_eq!(
            resolve_container_slot(27, 27),
            Some(SlotTarget::Player(MAIN_INV_START))
        );
        assert_eq!(
            resolve_container_slot(54, 27),
            Some(SlotTarget::Player(HOTBAR_START))
        );
        assert_eq!(
            resolve_container_slot(62, 27),
            Some(SlotTarget::Player(HOTBAR_START + HOTBAR_LEN - 1))
        );
        assert_eq!(resolve_container_slot(63, 27), None);
    }

    #[test]
    fn window_kind_parses_all_known_types() {
        assert_eq!(
            WindowKind::from_type_string("minecraft:chest"),
            Some(WindowKind::Chest)
        );
        assert_eq!(
            WindowKind::from_type_string("EntityHorse"),
            Some(WindowKind::Horse)
        );
        assert_eq!(WindowKind::from_type_string("unknown:thing"), None);
    }
}
