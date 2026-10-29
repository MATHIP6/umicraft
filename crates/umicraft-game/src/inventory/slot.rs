// SPDX-License-Identifier: MIT OR Apache-2.0

//! What each slot of a window accepts, and the slot layout of every window.
//!
//! Mirrors the `Slot` subclasses of MCP (`SlotCrafting`, `SlotFurnaceOutput`,
//! `ContainerPlayer.1` for armour, `SlotBeacon`, …). Rules that need recipe or
//! fuel tables (furnace fuel, brewing ingredients) are not predicted: the
//! server rejects them (ADR-0025).

use umicraft_protocol::packets::play::ItemStack;

use super::kind::{WindowKind, HOTBAR_LEN, MAIN_INV_LEN};
use super::stack::max_stack;
use crate::items::{self, ArmorSlot, ItemKind};

const PUMPKIN_BLOCK: i16 = 86;
const SKULL_ITEM: i16 = 397;
const DYE: i16 = 351;
const BLUE_DYE_DAMAGE: i16 = 4;
const POTION: i16 = 373;
const GLASS_BOTTLE: i16 = 374;
const BEACON_PAYMENTS: [i16; 4] = [264, 265, 266, 388];
const SADDLE: i16 = 329;
const HORSE_ARMORS: [i16; 3] = [417, 418, 419];

/// The behaviour of one window slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    /// Any item, up to the item's stack limit.
    Normal,
    /// An armour slot: only the matching piece, one at a time.
    Armor(ArmorSlot),
    /// The crafting result: nothing can be placed, taking consumes the
    /// `grid_len` ingredient slots that follow it.
    CraftingResult {
        /// Number of grid slots after the result slot.
        grid_len: u8,
    },
    /// A take-only slot (furnace, anvil and merchant results).
    Output,
    /// A brewing stand bottle: a potion or an empty bottle.
    PotionBottle,
    /// The enchanting table lapis slot (blue dye).
    EnchantLapis,
    /// The beacon payment slot.
    BeaconPayment,
    /// The horse saddle slot.
    Saddle,
    /// The horse armour slot.
    HorseArmor,
}

impl SlotKind {
    /// `Slot.isItemValid`.
    #[must_use]
    pub fn accepts(self, stack: &ItemStack) -> bool {
        match self {
            Self::Normal => true,
            Self::CraftingResult { .. } | Self::Output => false,
            Self::Armor(slot) => accepts_armor(slot, stack),
            Self::PotionBottle => matches!(stack.item_id, POTION | GLASS_BOTTLE),
            Self::EnchantLapis => stack.item_id == DYE && stack.damage == BLUE_DYE_DAMAGE,
            Self::BeaconPayment => BEACON_PAYMENTS.contains(&stack.item_id),
            Self::Saddle => stack.item_id == SADDLE,
            Self::HorseArmor => HORSE_ARMORS.contains(&stack.item_id),
        }
    }

    /// `Slot.getItemStackLimit` for a stack, before the item's own limit.
    #[must_use]
    pub fn limit(self, stack: &ItemStack) -> i32 {
        match self {
            Self::Armor(_)
            | Self::PotionBottle
            | Self::BeaconPayment
            | Self::Saddle
            | Self::HorseArmor => 1,
            _ => max_stack(stack),
        }
    }
}

fn accepts_armor(slot: ArmorSlot, stack: &ItemStack) -> bool {
    if slot == ArmorSlot::Helmet && matches!(stack.item_id, PUMPKIN_BLOCK | SKULL_ITEM) {
        return true;
    }
    matches!(
        items::lookup(i32::from(stack.item_id)).map(|def| def.kind),
        Some(ItemKind::Armor(_, worn)) if worn == slot
    )
}

