// SPDX-License-Identifier: MIT OR Apache-2.0

//! The view of the player the render thread draws: the camera, the third
//! person orbit and the facts of the F3 screen.

use glam::DVec3;
use umicraft_game::combat::hurt_roll;
use umicraft_game::gamemode::GameMode;
use umicraft_game::hand::HandPose;
use umicraft_game::view::{camera_pose, Perspective};
use umicraft_game::{GameState, RenderState};
use umicraft_world::BlockPos;

use super::state::Game;

/// The view of the player at the end of the current tick, and whether it is
/// the player's own (`false` while the camera follows another entity).
///
/// `None` until the first state exists.
pub(super) fn player_view(game: &Game) -> Option<(RenderState, bool)> {
    let state = game.state.as_ref()?;
    let followed = game
        .camera_entity()
        .and_then(|id| game.entities.camera_view(id, 1.0));
    let mut view = followed.unwrap_or_else(|| third_person_view(game, own_eye(game, state, true)));
    if followed.is_none() && game.dead() {
        view.effects = view.effects.with_death(game.vitals.death_time as f32);
    }
    if followed.is_none() {
        view.effects = view
            .effects
            .with_hurt(hurt_roll(game.body.animation.hurt_time(), 1.0));
    }
    // MCP `EntityRenderer.renderHand`: only the first-person view of a
    // player who plays shows the hand.
    let hand_visible = followed.is_none()
        && game.perspective == Perspective::FirstPerson
        && game.gamemode != GameMode::Spectator;
    view.effects.hand = HandPose {
        visible: hand_visible,
        shows_arm: game.hand.pose().shows_arm && hand_visible,
        item_use: game.item_use.view(),
        fishing: game.body.bobber.is_some(),
        ..game.hand.pose()
    };
    Some((view, followed.is_none()))
}

/// The world facts of the F3 screen, empty before the first chunk.
pub(super) fn debug_world(game: &Game) -> crate::debug_world::DebugWorld {
    let (Some(world), Some(state)) = (game.world.as_ref(), game.state.as_ref()) else {
        return crate::debug_world::DebugWorld::default();
    };
    let feet = state.position();
    crate::debug_world::collect(
        world,
        &crate::debug_world::DebugInputs {
            loaded_entities: game.entities.len(),
            particles: game.particles.len(),
            feet: [feet.x, feet.y, feet.z],
            target: game.target().map(|hit| hit.block),
            // A client that has not seen the packet assumes normal.
            difficulty: game.server.difficulty().unwrap_or(2),
            world_time: game.world_time,
            tick_micros: game.metrics.last_tick_micros,
            net_event_micros: game.metrics.last_net_micros,
            input_processing_micros: game.metrics.last_input_processing_us,
            input_queue_latency_micros: game.metrics.last_input_queue_latency_us,
            resyncs: game.status_data.resyncs.min(u64::from(u32::MAX)) as u32,
            real_tps: game.metrics.real_tps,
            tick_delay_us: game.metrics.last_tick_delay_us,
            max_tick_delay_us: game.metrics.max_tick_delay_us,
            catch_up_ticks: game.metrics.catch_up_ticks_total,
            net_queue_depth: game.metrics.net_queue_len,
            dirty_chunks: game.dirty.len(),
        },
    )
}

/// The base temperature of the biome at `position`, which the sky takes its
/// colour from (`World.getSkyColor` reads the biome at the camera). The
/// default plains value applies without a loaded column.
pub(super) fn sky_temperature(game: &Game, position: DVec3) -> f32 {
    let block = BlockPos::new(position.x.floor() as i32, 0, position.z.floor() as i32);
    game.world
        .as_ref()
        .and_then(|world| world.biome(block))
        .map_or(
            umicraft_render::RenderEnvironment::PLAINS_TEMPERATURE,
            umicraft_mesh::tint::biome_temperature,
        )
}

/// The first-person view of the player at the end of the tick (`at_end`) or
/// at its start, with the eye where the 1.7 sneak camera puts it when the
/// 1.7 Animations mod asks for it (ADR-0079).
fn own_eye(game: &Game, state: &GameState, at_end: bool) -> RenderState {
    let mut view = state.interpolated(if at_end { 1.0 } else { 0.0 });
    if game.legacy.sneak_camera {
        let correction = game
            .sneak_camera
            .eye_correction(state.player().sneaking, at_end);
        view.position.y += correction;
        view.eye.y += correction;
    }
    view
}

/// Moves the first-person view of the player to the camera of the chosen
/// perspective (MCP `EntityRenderer.orientCamera`).
fn third_person_view(game: &Game, first_person: RenderState) -> RenderState {
    let Some(world) = game.world.as_ref() else {
        return first_person;
    };
    let eye = first_person.position;
    let pose = camera_pose(
        game.perspective,
        eye,
        game.yaw.to_radians(),
        game.pitch.to_radians(),
        world,
    );
    // The same angles from the eye of the previous tick: the wall rays of
    // `orientCamera` start from the interpolated eye every frame, so the
    // renderer blends this orbit with the one above over the tick.
    let start = game
        .state
        .as_ref()
        .map_or(eye, |state| own_eye(game, state, false).position);
    let start_pose = camera_pose(
        game.perspective,
        start,
        game.yaw.to_radians(),
        game.pitch.to_radians(),
        world,
    );
    RenderState {
        position: pose.position,
        eye: first_person.position,
        orbit_start: (start_pose.position - start).as_vec3(),
        yaw: pose.yaw,
        pitch: pose.pitch,
        ..first_person
    }
}
