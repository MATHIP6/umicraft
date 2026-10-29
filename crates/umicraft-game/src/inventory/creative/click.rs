// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mouse clicks in the creative window (MCP
//! `GuiContainerCreative.handleMouseClick`).
//!
//! The item tabs and the inventory tab behave differently:
//!
//! * In an item tab the grid is a palette: clicking copies a stack onto the
//!   cursor, the hotbar below it is the player's real hotbar, and a click
//!   outside the window drops the cursor stack.
//! * In the inventory tab the player window (`inventoryContainer`) is clicked
//!   with the normal window rules, and the trash slot deletes the cursor.
//!
//! No `Click Window` packet exists for any of it. Changes to the player's
//! slots are reported with `Creative Inventory Action`
//! ([`CreativeAction`]); the cursor is client-only and so are the grid
//! copies.

use umicraft_protocol::packets::play::ItemStack;

use super::action::{slot_changes, CreativeAction};
use super::state::CreativeScreen;
use super::target::CreativeSlot;
use crate::inventory::click::{drag_sequence, ClickRequest, DragButton};
use crate::inventory::stack::{count, max_stack, with_count};
use crate::inventory::{WindowManager, PLAYER_SLOT_COUNT};

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    /// Button 0.
    Left,
    /// Button 1.
    Right,
    /// Button 2 (the pick-block key): clones a stack.
    Middle,
}

impl MouseButton {
    fn wire(self) -> i8 {
        match self {
            Self::Left => 0,
            Self::Right => 1,
            Self::Middle => 2,
        }
    }
}

/// `ItemStack.isItemEqual`: same item and damage, the NBT is ignored.
fn same_kind(a: &ItemStack, b: &ItemStack) -> bool {
    a.item_id == b.item_id && a.damage == b.damage
}

impl CreativeScreen {
    /// Handles a press-and-release on `target` (`None` is outside the
    /// window), with the shift key state.
    ///
    /// Returns the `Creative Inventory Action`s to send, in order; the
    /// [`WindowManager`] is already updated. A press that starts a drag
    /// split must go to [`drag_split`](Self::drag_split) instead.
    ///
    /// | Where | Cursor empty | Cursor holds a stack |
    /// |---|---|---|
    /// | grid | left/right: copy of one item; shift: a full stack; middle: a full stack | same item: left +1, shift full, right -1; other item or empty cell: delete |
    /// | hotbar (item tab) | normal click; shift deletes; middle clones | normal click |
    /// | outside (item tab) | nothing | left drops all, right drops one |
    /// | player slot (inventory tab) | normal window click, shift transfers | normal window click; outside drops it all |
    /// | trash | nothing | deletes the cursor; shift-click anywhere on it clears the whole inventory |
    pub fn click(
        &mut self,
        window: &mut WindowManager,
        target: Option<CreativeSlot>,
        button: MouseButton,
        shift: bool,
    ) -> Vec<CreativeAction> {
        self.restart_search_on_key = true;
        let shift = shift && target.is_some();
        match target {
            None if !self.tab.shows_player_inventory() => outside_drop(window, button),
            Some(CreativeSlot::Trash) if shift => clear_inventory(window),
            _ if self.tab.shows_player_inventory() => {
                self.inventory_tab_click(window, target, button, shift)
            }
            Some(CreativeSlot::Grid(cell)) => self.grid_click(window, cell, button, shift),
            Some(CreativeSlot::Hotbar(index)) => hotbar_click(window, index, button, shift),
            _ => Vec::new(),
        }
    }

    /// Finishes a drag split over `slots` with the given button
    /// (`Container` drag modes 0 to 2 sent as mode 5).
    ///
    /// Only the hotbar (item tabs) and the player slots (inventory tab) can
    /// take part; other slots in the list are ignored. In an item tab the
    /// nine hotbar slots are reported afterwards, as vanilla does.
    pub fn drag_split(
        &mut self,
        window: &mut WindowManager,
        button: MouseButton,
        slots: &[CreativeSlot],
    ) -> Vec<CreativeAction> {
        self.restart_search_on_key = true;
        let wanted: Vec<i16> = slots
            .iter()
            .filter_map(|slot| slot.window_slot())
            .map(|slot| slot as i16)
            .collect();
        let targets = window.drag_targets(&wanted);
        let drag_button = match button {
            MouseButton::Left => DragButton::Left,
            MouseButton::Right => DragButton::Right,
            MouseButton::Middle => DragButton::Middle,
        };
        let before = window.inventory().all_slots();
        for request in drag_sequence(&targets, drag_button) {
            window.click_local(request, true);
        }
        let after = window.inventory().all_slots();
        let mut actions = slot_changes(&before, &after, None);
        if !self.tab.shows_player_inventory() {
            actions = hotbar_actions(&after);
        }
        actions
    }

    /// Handles a double click on `target`, which collects matching stacks
    /// onto the cursor. Only the hotbar slots of the inventory tab take part
    /// (`ContainerCreative.canMergeSlot` is limited to the lowest row).
    pub fn double_click(
        &mut self,
        window: &mut WindowManager,
        target: CreativeSlot,
    ) -> Vec<CreativeAction> {
        self.restart_search_on_key = true;
        match target {
            CreativeSlot::Player(slot) if self.tab.shows_player_inventory() && slot >= 36 => {
                apply(window, ClickRequest::double(slot as i16), None)
            }
            _ => Vec::new(),
        }
    }

