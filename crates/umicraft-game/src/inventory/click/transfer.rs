// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mode 1: shift-click moves a stack to the other part of the window
//! (MCP `Container.transferStackInSlot` and `mergeItemStack`).
//!
//! Every container of vanilla has its own transfer plan. The player window and
//! the crafting table keep theirs; every other container uses the generic
//! plan "container → inventory, inventory → container", merging only into
//! slots that accept the item (ADR-0025).

use std::ops::Range;

use umicraft_protocol::packets::play::ItemStack;

use super::super::kind::{HOTBAR_LEN, MAIN_INV_LEN};
use super::super::slot::SlotKind;
use super::super::space::ClickSpace;
use super::super::stack::{
    add_count, count, is_stackable, max_stack, normalize, same_item, set_count, with_count,
};
use crate::items::{self, ItemKind};

/// Safety bound of the shift-click repetition.
const MAX_REPEATS: usize = 4096;

/// Where a shift-clicked stack goes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Plan {
    range: Range<usize>,
    reverse: bool,
}

impl Plan {
    fn new(range: Range<usize>, reverse: bool) -> Self {
        Self { range, reverse }
    }
}

/// Shift-clicks a slot: moves its stack until nothing more moves.
///
/// Returns a copy of the stack as it was before the first move, the "clicked
/// item" of the packet.
pub(super) fn shift_click(space: &mut ClickSpace<'_>, slot: usize) -> Option<ItemStack> {
    let first = transfer_stack(space, slot)?;
    for _ in 0..MAX_REPEATS {
        let same_item_left = space.get(slot).is_some_and(|s| s.item_id == first.item_id);
        if !same_item_left || transfer_stack(space, slot).is_none() {
            break;
        }
    }
    Some(first)
}

/// One `transferStackInSlot`. `None` when nothing moved.
fn transfer_stack(space: &mut ClickSpace<'_>, slot: usize) -> Option<ItemStack> {
    let original = space.get(slot)?.clone();
    let plan = plan_for(space, slot, &original);
    let mut moving = original.clone();
    if !merge_item_stack(space, &mut moving, plan.range, plan.reverse)
        || count(&moving) == count(&original)
    {
        return None;
    }
    space.set(slot, normalize(Some(moving)));
    space.picked_up(slot);
    Some(original)
}

fn plan_for(space: &ClickSpace<'_>, slot: usize, item: &ItemStack) -> Plan {
    let inventory = HOTBAR_LEN + MAIN_INV_LEN;
    let total = space.len();
    let own = total - inventory;
    if matches!(space.kind(0), SlotKind::CraftingResult { grid_len: 4 }) && own == 9 {
        return player_window_plan(space, slot, item);
    }
    if matches!(space.kind(0), SlotKind::CraftingResult { grid_len: 9 }) {
        return match slot {
            0 => Plan::new(own..total, true),
            s if s < own => Plan::new(own..total, false),
            s if s < own + MAIN_INV_LEN => Plan::new(own + MAIN_INV_LEN..total, false),
            _ => Plan::new(own..own + MAIN_INV_LEN, false),
        };
    }
    if slot < own {
        Plan::new(own..total, true)
    } else {
        Plan::new(0..own, false)
    }
}

/// `ContainerPlayer.transferStackInSlot`.
fn player_window_plan(space: &ClickSpace<'_>, slot: usize, item: &ItemStack) -> Plan {
    let armor_slot = match items::lookup(i32::from(item.item_id)).map(|def| def.kind) {
        Some(ItemKind::Armor(_, worn)) => Some(5 + worn as usize),
        _ => None,
    };
    match slot {
        0 => Plan::new(9..45, true),
        1..=8 => Plan::new(9..45, false),
        _ => match armor_slot {
            Some(target) if space.get(target).is_none() => Plan::new(target..target + 1, false),
            _ if slot < 36 => Plan::new(36..45, false),
            _ => Plan::new(9..36, false),
        },
    }
}

