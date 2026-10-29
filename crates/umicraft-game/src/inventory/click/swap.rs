// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mode 2: a number key swaps the hovered slot with a hotbar slot.

use super::super::space::ClickSpace;

/// Swaps `slot` with hotbar slot `key` (`Container.slotClick`, mode 2).
///
/// When the hotbar item cannot go into the slot, it moves to the first empty
/// inventory slot instead; with no room nothing happens.
pub(super) fn number_key(space: &mut ClickSpace<'_>, slot: usize, key: usize) {
    let hotbar = ClickSpace::hotbar_slot(key);
    let hotbar_item = space.inventory().slot(hotbar).cloned();
    let fits_slot = |space: &ClickSpace<'_>| {
        hotbar_item
            .as_ref()
            .is_none_or(|item| space.is_player_slot(slot) && space.kind(slot).accepts(item))
    };
    let mut flag = fits_slot(space);
    let mut spare = None;
    if !flag {
        spare = space.inventory().first_empty();
        flag = spare.is_some();
    }
    let slot_item = space.get(slot).cloned();
    match slot_item {
        Some(taken) if flag => {
            space.inventory().set_slot(hotbar, Some(taken.clone()));
            let swaps_in_place = fits_slot(space);
            match (&hotbar_item, swaps_in_place) {
                (Some(old), false) => {
                    if spare.is_some() {
                        space.inventory().add_stack(old.clone());
                        space.set(slot, None);
                        space.picked_up(slot);
                    }
                }
                _ => {
                    space.set(slot, hotbar_item);
                    space.picked_up(slot);
                }
            }
        }
        None => {
            if let Some(item) = hotbar_item.filter(|item| space.kind(slot).accepts(item)) {
                space.inventory().set_slot(hotbar, None);
                space.set(slot, Some(item));
            }
        }
        Some(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::kind::{WindowKind, HOTBAR_START};
    use super::super::super::space::fixtures::TestWindow;
    use super::super::super::stack::fixtures::stack;
    use super::super::{slot_click, ClickRequest, DragState};

    fn press(w: &mut TestWindow, slot: i16, key: u8) {
        slot_click(
            &mut w.space(),
            &mut None,
            &mut DragState::default(),
            ClickRequest::number_key(slot, key),
            false,
        );
    }

    #[test]
    fn number_key_swaps_with_the_hotbar() {
        let mut w = TestWindow::player();
        w.inv.set_slot(10, Some(stack(1, 5)));
        w.inv.set_slot(HOTBAR_START + 2, Some(stack(2, 3)));
        press(&mut w, 10, 2);
        assert_eq!(w.inv.slot(10), Some(&stack(2, 3)));
        assert_eq!(w.inv.slot(HOTBAR_START + 2), Some(&stack(1, 5)));
    }

    #[test]
    fn number_key_moves_a_container_item_to_an_empty_hotbar_slot() {
        let mut w = TestWindow::container(WindowKind::Chest, 27);
        w.own[4] = Some(stack(1, 5));
        press(&mut w, 4, 0);
        assert!(w.own[4].is_none());
        assert_eq!(w.inv.slot(HOTBAR_START), Some(&stack(1, 5)));
    }

    #[test]
    fn number_key_stores_a_hotbar_item_into_an_empty_container_slot() {
        let mut w = TestWindow::container(WindowKind::Chest, 27);
        w.inv.set_slot(HOTBAR_START + 1, Some(stack(2, 7)));
        press(&mut w, 0, 1);
        assert_eq!(w.own[0], Some(stack(2, 7)));
        assert!(w.inv.slot(HOTBAR_START + 1).is_none());
    }

    #[test]
    fn number_key_sends_the_displaced_item_to_the_inventory() {
        // A container slot that takes the hotbar item swaps only if valid;
        // an armour slot refuses a stick, so the stick goes to a spare slot.
        let mut w = TestWindow::player();
        w.inv.set_slot(5, Some(stack(298, 1)));
        w.inv.set_slot(HOTBAR_START, Some(stack(280, 1)));
        press(&mut w, 5, 0);
        assert!(w.inv.slot(5).is_none());
        assert_eq!(w.inv.slot(HOTBAR_START), Some(&stack(298, 1)));
        assert!(w
            .inv
            .slot(HOTBAR_START + 1)
            .is_some_and(|s| s.item_id == 280));
    }
}
