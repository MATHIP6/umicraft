// SPDX-License-Identifier: MIT OR Apache-2.0

//! The client-side particles the world, the mobs and the packets create (P8).

use glam::DVec3;
use umicraft_game::entity::{EntityId, EntityKind};
use umicraft_game::gamemode::GameMode;
use umicraft_game::item_use::UseAction;
use umicraft_game::particles::{is_bite_tick, spawner_active, Eater, EffectContext, SpawnFilter};
use umicraft_game::raycast::RayHit;
use umicraft_game::shapes::Aabb;
use umicraft_world::{BlockPos, BlockState};

use super::state::Game;

/// The barrier item (id 166), whose holder in creative sees barrier blocks.
const BARRIER_ITEM: i16 = 166;

/// The `Effect` id of a block breaking (`RenderGlobal.playAuxSFX`, case 2001).
const BLOCK_BREAK_EFFECT: i32 = 2001;

/// The particles the world makes around the player this tick.
pub(super) fn spawn_ambient_particles(game: &mut Game) {
    let (Some(state), Some(world)) = (game.state.as_ref(), game.world.as_ref()) else {
        return;
    };
    let player = state.player();
    let viewer = crate::scene::particle_ambient::Viewer {
        position: player.position,
        motion: player.velocity,
        running: player.sprinting && !player.in_water && player.on_ground,
        sees_barriers: game.gamemode == GameMode::Creative
            && game
                .windows
                .selected_item()
                .is_some_and(|stack| stack.item_id == BARRIER_ITEM),
    };
    let rain = game.ambient.tick(
        &mut game.particles,
        world,
        &viewer,
        game.server.weather().rain_strength(),
        i32::try_from(game.status_data.ticks).unwrap_or(0),
        game.particle_setting,
    );
    if let Some(state) = game.state.as_mut() {
        state.queue_sound_cues(rain);
    }
}

/// The block id of a mob spawner.
const MOB_SPAWNER: u16 = 52;
/// The `data` of the spawn packet of a spawner minecart.
const SPAWNER_CART: i32 = 4;
/// Half the side of the cube searched for spawners: their activating range.
const SPAWNER_REACH: i32 = 16;

/// The smoke and flames of the spawners the local player wakes this tick:
/// the spawner blocks and the spawner minecarts within 16 blocks
/// (`MobSpawnerBaseLogic.updateSpawner`, `isActivated`). A spectator wakes
/// none (`isAnyPlayerWithinRangeAt` skips them). Other players' range is not
/// counted: their spawners are out of the local player's particle reach.
pub(super) fn spawn_spawner_particles(game: &mut Game) {
    if game.gamemode == GameMode::Spectator {
        return;
    }
    let (Some(filter), Some(world)) = (particle_filter(game), game.world.as_ref()) else {
        return;
    };
    let feet = filter.viewer;
    let particles = &mut game.particles;
    let center = BlockPos::new(
        feet.x.floor() as i32,
        feet.y.floor() as i32,
        feet.z.floor() as i32,
    );
    world.for_each_block_near(center, SPAWNER_REACH, MOB_SPAWNER, |block| {
        if spawner_active(block, feet) {
            particles.spawn_spawner_puff(block, &filter);
        }
    });
    // `EntityMinecartMobSpawner`: the logic sits in the block of the cart.
    for cart in
        game.entities.entities().values().filter(|entity| {
            entity.kind == EntityKind::Minecart && entity.object_data == SPAWNER_CART
        })
    {
        let block = BlockPos::new(
            cart.position.x.floor() as i32,
            cart.position.y.floor() as i32,
            cart.position.z.floor() as i32,
        );
        if spawner_active(block, feet) {
            particles.spawn_spawner_puff(block, &filter);
        }
    }
}

/// Creates the particles the mobs give off by themselves this tick (blaze
/// smoke, enderman sparks, potion swirls...) and the puffs of the bodies whose
/// death fall ended. Run after the entities moved and before the particles
/// tick, as `Minecraft.runTick` updates the world before the effect renderer.
pub(super) fn spawn_mob_particles(game: &mut Game) {
    let (Some(filter), Some(world)) = (particle_filter(game), game.world.as_ref()) else {
        game.entities.drain_deaths().for_each(drop);
        return;
    };
    for entity in game.entities.entities().values() {
        game.particles.spawn_mob_tick(entity, world, &filter);
        if let Some(end) = game.entities.beam_end(entity) {
            game.particles.spawn_beam_bubbles(entity, end, &filter);
        }
    }
    for body in game.entities.drain_deaths() {
        game.particles.spawn_death_puff(&body, &filter);
    }
}

