// SPDX-License-Identifier: MIT OR Apache-2.0

//! Inventory and container window model (M9, GAM-5).
//!
//! Manages the player's 45-slot inventory, an optional server-opened container
//! window, and the cursor item. The model is pure data — no I/O, no packets —
//! and produces [`WindowView`] snapshots for the render thread.
//!
//! ## Slot numbering
//!
//! The player inventory (window id 0) maps:
//! - 0: crafting output
//! - 1–4: crafting grid
//! - 5–8: armor (helmet, chest, legs, boots)
//! - 9–35: main inventory (3 rows of 9)
//! - 36–44: hotbar
//!
//! Container windows map container slots first (0..*N*), then the player's main
//! inventory (*N*..*N*+27), then the hotbar (*N*+27..*N*+36).

use umicraft_protocol::packets::play::ItemStack;

/// Number of slots in the player inventory.
pub const PLAYER_SLOT_COUNT: usize = 45;

/// First hotbar slot index in the player inventory.
pub const HOTBAR_START: usize = 36;

/// The window slot of the helmet.
const HELMET_SLOT: usize = 5;

/// The player window slot of the boots.
const BOOTS_SLOT: usize = 8;

/// Number of hotbar slots.
const HOTBAR_LEN: usize = 9;

/// Number of main inventory slots (excluding hotbar, armor, crafting).
const MAIN_INV_LEN: usize = 27;

/// First main inventory slot index.
const MAIN_INV_START: usize = 9;

/// The 1.8 container types the server can open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowKind {
    /// `minecraft:chest` — 27 or 54 slots.
    Chest,
    /// `minecraft:crafting_table` — 10 slots.
    CraftingTable,
    /// `minecraft:furnace` — 3 slots.
    Furnace,
    /// `minecraft:dispenser` — 9 slots.
    Dispenser,
    /// `minecraft:enchanting_table` — 2 slots.
    EnchantingTable,
    /// `minecraft:brewing_stand` — 5 slots.
    BrewingStand,
    /// `minecraft:villager` — 3 slots.
    Villager,
    /// `minecraft:beacon` — 1 slot.
    Beacon,
    /// `minecraft:anvil` — 3 slots.
    Anvil,
    /// `minecraft:hopper` — 5 slots.
    Hopper,
    /// `minecraft:dropper` — 9 slots.
    Dropper,
    /// `EntityHorse` — variable.
    Horse,
}

impl WindowKind {
    /// Parses the window type string sent by the server in `Open Window`.
    ///
    /// Returns `None` for unknown types.
    #[must_use]
    pub fn from_type_string(s: &str) -> Option<Self> {
        Some(match s {
            "minecraft:chest" => Self::Chest,
            "minecraft:crafting_table" => Self::CraftingTable,
            "minecraft:furnace" => Self::Furnace,
            "minecraft:dispenser" => Self::Dispenser,
            "minecraft:enchanting_table" => Self::EnchantingTable,
            "minecraft:brewing_stand" => Self::BrewingStand,
            "minecraft:villager" => Self::Villager,
            "minecraft:beacon" => Self::Beacon,
            "minecraft:anvil" => Self::Anvil,
            "minecraft:hopper" => Self::Hopper,
            "minecraft:dropper" => Self::Dropper,
            "EntityHorse" => Self::Horse,
            _ => return None,
        })
    }
}

/// Where a window-relative slot index maps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotTarget {
    /// A slot in the container portion.
    Container(usize),
    /// A slot in the player inventory (absolute index 0–44).
    Player(usize),
}

/// Resolves a window-relative slot index for a container window.
///
/// Container windows lay out: container slots (0..*container_size*), then the
/// player's main inventory, then the hotbar.
///
/// Returns `None` if `slot` is out of range.
#[must_use]
pub fn resolve_container_slot(slot: i16, container_size: usize) -> Option<SlotTarget> {
    let slot = slot as usize;
    if slot < container_size {
        Some(SlotTarget::Container(slot))
    } else if slot < container_size + MAIN_INV_LEN {
        let offset = slot - container_size;
        Some(SlotTarget::Player(MAIN_INV_START + offset))
    } else if slot < container_size + MAIN_INV_LEN + HOTBAR_LEN {
        let offset = slot - container_size - MAIN_INV_LEN;
        Some(SlotTarget::Player(HOTBAR_START + offset))
    } else {
        None
    }
}

