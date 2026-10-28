// SPDX-License-Identifier: MIT OR Apache-2.0

//! [`WindowManager`]: the single source of truth for the local player's items.

use umicraft_protocol::packets::play::ItemStack;

use super::armor;
use super::click::{slot_click, ClickRequest, DragState};
use super::kind::{
    resolve_container_slot, SlotTarget, WindowKind, HOTBAR_LEN, HOTBAR_START, MAIN_INV_LEN,
    PLAYER_SLOT_COUNT,
};
use super::player::{main_index_to_slot, PlayerInventory};
use super::slot::{container_window_kinds, player_window_kinds, SlotKind};
use super::space::ClickSpace;
use super::stack::{from_wire, to_wire};
use super::transactions::{Confirmation, Snapshot, Transactions};
use super::view::{ContainerView, WindowView};

/// The first armour slot of the player window.
const HELMET_SLOT: usize = 5;
/// The last armour slot of the player window.
const BOOTS_SLOT: usize = 8;

/// A server-opened container window.
#[derive(Debug, Clone)]
pub struct ContainerWindow {
    /// The window id assigned by the server.
    pub window_id: u8,
    /// The kind of container.
    pub kind: WindowKind,
    /// The title as JSON chat text.
    pub title: String,
    /// The container's own slots (not the player inventory portion).
    pub container_slots: Vec<Option<ItemStack>>,
    /// Properties indexed by property id (e.g. furnace progress).
    pub properties: Vec<i16>,
    kinds: Vec<SlotKind>,
}

/// The fields of the `Click Window` packet for a predicted click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickCommand {
    /// The window id (0 for the player inventory).
    pub window_id: i8,
    /// The clicked slot, or −999 outside the window.
    pub slot: i16,
    /// The button.
    pub button: i8,
    /// The click mode.
    pub mode: i8,
    /// The transaction number the server echoes in `Confirm Transaction`.
    pub action_number: i16,
    /// The stack the click acted on ([`ItemStack::EMPTY`] when none).
    pub clicked_item: ItemStack,
}

/// Manages the player inventory, an optional open container, and the cursor.
#[derive(Debug, Clone, Default)]
pub struct WindowManager {
    inventory: PlayerInventory,
    container: Option<ContainerWindow>,
    cursor: Option<ItemStack>,
    drag: DragState,
    transactions: Transactions,
    action_counter: i16,
    selected_slot: u8,
}

impl WindowManager {
    /// Creates a new window manager with an empty inventory.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bulk-updates slots from a `Window Items` packet.
    ///
    /// The server's list replaces the window, so the predicted clicks still
    /// waiting for a confirmation are forgotten.
    pub fn apply_window_items(&mut self, window_id: u8, items: &[ItemStack]) {
        if window_id == 0 {
            for (i, item) in items.iter().enumerate().take(PLAYER_SLOT_COUNT) {
                self.inventory.set_slot(i, from_wire(item.clone()));
            }
        } else if let Some(container) = self.container.as_mut().filter(|c| c.window_id == window_id)
        {
            let own = container.container_slots.len();
            for (i, item) in items
                .iter()
                .enumerate()
                .take(own + MAIN_INV_LEN + HOTBAR_LEN)
            {
                let value = from_wire(item.clone());
                match resolve_container_slot(i, own) {
                    Some(SlotTarget::Container(idx)) => container.container_slots[idx] = value,
                    Some(SlotTarget::Player(idx)) => {
                        self.inventory.set_slot(idx, value);
                    }
                    None => {}
                }
            }
        } else {
            return;
        }
        self.transactions.clear();
    }

    /// Updates a single slot from a `Set Slot` packet.
    ///
    /// `window_id == -1` and `slot == -1` is the cursor item.
    pub fn apply_set_slot(&mut self, window_id: i8, slot: i16, item: ItemStack) {
        let value = from_wire(item);
        if window_id == -1 && slot == -1 {
            self.cursor = value;
            return;
        }
        let Ok(slot) = usize::try_from(slot) else {
            return;
        };
        if window_id == 0 {
            self.inventory.set_slot(slot, value);
        } else if let Some(container) = self
            .container
            .as_mut()
            .filter(|c| i16::from(c.window_id) == i16::from(window_id))
        {
            match resolve_container_slot(slot, container.container_slots.len()) {
                Some(SlotTarget::Container(idx)) => container.container_slots[idx] = value,
                Some(SlotTarget::Player(idx)) => {
                    self.inventory.set_slot(idx, value);
                }
                None => {}
            }
        }
    }