/// The crumbs of the local player's own meal (MCP
/// `EntityPlayer.updateItemUse`): the stored entities make theirs in
/// `spawn_mob_particles`, but the local body is not stored, so its use state
/// comes from `item_use`.
pub(super) fn spawn_local_crumbs(game: &mut Game) {
    let Some(view) = game.item_use.view() else {
        return;
    };
    if view.action != UseAction::Eat || !is_bite_tick(view.count as u32) {
        return;
    }
    let (Some(filter), Some(state), Some(food)) = (
        particle_filter(game),
        game.state.as_ref(),
        game.windows.selected_item(),
    ) else {
        return;
    };
    let eater = Eater {
        eyes: state.player().eye_position(),
        yaw: game.yaw,
        pitch: game.pitch,
    };
    let id = food.item_id.unsigned_abs();
    let damage = food.damage.unsigned_abs();
    game.particles.spawn_crumbs(&eater, id, damage, &filter);
    // The server relays the bite sound to the other players only.
    let cue = game.particles.bite_cue(state.position());
    if let Some(state) = game.state.as_mut() {
        state.queue_sound_cues([cue]);
    }
}

/// The particles of an `Entity Status` for another entity (MCP
/// `handleStatusUpdate`): hearts, smoke, the witch's magic, the spawner puff.
pub(super) fn spawn_status_particles(game: &mut Game, id: EntityId, status: i8) {
    let (Some(filter), Some(world)) = (particle_filter(game), game.world.as_ref()) else {
        return;
    };
    if let Some(entity) = game.entities.entities().get(&id) {
        game.particles
            .spawn_mob_status(entity, status, world, &filter);
    }
}

/// Creates the particles a `Particle` packet asks for, seen from the player
/// (MCP `NetHandlerPlayClient.handleParticles`). Before the player is placed
/// there is no viewer, so the packet is dropped.
pub(super) fn spawn_particles(
    game: &mut Game,
    packet: &umicraft_protocol::packets::play::clientbound::Particle,
) {
    if let Some(filter) = particle_filter(game) {
        game.particles.spawn_from_packet(packet, &filter);
    }
}

/// The spawn filter for the local viewer; `None` before the player is placed.
pub(super) fn particle_filter(game: &Game) -> Option<SpawnFilter> {
    Some(SpawnFilter {
        viewer: game.state.as_ref()?.position(),
        setting: game.particle_setting,
    })
}

/// `Effect` 2001 (`RenderGlobal.playAuxSFX`): a block broke somewhere, so its
/// fragments fly. The data is the broken state, `id | metadata << 12`.
pub(super) fn apply_effect(
    game: &mut Game,
    packet: &umicraft_protocol::packets::play::clientbound::Effect,
) {
    if packet.effect_id != BLOCK_BREAK_EFFECT {
        spawn_effect_particles(game, packet);
        return;
    }
    let (Ok(id), Ok(metadata)) = (
        u16::try_from(packet.data & 0xFFF),
        u8::try_from((packet.data >> 12) & 0xFF),
    ) else {
        return;
    };
    game.particles.add_block_destroy_effects(
        BlockPos::new(packet.x, packet.y, packet.z),
        BlockState::new(id, metadata),
        [1.0; 3],
    );
}

/// The particles of the other `Effect` ids (smoke, potion splash, ender eye,
/// spawner, bone meal); the rest are sounds.
fn spawn_effect_particles(
    game: &mut Game,
    packet: &umicraft_protocol::packets::play::clientbound::Effect,
) {
    let Some(filter) = particle_filter(game) else {
        return;
    };
    let pos = BlockPos::new(packet.x, packet.y, packet.z);
    let block_max_y = game.world.as_ref().and_then(|world| {
        let state = world.block(pos);
        (state.id() != 0).then(|| {
            umicraft_game::shapes::selection_shape(state, world, pos)
                .boxes()
                .iter()
                .map(|bounds| bounds.max.y)
                .fold(0.0, f64::max)
        })
    });
    let context = EffectContext::new(packet.data, block_max_y);
    game.particles
        .spawn_effect(packet.effect_id, pos, packet.data, &context, &filter);
}

/// Chips the face being dug while the dig goes on (MCP
/// `PlayerControllerMP.onPlayerDamageBlock` calls `addBlockHitEffects` on every
/// tick of a dig).
pub(super) fn chip_dug_block(game: &mut Game, hit: Option<&RayHit>) {
    let (Some(hit), Some(world)) = (hit, game.world.as_ref()) else {
        return;
    };
    if game.interaction.digging_block() != Some(hit.block) {
        return;
    }
    let state = world.block(hit.block);
    let shape = umicraft_game::shapes::selection_shape(state, world, hit.block);
    let bounds = shape
        .boxes()
        .iter()
        .copied()
        .reduce(|a, b| Aabb::new(a.min.min(b.min), a.max.max(b.max)))
        .unwrap_or_else(|| Aabb::new(DVec3::ZERO, DVec3::ONE));
    game.particles
        .add_block_hit_effects(hit.block, state, bounds, hit.face, [1.0; 3]);
}
