// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the game thread publishes to the render thread. Every send is a
//! `try_send`: the game thread never waits for the window.

use std::sync::mpsc::{self, SyncSender};
use std::sync::Arc;
use std::time::Instant;

use glam::DVec3;
use umicraft_game::chat::lang::LangTable;
use umicraft_game::chat::unformatted_text;
use umicraft_game::entity::guardian_beam::{aim_guardians, BeamTarget};
use umicraft_game::entity::leash::{leash_entities, LeashFrame};
use umicraft_game::entity::light::{eye_light, light_entities};
use umicraft_game::entity::painting::light_paintings;
use umicraft_game::entity::rail::rail_entities;
use umicraft_game::entity::shadow::shadow_entities;
use umicraft_game::entity::{EntityId, EntityKind};
use umicraft_game::sound::looping::{minecart_loops, LoopingSound};
use umicraft_game::view::Perspective;
use umicraft_mesh::{BlockEntityKind, ChunkSnapshot};
use umicraft_world::{BlockPos, LightLevel, World};

use super::look::Look;
use super::messages::{ChunkEvent, Frame, RenderEvent, TextScreenRequest};
use super::state::Game;
use super::view::{biome_at, debug_world, player_view, sky_temperature};
use crate::pack_offer::ServerPackRequest;
use crate::scene::fog::fog_context;

/// Publishes only the view of the player, for a look that changed between two
/// ticks.
///
/// A full [`Frame`] would restart the render thread's interpolation (its
/// `tick_time` is the arrival) and, at mouse rate, fill the frame queue until
/// tick frames are dropped: entities and the camera then stutter while moving.
pub(super) fn publish_look(game: &Game) {
    if let Some((view, _)) = player_view(game) {
        let look = Look {
            view,
            body_pitch: game.pitch,
        };
        game.look.store(look);
    }
}

/// Publishes the sound cues the tick produced, once per tick.
///
/// Cues travel as their own event, not inside the [`Frame`]: a frame is skipped
/// when the queue is full, and a footstep should not disappear with it. When
/// even this event does not fit, the render thread is far behind and the cues
/// are dropped (a sound that late is worthless); the game thread never waits.
pub(super) fn publish_sounds(game: &mut Game, frames: &SyncSender<RenderEvent>) {
    let Some(state) = game.state.as_mut() else {
        return;
    };
    let cues = state.drain_sound_cues();
    if !cues.is_empty() {
        let _ = frames.try_send(RenderEvent::Sounds(cues));
    }
}

/// The looping sounds of this tick: the rolling minecarts, and the inside
/// of the one the local player rides (`MovingSoundMinecart`,
/// `MovingSoundMinecartRiding`).
fn looping_sounds(game: &Game) -> Vec<LoopingSound> {
    let mut loops = Vec::new();
    minecart_loops(&game.entities, game.vehicle.map(EntityId), &mut loops);
    loops
}

/// Publishes the current player state as a frame.
///
/// The frame queue is polled by the render thread, so the game thread never
/// blocks on it: a full queue drops the frame and the render thread keeps the
/// one it has.
pub(super) fn publish_frame(game: &Game, frames: &SyncSender<RenderEvent>, tick_time: Instant) {
    let Some((view, first_person)) = player_view(game) else {
        return;
    };
    let mut entities = game.entities.frame_data();
    if let Some(vehicle) = game.vehicle {
        for frame in &mut entities {
            frame.ridden |= frame.id.0 == vehicle;
        }
    }
    if first_person && game.perspective.shows_player() {
        entities.push(game.body.frame_data());
    }
    // The line of the bobber of the local player ties to their rod, which is
    // not among the frames from the first person: the render thread places
    // that end from the camera of the frame.
    if first_person && !game.perspective.shows_player() {
        for bobber in &mut entities {
            if bobber.kind == EntityKind::FishHook && bobber.object_data == game.entity_id {
                bobber.tied_to_hand = true;
            }
        }
    }
    // Items are lit by the block they are in; before the first chunk the
    // default (full sky) stays.
    // The local player holds leashes too, and is not among the frames from the
    // first person.
    let eye_height = umicraft_game::physics::EYE_HEIGHT as f32;
    let feet = view.position - DVec3::Y * umicraft_game::physics::EYE_HEIGHT;
    let me = LeashFrame {
        prev_position: feet,
        position: feet,
        prev_yaw: (view.yaw - std::f32::consts::PI).to_degrees(),
        yaw: (view.yaw - std::f32::consts::PI).to_degrees(),
        prev_pitch: (-view.pitch).to_degrees(),
        pitch: (-view.pitch).to_degrees(),
        eye_height,
        hanging: false,
    };
    leash_entities(&mut entities, Some((EntityId(game.entity_id), me)));
    // The guardians beam at the local player too, from the feet of the two
    // last ticks (the view is the orbit camera in the third person).
    let me = game.state.as_ref().map(|state| {
        let target = BeamTarget::of_feet(
            state.previous_position(),
            state.player().position,
            umicraft_game::physics::PLAYER_HEIGHT,
        );
        (EntityId(game.entity_id), target)
    });
    aim_guardians(&mut entities, me);
    let hand_light = game.world.as_ref().map_or(LightLevel::FULL_SKY, |world| {
        rail_entities(world, &mut entities);
        light_entities(world, &mut entities);
        light_paintings(world, &mut entities);
        shadow_entities(world, &mut entities, game.clock.time());
        eye_light(world, view.position)
    });
    let _ = frames.try_send(RenderEvent::Frame(Box::new(Frame {
        perspective: game.perspective,
        state: view,
        tick_time,
        world_time: game.clock.time(),
        dimension: game.dimension,
        flat_world: game.flat_world,
        gamemode: game.gamemode,
        spawn: game.spawn,
        entities: entities.into(),
        local_body: Arc::new(game.body.frame_data()),
        placement: game.placements,
        hand_light,
        selection: game.selection_outline().into(),
        cracks: game.cracks().into(),
        maps: Arc::new(game.server.maps().clone()),
        chest_lids: Arc::new(game.server.chest_lids().clone()),
        moving_blocks: Arc::new(game.server.moving_blocks().clone()),
        block_entity_data: game.server.block_entities().entity_table(),
        signs: game.server.block_entities().sign_table(),
        sky_temperature: sky_temperature(game, view.position),
        sky_biome: biome_at(game, view.position),
        fog: fog_context(
            game.world.as_ref(),
            game.server.effects(),
            view.position,
            game.dimension,
            game.clock.time(),
        ),
        border: crate::scene::border::border_view(game.server.border()),
        weather: crate::scene::weather::WeatherView::new(
            game.server.weather(),
            game.world.as_ref(),
            view.position.x.floor() as i32,
            view.position.z.floor() as i32,
        ),
        particles: crate::scene::particles::collect(&game.particles, game.world.as_ref()).into(),
        overlays: game
            .overlays
            .inputs(game.perspective == Perspective::FirstPerson),
        debug: Arc::new(debug_world(game)),
        loops: looping_sounds(game).into(),
    })));
}

