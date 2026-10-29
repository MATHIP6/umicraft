// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the creative click handling: every path and its exact actions.

use umicraft_protocol::packets::play::ItemStack;

use super::state::tests::key_resolver;
use super::*;
use crate::inventory::stack::fixtures::stack;
use crate::inventory::WindowManager;

const STONE: i16 = 1;
const DIRT: i16 = 3;

pub(super) fn screen(tab: CreativeTab) -> CreativeScreen {
    let mut screen = CreativeScreen::new();
    screen.select_tab(tab, &key_resolver);
    screen
}

pub(super) fn window_with_cursor(cursor: Option<ItemStack>) -> WindowManager {
    let mut window = WindowManager::new();
    window.set_cursor(cursor);
    window
}

pub(super) fn set(slot: usize, item: &ItemStack) -> CreativeAction {
    CreativeAction::set_slot(slot, Some(item))
}

pub(super) fn clear(slot: usize) -> CreativeAction {
    CreativeAction::set_slot(slot, None)
}

fn click(
    screen: &mut CreativeScreen,
    window: &mut WindowManager,
    target: Option<CreativeSlot>,
    button: MouseButton,
) -> Vec<CreativeAction> {
    screen.click(window, target, button, false)
}

#[test]
fn clicking_a_grid_item_copies_one_onto_the_cursor() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    for button in [MouseButton::Left, MouseButton::Right] {
        let mut window = window_with_cursor(None);
        let actions = click(
            &mut screen,
            &mut window,
            Some(CreativeSlot::Grid(0)),
            button,
        );
        assert!(actions.is_empty());
        assert_eq!(window.cursor(), Some(&stack(STONE, 1)));
    }
}

#[test]
fn shift_clicking_a_grid_item_takes_a_full_stack() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(None);
    let actions = screen.click(
        &mut window,
        Some(CreativeSlot::Grid(7)),
        MouseButton::Left,
        true,
    );
    assert!(actions.is_empty());
    assert_eq!(window.cursor(), Some(&stack(2, 64)));
}

#[test]
fn a_tool_stack_from_the_grid_is_limited_to_one() {
    let mut screen = screen(CreativeTab::Combat);
    let mut window = window_with_cursor(None);
    screen.click(
        &mut window,
        Some(CreativeSlot::Grid(0)),
        MouseButton::Left,
        true,
    );
    assert_eq!(
        window.cursor().map(|s| (s.item_id, s.stack_size)),
        Some((261, 1))
    );
}

#[test]
fn the_middle_button_clones_a_full_stack_only_with_an_empty_cursor() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(None);
    click(
        &mut screen,
        &mut window,
        Some(CreativeSlot::Grid(0)),
        MouseButton::Middle,
    );
    assert_eq!(window.cursor(), Some(&stack(STONE, 64)));
    let mut window = window_with_cursor(Some(stack(DIRT, 2)));
    click(
        &mut screen,
        &mut window,
        Some(CreativeSlot::Grid(0)),
        MouseButton::Middle,
    );
    assert_eq!(window.cursor(), Some(&stack(DIRT, 2)));
}

#[test]
fn clicking_the_same_item_grows_or_shrinks_the_cursor() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let grid = Some(CreativeSlot::Grid(0));
    let mut window = window_with_cursor(Some(stack(STONE, 10)));
    click(&mut screen, &mut window, grid, MouseButton::Left);
    assert_eq!(window.cursor(), Some(&stack(STONE, 11)));
    click(&mut screen, &mut window, grid, MouseButton::Right);
    assert_eq!(window.cursor(), Some(&stack(STONE, 10)));
    screen.click(&mut window, grid, MouseButton::Left, true);
    assert_eq!(window.cursor(), Some(&stack(STONE, 64)));
    click(&mut screen, &mut window, grid, MouseButton::Left);
    assert_eq!(window.cursor(), Some(&stack(STONE, 64)));
}

#[test]
fn a_right_click_on_a_single_item_clears_the_cursor() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(Some(stack(STONE, 1)));
    click(
        &mut screen,
        &mut window,
        Some(CreativeSlot::Grid(0)),
        MouseButton::Right,
    );
    assert_eq!(window.cursor(), None);
}

