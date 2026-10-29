// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mode 6: a double click gathers the matching stacks onto the cursor.

use umicraft_protocol::packets::play::ItemStack;

use super::super::slot::SlotKind;
use super::super::space::ClickSpace;
use super::super::stack::{add_count, count, max_stack, same_item};

/// Collects stacks equal to the cursor stack, scanning from the start of the
/// window (button 0) or from its end (any other button). Partial stacks are
/// taken first, full ones in a second pass.
pub(super) fn collect(
    space: &mut ClickSpace<'_>,
    cursor: &mut Option<ItemStack>,
    slot: usize,
    button: i8,
) {
    let Some(held) = cursor.as_mut() else { return };
    if space.get(slot).is_some() {
        return;
    }
    let order: Vec<usize> = if button == 0 {
        (0..space.len()).collect()
    } else {
        (0..space.len()).rev().collect()
    };
    for pass in 0..2 {
        for &index in &order {
            if count(held) >= max_stack(held) {
                return;
            }
            let Some(there) = space.get(index) else {
                continue;
            };
            let skip_full = pass == 0 && count(there) == max_stack(there);
            let mergeable = !matches!(space.kind(index), SlotKind::CraftingResult { .. });
            if !mergeable || skip_full || !same_item(there, held) {
                continue;
            }
            let moved = (max_stack(held) - count(held)).min(count(there));
            space.decrease(index, moved);
            add_count(held, moved);
            space.picked_up(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::space::fixtures::TestWindow;
    use super::super::super::stack::fixtures::stack;
    use super::super::{slot_click, ClickRequest, DragState};
    use super::*;

    fn double_click(w: &mut TestWindow, cursor: &mut Option<ItemStack>, slot: i16) {
        slot_click(
            &mut w.space(),
            cursor,
            &mut DragState::default(),
            ClickRequest::double(slot),
            false,
        );
    }

    #[test]
    fn the_cursor_gathers_partial_stacks_first() {
        let mut w = TestWindow::player();
        w.inv.set_slot(11, Some(stack(1, 10)));
        w.inv.set_slot(12, Some(stack(1, 20)));
        w.inv.set_slot(13, Some(stack(2, 20)));
        let mut cursor = Some(stack(1, 30));
        double_click(&mut w, &mut cursor, 20);
        assert_eq!(cursor, Some(stack(1, 60)));
        assert!(w.inv.slot(11).is_none());
        assert!(w.inv.slot(12).is_none());
        assert_eq!(w.inv.slot(13), Some(&stack(2, 20)));
    }

    #[test]
    fn full_stacks_are_taken_when_the_partial_ones_are_not_enough() {
        let mut w = TestWindow::player();
        w.inv.set_slot(10, Some(stack(1, 64)));
        w.inv.set_slot(11, Some(stack(1, 10)));
        let mut cursor = Some(stack(1, 50));
        double_click(&mut w, &mut cursor, 20);
        assert_eq!(cursor, Some(stack(1, 64)));
        assert_eq!(w.inv.slot(10), Some(&stack(1, 60)));
        assert!(w.inv.slot(11).is_none());
    }

    #[test]
    fn nothing_is_collected_from_the_crafting_result() {
        let mut w = TestWindow::player();
        w.inv.set_slot(0, Some(stack(1, 4)));
        let mut cursor = Some(stack(1, 1));
        double_click(&mut w, &mut cursor, 20);
        assert_eq!(cursor, Some(stack(1, 1)));
    }
}