    /// Opens a new container window, replacing any previous one.
    pub fn open_window(&mut self, window_id: u8, kind: WindowKind, title: String, slot_count: u8) {
        let own = usize::from(slot_count);
        self.container = Some(ContainerWindow {
            window_id,
            kind,
            title,
            container_slots: vec![None; own],
            properties: Vec::new(),
            kinds: container_window_kinds(kind, own),
        });
        self.reset_transactions();
    }

    /// Closes a container window and clears the cursor.
    pub fn close_window(&mut self, window_id: u8) {
        let matches = self
            .container
            .as_ref()
            .is_none_or(|c| c.window_id == window_id);
        if matches || window_id == 0 {
            self.container = None;
            self.cursor = None;
            self.reset_transactions();
        }
    }

    fn reset_transactions(&mut self) {
        self.action_counter = 0;
        self.transactions.clear();
        self.drag.reset();
    }

    /// Updates a window property (e.g. furnace progress, enchanting level).
    pub fn apply_property(&mut self, window_id: u8, property: i16, value: i16) {
        let Some(container) = self.container.as_mut().filter(|c| c.window_id == window_id) else {
            return;
        };
        let Ok(idx) = usize::try_from(property) else {
            return;
        };
        if idx >= container.properties.len() {
            container.properties.resize(idx + 1, 0);
        }
        container.properties[idx] = value;
    }

    /// Returns the next action number for a `Click Window` packet
    /// (`Container.getNextTransactionID`: the first one is 1).
    ///
    /// Wraps around at `i16::MAX`, staying positive.
    pub fn next_action_number(&mut self) -> i16 {
        self.action_counter = self.action_counter.checked_add(1).unwrap_or(1);
        self.action_counter
    }

    /// Predicts a click locally and returns the packet to send.
    ///
    /// `creative` is the player's game mode (it unlocks the middle click and
    /// the middle drag).
    pub fn click(&mut self, request: ClickRequest, creative: bool) -> ClickCommand {
        let window_id = self.container.as_ref().map_or(0, |c| c.window_id);
        let action_number = self.next_action_number();
        self.transactions
            .record(window_id, action_number, self.snapshot());
        let clicked = self.apply_click(request, creative);
        ClickCommand {
            window_id: window_id as i8,
            slot: request.slot,
            button: request.button,
            mode: request.mode as i8,
            action_number,
            clicked_item: to_wire(clicked.as_ref()),
        }
    }

    fn apply_click(&mut self, request: ClickRequest, creative: bool) -> Option<ItemStack> {
        let Self {
            inventory,
            container,
            cursor,
            drag,
            ..
        } = self;
        match container {
            Some(window) => {
                let mut space = ClickSpace::container_window(
                    inventory,
                    &mut window.container_slots,
                    &window.kinds,
                );
                slot_click(&mut space, cursor, drag, request, creative)
            }
            None => {
                let kinds = player_window_kinds();
                let mut space = ClickSpace::player_window(inventory, &kinds);
                slot_click(&mut space, cursor, drag, request, creative)
            }
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            inventory: self.inventory.clone(),
            own: self
                .container
                .as_ref()
                .map(|c| c.container_slots.clone())
                .unwrap_or_default(),
            cursor: self.cursor.clone(),
        }
    }

    /// Applies a `Confirm Transaction`: an accepted click is forgotten, a
    /// rejected one is rolled back to the state before it.
    pub fn confirm_transaction(
        &mut self,
        window_id: u8,
        action_number: i16,
        accepted: bool,
    ) -> Confirmation {
        let confirmation = self
            .transactions
            .confirm(window_id, action_number, accepted);
        if let Confirmation::Rejected(before) = &confirmation {
            self.inventory = before.inventory.clone();
            if let Some(container) = self.container.as_mut() {
                container.container_slots.clone_from(&before.own);
            }
            self.cursor.clone_from(&before.cursor);
            self.drag.reset();
        }
        confirmation
    }

    /// Returns the 9 hotbar items.
    #[must_use]
    pub fn hotbar(&self) -> [Option<ItemStack>; HOTBAR_LEN] {
        self.inventory.hotbar()
    }

    /// Returns the item in the selected hotbar slot.
    #[must_use]
    pub fn selected_item(&self) -> Option<&ItemStack> {
        self.inventory
            .slot(HOTBAR_START + usize::from(self.selected_slot))
    }

    /// The helmet in the armour slot (slot 5 of the player window).
    #[must_use]
    pub fn helmet(&self) -> Option<&ItemStack> {
        self.inventory.slot(HELMET_SLOT)
    }

    /// The boots in the armour slot (slot 8 of the player window).
    #[must_use]
    pub fn boots(&self) -> Option<&ItemStack> {
        self.inventory.slot(BOOTS_SLOT)
    }

