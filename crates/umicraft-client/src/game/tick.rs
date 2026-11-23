// SPDX-License-Identifier: MIT OR Apache-2.0

//! The 20 Hz simulation tick, in the order of MCP `Minecraft.runTick`.

use std::sync::mpsc::SyncSender;
use std::time::Instant;

use umicraft_game::collision::{boat_box, WithEntities, WithMovingBlocks};
use umicraft_game::entity::{EntityId, EntityKind};
use umicraft_game::gamemode::GameMode;
use umicraft_game::input::RawInput;
use umicraft_game::item_use::drop_item;
use umicraft_game::piston::motion;
use umicraft_game::InputIntent;

use super::abilities::{apply_movement_stats, settle_after_physics};
use super::actions::attack::run_attack;
use super::actions::hotbar::{
    run_hotbar_keys, run_pick_blocks, sync_held_item, sync_held_item_before_click,
};
use super::actions::tick_hand;
use super::actions::use_item::run_use;
use super::body::follow_body;
use super::combat;
use super::commands::{command_of, CommandQueue};
use super::hud::tick_hud_view;
use super::messages::{ChunkEvent, RenderEvent};
use super::mod_world::lend;
use super::movement::report_movement;
use super::particles::{spawn_ambient_particles, spawn_local_crumbs, spawn_mob_particles};
use super::publish::{
    publish_chunks, publish_frame, publish_sounds, publish_status, publish_text_screens,
};
use super::state::Game;
use crate::hud::social as social_view;
use crate::network::ClientCommand;

/// The number of ticks between two status updates.
const STATUS_PERIOD: u64 = 20;

/// Potion id of Nausea.
const NAUSEA: u8 = 9;

/// Advances the overlay state of the local player by one tick.
fn tick_overlays(game: &mut Game) {
    let (Some(state), Some(world)) = (game.state.as_ref(), game.world.as_ref()) else {
        return;
    };
    let facts = crate::scene::overlay_state::PlayerFacts {
        world,
        feet: state.position(),
        eye: state.player().eye_position(),
        spectator: game.gamemode == GameMode::Spectator,
        riding: game.vehicle.is_some(),
        nausea_ticks: game
            .server
            .effects()
            .get(NAUSEA)
            .map(|effect| u32::try_from(effect.duration).unwrap_or(0)),
        helmet: game.windows.helmet().map(|stack| stack.item_id),
        border: game.server.border(),
    };
    game.overlays.tick(&facts);
}

/// Cycles the perspective once per `F5` press since the last tick.
/// Gathers the collision boxes of the boats (MCP
/// `World.getCollidingBoundingBoxes` adds `EntityBoat.getCollisionBoundingBox`;
/// no other entity is solid for a player).
pub(super) fn gather_solid_entities(game: &mut Game) {
    game.solid_entities.clear();
    game.solid_entities.extend(
        game.entities
            .entities()
            .values()
            .filter(|entity| entity.kind == EntityKind::Boat)
            .map(|boat| boat_box(boat.position)),
    );
}

pub(super) fn apply_perspective_presses(game: &mut Game) {
    for _ in 0..std::mem::take(&mut game.controls.perspective_presses) {
        game.perspective = game.perspective.next();
    }
}

