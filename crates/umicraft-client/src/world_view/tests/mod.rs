// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the world view: the harness that feeds it frames and chunks,
//! then one file per concern.

use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::{self, SyncSender};
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use umicraft_game::entity::wrap_degrees;
use umicraft_game::entity::{BodyYaw, EntityId, EntityKind, EntityRecord};
use umicraft_game::gamemode::GameMode;
use umicraft_game::raycast::look_direction;
use umicraft_game::server::world_border::Border;
use umicraft_game::sound::cue::SoundCue;
use umicraft_game::view::Perspective;
use umicraft_game::RenderState;
use umicraft_mesh::ChunkSnapshot;
use umicraft_mesh::Mesh;
use umicraft_render::{Camera, FogContext, HudState, OutlineBox};
use umicraft_ui::menus::ConnectStage;
use umicraft_world::{BlockPos, BlockState, ChunkPayload, World};
use umicraft_world::{ChunkPos, LightLevel};

use super::meshing::*;
use super::pacing::*;
use super::*;
use crate::game::{ChunkEvent, Frame, GameStatus, RenderEvent, TICK};
use crate::resources::terrain::TerrainTextures;
use crate::scene::cracks::{Crack, STAGES};
use crate::screens::connection_end::ConnectionEnd;
use crate::test_support::TextureTarget;

mod body_yaw;
mod camera;
mod camera_audit;
mod meshing;
mod pacing;
mod perspective_audit;
mod scene;
mod session;

/// A renderer without a GPU: the face count of every uploaded chunk.
#[derive(Default)]
struct Meshes {
    uploaded: HashMap<ChunkPos, usize>,
    uploads: usize,
    clears: usize,
}

impl MeshTarget for Meshes {
    fn upload_mesh(&mut self, pos: ChunkPos, mesh: &Mesh) {
        self.uploads += 1;
        if mesh.is_empty() {
            self.uploaded.remove(&pos);
        } else {
            self.uploaded.insert(pos, mesh.face_count());
        }
    }

    fn clear_meshes(&mut self) {
        self.clears += 1;
        self.uploaded.clear();
    }
}

struct Harness {
    world_view: WorldView,
    frames: (SyncSender<RenderEvent>, Receiver<RenderEvent>),
    chunks: (SyncSender<ChunkEvent>, Receiver<ChunkEvent>),
    target: Meshes,
}

impl Harness {
    fn new() -> Self {
        let terrain = TerrainTextures::Procedural.install(&mut TextureTarget::default());
        Self {
            world_view: WorldView::new(&terrain, 1).expect("world view"),
            frames: mpsc::sync_channel(16),
            chunks: mpsc::sync_channel(16),
            target: Meshes::default(),
        }
    }

    fn send_chunk(&self, pos: ChunkPos, blocks: &[BlockPos]) {
        let snapshot = ChunkSnapshot::from_world(&world_with(pos, blocks), pos);
        self.chunks
            .0
            .send(ChunkEvent {
                pos,
                snapshot: Some(snapshot),
            })
            .expect("chunk queue");
    }

    fn send_unload(&self, pos: ChunkPos) {
        self.chunks
            .0
            .send(ChunkEvent {
                pos,
                snapshot: None,
            })
            .expect("chunk queue");
    }

    fn sync(&mut self) {
        self.world_view
            .sync(&self.frames.1, &self.chunks.1, &mut self.target);
    }

    /// Syncs until `done` holds or a second passed.
    fn sync_until(&mut self, done: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(1);
        while !done(self) && Instant::now() < deadline {
            self.sync();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(done(self), "the condition never held");
    }
}

/// A world holding `pos` with one stone block per entry of `blocks`.
fn world_with(pos: ChunkPos, blocks: &[BlockPos]) -> World {
    let mut world = World::new(0);
    let payload = vec![0u8; umicraft_world::chunk::payload_len(1, true, true)];
    let column = ChunkPayload {
        primary_bit_mask: 1,
        ground_up: true,
        data: &payload,
    };
    world.apply_chunk(pos, column).expect("an empty column");
    for block in blocks {
        world.set_block(*block, BlockState::new(1, 0));
    }
    world
}

const ORIGIN: ChunkPos = ChunkPos::new(0, 0);

fn frame(position: glam::Vec3, tick_time: Instant, world_time: i64) -> Box<Frame> {
    Box::new(Frame {
        perspective: Perspective::FirstPerson,
        state: RenderState {
            position: position.as_dvec3(),
            eye: position.as_dvec3(),
            orbit_start: glam::Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            on_ground: true,
            effects: umicraft_game::view::ViewEffects::default(),
        },
        tick_time,
        world_time,
        dimension: -1,
        flat_world: true,
        gamemode: GameMode::default(),
        spawn: None,
        entities: Arc::from([]),
        local_body: Arc::new(
            EntityRecord::new(EntityId(0), EntityKind::Player, glam::DVec3::ZERO, 0.0, 0.0)
                .frame_data(),
        ),
        placement: 0,
        hand_light: LightLevel::FULL_SKY,
        selection: Arc::from([]),
        cracks: Arc::from([]),
        maps: Arc::default(),
        chest_lids: Arc::default(),
        moving_blocks: Arc::default(),
        block_entity_data: Arc::default(),
        sky_temperature: umicraft_render::RenderEnvironment::PLAINS_TEMPERATURE,
        fog: FogContext::DEFAULT,
        border: crate::scene::border::border_view(&Border::default()),
        weather: crate::scene::weather::WeatherView::default(),
        particles: Arc::from([]),
        overlays: crate::scene::overlay_state::OverlayInputs::default(),
        debug: Arc::default(),
        signs: Arc::default(),
    })
}