    /// The armour bar value, read from the four armour slots.
    #[must_use]
    pub fn armor_value(&self) -> u8 {
        armor::total_armor((HELMET_SLOT..=BOOTS_SLOT).map(|slot| self.inventory.slot(slot)))
    }

    /// The 36 slots of the main inventory in vanilla order: the hotbar first,
    /// then the three rows of the backpack (MCP `InventoryPlayer.mainInventory`).
    #[must_use]
    pub fn main_inventory(&self) -> [Option<ItemStack>; 36] {
        std::array::from_fn(|index| self.inventory.slot(main_index_to_slot(index)).cloned())
    }

    /// Sets the selected hotbar slot (0–8).
    pub fn set_selected_slot(&mut self, slot: u8) {
        self.selected_slot = slot.min(8);
    }

    /// Returns the currently selected hotbar slot index (0–8).
    #[must_use]
    pub fn selected_slot(&self) -> u8 {
        self.selected_slot
    }

    /// Returns a reference to the player inventory.
    #[must_use]
    pub fn inventory(&self) -> &PlayerInventory {
        &self.inventory
    }

    /// Mutable access to the player inventory (tests and creative actions).
    pub fn inventory_mut(&mut self) -> &mut PlayerInventory {
        &mut self.inventory
    }

    /// Returns a reference to the open container, if any.
    #[must_use]
    pub fn container(&self) -> Option<&ContainerWindow> {
        self.container.as_ref()
    }

    /// Returns the cursor item.
    #[must_use]
    pub fn cursor(&self) -> Option<&ItemStack> {
        self.cursor.as_ref()
    }

    /// Number of predicted clicks waiting for the server.
    #[must_use]
    pub fn pending_clicks(&self) -> usize {
        self.transactions.len()
    }

