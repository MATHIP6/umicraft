// SPDX-License-Identifier: MIT OR Apache-2.0

//! Modes 0 (left and right click), 3 (middle) and 4 (drop key).

use umicraft_protocol::packets::play::ItemStack;

use super::super::space::ClickSpace;
use super::super::stack::{add_count, count, max_stack, normalize, same_item, split, with_count};

/// A click outside the window: drops the cursor stack (button 0) or one item
/// of it (button 1). Dropping has no local side effect beyond the cursor.
pub(super) fn drop_cursor(cursor: &mut Option<ItemStack>, button: i8) {
    match (button, cursor.as_mut()) {
        (0, _) => *cursor = None,
        (1, Some(held)) => {
            split(held, 1);
            *cursor = normalize(cursor.take());
        }
        _ => {}
    }
}

/// A plain left or right click on a slot. Returns a copy of the slot's
/// stack before the click.
pub(super) fn click(
    space: &mut ClickSpace<'_>,
    cursor: &mut Option<ItemStack>,
    slot: usize,
    button: i8,
) -> Option<ItemStack> {
    let before = space.get(slot).cloned();
    let held_fits = cursor.as_ref().map(|held| space.kind(slot).accepts(held));
    match (&before, held_fits) {
        (None, _) => place_into_empty(space, cursor, slot, button),
        (Some(there), None) => take_from_slot(space, cursor, slot, there, button),
        (Some(there), Some(true)) => put_on_stack(space, cursor, slot, there, button),
        (Some(there), Some(false)) => merge_into_cursor(space, cursor, slot, there),
    }
    *cursor = normalize(cursor.take());
    before
}

fn place_into_empty(
    space: &mut ClickSpace<'_>,
    cursor: &mut Option<ItemStack>,
    slot: usize,
    button: i8,
) {
    let Some(held) = cursor.as_mut() else { return };
    let kind = space.kind(slot);
    if !kind.accepts(held) {
        return;
    }
    let wanted = if button == 0 { count(held) } else { 1 };
    let amount = wanted.min(kind.limit(held));
    if count(held) >= amount {
        space.set(slot, Some(split(held, amount)));
    }
}

fn take_from_slot(
    space: &mut ClickSpace<'_>,
    cursor: &mut Option<ItemStack>,
    slot: usize,
    there: &ItemStack,
    button: i8,
) {
    let amount = if button == 0 {
        count(there)
    } else {
        (count(there) + 1) / 2
    };
    *cursor = space.decrease(slot, amount);
    space.picked_up(slot);
}

fn put_on_stack(
    space: &mut ClickSpace<'_>,
    cursor: &mut Option<ItemStack>,
    slot: usize,
    there: &ItemStack,
    button: i8,
) {
    let Some(held) = cursor.as_mut() else { return };
    let limit = space.kind(slot).limit(held);
    if same_item(there, held) {
        let wanted = if button == 0 { count(held) } else { 1 };
        let room = (limit - count(there)).min(max_stack(held) - count(there));
        let moved = wanted.min(room).max(0);
        split(held, moved);
        space.grow(slot, moved);
    } else if count(held) <= limit {
        let held = held.clone();
        *cursor = Some(there.clone());
        space.set(slot, Some(held));
    }
}

/// The cursor stack cannot go into the slot: pick the slot's items up if they
/// are the same kind and fit on the cursor.
fn merge_into_cursor(
    space: &mut ClickSpace<'_>,
    cursor: &mut Option<ItemStack>,
    slot: usize,
    there: &ItemStack,
) {
    let Some(held) = cursor.as_mut() else { return };
    if !same_item(there, held) || max_stack(held) <= 1 {
        return;
    }
    let size = count(there);
    if size > 0 && size + count(held) <= max_stack(held) {
        add_count(held, size);
        space.decrease(slot, size);
        space.picked_up(slot);
    }
}

/// Mode 3: the creative middle click puts a full stack on the cursor.
pub(super) fn clone_stack(space: &ClickSpace<'_>, cursor: &mut Option<ItemStack>, slot: usize) {
    if let Some(stack) = space.get(slot) {
        *cursor = Some(with_count(stack, max_stack(stack)));
    }
}

