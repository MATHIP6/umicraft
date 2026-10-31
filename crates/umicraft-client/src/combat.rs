// SPDX-License-Identifier: MIT OR Apache-2.0

//! The combat side of the game thread: what the crosshair points at and the
//! consequences of a hit on the local player (ADR-0029).
//!
//! The rules live in `umicraft_game::combat`; this module only gathers their
//! inputs from [`Game`], so `game.rs` does not grow with each combat feature.

use umicraft_game::combat::sprint::KNOCKBACK;
use umicraft_game::combat::{pick_target, sprint_reset, Pointed, TargetQuery};
use umicraft_game::digging::enchantment_level;
use umicraft_game::entity::{EntityId, EntityRecord};
use umicraft_game::gamemode::GameMode;
use umicraft_game::raycast::look_direction;

use super::Game;

/// What the crosshair points at: the block ray first, then the entities
/// (MCP `EntityRenderer.getMouseOver`).
pub(super) fn pointed(game: &Game) -> Pointed {
    let Some(eye) = game.eye() else {
        return Pointed::Miss;
    };
    let query = TargetQuery {
        eye,
        look: look_direction(game.yaw, game.pitch),
        block_reach: game.reach(),
        extended_reach: game.gamemode == GameMode::Creative,
        block: game.block_target(),
        vehicle: game.vehicle.map(EntityId),
    };
    pick_target(
        &query,
        game.entities
            .entities()
            .values()
            .filter(|entity| !is_spectator(game, entity)),
    )
}

/// True for a player in spectator mode, whom the ray ignores
/// (MCP `EntitySelectors.NOT_SPECTATING`).
fn is_spectator(game: &Game, entity: &EntityRecord) -> bool {
    entity
        .uuid
        .and_then(|uuid| game.player_list.get(&u128::from_be_bytes(uuid)))
        .is_some_and(|entry| {
            u8::try_from(entry.game_mode)
                .is_ok_and(|mode| GameMode::from_wire(mode) == GameMode::Spectator)
        })
}

/// Applies what a hit on `target` does to the local player: the sprint
/// reset of `EntityPlayer.attackTargetEntityWithCurrentItem`.
pub(super) fn after_attack(game: &mut Game, target: EntityId) {
    let Some(kind) = game.entities.kind(target) else {
        return;
    };
    let knockback = game
        .windows
        .selected_item()
        .and_then(|stack| stack.nbt.as_deref())
        .map_or(0, |nbt| enchantment_level(nbt, KNOCKBACK));
    let Some(state) = game.state.as_mut() else {
        return;
    };
    if let Some(reset) = sprint_reset(kind, state.player().sprinting, knockback) {
        state.reset_sprint(reset);
    }
}

/// Logs what a click saw: the target and the entity nearest to the eye.
///
/// Tells apart "no entity known", "entity out of reach" and "ray misses the
/// box" when an attack does not land.
pub(super) fn log_click(game: &Game, pointed: &Pointed) {
    let nearest = game.eye().and_then(|eye| {
        game.entities
            .entities()
            .values()
            .map(|entity| (eye.distance(entity.position), entity))
            .min_by(|a, b| a.0.total_cmp(&b.0))
    });
    tracing::info!(
        ?pointed,
        known_entities = game.entities.len(),
        nearest = ?nearest.map(|(distance, entity)| (entity.id, entity.kind, entity.position, distance)),
        "attack click"
    );
}