/// The kinds of the container's own slots (before the 36 player slots).
fn own_slot_kinds(kind: WindowKind, count: usize) -> Vec<SlotKind> {
    use SlotKind::{Normal, Output};
    let fixed: &[SlotKind] = match kind {
        WindowKind::CraftingTable => {
            return std::iter::once(SlotKind::CraftingResult { grid_len: 9 })
                .chain(std::iter::repeat_n(Normal, 9))
                .collect()
        }
        WindowKind::Furnace => &[Normal, Normal, Output],
        WindowKind::BrewingStand => &[
            SlotKind::PotionBottle,
            SlotKind::PotionBottle,
            SlotKind::PotionBottle,
            Normal,
        ],
        WindowKind::EnchantingTable => &[Normal, SlotKind::EnchantLapis],
        WindowKind::Anvil | WindowKind::Villager => &[Normal, Normal, Output],
        WindowKind::Beacon => &[SlotKind::BeaconPayment],
        WindowKind::Horse => &[SlotKind::Saddle, SlotKind::HorseArmor],
        WindowKind::Chest | WindowKind::Dispenser | WindowKind::Dropper | WindowKind::Hopper => &[],
    };
    let mut kinds = fixed.to_vec();
    kinds.resize(count.max(fixed.len()), Normal);
    kinds
}

/// The kinds of every slot of the player window (id 0): the result, the 2×2
/// grid, the four armour slots, then the 36 inventory slots.
#[must_use]
pub fn player_window_kinds() -> Vec<SlotKind> {
    let mut kinds = vec![SlotKind::CraftingResult { grid_len: 4 }];
    kinds.extend([SlotKind::Normal; 4]);
    kinds.extend([
        SlotKind::Armor(ArmorSlot::Helmet),
        SlotKind::Armor(ArmorSlot::Chestplate),
        SlotKind::Armor(ArmorSlot::Leggings),
        SlotKind::Armor(ArmorSlot::Boots),
    ]);
    kinds.resize(kinds.len() + MAIN_INV_LEN + HOTBAR_LEN, SlotKind::Normal);
    kinds
}

/// The kinds of every slot of a container window: its own slots followed by
/// the 36 player slots.
#[must_use]
pub fn container_window_kinds(kind: WindowKind, own_count: usize) -> Vec<SlotKind> {
    let mut kinds = own_slot_kinds(kind, own_count);
    kinds.resize(kinds.len() + MAIN_INV_LEN + HOTBAR_LEN, SlotKind::Normal);
    kinds
}

#[cfg(test)]
mod tests {
    use super::super::stack::fixtures::stack;
    use super::*;

    #[test]
    fn armour_slots_take_only_their_piece() {
        let helmet = SlotKind::Armor(ArmorSlot::Helmet);
        assert!(helmet.accepts(&stack(298, 1)));
        assert!(!helmet.accepts(&stack(299, 1)));
        assert!(helmet.accepts(&stack(PUMPKIN_BLOCK, 1)));
        assert_eq!(helmet.limit(&stack(298, 1)), 1);
    }

    #[test]
    fn result_slots_accept_nothing() {
        assert!(!SlotKind::Output.accepts(&stack(1, 1)));
        assert!(!SlotKind::CraftingResult { grid_len: 4 }.accepts(&stack(1, 1)));
    }

    #[test]
    fn lapis_slot_wants_blue_dye() {
        let mut lapis = stack(DYE, 1);
        assert!(!SlotKind::EnchantLapis.accepts(&lapis));
        lapis.damage = BLUE_DYE_DAMAGE;
        assert!(SlotKind::EnchantLapis.accepts(&lapis));
    }

    #[test]
    fn window_layouts_have_the_vanilla_slot_counts() {
        assert_eq!(player_window_kinds().len(), 45);
        assert_eq!(container_window_kinds(WindowKind::Chest, 54).len(), 90);
        assert_eq!(container_window_kinds(WindowKind::Furnace, 3).len(), 39);
        assert_eq!(
            container_window_kinds(WindowKind::CraftingTable, 10).len(),
            46
        );
        assert_eq!(
            container_window_kinds(WindowKind::BrewingStand, 4).len(),
            40
        );
    }

    #[test]
    fn the_furnace_result_is_take_only() {
        let kinds = container_window_kinds(WindowKind::Furnace, 3);
        assert_eq!(kinds[2], SlotKind::Output);
    }
}