    /// A click in the inventory tab: the normal player-window rules.
    fn inventory_tab_click(
        &mut self,
        window: &mut WindowManager,
        target: Option<CreativeSlot>,
        button: MouseButton,
        shift: bool,
    ) -> Vec<CreativeAction> {
        match target {
            Some(CreativeSlot::Trash) => {
                window.set_cursor(None);
                Vec::new()
            }
            None if button != MouseButton::Middle => drop_whole_cursor(window),
            None => Vec::new(),
            Some(slot) => match slot.window_slot() {
                Some(index) => apply(window, request(index, button, shift), None),
                None => Vec::new(),
            },
        }
    }

    /// A click on the item grid: copies, grows or deletes the cursor stack.
    fn grid_click(
        &mut self,
        window: &mut WindowManager,
        cell: usize,
        button: MouseButton,
        shift: bool,
    ) -> Vec<CreativeAction> {
        let listed = self.grid_stack(cell).cloned();
        let held = window.cursor().cloned();
        if button == MouseButton::Middle {
            if let (None, Some(item)) = (&held, &listed) {
                window.set_cursor(Some(with_count(item, max_stack(item))));
            }
            return Vec::new();
        }
        let next = match (held, listed) {
            (Some(mut held), Some(item)) if same_kind(&held, &item) => {
                resize_cursor(&mut held, button, shift);
                Some(held)
            }
            (None, Some(item)) => {
                let size = if shift {
                    max_stack(&item)
                } else {
                    count(&item)
                };
                Some(with_count(&item, size))
            }
            _ => None,
        };
        window.set_cursor(next);
        Vec::new()
    }
}

/// Left grows the cursor stack by one (shift: to the limit), right shrinks it
/// by one.
fn resize_cursor(held: &mut ItemStack, button: MouseButton, shift: bool) {
    let size = count(held);
    let limit = max_stack(held);
    let new_size = match (button, shift) {
        (MouseButton::Left, true) => limit,
        (MouseButton::Left, false) if size < limit => size + 1,
        (MouseButton::Left, false) => size,
        _ => size - 1,
    };
    *held = with_count(held, new_size);
}

/// A click outside the window of an item tab: drops the cursor stack (left)
/// or a single item of it (right).
fn outside_drop(window: &mut WindowManager, button: MouseButton) -> Vec<CreativeAction> {
    let Some(mut held) = window.cursor().cloned() else {
        return Vec::new();
    };
    match button {
        MouseButton::Left => {
            window.set_cursor(None);
            vec![CreativeAction::drop_stack(held)]
        }
        MouseButton::Right => {
            let one = with_count(&held, 1);
            held = with_count(&held, count(&held) - 1);
            window.set_cursor(Some(held));
            vec![CreativeAction::drop_stack(one)]
        }
        MouseButton::Middle => Vec::new(),
    }
}

/// The drop in the inventory tab ignores the button: the whole cursor stack goes.
fn drop_whole_cursor(window: &mut WindowManager) -> Vec<CreativeAction> {
    match window.cursor().cloned() {
        Some(held) => {
            window.set_cursor(None);
            vec![CreativeAction::drop_stack(held)]
        }
        None => Vec::new(),
    }
}

/// Shift-click on the trash: every slot of the player window is emptied.
fn clear_inventory(window: &mut WindowManager) -> Vec<CreativeAction> {
    for slot in 0..PLAYER_SLOT_COUNT {
        window.inventory_mut().set_slot(slot, None);
    }
    (0..PLAYER_SLOT_COUNT)
        .map(|slot| CreativeAction::set_slot(slot, None))
        .collect()
}

/// A click on a hotbar slot of an item tab: the player's real slot, but shift
/// deletes the stack instead of moving it.
fn hotbar_click(
    window: &mut WindowManager,
    index: usize,
    button: MouseButton,
    shift: bool,
) -> Vec<CreativeAction> {
    let Some(slot) = CreativeSlot::Hotbar(index).window_slot() else {
        return Vec::new();
    };
    if shift {
        window.inventory_mut().set_slot(slot, None);
        return vec![CreativeAction::set_slot(slot, None)];
    }
    apply(window, request(slot, button, false), Some(slot))
}

/// The window request of a normal, shift or middle click on a window slot.
fn request(slot: usize, button: MouseButton, shift: bool) -> ClickRequest {
    let slot = slot as i16;
    match (button, shift) {
        (MouseButton::Middle, _) => ClickRequest::middle(slot),
        (button, true) => ClickRequest::shift(slot, button.wire()),
        (MouseButton::Left, false) => ClickRequest::left(slot),
        (_, false) => ClickRequest::right(slot),
    }
}

/// Runs a request on the local window and reports the slots it changed
/// (`detectAndSendChanges`), plus `always` even when unchanged.
pub(super) fn apply(
    window: &mut WindowManager,
    request: ClickRequest,
    always: Option<usize>,
) -> Vec<CreativeAction> {
    let before = window.inventory().all_slots();
    window.click_local(request, true);
    slot_changes(&before, &window.inventory().all_slots(), always)
}

/// The nine hotbar slots as actions (`getDragEvent == 2` resends them all).
fn hotbar_actions(slots: &[Option<ItemStack>; PLAYER_SLOT_COUNT]) -> Vec<CreativeAction> {
    (crate::inventory::HOTBAR_START..PLAYER_SLOT_COUNT)
        .map(|slot| CreativeAction::set_slot(slot, slots[slot].as_ref()))
        .collect()
}