#[test]
fn clicking_another_item_or_an_empty_cell_deletes_the_cursor() {
    let mut screen = screen(CreativeTab::Redstone);
    let mut window = window_with_cursor(Some(stack(STONE, 5)));
    click(
        &mut screen,
        &mut window,
        Some(CreativeSlot::Grid(0)),
        MouseButton::Left,
    );
    assert_eq!(window.cursor(), None);
    let mut window = window_with_cursor(Some(stack(STONE, 5)));
    click(
        &mut screen,
        &mut window,
        Some(CreativeSlot::Grid(44)),
        MouseButton::Left,
    );
    assert_eq!(window.cursor(), None);
}

#[test]
fn clicking_an_empty_cell_with_an_empty_cursor_does_nothing() {
    let mut screen = screen(CreativeTab::Redstone);
    let mut window = window_with_cursor(None);
    let actions = click(
        &mut screen,
        &mut window,
        Some(CreativeSlot::Grid(44)),
        MouseButton::Left,
    );
    assert!(actions.is_empty() && window.cursor().is_none());
}

#[test]
fn placing_the_cursor_on_a_hotbar_slot_sends_that_slot() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(Some(stack(STONE, 5)));
    let actions = click(
        &mut screen,
        &mut window,
        Some(CreativeSlot::Hotbar(0)),
        MouseButton::Left,
    );
    assert_eq!(actions, vec![set(36, &stack(STONE, 5))]);
    assert_eq!(window.cursor(), None);
    assert_eq!(window.hotbar()[0], Some(stack(STONE, 5)));
}

#[test]
fn a_right_click_places_one_item_and_keeps_the_rest_on_the_cursor() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(Some(stack(STONE, 5)));
    let actions = click(
        &mut screen,
        &mut window,
        Some(CreativeSlot::Hotbar(2)),
        MouseButton::Right,
    );
    assert_eq!(actions, vec![set(38, &stack(STONE, 1))]);
    assert_eq!(window.cursor(), Some(&stack(STONE, 4)));
}

#[test]
fn picking_up_from_the_hotbar_reports_the_emptied_slot() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(None);
    window.inventory_mut().set_slot(37, Some(stack(DIRT, 9)));
    let actions = click(
        &mut screen,
        &mut window,
        Some(CreativeSlot::Hotbar(1)),
        MouseButton::Left,
    );
    assert_eq!(actions, vec![clear(37)]);
    assert_eq!(window.cursor(), Some(&stack(DIRT, 9)));
}

#[test]
fn clicking_an_unchanged_hotbar_slot_still_sends_it() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(None);
    let actions = click(
        &mut screen,
        &mut window,
        Some(CreativeSlot::Hotbar(4)),
        MouseButton::Left,
    );
    assert_eq!(actions, vec![clear(40)]);
}

#[test]
fn shift_clicking_a_hotbar_slot_deletes_its_stack() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(None);
    window.inventory_mut().set_slot(36, Some(stack(DIRT, 9)));
    let actions = screen.click(
        &mut window,
        Some(CreativeSlot::Hotbar(0)),
        MouseButton::Left,
        true,
    );
    assert_eq!(actions, vec![clear(36)]);
    assert_eq!(window.hotbar()[0], None);
    assert_eq!(window.cursor(), None);
}

#[test]
fn the_middle_button_on_the_hotbar_clones_the_stack() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(None);
    window.inventory_mut().set_slot(36, Some(stack(DIRT, 9)));
    let actions = click(
        &mut screen,
        &mut window,
        Some(CreativeSlot::Hotbar(0)),
        MouseButton::Middle,
    );
    assert_eq!(actions, vec![set(36, &stack(DIRT, 9))]);
    assert_eq!(window.cursor(), Some(&stack(DIRT, 64)));
}

#[test]
fn clicking_outside_drops_the_cursor_stack_or_one_item() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(Some(stack(STONE, 5)));
    let actions = click(&mut screen, &mut window, None, MouseButton::Right);
    assert_eq!(actions, vec![CreativeAction::drop_stack(stack(STONE, 1))]);
    assert_eq!(window.cursor(), Some(&stack(STONE, 4)));
    let actions = click(&mut screen, &mut window, None, MouseButton::Left);
    assert_eq!(actions, vec![CreativeAction::drop_stack(stack(STONE, 4))]);
    assert_eq!(window.cursor(), None);
}

