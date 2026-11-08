// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the creative keyboard shortcuts, drag splits and double clicks.

use super::click_tests::{clear, screen, set, window_with_cursor};
use super::*;
use crate::inventory::stack::fixtures::stack;

const STONE: i16 = 1;
const DIRT: i16 = 3;

#[test]
fn number_keys_over_the_grid_fill_a_hotbar_slot_with_a_full_stack() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(None);
    let actions = screen.number_key(&mut window, Some(CreativeSlot::Grid(8)), 4);
    assert_eq!(actions, Some(vec![set(40, &stack(DIRT, 64))]));
    assert_eq!(window.hotbar()[4], Some(stack(DIRT, 64)));
}

#[test]
fn number_keys_do_nothing_with_a_held_stack_or_without_a_hovered_slot() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(Some(stack(STONE, 1)));
    assert_eq!(
        screen.number_key(&mut window, Some(CreativeSlot::Grid(0)), 0),
        None
    );
    let mut window = window_with_cursor(None);
    assert_eq!(screen.number_key(&mut window, None, 0), None);
    assert_eq!(
        screen.number_key(&mut window, Some(CreativeSlot::Grid(0)), 9),
        None
    );
}

#[test]
fn number_keys_over_an_empty_grid_cell_are_consumed_without_effect() {
    let mut screen = screen(CreativeTab::Redstone);
    let mut window = window_with_cursor(None);
    let actions = screen.number_key(&mut window, Some(CreativeSlot::Grid(44)), 0);
    assert_eq!(actions, Some(Vec::new()));
}

#[test]
fn number_keys_over_a_hotbar_slot_swap_two_slots_and_report_both() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(None);
    window.inventory_mut().set_slot(36, Some(stack(DIRT, 2)));
    window.inventory_mut().set_slot(38, Some(stack(STONE, 3)));
    let actions = screen.number_key(&mut window, Some(CreativeSlot::Hotbar(0)), 2);
    assert_eq!(
        actions,
        Some(vec![set(36, &stack(STONE, 3)), set(38, &stack(DIRT, 2))])
    );
}

#[test]
fn number_keys_in_the_inventory_tab_swap_with_the_hotbar() {
    let mut screen = screen(CreativeTab::Inventory);
    let mut window = window_with_cursor(None);
    window.inventory_mut().set_slot(10, Some(stack(DIRT, 2)));
    let actions = screen.number_key(&mut window, Some(CreativeSlot::Player(10)), 0);
    assert_eq!(actions, Some(vec![clear(10), set(36, &stack(DIRT, 2))]));
}

#[test]
fn the_drop_key_over_the_grid_drops_one_or_a_stack_into_the_world() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(None);
    let one = screen.drop_key(&mut window, Some(CreativeSlot::Grid(0)), false);
    assert_eq!(one, vec![CreativeAction::drop_stack(stack(STONE, 1))]);
    let all = screen.drop_key(&mut window, Some(CreativeSlot::Grid(0)), true);
    assert_eq!(all, vec![CreativeAction::drop_stack(stack(STONE, 64))]);
    assert!(screen.drop_key(&mut window, None, false).is_empty());
}

#[test]
fn the_drop_key_in_the_inventory_tab_drops_and_updates_the_slot() {
    let mut screen = screen(CreativeTab::Inventory);
    let mut window = window_with_cursor(None);
    window.inventory_mut().set_slot(12, Some(stack(DIRT, 5)));
    let actions = screen.drop_key(&mut window, Some(CreativeSlot::Player(12)), false);
    assert_eq!(
        actions,
        vec![
            CreativeAction::drop_stack(stack(DIRT, 1)),
            set(12, &stack(DIRT, 4))
        ]
    );
    let actions = screen.drop_key(&mut window, Some(CreativeSlot::Player(12)), true);
    assert_eq!(
        actions,
        vec![CreativeAction::drop_stack(stack(DIRT, 4)), clear(12)]
    );
    assert!(screen
        .drop_key(&mut window, Some(CreativeSlot::Player(12)), true)
        .is_empty());
}

#[test]
fn the_drop_key_over_a_hotbar_slot_only_updates_the_slot() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(None);
    window.inventory_mut().set_slot(36, Some(stack(DIRT, 5)));
    let actions = screen.drop_key(&mut window, Some(CreativeSlot::Hotbar(0)), false);
    assert_eq!(actions, vec![set(36, &stack(DIRT, 4))]);
    assert!(screen
        .drop_key(&mut window, Some(CreativeSlot::Hotbar(1)), false)
        .is_empty());
}

#[test]
fn a_drag_split_over_the_hotbar_resends_all_nine_slots() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(Some(stack(STONE, 8)));
    let slots: Vec<_> = (0..4).map(CreativeSlot::Hotbar).collect();
    let actions = screen.drag_split(&mut window, MouseButton::Left, &slots);
    assert_eq!(actions.len(), 9);
    assert_eq!(actions[0], set(36, &stack(STONE, 2)));
    assert_eq!(actions[3], set(39, &stack(STONE, 2)));
    assert_eq!(actions[4], clear(40));
    assert_eq!(window.cursor(), None);
}

