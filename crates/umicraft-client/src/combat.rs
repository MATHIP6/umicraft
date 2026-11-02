// SPDX-License-Identifier: MIT OR Apache-2.0

//! The combat side of the game thread: what the crosshair points at and the
//! consequences of a hit on the local player (ADR-0029).
//!
//! The rules live in `umicraft_game::combat`; this module only gathers their
//! inputs from [`Game`], so `game.rs` does not grow with each combat feature.

use glam::DVec3;
use umicraft_game::combat::sprint::KNOCKBACK;
use umicraft_game::combat::{
    body_size, burst, local_stars, pick_target, sprint_reset, CriticalState, HitEmitter, HitStar,
    Pointed, TargetQuery, SHARPNESS,
};
use umicraft_game::digging::enchantment_level;
use umicraft_game::entity::{EntityId, EntityRecord};
use umicraft_game::gamemode::GameMode;
use umicraft_game::media::is_on_ladder;
use umicraft_game::particles::{ParticleKind, SpawnRequest};
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
    let sharpness = game
        .windows
        .selected_item()
        .and_then(|stack| stack.nbt.as_deref())
        .map_or(0, |nbt| enchantment_level(nbt, SHARPNESS));
    let riding = game.vehicle.is_some();
    let Some(state) = game.state.as_mut() else {
        return;
    };
    if let Some(reset) = sprint_reset(kind, state.player().sprinting, knockback) {
        state.reset_sprint(reset);
    }
    let player = state.player();
    let critical = CriticalState {
        falling: player.fall_distance > 0.0,
        on_ground: player.on_ground,
        on_ladder: game
            .world
            .as_ref()
            .is_some_and(|world| is_on_ladder(player, world)),
        in_water: player.in_water,
        blind: player.blinded,
        riding,
    };
    let stars = local_stars(kind, critical, sharpness);
    if stars.critical {
        emit_stars(game, target, HitStar::Critical);
    }
    if stars.magic {
        emit_stars(game, target, HitStar::Magic);
    }
}

/// Starts a burst of stars on `target` (MCP
/// `EffectRenderer.emitParticleAtEntity`): the first burst comes at once, two
/// more on the next ticks.
pub(super) fn emit_stars(game: &mut Game, target: EntityId, star: HitStar) {
    let mut emitter = HitEmitter::new(target, star);
    if emitter.tick() {
        throw_stars(game, &emitter);
        game.hit_emitters.push(emitter);
    }
}

/// Bursts once for every emitter that is still alive.
pub(super) fn tick_emitters(game: &mut Game) {
    let mut emitters = std::mem::take(&mut game.hit_emitters);
    emitters.retain_mut(|emitter| {
        let alive = emitter.tick();
        if alive {
            throw_stars(game, emitter);
        }
        alive
    });
    // Emitters started while these ticked (none today) stay in front.
    emitters.append(&mut game.hit_emitters);
    game.hit_emitters = emitters;
}

/// The feet, width and height of the body of `id`, the local player included.
fn body_of(game: &Game, id: EntityId) -> Option<(DVec3, f64, f64)> {
    if id.0 == game.entity_id {
        let feet = game.state.as_ref()?.position();
        return Some((feet, 0.6, 1.8));
    }
    let entity = game.entities.entities().get(&id)?;
    let (width, height) = body_size(entity)?;
    Some((entity.position, width, height))
}

/// One burst of `EntityParticleEmitter.onUpdate`.
fn throw_stars(game: &mut Game, emitter: &HitEmitter) {
    let Some((feet, width, height)) = body_of(game, emitter.target) else {
        return;
    };
    let Some(filter) = super::particle_filter(game) else {
        return;
    };
    let kind = match emitter.star {
        HitStar::Critical => ParticleKind::Crit,
        HitStar::Magic => ParticleKind::MagicCrit,
    };
    for star in burst(game.particles.rng(), feet, width, height) {
        game.particles.spawn(
            &SpawnRequest {
                kind,
                arguments: &[],
                ignore_range: false,
                position: star.position,
                velocity: star.velocity,
            },
            &filter,
        );
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
    tracing::debug!(
        ?pointed,
        known_entities = game.entities.len(),
        nearest = ?nearest.map(|(distance, entity)| (entity.id, entity.kind, entity.position, distance)),
        "attack click"
    );
}