/// The player's own 45-slot inventory (window id 0).
#[derive(Debug, Clone)]
pub struct PlayerInventory {
    slots: [Option<ItemStack>; PLAYER_SLOT_COUNT],
}

impl Default for PlayerInventory {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayerInventory {
    /// Creates an empty player inventory.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
        }
    }

    /// Returns the item in the given slot, if any.
    #[must_use]
    pub fn slot(&self, index: usize) -> Option<&ItemStack> {
        self.slots.get(index).and_then(|s| s.as_ref())
    }

    /// Sets the item in the given slot.
    ///
    /// Returns `false` if `index` is out of range.
    pub fn set_slot(&mut self, index: usize, item: Option<ItemStack>) -> bool {
        if let Some(slot) = self.slots.get_mut(index) {
            *slot = item;
            true
        } else {
            false
        }
    }

    /// Returns the 9 hotbar slots.
    #[must_use]
    pub fn hotbar(&self) -> [Option<ItemStack>; HOTBAR_LEN] {
        let mut out = std::array::from_fn(|_| None);
        for (i, slot) in self.slots[HOTBAR_START..HOTBAR_START + HOTBAR_LEN]
            .iter()
            .enumerate()
        {
            out[i] = slot.clone();
        }
        out
    }

    /// Copies all 45 slots into a fixed-size array.
    #[must_use]
    pub fn all_slots(&self) -> [Option<ItemStack>; PLAYER_SLOT_COUNT] {
        self.slots.clone()
    }
}

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
}

/// An immutable snapshot of one container for the render thread.
#[derive(Debug, Clone, PartialEq)]
pub struct ContainerView {
    /// The kind of container.
    pub kind: WindowKind,
    /// The title as JSON chat text.
    pub title: String,
    /// The container's own slots.
    pub slots: Vec<Option<ItemStack>>,
    /// The properties.
    pub properties: Vec<i16>,
}

/// An immutable snapshot of the full window state for the render thread.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowView {
    /// All 45 player inventory slots.
    pub player_slots: [Option<ItemStack>; PLAYER_SLOT_COUNT],
    /// The open container, if any.
    pub container: Option<ContainerView>,
    /// The item held by the cursor (mid-drag).
    pub cursor: Option<ItemStack>,
    /// The currently selected hotbar slot (0–8).
    pub selected_slot: u8,
}

