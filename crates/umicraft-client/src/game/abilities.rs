// SPDX-License-Identifier: MIT OR Apache-2.0

//! The game mode, the abilities and the movement stats the server grants
//! the local player.

use umicraft_game::digging::{enchantment_level, DEPTH_STRIDER};
use umicraft_game::gamemode::GameMode;
use umicraft_game::item_use::UseAction;
use umicraft_game::movement::movement_stats;
use umicraft_game::physics::{DEFAULT_FLY_SPEED, DEFAULT_WALK_SPEED, SPRINT_FOOD_LIMIT};

use super::commands::CommandQueue;
use super::state::Game;
use crate::network::ClientCommand;

/// Switches the gamemode of the local player.
///
/// The server announces the abilities of the new mode in a separate
/// `Player Abilities` packet; the no-clip flag only depends on the mode.
pub(super) fn set_gamemode(game: &mut Game, mode: GameMode) {
    game.gamemode = mode;
    game.status_data.gamemode = mode.id();
    game.hud_dirty = true;
    apply_mode_rules(game);
}

/// Hands the abilities of the server and the rules of the gamemode to the
/// player simulation.
///
/// MCP `PlayerCapabilities` is what the server last sent; a spectator
/// always may fly and never collides (`EntityPlayerSP.onLivingUpdate`).
pub(super) fn apply_mode_rules(game: &mut Game) {
    let Some(state) = game.state.as_mut() else {
        return;
    };
    let abilities = game.server.abilities();
    let allow_flying = abilities.is_some_and(|a| a.allow_flying()) || game.gamemode.allows_flight();
    let flying = abilities.is_some_and(|a| a.flying());
    let fly_speed = abilities.map_or(DEFAULT_FLY_SPEED, |a| a.fly_speed);
    state.set_abilities(allow_flying, flying, fly_speed);
    state.set_no_clip(game.gamemode.no_clip());
}

/// Hands the speed attribute, the effects and the food level of the server
/// state to the simulation (`umicraft_game::movement`).
pub(super) fn apply_movement_stats(game: &mut Game) {
    let Some(state) = game.state.as_mut() else {
        return;
    };
    let walk_speed = game
        .server
        .abilities()
        .map_or(DEFAULT_WALK_SPEED, |abilities| abilities.walk_speed);
    let stats = movement_stats(
        game.server.attributes(),
        game.server.effects(),
        f64::from(walk_speed),
    );
    state.set_movement(stats, walk_speed, game.food > SPRINT_FOOD_LIMIT);
    let strider = game
        .windows
        .boots()
        .and_then(|boots| boots.nbt.as_deref())
        .map_or(0, |nbt| enchantment_level(nbt, DEPTH_STRIDER));
    state.set_depth_strider(u8::try_from(strider).unwrap_or(u8::MAX));
    state.set_bow_draw(
        game.item_use
            .view()
            .filter(|using| using.action == UseAction::Bow)
            .map(|using| using.ticks_used),
    );
}

/// Tells the server about a change of the flying state.
///
/// MCP `EntityPlayerSP.sendPlayerAbilities` (`C13PacketPlayerAbilities`)
/// repeats the capabilities with the new `isFlying`.
pub(super) fn report_flight(game: &Game, was_flying: bool, queue: &mut CommandQueue) {
    let (Some(state), Some(abilities)) = (game.state.as_ref(), game.server.abilities()) else {
        return;
    };
    let flying = state.player().flying;
    if flying != was_flying {
        queue.push(ClientCommand::PlayerAbilities {
            flags: abilities.flags,
            flying,
            fly_speed: abilities.fly_speed,
            walk_speed: abilities.walk_speed,
        });
    }
}
