// SPDX-License-Identifier: MIT OR Apache-2.0

//! Keyboard shortcuts over a hovered slot (MCP `GuiContainer.keyTyped` and
//! `checkHotbarKeys`, plus the creative overrides).

use super::action::CreativeAction;
use super::click::apply;
use super::state::CreativeScreen;
use super::target::CreativeSlot;
use crate::inventory::click::ClickRequest;
use crate::inventory::stack::{max_stack, split, with_count};
use crate::inventory::{WindowManager, HOTBAR_LEN, HOTBAR_START};

impl CreativeScreen {
    /// A number key (`key` 0 to 8, the hotbar slot) pressed over `hovered`.
    ///
    /// Returns `None` when vanilla does not treat it as a shortcut: the
    /// cursor holds a stack, nothing is hovered, or `key` is out of range
    /// (on the search tab the caller then types the digit). Otherwise the
    /// actions to send, possibly none:
    ///
    /// * grid: the hotbar slot gets a full stack of the hovered item;
    /// * hotbar slot: swapped with the hotbar slot `key`;
    /// * player slot (inventory tab): the normal number-key swap.
    pub fn number_key(
        &mut self,
        window: &mut WindowManager,
        hovered: Option<CreativeSlot>,
        key: usize,
    ) -> Option<Vec<CreativeAction>> {
        if key >= HOTBAR_LEN || window.cursor().is_some() {
            return None;
        }
        let hovered = hovered?;
        self.restart_search_if_pending();
        let actions = match hovered {
            CreativeSlot::Grid(cell) => self.fill_hotbar_slot(window, cell, key),
            CreativeSlot::Hotbar(index) => {
                let slot = HOTBAR_START + index;
                apply(
                    window,
                    ClickRequest::number_key(slot as i16, key as u8),
                    Some(slot),
                )
            }
            CreativeSlot::Player(slot) => apply(
                window,
                ClickRequest::number_key(slot as i16, key as u8),
                None,
            ),
            CreativeSlot::Trash => Vec::new(),
        };
        self.restart_search_on_key = true;
        Some(actions)
    }

    /// The drop key (Q) pressed over `hovered`; `whole_stack` is Ctrl + Q.
    /// On the search tab the key types into the search box instead, so the
    /// caller tries [`search_input`](Self::search_input) first.
    ///
    /// Does nothing over an empty slot. Otherwise:
    ///
    /// * grid: drops one item (or a full stack) of the hovered item;
    /// * player slot (inventory tab): drops one item (or the whole stack)
    ///   from the slot and reports the slot;
    /// * hotbar slot (item tab): vanilla only drops a client-side entity, so
    ///   the slot update is the whole effect.
    pub fn drop_key(
        &mut self,
        window: &mut WindowManager,
        hovered: Option<CreativeSlot>,
        whole_stack: bool,
    ) -> Vec<CreativeAction> {
        self.restart_search_on_key = true;
        match hovered {
            Some(CreativeSlot::Grid(cell)) => self.drop_listed(cell, whole_stack),
            Some(CreativeSlot::Player(slot)) => drop_from_slot(window, slot, whole_stack),
            Some(CreativeSlot::Hotbar(index)) => {
                let slot = HOTBAR_START + index;
                if window.inventory().slot(slot).is_none() {
                    return Vec::new();
                }
                apply(
                    window,
                    ClickRequest::drop_key(slot as i16, whole_stack),
                    Some(slot),
                )
            }
            _ => Vec::new(),
        }
    }

    /// Number key over the grid: a full stack of the listed item goes into
    /// hotbar slot `key`.
    fn fill_hotbar_slot(
        &self,
        window: &mut WindowManager,
        cell: usize,
        key: usize,
    ) -> Vec<CreativeAction> {
        let Some(item) = self.grid_stack(cell) else {
            return Vec::new();
        };
        let stack = with_count(item, max_stack(item));
        let slot = HOTBAR_START + key;
        window.inventory_mut().set_slot(slot, Some(stack.clone()));
        vec![CreativeAction::set_slot(slot, Some(&stack))]
    }

    /// Drop key over the grid: the dropped copy is 1 item, or a full stack.
    fn drop_listed(&self, cell: usize, whole_stack: bool) -> Vec<CreativeAction> {
        match self.grid_stack(cell) {
            Some(item) => {
                let size = if whole_stack { max_stack(item) } else { 1 };
                vec![CreativeAction::drop_stack(with_count(item, size))]
            }
            None => Vec::new(),
        }
    }
}

/// Drops from a player-window slot in the inventory tab: the dropped stack
/// first, then the slot that shrank.
fn drop_from_slot(
    window: &mut WindowManager,
    slot: usize,
    whole_stack: bool,
) -> Vec<CreativeAction> {
    let Some(mut stack) = window.inventory().slot(slot).cloned() else {
        return Vec::new();
    };
    let amount = if whole_stack { max_stack(&stack) } else { 1 };
    let dropped = split(&mut stack, amount);
    let mut actions = vec![CreativeAction::drop_stack(dropped)];
    actions.extend(apply(
        window,
        ClickRequest::drop_key(slot as i16, whole_stack),
        None,
    ));
    actions
}