/// Manages the player inventory, an optional open container, and the cursor.
#[derive(Debug, Clone)]
pub struct WindowManager {
    inventory: PlayerInventory,
    container: Option<ContainerWindow>,
    cursor: Option<ItemStack>,
    action_counter: i16,
    selected_slot: u8,
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowManager {
    /// Creates a new window manager with an empty inventory.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inventory: PlayerInventory::new(),
            container: None,
            cursor: None,
            action_counter: 0,
            selected_slot: 0,
        }
    }

    /// Bulk-updates slots from a `Window Items` packet.
    ///
    /// For window id 0, the items fill the player inventory (up to 45 slots).
    /// For a container window, the items fill the container slots first, then
    /// the player's main inventory and hotbar.
    pub fn apply_window_items(&mut self, window_id: u8, items: &[ItemStack]) {
        if window_id == 0 {
            for (i, item) in items.iter().enumerate().take(PLAYER_SLOT_COUNT) {
                let value = if item.item_id == -1 {
                    None
                } else {
                    Some(item.clone())
                };
                self.inventory.set_slot(i, value);
            }
            return;
        }

        if let Some(container) = &mut self.container {
            if container.window_id != window_id {
                return;
            }
            let cs = container.container_slots.len();
            let total = cs + MAIN_INV_LEN + HOTBAR_LEN;
            for (i, item) in items.iter().enumerate().take(total) {
                let value = if item.item_id == -1 {
                    None
                } else {
                    Some(item.clone())
                };
                match resolve_container_slot(i as i16, cs) {
                    Some(SlotTarget::Container(idx)) => {
                        if let Some(slot) = container.container_slots.get_mut(idx) {
                            *slot = value;
                        }
                    }
                    Some(SlotTarget::Player(idx)) => {
                        self.inventory.set_slot(idx, value);
                    }
                    None => {}
                }
            }
        }
    }

    /// Updates a single slot from a `Set Slot` packet.
    ///
    /// Special case: `window_id == -1` and `slot == -1` means the cursor item.
    pub fn apply_set_slot(&mut self, window_id: i8, slot: i16, item: ItemStack) {
        let value = if item.item_id == -1 { None } else { Some(item) };

        // Cursor item.
        if window_id == -1 && slot == -1 {
            self.cursor = value;
            return;
        }

        if window_id == 0 {
            self.inventory.set_slot(slot as usize, value);
            return;
        }

        if let Some(container) = &mut self.container {
            if container.window_id != window_id as u8 {
                return;
            }
            let cs = container.container_slots.len();
            match resolve_container_slot(slot, cs) {
                Some(SlotTarget::Container(idx)) => {
                    if let Some(s) = container.container_slots.get_mut(idx) {
                        *s = value;
                    }
                }
                Some(SlotTarget::Player(idx)) => {
                    self.inventory.set_slot(idx, value);
                }
                None => {}
            }
        }
    }

    /// Opens a new container window.
    ///
    /// Any previously open container is silently replaced.
    pub fn open_window(&mut self, window_id: u8, kind: WindowKind, title: String, slot_count: u8) {
        self.container = Some(ContainerWindow {
            window_id,
            kind,
            title,
            container_slots: vec![None; slot_count as usize],
            properties: Vec::new(),
        });
    }

    /// Closes a container window, returning the cursor item to the inventory
    /// if present.
    ///
    /// If `window_id` matches the open container (or is 0), the container is
    /// dropped and the cursor is cleared.
    pub fn close_window(&mut self, window_id: u8) {
        let matches = match &self.container {
            Some(c) => c.window_id == window_id,
            None => window_id == 0,
        };
        if matches || window_id == 0 {
            self.container = None;
            self.cursor = None;
        }
    }

    /// Updates a window property (e.g. furnace progress, enchanting level).
    pub fn apply_property(&mut self, window_id: u8, property: i16, value: i16) {
        if let Some(container) = &mut self.container {
            if container.window_id != window_id {
                return;
            }
            let idx = property as usize;
            if idx >= container.properties.len() {
                container.properties.resize(idx + 1, 0);
            }
            container.properties[idx] = value;
        }
    }

    /// Returns the next action number for a `Click Window` packet.
    ///
    /// Wraps around at `i16::MAX`.
    pub fn next_action_number(&mut self) -> i16 {
        let n = self.action_counter;
        self.action_counter = self.action_counter.wrapping_add(1);
        if self.action_counter < 0 {
            self.action_counter = 1;
        }
        n
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
            .slot(HOTBAR_START + self.selected_slot as usize)
    }

    /// The helmet in the armor slot (slot 5 of the player window).
    #[must_use]
    pub fn helmet(&self) -> Option<&ItemStack> {
        self.inventory.slot(HELMET_SLOT)
    }

    /// The 36 slots of the main inventory in vanilla order: the hotbar first,
    /// then the three rows of the backpack (MCP `InventoryPlayer.mainInventory`).
    #[must_use]
    pub fn main_inventory(&self) -> [Option<ItemStack>; 36] {
        std::array::from_fn(|index| {
            let slot = if index < HOTBAR_LEN {
                HOTBAR_START + index
            } else {
                index
            };
            self.inventory.slot(slot).cloned()
        })
    }

    /// The boots in the armor slot (slot 8 of the player window).
    #[must_use]
    pub fn boots(&self) -> Option<&ItemStack> {
        self.inventory.slot(BOOTS_SLOT)
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

    /// Produces an immutable snapshot for the render thread.
    #[must_use]
    pub fn view(&self) -> WindowView {
        WindowView {
            player_slots: self.inventory.all_slots(),
            container: self.container.as_ref().map(|c| ContainerView {
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
    use super::*;

    fn stone() -> ItemStack {
        ItemStack {
            item_id: 1,
            stack_size: 64,
            damage: 0,
            nbt: None,
        }
    }

    fn diamond_sword() -> ItemStack {
        ItemStack {
            item_id: 276,
            stack_size: 1,
            damage: 0,
            nbt: None,
        }
    }

    #[test]
    fn apply_window_items_populates_the_player_inventory() {
        let mut wm = WindowManager::new();
        let mut items: Vec<ItemStack> = vec![ItemStack::EMPTY; 45];
        items[36] = stone();
        items[44] = diamond_sword();
        wm.apply_window_items(0, &items);
        assert_eq!(wm.hotbar()[0], Some(stone()));
        assert_eq!(wm.hotbar()[8], Some(diamond_sword()));
        assert_eq!(wm.inventory().slot(0), None);
    }

    #[test]
    fn apply_set_slot_on_the_hotbar_updates_hotbar() {
        let mut wm = WindowManager::new();
        wm.apply_set_slot(0, 36, stone());
        assert_eq!(wm.hotbar()[0], Some(stone()));
    }

    #[test]
    fn set_slot_cursor_item() {
        let mut wm = WindowManager::new();
        wm.apply_set_slot(-1, -1, stone());
        assert_eq!(wm.cursor(), Some(&stone()));

        wm.apply_set_slot(-1, -1, ItemStack::EMPTY);
        assert_eq!(wm.cursor(), None);
    }

    #[test]
    fn container_slot_mapping_for_a_chest() {
        // A single chest has 27 container slots.
        assert_eq!(
            resolve_container_slot(0, 27),
            Some(SlotTarget::Container(0))
        );
        assert_eq!(
            resolve_container_slot(26, 27),
            Some(SlotTarget::Container(26))
        );
        // Slot 27 maps to the start of the player's main inventory (slot 9).
        assert_eq!(
            resolve_container_slot(27, 27),
            Some(SlotTarget::Player(MAIN_INV_START))
        );
        // Slot 53 maps to the end of the main inventory.
        assert_eq!(
            resolve_container_slot(53, 27),
            Some(SlotTarget::Player(MAIN_INV_START + MAIN_INV_LEN - 1))
        );
        // Slot 54 maps to the start of the hotbar.
        assert_eq!(
            resolve_container_slot(54, 27),
            Some(SlotTarget::Player(HOTBAR_START))
        );
        // Slot 62 maps to the last hotbar slot.
        assert_eq!(
            resolve_container_slot(62, 27),
            Some(SlotTarget::Player(HOTBAR_START + HOTBAR_LEN - 1))
        );
        // Slot 63 is out of range.
        assert_eq!(resolve_container_slot(63, 27), None);
    }

    #[test]
    fn open_and_close_window_lifecycle() {
        let mut wm = WindowManager::new();
        assert!(wm.container().is_none());

        wm.open_window(1, WindowKind::Chest, "Chest".to_string(), 27);
        assert!(wm.container().is_some());
        assert_eq!(wm.container().unwrap().window_id, 1);
        assert_eq!(wm.container().unwrap().kind, WindowKind::Chest);
        assert_eq!(wm.container().unwrap().container_slots.len(), 27);

        wm.close_window(1);
        assert!(wm.container().is_none());
    }

    #[test]
    fn close_window_clears_the_cursor() {
        let mut wm = WindowManager::new();
        wm.open_window(1, WindowKind::Chest, "Chest".to_string(), 27);
        wm.cursor = Some(stone());
        wm.close_window(1);
        assert!(wm.cursor().is_none());
    }

    #[test]
    fn apply_window_items_fills_container_and_player_slots() {
        let mut wm = WindowManager::new();
        wm.open_window(1, WindowKind::Chest, "Chest".to_string(), 27);

        // 27 container slots + 27 main + 9 hotbar = 63 total.
        let mut items = vec![ItemStack::EMPTY; 63];
        items[0] = stone(); // container slot 0
        items[27] = diamond_sword(); // player main slot 9
        items[54] = stone(); // player hotbar slot 36

        wm.apply_window_items(1, &items);
        assert_eq!(wm.container().unwrap().container_slots[0], Some(stone()));
        assert_eq!(wm.inventory().slot(MAIN_INV_START), Some(&diamond_sword()));
        assert_eq!(wm.hotbar()[0], Some(stone()));
    }

    #[test]
    fn apply_set_slot_on_a_container_window() {
        let mut wm = WindowManager::new();
        wm.open_window(1, WindowKind::Chest, "Chest".to_string(), 27);
        wm.apply_set_slot(1, 0, stone());
        assert_eq!(wm.container().unwrap().container_slots[0], Some(stone()));

        // Slot in the player inventory portion of the container window.
        wm.apply_set_slot(1, 54, diamond_sword());
        assert_eq!(wm.hotbar()[0], Some(diamond_sword()));
    }

    #[test]
    fn apply_property_stores_the_value() {
        let mut wm = WindowManager::new();
        wm.open_window(1, WindowKind::Furnace, "Furnace".to_string(), 3);
        wm.apply_property(1, 0, 200);
        wm.apply_property(1, 2, 100);
        let props = &wm.container().unwrap().properties;
        assert_eq!(props[0], 200);
        assert_eq!(props[1], 0);
        assert_eq!(props[2], 100);
    }

    #[test]
    fn next_action_number_increments() {
        let mut wm = WindowManager::new();
        assert_eq!(wm.next_action_number(), 0);
        assert_eq!(wm.next_action_number(), 1);
        assert_eq!(wm.next_action_number(), 2);
    }

    #[test]
    fn next_action_number_wraps_positive() {
        let mut wm = WindowManager::new();
        wm.action_counter = i16::MAX;
        let n = wm.next_action_number();
        assert_eq!(n, i16::MAX);
        // After wrapping past i16::MAX, the counter resets to 1 (stays positive).
        assert_eq!(wm.next_action_number(), 1);
    }

    #[test]
    fn selected_item_returns_the_hotbar_item() {
        let mut wm = WindowManager::new();
        wm.apply_set_slot(0, 38, diamond_sword()); // hotbar slot 2
        wm.set_selected_slot(2);
        assert_eq!(wm.selected_item(), Some(&diamond_sword()));
        wm.set_selected_slot(0);
        assert_eq!(wm.selected_item(), None);
    }

    #[test]
    fn view_snapshot_captures_the_full_state() {
        let mut wm = WindowManager::new();
        wm.apply_set_slot(0, 36, stone());
        wm.set_selected_slot(0);
        wm.open_window(1, WindowKind::Chest, "Chest".to_string(), 27);
        wm.cursor = Some(diamond_sword());

        let view = wm.view();
        assert_eq!(view.player_slots[36], Some(stone()));
        assert_eq!(view.selected_slot, 0);
        assert_eq!(view.cursor, Some(diamond_sword()));
        assert!(view.container.is_some());
        let cv = view.container.unwrap();
        assert_eq!(cv.kind, WindowKind::Chest);
        assert_eq!(cv.slots.len(), 27);
    }

    #[test]
    fn window_kind_parses_all_known_types() {
        assert_eq!(
            WindowKind::from_type_string("minecraft:chest"),
            Some(WindowKind::Chest)
        );
        assert_eq!(
            WindowKind::from_type_string("minecraft:furnace"),
            Some(WindowKind::Furnace)
        );
        assert_eq!(
            WindowKind::from_type_string("EntityHorse"),
            Some(WindowKind::Horse)
        );
        assert_eq!(WindowKind::from_type_string("unknown:thing"), None);
    }

    #[test]
    fn mismatched_window_id_is_ignored() {
        let mut wm = WindowManager::new();
        wm.open_window(1, WindowKind::Chest, "Chest".to_string(), 27);
        // Set slot on a different window id: should be ignored.
        wm.apply_set_slot(2, 0, stone());
        assert_eq!(wm.container().unwrap().container_slots[0], None);

        // Window items on a different window id: ignored.
        wm.apply_window_items(2, &[stone()]);
        assert_eq!(wm.container().unwrap().container_slots[0], None);

        // Property on a different window id: ignored.
        wm.apply_property(2, 0, 100);
        assert!(wm.container().unwrap().properties.is_empty());
    }
}
