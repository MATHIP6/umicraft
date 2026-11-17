// SPDX-License-Identifier: MIT OR Apache-2.0

//! The player's actions: the input updates of the window and the clicks,
//! keys and hand animation the tick runs.

use umicraft_game::digging::{HASTE, MINING_FATIGUE};
use umicraft_game::hand::SwingEffects;
use umicraft_game::input::set_angles;

use super::commands::CommandQueue;
use super::state::Game;
use crate::input::InputUpdate;
use hotbar::select_slot;

pub(super) mod attack;
pub(super) mod hotbar;
pub(super) mod use_item;

/// The Haste and Mining Fatigue effects that change the arm swing speed.
pub(super) fn swing_effects(game: &Game) -> SwingEffects {
    let effects = game.server.effects();
    let amplifier = |id: u8| effects.get(id).map(|effect| effect.amplifier);
    SwingEffects {
        haste: amplifier(HASTE),
        fatigue: amplifier(MINING_FATIGUE),
    }
}

/// Advances the first-person hand by one tick (arm swing, equip animation
/// and sway).
pub(super) fn tick_hand(game: &mut Game) {
    let effects = swing_effects(game);
    let held = game.windows.selected_item();
    game.hand.tick(held, game.yaw, game.pitch, effects);
}

/// Handles one input update.
///
/// The hotbar, drop and pick-block keys only record the request here; their
/// packets leave in the tick, where MCP `Minecraft.runTick` handles the keys
/// (`queue` is kept for the callers).
pub(super) fn handle_input(game: &mut Game, update: InputUpdate, _queue: &mut CommandQueue) {
    game.controls.input = update.state;
    // Degrees, continuous, never wrapped: MCP `Entity.setAngles` adds to
    // `rotationYaw` and the packet carries it as is, so 359.9 turns into
    // 360.2, not 0.2 (ADR-0076).
    (game.yaw, game.pitch) = set_angles(game.yaw, game.pitch, (update.look_yaw, update.look_pitch));

    // The wheel first, then the keys, as `runTick` does.
    if let Some(slot) = update.select_slot {
        select_slot(game, slot);
    }
    if update.hotbar_keys != [0; 9] {
        if let Some(slot) = game.hotbar.add_key_presses(update.hotbar_keys) {
            select_slot(game, slot);
        }
    }

    // The perspective changes with the next tick, with the frame that
    // brings the body of the player (`Minecraft.runTick` reads the key once
    // per tick). Switching at once left the view, published right away when
    // the mouse moves, ahead of the entities of the last frame: back in the
    // first person the camera sat inside the head that was still drawn.
    if update.toggle_perspective {
        game.controls.perspective_presses = game.controls.perspective_presses.saturating_add(1);
    }
    game.controls.attack_pressed |= update.break_block;
    game.controls.use_pressed |= update.place_block;
    if update.pick_block {
        game.hotbar.picks = game.hotbar.picks.saturating_add(1);
    }
    if let Some(kind) = update.drop_item {
        game.hotbar.drops.push(kind);
    }
}