/// `Container.mergeItemStack`: puts `stack` into the slots of `range`,
/// topping up equal stacks first and then filling the first empty slot.
/// Slots that refuse the item are skipped. Returns whether anything moved.
pub fn merge_item_stack(
    space: &mut ClickSpace<'_>,
    stack: &mut ItemStack,
    range: Range<usize>,
    reverse: bool,
) -> bool {
    let order: Vec<usize> = if reverse {
        range.rev().collect()
    } else {
        range.collect()
    };
    let mut moved = false;
    if is_stackable(stack) {
        for &index in &order {
            if count(stack) == 0 {
                break;
            }
            let limit = max_stack(stack);
            let accepts = space.kind(index).accepts(stack);
            let Some(there) = space.get_mut(index) else {
                continue;
            };
            if !accepts || !same_item(there, stack) || count(there) >= limit {
                continue;
            }
            let room = limit - count(there);
            let amount = room.min(count(stack));
            add_count(there, amount);
            add_count(stack, -amount);
            moved = true;
        }
    }
    if count(stack) > 0 {
        let target = order
            .iter()
            .copied()
            .find(|&index| space.get(index).is_none() && space.kind(index).accepts(stack));
        if let Some(index) = target {
            space.set(index, Some(with_count(stack, count(stack))));
            set_count(stack, 0);
            moved = true;
        }
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::super::super::kind::{WindowKind, HOTBAR_START};
    use super::super::super::space::fixtures::TestWindow;
    use super::super::super::stack::fixtures::stack;
    use super::super::{slot_click, ClickRequest, DragState};
    use super::*;

    fn shift(w: &mut TestWindow, slot: i16) -> Option<ItemStack> {
        slot_click(
            &mut w.space(),
            &mut None,
            &mut DragState::default(),
            ClickRequest::shift(slot, 0),
            false,
        )
    }

    #[test]
    fn a_backpack_stack_goes_to_the_hotbar() {
        let mut w = TestWindow::player();
        w.inv.set_slot(10, Some(stack(1, 5)));
        let clicked = shift(&mut w, 10);
        assert_eq!(clicked, Some(stack(1, 5)));
        assert!(w.inv.slot(10).is_none());
        assert_eq!(w.inv.slot(HOTBAR_START), Some(&stack(1, 5)));
    }

    #[test]
    fn a_hotbar_stack_goes_to_the_backpack() {
        let mut w = TestWindow::player();
        w.inv.set_slot(HOTBAR_START, Some(stack(1, 5)));
        shift(&mut w, 36);
        assert_eq!(w.inv.slot(9), Some(&stack(1, 5)));
    }

    #[test]
    fn armour_goes_to_its_slot() {
        let mut w = TestWindow::player();
        w.inv.set_slot(HOTBAR_START, Some(stack(299, 1)));
        shift(&mut w, 36);
        assert_eq!(w.inv.slot(6), Some(&stack(299, 1)));
        assert!(w.inv.slot(HOTBAR_START).is_none());
    }

    #[test]
    fn a_chest_stack_goes_to_the_inventory_from_the_end() {
        let mut w = TestWindow::container(WindowKind::Chest, 27);
        w.own[0] = Some(stack(1, 5));
        shift(&mut w, 0);
        assert!(w.own[0].is_none());
        // Reverse order fills the last hotbar slot first.
        assert_eq!(w.inv.slot(44), Some(&stack(1, 5)));
    }

    #[test]
    fn an_inventory_stack_goes_to_the_chest_and_tops_up_first() {
        let mut w = TestWindow::container(WindowKind::Chest, 27);
        w.own[4] = Some(stack(1, 60));
        w.inv.set_slot(9, Some(stack(1, 10)));
        shift(&mut w, 27);
        assert_eq!(w.own[4], Some(stack(1, 64)));
        assert_eq!(w.own[0], Some(stack(1, 6)));
    }

    #[test]
    fn a_full_destination_moves_nothing() {
        let mut w = TestWindow::container(WindowKind::Hopper, 5);
        for i in 0..5 {
            w.own[i] = Some(stack(2, 64));
        }
        w.inv.set_slot(9, Some(stack(1, 3)));
        assert_eq!(shift(&mut w, 5), None);
        assert_eq!(w.inv.slot(9), Some(&stack(1, 3)));
    }

    #[test]
    fn nothing_is_shifted_into_the_furnace_output() {
        let mut w = TestWindow::container(WindowKind::Furnace, 3);
        w.own[0] = Some(stack(1, 1));
        w.own[1] = Some(stack(2, 1));
        w.inv.set_slot(9, Some(stack(3, 1)));
        shift(&mut w, 3);
        assert!(w.own[2].is_none());
    }

    #[test]
    fn crafting_table_result_goes_to_the_inventory() {
        let mut w = TestWindow::container(WindowKind::CraftingTable, 10);
        w.own[0] = Some(stack(5, 4));
        w.own[1] = Some(stack(17, 1));
        shift(&mut w, 0);
        assert!(w.own[1].is_none());
        assert_eq!(w.inv.slot(44), Some(&stack(5, 4)));
    }
}