#[test]
fn a_right_drag_in_the_inventory_tab_reports_the_changed_slots() {
    let mut screen = screen(CreativeTab::Inventory);
    let mut window = window_with_cursor(Some(stack(STONE, 8)));
    let slots = [
        CreativeSlot::Player(9),
        CreativeSlot::Player(10),
        CreativeSlot::Grid(0),
    ];
    let actions = screen.drag_split(&mut window, MouseButton::Right, &slots);
    assert_eq!(
        actions,
        vec![set(9, &stack(STONE, 1)), set(10, &stack(STONE, 1))]
    );
    assert_eq!(window.cursor(), Some(&stack(STONE, 6)));
}

#[test]
fn double_clicking_an_empty_hotbar_slot_collects_matching_stacks() {
    let mut screen = screen(CreativeTab::Inventory);
    let mut window = window_with_cursor(Some(stack(STONE, 10)));
    window.inventory_mut().set_slot(37, Some(stack(STONE, 5)));
    window.inventory_mut().set_slot(38, Some(stack(STONE, 2)));
    let actions = screen.double_click(&mut window, CreativeSlot::Player(36));
    assert_eq!(window.cursor(), Some(&stack(STONE, 17)));
    assert!(actions.contains(&clear(37)));
    assert!(screen
        .double_click(&mut window, CreativeSlot::Player(10))
        .is_empty());
}

#[test]
fn double_clicking_an_item_tab_hotbar_gathers_from_the_hotbar_only() {
    let mut screen = screen(CreativeTab::Food);
    let mut window = window_with_cursor(Some(stack(STONE, 10)));
    window.inventory_mut().set_slot(38, Some(stack(STONE, 5)));
    window.inventory_mut().set_slot(20, Some(stack(STONE, 9)));
    let actions = screen.double_click(&mut window, CreativeSlot::Hotbar(0));
    assert_eq!(window.cursor(), Some(&stack(STONE, 15)));
    assert_eq!(actions, vec![clear(38)]);
    assert_eq!(window.inventory().slot(20), Some(&stack(STONE, 9)));
}

#[test]
fn an_item_tab_double_click_needs_an_empty_clicked_slot_and_a_cursor_stack() {
    let mut screen = screen(CreativeTab::Food);
    let mut window = window_with_cursor(Some(stack(STONE, 10)));
    window.inventory_mut().set_slot(36, Some(stack(DIRT, 1)));
    window.inventory_mut().set_slot(38, Some(stack(STONE, 5)));
    assert!(screen
        .double_click(&mut window, CreativeSlot::Hotbar(0))
        .is_empty());
    let mut empty_cursor = window_with_cursor(None);
    assert!(screen
        .double_click(&mut empty_cursor, CreativeSlot::Hotbar(1))
        .is_empty());
    assert!(screen
        .double_click(&mut window, CreativeSlot::Grid(0))
        .is_empty());
}

#[test]
fn shift_double_clicking_an_item_tab_hotbar_deletes_every_equal_stack() {
    let mut screen = screen(CreativeTab::Food);
    let mut window = window_with_cursor(None);
    window.inventory_mut().set_slot(36, Some(stack(STONE, 5)));
    window.inventory_mut().set_slot(37, Some(stack(DIRT, 2)));
    window.inventory_mut().set_slot(40, Some(stack(STONE, 64)));
    screen.click(
        &mut window,
        Some(CreativeSlot::Hotbar(0)),
        MouseButton::Left,
        true,
    );
    window.inventory_mut().set_slot(36, Some(stack(STONE, 3)));
    let actions = screen.shift_double_click(&mut window, CreativeSlot::Hotbar(0));
    assert_eq!(actions, vec![clear(36), clear(40)]);
    assert_eq!(window.inventory().slot(37), Some(&stack(DIRT, 2)));
}

#[test]
fn shift_double_clicking_the_inventory_tab_hotbar_transfers_every_equal_stack() {
    let mut screen = screen(CreativeTab::Inventory);
    let mut window = window_with_cursor(None);
    window.inventory_mut().set_slot(36, Some(stack(STONE, 5)));
    window.inventory_mut().set_slot(37, Some(stack(STONE, 6)));
    window.inventory_mut().set_slot(38, Some(stack(DIRT, 2)));
    screen.click(
        &mut window,
        Some(CreativeSlot::Player(36)),
        MouseButton::Left,
        true,
    );
    window.inventory_mut().set_slot(36, Some(stack(STONE, 5)));
    let actions = screen.shift_double_click(&mut window, CreativeSlot::Player(36));
    assert!(!actions.is_empty());
    assert_eq!(window.inventory().slot(38), Some(&stack(DIRT, 2)));
}

#[test]
fn a_middle_drag_gives_each_crossed_hotbar_slot_a_full_stack() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    // The cursor stack bounds the number of slots, as for any drag
    // (`stackSize > dragSplittingSlots.size()`): 5 items cover 3 slots.
    let mut window = window_with_cursor(Some(stack(STONE, 5)));
    let slots: Vec<_> = (0..3).map(CreativeSlot::Hotbar).collect();
    let actions = screen.drag_split(&mut window, MouseButton::Middle, &slots);
    assert_eq!(actions.len(), 9);
    for (index, action) in actions.iter().take(3).enumerate() {
        assert_eq!(*action, set(36 + index, &stack(STONE, 64)));
    }
    // Three full stacks use up the five items on the cursor.
    assert_eq!(window.cursor(), None);
}
