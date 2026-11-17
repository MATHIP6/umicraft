// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hotbar: slot selection, its sync with the server and pick-block.

use umicraft_game::gamemode::GameMode;
use umicraft_game::pick::{pick_block as pick_in, picked_item};

use crate::game::commands::CommandQueue;
use crate::game::state::Game;
use crate::network::ClientCommand;

/// Picks the targeted block into the hotbar (the middle button).
///
/// MCP `Minecraft.middleClickMouse`: the hotbar slot that holds the item is
/// selected; a creative player gets the item in the first free slot and
/// tells the server about the slot.
pub(in crate::game) fn pick_block(game: &mut Game, queue: &mut CommandQueue) {
    if game.dead() {
        return;
    }
    let (Some(hit), Some(world)) = (game.target(), game.world.as_ref()) else {
        return;
    };
    let Some(picked) = picked_item(world.block(hit.block)) else {
        return;
    };
    let creative = game.gamemode == GameMode::Creative;
    let pick = pick_in(
        &game.windows.main_inventory(),
        game.hotbar.selected,
        picked,
        creative,
    );
    for (index, stack) in pick.changed {
        game.windows
            .apply_set_slot(0, window_slot(index), stack.clone());
        if creative && index == usize::from(pick.select.unwrap_or(game.hotbar.selected)) {
            queue.push(ClientCommand::CreativeSlot {
                slot: window_slot(index),
                item: stack,
            });
        }
    }
    if let Some(slot) = pick.select {
        select_slot(game, slot);
    }
}

/// The pick-block presses of the tick (MCP `Minecraft.runTick`, after the
/// attack and the use). While an item is in use the presses are read and
/// dropped.
pub(in crate::game) fn run_pick_blocks(game: &mut Game, queue: &mut CommandQueue) {
    let presses = std::mem::take(&mut game.hotbar.picks);
    if game.item_use.is_using() {
        return;
    }
    for _ in 0..presses {
        pick_block(game, queue);
    }
    // `setCurrentItem` changes the slot at once; the server hears of it at
    // the next sync.
    game.hotbar.applied = game.hotbar.selected;
}

/// The slot of the player window that holds a main inventory index
/// (the hotbar is the last row of the window).
fn window_slot(index: usize) -> i16 {
    let slot = if index < 9 { 36 + index } else { index };
    slot as i16
}

/// Selects a hotbar slot. Only the player's view changes; the server is told
/// by [`sync_held_item`] in a tick, once, whatever the number of presses.
pub(in crate::game) fn select_slot(game: &mut Game, slot: u8) {
    if slot < 9 && slot != game.hotbar.selected {
        game.hotbar.selected = slot;
        game.windows.set_selected_slot(slot);
        game.hud_dirty = true;
    }
}

/// The hotbar keys of the tick (MCP `Minecraft.runTick`: `for l in 0..9 { if
/// keyBindsHotbar[l].isPressed() { currentItem = l } }`). Each key gives up one
/// press per tick, in the order of the keys, so with several keys down the
/// highest wins whatever order they were pressed in; a key pressed twice
/// selects again at the next tick. The wheel, handled before, only counts
/// when no key was pressed.
pub(in crate::game) fn run_hotbar_keys(game: &mut Game) {
    let mut keyed = None;
    for (slot, presses) in game.hotbar.key_presses.iter_mut().enumerate() {
        if *presses > 0 {
            *presses -= 1;
            keyed = Some(slot as u8);
        }
    }
    if let Some(slot) = keyed {
        select_slot(game, slot);
    }
    game.hotbar.applied = game.hotbar.selected;
}

/// Tells the server the slot when it differs from the last one told (MCP
/// `PlayerControllerMP.syncCurrentPlayItem`, called by `updateController` at
/// the start of a tick).
pub(in crate::game) fn sync_held_item(game: &mut Game, queue: &mut CommandQueue) {
    if game.hotbar.applied != game.hotbar.synced {
        game.hotbar.synced = game.hotbar.applied;
        queue.push(ClientCommand::HeldItemChange {
            slot: i16::from(game.hotbar.synced),
        });
    }
}

/// The same sync, for the controller calls of a click: it goes ahead of the
/// first packet queued since `since` that is not the swing, which `clickMouse`
/// sends before it calls the controller. A click that queued only a swing did
/// not reach the controller and syncs nothing.
pub(in crate::game) fn sync_held_item_before_click(
    game: &mut Game,
    queue: &mut CommandQueue,
    since: usize,
) {
    if game.hotbar.applied == game.hotbar.synced {
        return;
    }
    let first =
        (since..queue.ordered.len()).find(|&at| !matches!(queue.ordered[at], ClientCommand::Swing));
    if let Some(at) = first {
        game.hotbar.synced = game.hotbar.applied;
        queue.ordered.insert(
            at,
            ClientCommand::HeldItemChange {
                slot: i16::from(game.hotbar.synced),
            },
        );
    }
}