/// Hands the resource pack offer the server just made to the window, which
/// asks the player or downloads it (NET-11).
pub(super) fn publish_server_pack(game: &Game, frames: &SyncSender<RenderEvent>) {
    if let Some(offer) = game.server.resource_pack_offer() {
        let request = ServerPackRequest::from(offer);
        let _ = frames.try_send(RenderEvent::ServerPack(Box::new(request)));
    }
}

/// Asks the window to open the sign editor the server requested and the book
/// the player used.
pub(super) fn publish_text_screens(game: &mut Game, frames: &SyncSender<RenderEvent>) {
    let mut requests = Vec::new();
    if let Some(position) = game.server.take_sign_editor() {
        let lang = LangTable::default();
        let lines = game
            .server
            .block_entities()
            .signs()
            .find(|(at, _)| *at == position)
            .map_or_else(Default::default, |(_, lines)| {
                lines.clone().map(|line| unformatted_text(&line, &lang))
            });
        let (x, y, z) = position;
        // `GuiEditSign` draws a standing sign for the standing sign block and
        // a wall sign for anything else.
        let standing = game.world.as_ref().is_some_and(|world| {
            BlockEntityKind::of(world.block(BlockPos::new(x, y, z)))
                == Some(BlockEntityKind::StandingSign)
        });
        requests.push(TextScreenRequest::Sign {
            position,
            lines,
            standing,
        });
    }
    if let Some(stack) = game.pending_book.take() {
        requests.push(TextScreenRequest::Book {
            stack,
            author: game.own_name.clone(),
        });
    }
    for request in requests {
        let _ = frames.try_send(RenderEvent::TextScreen(Box::new(request)));
    }
}

/// Sends the snapshots of the chunks whose geometry changed, or eviction
/// events for unloaded chunks.
pub(super) fn publish_chunks(game: &mut Game, chunks: &SyncSender<ChunkEvent>) {
    if game.dirty.is_empty() {
        return;
    }
    let Some(world) = game.world.as_ref() else {
        game.dirty.clear();
        return;
    };
    let mut full = false;
    game.dirty.retain(|&pos| {
        if full {
            return true;
        }
        let snapshot = world
            .chunk(pos)
            .map(|_| ChunkSnapshot::from_world(world, pos));
        match chunks.try_send(ChunkEvent { pos, snapshot }) {
            Ok(()) => false,
            Err(mpsc::TrySendError::Full(_)) | Err(mpsc::TrySendError::Disconnected(_)) => {
                full = true;
                true
            }
        }
    });
}

/// Publishes the inventory state to the render thread.
pub(super) fn publish_inventory(game: &Game, frames: &SyncSender<RenderEvent>) {
    let _ = frames.try_send(RenderEvent::Inventory(Box::new(game.windows.view())));
}

/// Publishes the statistic counters the server sent, by name.
pub(super) fn publish_stats(game: &Game, frames: &SyncSender<RenderEvent>) {
    let stats = game
        .server
        .statistics()
        .iter()
        .map(|(name, value)| (name.clone(), *value))
        .collect();
    let _ = frames.try_send(RenderEvent::Stats(stats));
}

/// Publishes the statistics of the session.
pub(super) fn publish_status(game: &mut Game, frames: &SyncSender<RenderEvent>) {
    game.status_data.status = game.status.clone();
    game.status_data.chunks = game.world.as_ref().map_or(0, World::chunk_count);
    let _ = frames.try_send(RenderEvent::Status(Box::new(game.status_data.clone())));
}