    /// Produces an immutable snapshot for the render thread.
    #[must_use]
    pub fn view(&self) -> WindowView {
        WindowView {
            player_slots: self.inventory.all_slots(),
            container: self.container.as_ref().map(|c| ContainerView {
                window_id: c.window_id,
                kind: c.kind,
                title: c.title.clone(),
                slots: c.container_slots.clone(),
                properties: c.properties.clone(),
            }),
            cursor: self.cursor.clone(),
            selected_slot: self.selected_slot,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::stack::fixtures::stack;
    use super::*;

    fn diamond_sword() -> ItemStack {
        stack(276, 1)
    }

    #[test]
    fn apply_window_items_populates_the_player_inventory() {
        let mut wm = WindowManager::new();
        let mut items = vec![ItemStack::EMPTY; 45];
        items[36] = stack(1, 64);
        items[44] = diamond_sword();
        wm.apply_window_items(0, &items);
        assert_eq!(wm.hotbar()[0], Some(stack(1, 64)));
        assert_eq!(wm.hotbar()[8], Some(diamond_sword()));
        assert_eq!(wm.inventory().slot(0), None);
    }

    #[test]
    fn set_slot_updates_the_cursor_and_the_hotbar() {
        let mut wm = WindowManager::new();
        wm.apply_set_slot(0, 36, stack(1, 64));
        assert_eq!(wm.hotbar()[0], Some(stack(1, 64)));
        wm.apply_set_slot(-1, -1, stack(1, 2));
        assert_eq!(wm.cursor(), Some(&stack(1, 2)));
        wm.apply_set_slot(-1, -1, ItemStack::EMPTY);
        assert_eq!(wm.cursor(), None);
    }

    #[test]
    fn open_and_close_window_lifecycle() {
        let mut wm = WindowManager::new();
        wm.open_window(1, WindowKind::Chest, "Chest".to_string(), 27);
        assert_eq!(wm.container().map(|c| c.container_slots.len()), Some(27));
        wm.cursor = Some(stack(1, 1));
        wm.close_window(1);
        assert!(wm.container().is_none());
        assert!(wm.cursor().is_none());
    }

    #[test]
    fn container_windows_map_both_storages() {
        let mut wm = WindowManager::new();
        wm.open_window(1, WindowKind::Chest, "Chest".to_string(), 27);
        let mut items = vec![ItemStack::EMPTY; 63];
        items[0] = stack(1, 64);
        items[27] = diamond_sword();
        items[54] = stack(1, 5);
        wm.apply_window_items(1, &items);
        assert_eq!(
            wm.container().unwrap().container_slots[0],
            Some(stack(1, 64))
        );
        assert_eq!(wm.inventory().slot(9), Some(&diamond_sword()));
        assert_eq!(wm.hotbar()[0], Some(stack(1, 5)));
        wm.apply_set_slot(1, 54, diamond_sword());
        assert_eq!(wm.hotbar()[0], Some(diamond_sword()));
    }

    #[test]
    fn packets_for_another_window_are_ignored() {
        let mut wm = WindowManager::new();
        wm.open_window(1, WindowKind::Chest, "Chest".to_string(), 27);
        wm.apply_set_slot(2, 0, stack(1, 1));
        wm.apply_window_items(2, &[stack(1, 1)]);
        wm.apply_property(2, 0, 100);
        let container = wm.container().unwrap();
        assert_eq!(container.container_slots[0], None);
        assert!(container.properties.is_empty());
    }

    #[test]
    fn properties_grow_on_demand() {
        let mut wm = WindowManager::new();
        wm.open_window(1, WindowKind::Furnace, "Furnace".to_string(), 3);
        wm.apply_property(1, 2, 100);
        assert_eq!(wm.container().unwrap().properties, vec![0, 0, 100]);
    }

    #[test]
    fn action_numbers_start_at_one_and_restart_with_each_window() {
        let mut wm = WindowManager::new();
        assert_eq!(wm.next_action_number(), 1);
        assert_eq!(wm.next_action_number(), 2);
        wm.open_window(1, WindowKind::Chest, String::new(), 27);
        assert_eq!(wm.next_action_number(), 1);
    }

    #[test]
    fn action_numbers_wrap_to_one() {
        let mut wm = WindowManager::new();
        wm.action_counter = i16::MAX;
        assert_eq!(wm.next_action_number(), 1);
    }

    #[test]
    fn a_click_is_predicted_and_described_for_the_wire() {
        let mut wm = WindowManager::new();
        wm.open_window(3, WindowKind::Chest, String::new(), 27);
        wm.container.as_mut().unwrap().container_slots[2] = Some(stack(1, 9));
        let command = wm.click(ClickRequest::left(2), false);
        assert_eq!(wm.cursor(), Some(&stack(1, 9)));
        assert_eq!(
            command,
            ClickCommand {
                window_id: 3,
                slot: 2,
                button: 0,
                mode: 0,
                action_number: 1,
                clicked_item: stack(1, 9),
            }
        );
        assert_eq!(wm.pending_clicks(), 1);
    }

    #[test]
    fn a_rejected_click_is_rolled_back() {
        let mut wm = WindowManager::new();
        wm.apply_set_slot(0, 10, stack(1, 9));
        let command = wm.click(ClickRequest::left(10), false);
        assert!(wm.inventory().slot(10).is_none());
        let outcome = wm.confirm_transaction(0, command.action_number, false);
        assert!(matches!(outcome, Confirmation::Rejected(_)));
        assert_eq!(wm.inventory().slot(10), Some(&stack(1, 9)));
        assert!(wm.cursor().is_none());
        assert_eq!(wm.pending_clicks(), 0);
    }

    #[test]
    fn an_accepted_click_keeps_the_prediction() {
        let mut wm = WindowManager::new();
        wm.apply_set_slot(0, 10, stack(1, 9));
        let command = wm.click(ClickRequest::left(10), false);
        assert_eq!(
            wm.confirm_transaction(0, command.action_number, true),
            Confirmation::Accepted
        );
        assert_eq!(wm.cursor(), Some(&stack(1, 9)));
    }

    #[test]
    fn a_window_resync_forgets_the_pending_clicks() {
        let mut wm = WindowManager::new();
        wm.apply_set_slot(0, 10, stack(1, 9));
        wm.click(ClickRequest::left(10), false);
        wm.apply_window_items(0, &vec![ItemStack::EMPTY; 45]);
        assert_eq!(wm.pending_clicks(), 0);
    }

    #[test]
    fn the_armour_bar_reads_the_armour_slots() {
        let mut wm = WindowManager::new();
        wm.apply_set_slot(0, 6, stack(311, 1));
        wm.apply_set_slot(0, 7, stack(312, 1));
        assert_eq!(wm.armor_value(), 14);
    }

    #[test]
    fn selected_item_returns_the_hotbar_item() {
        let mut wm = WindowManager::new();
        wm.apply_set_slot(0, 38, diamond_sword());
        wm.set_selected_slot(2);
        assert_eq!(wm.selected_item(), Some(&diamond_sword()));
        wm.set_selected_slot(0);
        assert_eq!(wm.selected_item(), None);
    }

    #[test]
    fn the_view_captures_the_full_state() {
        let mut wm = WindowManager::new();
        wm.apply_set_slot(0, 36, stack(1, 1));
        wm.open_window(1, WindowKind::Chest, "Chest".to_string(), 27);
        wm.cursor = Some(diamond_sword());
        let view = wm.view();
        assert_eq!(view.player_slots[36], Some(stack(1, 1)));
        assert_eq!(view.cursor, Some(diamond_sword()));
        assert_eq!(
            view.container.map(|c| (c.window_id, c.slots.len())),
            Some((1, 27))
        );
    }
}
