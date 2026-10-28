// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the mouse can point at in the creative window, and where it is.
//!
//! Positions are relative to the window's top-left corner and give the
//! top-left of the 16 by 16 item area (`Slot.xDisplayPosition`,
//! `yDisplayPosition` in MCP).

use super::scroll::{COLUMNS, GRID_CELLS};
use super::tabs::CreativeTab;
use crate::inventory::{HOTBAR_LEN, HOTBAR_START, PLAYER_SLOT_COUNT};

/// Size of the area in which a slot reacts to the mouse (16 pixels plus a
/// one pixel margin on every side, `GuiContainer.isPointInRegion`).
const HIT_MARGIN: i32 = 1;
/// Side of the item area of a slot.
const SLOT_SIZE: i32 = 16;
/// Where the trash slot sits (`new Slot(field_147060_v, 0, 173, 112)`).
const TRASH_POSITION: (i32, i32) = (173, 112);
/// Row of the hotbar slots in every tab.
const HOTBAR_Y: i32 = 112;

/// A slot of the creative window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CreativeSlot {
    /// A cell of the item grid, `row * 9 + column`, 0 to 44 (item tabs only).
    Grid(usize),
    /// A hotbar slot, 0 to 8 (item tabs only).
    Hotbar(usize),
    /// A slot of the player window, 0 to 44 (inventory tab only); the
    /// crafting slots 0 to 4 are not reachable.
    Player(usize),
    /// The delete slot (inventory tab only).
    Trash,
}

impl CreativeSlot {
    /// The top-left of the slot's item area (`setCurrentCreativeTab` and
    /// `ContainerCreative`), `None` for a slot that is not on screen
    /// (the crafting slots) or out of range.
    #[must_use]
    pub fn position(self) -> Option<(i32, i32)> {
        match self {
            Self::Grid(cell) if cell < GRID_CELLS => Some((
                9 + (cell % COLUMNS) as i32 * 18,
                18 + (cell / COLUMNS) as i32 * 18,
            )),
            Self::Hotbar(index) if index < HOTBAR_LEN => Some((9 + index as i32 * 18, HOTBAR_Y)),
            Self::Player(slot @ 5..=8) => {
                let offset = slot - 5;
                Some((9 + (offset / 2) as i32 * 54, 6 + (offset % 2) as i32 * 27))
            }
            Self::Player(slot @ 9..=44) => {
                let index = slot - 9;
                let x = 9 + (index % 9) as i32 * 18;
                let y = if slot >= HOTBAR_START {
                    HOTBAR_Y
                } else {
                    54 + (index / 9) as i32 * 18
                };
                Some((x, y))
            }
            Self::Trash => Some(TRASH_POSITION),
            _ => None,
        }
    }

    /// The slot as an index of the player window, when it is one: a hotbar
    /// slot is window slot `36 + index`.
    #[must_use]
    pub fn window_slot(self) -> Option<usize> {
        match self {
            Self::Hotbar(index) if index < HOTBAR_LEN => Some(HOTBAR_START + index),
            Self::Player(slot) if slot < PLAYER_SLOT_COUNT => Some(slot),
            _ => None,
        }
    }
}

/// The slots a tab shows, in the order the mouse tests them
/// (`getSlotAtPosition` returns the first hit).
#[must_use]
pub fn visible_slots(tab: CreativeTab) -> Vec<CreativeSlot> {
    if tab.shows_player_inventory() {
        (5..PLAYER_SLOT_COUNT)
            .map(CreativeSlot::Player)
            .chain([CreativeSlot::Trash])
            .collect()
    } else {
        (0..GRID_CELLS)
            .map(CreativeSlot::Grid)
            .chain((0..HOTBAR_LEN).map(CreativeSlot::Hotbar))
            .collect()
    }
}

/// The slot under a point relative to the window, `None` over no slot.
#[must_use]
pub fn slot_at(tab: CreativeTab, x: i32, y: i32) -> Option<CreativeSlot> {
    visible_slots(tab).into_iter().find(|slot| {
        slot.position().is_some_and(|(left, top)| {
            (left - HIT_MARGIN..left + SLOT_SIZE + HIT_MARGIN).contains(&x)
                && (top - HIT_MARGIN..top + SLOT_SIZE + HIT_MARGIN).contains(&y)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_and_hotbar_positions_follow_the_reference_layout() {
        assert_eq!(CreativeSlot::Grid(0).position(), Some((9, 18)));
        assert_eq!(CreativeSlot::Grid(44).position(), Some((153, 90)));
        assert_eq!(CreativeSlot::Hotbar(8).position(), Some((153, 112)));
        assert_eq!(CreativeSlot::Grid(45).position(), None);
    }

    #[test]
    fn inventory_tab_positions_place_armour_main_and_hotbar() {
        assert_eq!(CreativeSlot::Player(5).position(), Some((9, 6)));
        assert_eq!(CreativeSlot::Player(6).position(), Some((9, 33)));
        assert_eq!(CreativeSlot::Player(7).position(), Some((63, 6)));
        assert_eq!(CreativeSlot::Player(9).position(), Some((9, 54)));
        assert_eq!(CreativeSlot::Player(35).position(), Some((153, 90)));
        assert_eq!(CreativeSlot::Player(36).position(), Some((9, 112)));
        assert_eq!(CreativeSlot::Player(2).position(), None);
        assert_eq!(CreativeSlot::Trash.position(), Some((173, 112)));
    }

    #[test]
    fn window_slots_map_the_hotbar_onto_the_player_window() {
        assert_eq!(CreativeSlot::Hotbar(3).window_slot(), Some(39));
        assert_eq!(CreativeSlot::Player(20).window_slot(), Some(20));
        assert_eq!(CreativeSlot::Grid(3).window_slot(), None);
        assert_eq!(CreativeSlot::Trash.window_slot(), None);
    }

    #[test]
    fn slot_at_hits_with_a_one_pixel_margin() {
        let tab = CreativeTab::BuildingBlocks;
        assert_eq!(slot_at(tab, 9, 18), Some(CreativeSlot::Grid(0)));
        assert_eq!(slot_at(tab, 8, 17), Some(CreativeSlot::Grid(0)));
        assert_eq!(slot_at(tab, 25, 33), Some(CreativeSlot::Grid(0)));
        assert_eq!(slot_at(tab, 9, 112), Some(CreativeSlot::Hotbar(0)));
        assert_eq!(slot_at(tab, 0, 0), None);
    }

    #[test]
    fn the_inventory_tab_has_the_trash_but_no_grid() {
        let tab = CreativeTab::Inventory;
        assert_eq!(slot_at(tab, 173, 112), Some(CreativeSlot::Trash));
        assert_eq!(slot_at(tab, 100, 18), None);
        assert_eq!(slot_at(tab, 9, 112), Some(CreativeSlot::Player(36)));
        assert!(!visible_slots(tab).contains(&CreativeSlot::Player(1)));
    }
}