/// Runs one simulation tick.
pub(super) fn tick(
    game: &mut Game,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
    chunks: &SyncSender<ChunkEvent>,
    tick_time: Instant,
) {
    umicraft_core::profile_scope!("game tick");
    let tick_start = Instant::now();
    game.metrics.start_tick();
    game.status_data.ticks += 1;
    apply_perspective_presses(game);
    // MCP `Minecraft.runTick`: `updateController` syncs the slot, the hotbar
    // keys then select the next one, the drop key sends its packets, then
    // come the clicks (each syncing the slot before its controller packet)
    // and the pick-block.
    sync_held_item(game, queue);
    run_hotbar_keys(game);
    for kind in std::mem::take(&mut game.hotbar.drops) {
        if let Some(outgoing) = drop_item(game.gamemode, kind) {
            queue.push(command_of(outgoing));
        }
    }
    let since = queue.ordered.len();
    run_attack(game, queue);
    sync_held_item_before_click(game, queue, since);
    let since = queue.ordered.len();
    run_use(game, queue);
    sync_held_item_before_click(game, queue, since);
    run_pick_blocks(game, queue);
    publish_text_screens(game, frames);
    tick_hand(game);
    let sneaking = game
        .state
        .as_ref()
        .is_some_and(|state| state.player().sneaking);
    game.sneak_camera.tick(sneaking);

    if game.social.tick_messages() {
        game.hud_dirty = true;
    }
    if social_view::tick_boss_bar(&mut game.social.boss_bar, &game.entities) {
        game.hud_dirty = true;
    }

    // Phase E.2: the tick-driven scenario `e1` chooses the keys of the tick.
    // It counts client ticks and only sets the input state.
    let e1_keys = if game.world_ready() {
        let on_ground = game
            .state
            .as_ref()
            .is_some_and(|state| state.player().on_ground);
        game.e1.as_mut().map(|run| run.before(on_ground))
    } else {
        None
    };
    if let Some(keys) = e1_keys {
        game.controls.input.forward = f32::from(i8::from(keys.w) - i8::from(keys.s));
        game.controls.input.strafe = f32::from(i8::from(keys.a) - i8::from(keys.d));
        game.controls.input.jump = keys.jump;
        game.controls.input.sneak = keys.sneak;
        game.controls.input.sprint = keys.sprint;
    }

    // The intent of the tick: the raw key state with the yaw and the pitch
    // the player is looking at.
    let intent = RawInput {
        forward: game.controls.input.forward,
        strafe: game.controls.input.strafe,
        jump: game.controls.input.jump,
        sneak: game.controls.input.sneak,
        sprint: game.controls.input.sprint,
        using_item: game.item_use.is_using() && game.vehicle.is_none(),
        yaw: game.yaw,
        pitch: game.pitch,
    }
    .to_intent();

    // A player who spectates another entity does not control its own body
    // (MCP `EntityPlayerSP.updateEntityActionState`, `isCurrentViewEntity`).
    // A dead player does not move either (`EntityLivingBase.isMovementBlocked`).
    let intent = if game.spectating() || game.dead() {
        InputIntent::still(intent.yaw)
    } else {
        intent
    };
    // A removed dead player reports nothing until the respawn replaces it.
    let entity_removed = game.vitals.tick_death();
    apply_movement_stats(game);
    let was_flying = game
        .state
        .as_ref()
        .is_some_and(|state| state.player().flying);
    game.physics_mode.set(game.gamemode);

    // The simulation runs even before the player's column arrives: the
    // vanilla client does (`ChunkProviderClient.chunkExists` is always
    // true) and the physics sinks the player there instead of pulling it
    // down (`BlockSource::is_column_loaded`).
    let ready = game.world_ready();
    let seat = game
        .vehicle
        .and_then(|vehicle| game.entities.rider_feet(EntityId(vehicle)));
    gather_solid_entities(game);
    if let (Some(state), Some(world)) = (game.state.as_mut(), game.world.as_ref()) {
        match seat {
            Some(feet) => state.ride(feet),
            // The mods' physics layers may query the world (F3.6).
            None => lend(
                Some(world),
                &game.entities,
                &game.social.scoreboard,
                &game.hud_view,
                || {
                    state.tick(
                        &intent,
                        &WithMovingBlocks {
                            world: &WithEntities {
                                world,
                                boxes: &game.solid_entities,
                            },
                            moving: game.server.moving_blocks(),
                        },
                    )
                },
            ),
        }
    }
    if let (Some(run), Some(state)) = (game.e1.as_mut(), game.state.as_ref()) {
        if ready && e1_keys.is_some() {
            run.after(state.player());
        }
    }
    settle_after_physics(game, was_flying, queue);
    follow_body(game);
    tick_overlays(game);
    if let Some(state) = game.state.as_ref() {
        // A mob may watch the local player, whose body is not in the store.
        game.entities
            .set_viewer(EntityId(game.entity_id), state.position());
    }
    game.entities.tick();
    if let Some(world) = game.world.as_ref() {
        game.entities.tick_objects(world);
    }
    spawn_mob_particles(game);
    spawn_local_crumbs(game);
    if let Some(world) = game.world.as_ref() {
        game.particles.tick(world);
    }
    combat::tick_emitters(game);
    spawn_ambient_particles(game);
    game.server.tick();
    game.server
        .tick_lightning(game.entities.lightning_flashing());
    tick_pistons(game);
    // A player inside a block is pushed out sideways by the simulation
    // itself, like the vanilla client (`umicraft_game::physics`); it is
    // never lifted.

    let movement_start = queue.ordered.len();
    if !entity_removed {
        report_movement(game, &intent, queue);
    }
    publish_chunks(game, chunks);

    publish_frame(game, frames, tick_time);
    publish_sounds(game, frames);
    tick_hud_view(game);
    if let Some(strength) = game.hud_view.take_jump() {
        // Ahead of the movement report of this tick, where the queue used
        // to drain it before the movement slot.
        queue.push_before_movement(
            ClientCommand::RidingJump {
                entity_id: game.entity_id,
                strength,
            },
            movement_start,
        );
    }
    if game.hud_dirty {
        // The HUD is only published again when the last one was accepted, so
        // a full queue does not lose a change.
        if frames
            .try_send(RenderEvent::Hud(Box::new(game.hud())))
            .is_ok()
        {
            game.hud_dirty = false;
        }
    }
    if game.status_data.ticks.is_multiple_of(STATUS_PERIOD) {
        game.metrics
            .update_tps(game.status_data.ticks, Instant::now());
        publish_status(game, frames);
    }
    game.metrics.record_tick_time(tick_start.elapsed());
}

/// Advances the moving piston blocks and lets them push the local player.
///
/// The reference updates the entities (the player) first and the tile
/// entities after them (`World.updateEntities`), so the push lands on the
/// position the player just moved to. A spectator passes through blocks and
/// is not pushed; other entities are placed by the server.
fn tick_pistons(game: &mut Game) {
    let pushable = !game.spectating();
    let Some(world) = game.world.as_mut() else {
        return;
    };
    let mut player = game.state.as_mut().filter(|_| pushable);
    let dirty = motion::tick(
        world,
        game.server.moving_blocks_mut(),
        |world, block, progress, distance| {
            if let Some(state) = player.as_mut() {
                state.launch_by_piston(world, block, progress, distance);
            }
        },
    );
    game.dirty.extend(dirty);
}