/// Mode 4: the drop key removes one item (button 0) or the whole stack.
pub(super) fn drop_from_slot(space: &mut ClickSpace<'_>, slot: usize, button: i8) {
    let Some(stack) = space.get(slot) else { return };
    let amount = if button == 0 { 1 } else { count(stack) };
    if space.decrease(slot, amount).is_some() {
        space.picked_up(slot);
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::kind::WindowKind;
    use super::super::super::space::fixtures::TestWindow;
    use super::super::super::stack::fixtures::stack;
    use super::super::{slot_click, ClickRequest, DragState};
    use super::*;

    fn click_with(
        w: &mut TestWindow,
        cursor: &mut Option<ItemStack>,
        request: ClickRequest,
    ) -> Option<ItemStack> {
        slot_click(
            &mut w.space(),
            cursor,
            &mut DragState::default(),
            request,
            false,
        )
    }

    #[test]
    fn left_click_picks_up_the_whole_stack() {
        let mut w = TestWindow::container(WindowKind::Chest, 27);
        w.own[3] = Some(stack(1, 10));
        let mut cursor = None;
        let clicked = click_with(&mut w, &mut cursor, ClickRequest::left(3));
        assert_eq!(cursor, Some(stack(1, 10)));
        assert!(w.own[3].is_none());
        assert_eq!(clicked, Some(stack(1, 10)));
    }

    #[test]
    fn right_click_picks_up_the_larger_half() {
        let mut w = TestWindow::container(WindowKind::Chest, 27);
        w.own[0] = Some(stack(1, 11));
        let mut cursor = None;
        click_with(&mut w, &mut cursor, ClickRequest::right(0));
        assert_eq!(cursor, Some(stack(1, 6)));
        assert_eq!(w.own[0], Some(stack(1, 5)));
    }

    #[test]
    fn right_click_places_a_single_item() {
        let mut w = TestWindow::container(WindowKind::Chest, 27);
        let mut cursor = Some(stack(1, 5));
        click_with(&mut w, &mut cursor, ClickRequest::right(0));
        assert_eq!(w.own[0], Some(stack(1, 1)));
        assert_eq!(cursor, Some(stack(1, 4)));
    }

    #[test]
    fn left_click_merges_up_to_the_stack_limit() {
        let mut w = TestWindow::container(WindowKind::Chest, 27);
        w.own[0] = Some(stack(1, 60));
        let mut cursor = Some(stack(1, 10));
        click_with(&mut w, &mut cursor, ClickRequest::left(0));
        assert_eq!(w.own[0], Some(stack(1, 64)));
        assert_eq!(cursor, Some(stack(1, 6)));
    }

    #[test]
    fn left_click_swaps_different_items() {
        let mut w = TestWindow::container(WindowKind::Chest, 27);
        w.own[0] = Some(stack(2, 3));
        let mut cursor = Some(stack(1, 5));
        click_with(&mut w, &mut cursor, ClickRequest::left(0));
        assert_eq!(w.own[0], Some(stack(1, 5)));
        assert_eq!(cursor, Some(stack(2, 3)));
    }

    #[test]
    fn a_full_stack_is_pulled_onto_a_matching_cursor_stack() {
        // A furnace output slot refuses the cursor item but still gives its
        // items when they fit (Container.slotClick, last branch).
        let mut w = TestWindow::container(WindowKind::Furnace, 3);
        w.own[2] = Some(stack(1, 5));
        let mut cursor = Some(stack(1, 10));
        click_with(&mut w, &mut cursor, ClickRequest::left(2));
        assert_eq!(cursor, Some(stack(1, 15)));
        assert!(w.own[2].is_none());
    }

    #[test]
    fn armour_slot_refuses_other_items() {
        let mut w = TestWindow::player();
        let mut cursor = Some(stack(1, 1));
        click_with(&mut w, &mut cursor, ClickRequest::left(5));
        assert!(w.inv.slot(5).is_none());
        assert_eq!(cursor, Some(stack(1, 1)));
    }

    #[test]
    fn outside_click_drops_the_cursor() {
        let mut w = TestWindow::player();
        let mut cursor = Some(stack(1, 3));
        click_with(&mut w, &mut cursor, ClickRequest::outside(true));
        assert_eq!(cursor, Some(stack(1, 2)));
        click_with(&mut w, &mut cursor, ClickRequest::outside(false));
        assert!(cursor.is_none());
    }

    #[test]
    fn drop_key_removes_one_or_all() {
        let mut w = TestWindow::player();
        w.inv.set_slot(10, Some(stack(1, 5)));
        let mut cursor = None;
        click_with(&mut w, &mut cursor, ClickRequest::drop_key(10, false));
        assert_eq!(w.inv.slot(10), Some(&stack(1, 4)));
        click_with(&mut w, &mut cursor, ClickRequest::drop_key(10, true));
        assert!(w.inv.slot(10).is_none());
    }

    #[test]
    fn middle_click_clones_a_full_stack_in_creative() {
        let mut w = TestWindow::player();
        w.inv.set_slot(10, Some(stack(1, 1)));
        let mut cursor = None;
        let creative = |w: &mut TestWindow, c: &mut Option<ItemStack>, creative| {
            slot_click(
                &mut w.space(),
                c,
                &mut DragState::default(),
                ClickRequest::middle(10),
                creative,
            )
        };
        creative(&mut w, &mut cursor, false);
        assert!(cursor.is_none());
        creative(&mut w, &mut cursor, true);
        assert_eq!(cursor, Some(stack(1, 64)));
        assert_eq!(w.inv.slot(10), Some(&stack(1, 1)));
    }

    #[test]
    fn taking_the_crafting_result_consumes_the_grid() {
        let mut w = TestWindow::player();
        w.inv.set_slot(0, Some(stack(5, 4)));
        w.inv.set_slot(1, Some(stack(17, 1)));
        let mut cursor = None;
        click_with(&mut w, &mut cursor, ClickRequest::left(0));
        assert_eq!(cursor, Some(stack(5, 4)));
        assert!(w.inv.slot(1).is_none());
    }
}