#[test]
fn dropping_the_last_item_by_right_click_empties_the_cursor() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(Some(stack(STONE, 1)));
    let actions = click(&mut screen, &mut window, None, MouseButton::Right);
    assert_eq!(actions.len(), 1);
    assert_eq!(window.cursor(), None);
}

#[test]
fn clicking_outside_with_an_empty_cursor_does_nothing() {
    let mut screen = screen(CreativeTab::BuildingBlocks);
    let mut window = window_with_cursor(None);
    assert!(click(&mut screen, &mut window, None, MouseButton::Left).is_empty());
}

#[test]
fn the_inventory_tab_moves_stacks_with_the_normal_window_rules() {
    let mut screen = screen(CreativeTab::Inventory);
    let mut window = window_with_cursor(None);
    window.inventory_mut().set_slot(9, Some(stack(DIRT, 7)));
    let actions = click(
        &mut screen,
        &mut window,
        Some(CreativeSlot::Player(9)),
        MouseButton::Left,
    );
    assert_eq!(actions, vec![clear(9)]);
    assert_eq!(window.cursor(), Some(&stack(DIRT, 7)));
    let actions = click(
        &mut screen,
        &mut window,
        Some(CreativeSlot::Player(40)),
        MouseButton::Left,
    );
    assert_eq!(actions, vec![set(40, &stack(DIRT, 7))]);
    assert_eq!(window.cursor(), None);
}

#[test]
fn the_inventory_tab_swaps_a_held_stack_with_a_slot() {
    let mut screen = screen(CreativeTab::Inventory);
    let mut window = window_with_cursor(Some(stack(STONE, 3)));
    window.inventory_mut().set_slot(20, Some(stack(DIRT, 7)));
    let actions = click(
        &mut screen,
        &mut window,
        Some(CreativeSlot::Player(20)),
        MouseButton::Left,
    );
    assert_eq!(actions, vec![set(20, &stack(STONE, 3))]);
    assert_eq!(window.cursor(), Some(&stack(DIRT, 7)));
}

#[test]
fn shift_clicking_in_the_inventory_tab_transfers_between_hotbar_and_backpack() {
    let mut screen = screen(CreativeTab::Inventory);
    let mut window = window_with_cursor(None);
    window.inventory_mut().set_slot(36, Some(stack(DIRT, 7)));
    let actions = screen.click(
        &mut window,
        Some(CreativeSlot::Player(36)),
        MouseButton::Left,
        true,
    );
    assert_eq!(actions, vec![set(9, &stack(DIRT, 7)), clear(36)]);
}

#[test]
fn an_outside_click_in_the_inventory_tab_drops_the_whole_cursor() {
    let mut screen = screen(CreativeTab::Inventory);
    let mut window = window_with_cursor(Some(stack(STONE, 5)));
    let actions = click(&mut screen, &mut window, None, MouseButton::Right);
    assert_eq!(actions, vec![CreativeAction::drop_stack(stack(STONE, 5))]);
    assert_eq!(window.cursor(), None);
}

#[test]
fn the_trash_deletes_the_cursor_without_a_packet() {
    let mut screen = screen(CreativeTab::Inventory);
    let mut window = window_with_cursor(Some(stack(STONE, 5)));
    let actions = click(
        &mut screen,
        &mut window,
        Some(CreativeSlot::Trash),
        MouseButton::Left,
    );
    assert!(actions.is_empty());
    assert_eq!(window.cursor(), None);
}

#[test]
fn shift_clicking_the_trash_clears_every_slot() {
    let mut screen = screen(CreativeTab::Inventory);
    let mut window = window_with_cursor(None);
    window.inventory_mut().set_slot(9, Some(stack(DIRT, 7)));
    window.inventory_mut().set_slot(44, Some(stack(STONE, 7)));
    let actions = screen.click(
        &mut window,
        Some(CreativeSlot::Trash),
        MouseButton::Left,
        true,
    );
    assert_eq!(actions.len(), 45);
    assert!(actions
        .iter()
        .enumerate()
        .all(|(i, a)| a.slot == i as i16 && a.stack == ItemStack::EMPTY));
    assert!(window.inventory().all_slots().iter().all(Option::is_none));
}
