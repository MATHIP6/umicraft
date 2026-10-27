// SPDX-License-Identifier: MIT OR Apache-2.0

//! The render thread's view of the game: the interpolated camera and clock,
//! the chunks waiting for a mesh worker, and the meshes handed to the
//! renderer (ADR-0013).
//!
//! The window and the screenshot path both drive one [`WorldView`] with
//! [`WorldView::sync`], so they cannot drift apart: a chunk that becomes empty
//! is removed, a world reset drops every mesh of the old world (including the
//! ones still being meshed), and each chunk's faces are counted once.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use umicraft_game::entity::{EntityFrameData, EntityRenderSnapshot};
use umicraft_game::inventory::WindowView;
use umicraft_game::RenderState;
use umicraft_mesh::{ChunkSnapshot, Mesh, Vertex};
use umicraft_render::{Camera, HudState, OutlineBox, RenderEnvironment, Renderer};
use umicraft_world::ChunkPos;

use crate::cracks::{crack_mesh, Crack, STAGES};
use crate::error::ClientError;
use crate::game::{camera_angles, ChunkEvent, Frame, GameStatus, RenderEvent, TICK};
use crate::mesher::{MeshPool, MeshResult};
use crate::terrain_textures::InstalledTerrain;

/// Where finished chunk meshes go: the renderer, or a test double.
pub trait MeshTarget {
    /// Replaces the mesh of `pos`; an empty mesh removes it.
    fn upload_mesh(&mut self, pos: ChunkPos, mesh: &Mesh);

    /// Removes every chunk mesh.
    fn clear_meshes(&mut self);
}

impl MeshTarget for Renderer {
    fn upload_mesh(&mut self, pos: ChunkPos, mesh: &Mesh) {
        Renderer::upload_mesh(self, pos, mesh);
    }

    fn clear_meshes(&mut self) {
        Renderer::clear_meshes(self);
    }
}

/// The game state the render thread shows, and the meshing it drives.
pub struct WorldView {
    view: View,
    pool: MeshPool,
    /// Incremented by every world reset; jobs carry it so a mesh of the
    /// replaced world is dropped when it comes back.
    generation: u64,
    /// Snapshots waiting for a free mesh worker, at most one per chunk.
    waiting: Vec<(ChunkPos, ChunkSnapshot)>,
    /// Chunks with a job in flight for the current generation.
    pending: HashSet<ChunkPos>,
    /// Faces of every chunk whose current mesh is not empty.
    faces: HashMap<ChunkPos, usize>,
    hud: HudState,
    window_view: WindowView,
    status: GameStatus,
    ended: Option<String>,
    frames: usize,
    /// Active living entities interpolation data from the latest frame.
    entities: Arc<[EntityFrameData]>,
    /// Selection outline boxes from the latest frame.
    selection: Arc<[OutlineBox]>,
    /// The cracks of the newest frame, and the mesh built from them.
    cracks: Arc<[Crack]>,
    /// The texture layer of each destroy stage, when the terrain has them.
    destroy_layers: Option<[u32; STAGES]>,
    /// A crack mesh the renderer has not received yet.
    crack_mesh: Option<(Vec<Vertex>, Vec<u32>)>,
}

impl WorldView {
    /// Starts the mesh workers for the installed terrain textures.
    ///
    /// # Errors
    /// [`ClientError::Worker`] when a worker thread cannot be spawned.
    pub fn new(terrain: &InstalledTerrain, worker_count: usize) -> Result<Self, ClientError> {
        Ok(Self {
            view: View::default(),
            pool: MeshPool::new(worker_count, terrain)?,
            generation: 0,
            waiting: Vec::new(),
            pending: HashSet::new(),
            faces: HashMap::new(),
            hud: HudState::default(),
            window_view: WindowView {
                player_slots: std::array::from_fn(|_| None),
                container: None,
                cursor: None,
                selected_slot: 0,
            },
            status: GameStatus::default(),
            ended: None,
            frames: 0,
            entities: Arc::from([]),
            selection: Arc::from([]),
            cracks: Arc::from([]),
            destroy_layers: terrain.mesher().destroy_layers(),
            crack_mesh: None,
        })
    }

    /// Takes everything the game thread published and moves the meshing one
    /// step: finished meshes go to `target`, waiting chunks go to the workers.
    ///
    /// Never blocks: a full job queue keeps the chunk for the next call.
    pub fn sync(
        &mut self,
        frames: &Receiver<RenderEvent>,
        chunks: &Receiver<ChunkEvent>,
        target: &mut impl MeshTarget,
    ) {
        while let Ok(event) = frames.try_recv() {
            self.apply(event, target);
        }
        while let Ok(ChunkEvent { pos, snapshot }) = chunks.try_recv() {
            self.queue(pos, snapshot);
        }
        // Collect before dispatching: a job submitted by this call comes back
        // in a later one, after any reset this call applied.
        self.collect(target);
        self.dispatch();
    }

    /// Points `camera` at the player, interpolated for `now` (GAM-8).
    pub fn update_camera(&self, camera: &mut Camera, now: Instant) {
        let Some(state) = self.view.sample(now) else {
            return;
        };
        camera.position = state.position;
        let (yaw, pitch) = camera_angles(&state);
        camera.yaw = yaw;
        camera.pitch = pitch;
        camera.set_view_effects(state.effects.fov_modifier, state.effects.bob.matrix());
    }

    /// The sky, fog and light environment for `now`, in step with the camera.
    #[must_use]
    pub fn environment(&self, now: Instant, view_distance: u8) -> RenderEnvironment {
        self.view.environment(now, view_distance)
    }

    /// The newest HUD the game thread published.
    #[must_use]
    pub fn hud(&self) -> &HudState {
        &self.hud
    }

    /// The newest inventory/container state the game thread published.
    #[must_use]
    pub fn window_view(&self) -> &WindowView {
        &self.window_view
    }

    /// The newest session statistics.
    #[must_use]
    pub fn status(&self) -> &GameStatus {
        &self.status
    }

    /// Why the session ended, once it has.
    #[must_use]
    pub fn ended(&self) -> Option<&str> {
        self.ended.as_deref()
    }

    /// The chunks whose current mesh has faces.
    #[must_use]
    pub fn chunk_count(&self) -> usize {
        self.faces.len()
    }

    /// The faces of every current chunk mesh.
    #[must_use]
    pub fn face_count(&self) -> usize {
        self.faces.values().sum()
    }

    /// The simulation frames received so far.
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.frames
    }

    /// Interpolated entity render snapshots for the current render instant.
    ///
    /// Uses the same partial tick fraction as the camera so entities and the
    /// world stay in sync.
    #[must_use]
    pub fn entities(&self, now: Instant) -> Vec<EntityRenderSnapshot> {
        let alpha = self.view.partial_tick(now);
        self.entities.iter().map(|e| e.interpolate(alpha)).collect()
    }

    /// The world-space boxes of the block selection outline in the newest
    /// frame; empty when no block is targeted.
    #[must_use]
    pub fn selection(&self) -> &[OutlineBox] {
        &self.selection
    }

    /// The crack mesh built since the last call, for the renderer; empty
    /// vectors remove the cracks.
    pub fn take_crack_mesh(&mut self) -> Option<(Vec<Vertex>, Vec<u32>)> {
        self.crack_mesh.take()
    }

    /// The number of active living entities in the newest frame.
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    fn apply(&mut self, event: RenderEvent, target: &mut impl MeshTarget) {
        match event {
            RenderEvent::Frame(frame) => {
                self.frames += 1;
                self.entities = Arc::clone(&frame.entities);
                self.selection = Arc::clone(&frame.selection);
                if *self.cracks != *frame.cracks {
                    self.cracks = Arc::clone(&frame.cracks);
                    self.crack_mesh = self
                        .destroy_layers
                        .as_ref()
                        .map(|layers| crack_mesh(&self.cracks, layers));
                }
                self.view.push(frame);
            }
            RenderEvent::Hud(hud) => self.hud = *hud,
            RenderEvent::Inventory(view) => self.window_view = *view,
            RenderEvent::Status(status) => self.status = *status,
            RenderEvent::Ended(reason) => self.ended = Some(reason),
            RenderEvent::Reset => self.reset(target),
        }
    }

    /// Replaces the active terrain mesher and worker pool, clearing current meshes
    /// and invalidating all jobs in flight.
    pub fn reload_terrain(
        &mut self,
        terrain: &InstalledTerrain,
        worker_count: usize,
        target: &mut impl MeshTarget,
    ) {
        self.reset(target);
        if let Ok(pool) = MeshPool::new(worker_count, terrain) {
            self.pool = pool;
        }
        // The new array has other layers: rebuild the cracks with the next
        // frame.
        self.destroy_layers = terrain.mesher().destroy_layers();
        self.cracks = Arc::from([]);
    }

    /// Forgets the replaced world, including the jobs still in flight.
    fn reset(&mut self, target: &mut impl MeshTarget) {
        self.generation += 1;
        self.waiting.clear();
        self.pending.clear();
        self.faces.clear();
        self.entities = Arc::from([]);
        target.clear_meshes();
    }

    /// Queues a snapshot; a newer snapshot of a waiting chunk replaces it.
    fn queue(&mut self, pos: ChunkPos, snapshot: ChunkSnapshot) {
        match self.waiting.iter_mut().find(|(waiting, _)| *waiting == pos) {
            Some(entry) => entry.1 = snapshot,
            None => self.waiting.push((pos, snapshot)),
        }
    }

    fn collect(&mut self, target: &mut impl MeshTarget) {
        while let Some(MeshResult {
            pos,
            generation,
            mesh,
        }) = self.pool.try_recv()
        {
            if generation != self.generation {
                // A mesh of the world a reset replaced: never drawn.
                continue;
            }
            self.pending.remove(&pos);
            if mesh.is_empty() {
                self.faces.remove(&pos);
            } else {
                self.faces.insert(pos, mesh.face_count());
            }
            target.upload_mesh(pos, &mesh);
        }
    }

    /// Hands waiting chunks to the workers; a chunk already being meshed
    /// waits for its result first, so meshes arrive in snapshot order.
    fn dispatch(&mut self) {
        let mut index = 0;
        while index < self.waiting.len() {
            let pos = self.waiting[index].0;
            if self.pending.contains(&pos) {
                index += 1;
                continue;
            }
            let (_, snapshot) = self.waiting.remove(index);
            match self.pool.submit(pos, self.generation, snapshot) {
                Ok(()) => {
                    self.pending.insert(pos);
                }
                Err(snapshot) => {
                    // The job queue is full: retry on a later call.
                    self.waiting.insert(index, (pos, snapshot));
                    break;
                }
            }
        }
    }
}

/// The camera of the player, interpolated between the two newest frames.
#[derive(Debug, Default)]
struct View {
    /// The state of the previous tick.
    previous: Option<RenderState>,
    /// The state of the newest tick.
    current: Option<RenderState>,
    /// The instant of the newest tick.
    tick_time: Option<Instant>,
    /// The newest signed world time in ticks.
    world_time: i64,
    /// The dimension of the newest frame.
    dimension: i8,
}

impl View {
    /// Stores a new frame.
    fn push(&mut self, frame: Frame) {
        self.previous = self.current;
        self.current = Some(frame.state);
        self.tick_time = Some(frame.tick_time);
        self.world_time = frame.world_time;
        self.dimension = frame.dimension;
    }

    /// The interpolation fraction of the two newest ticks for `now`.
    ///
    /// The value is clamped to `0.0..=1.0` and is the same fraction
    /// [`View::sample`] uses, so the camera and the world clock stay in step.
    fn partial_tick(&self, now: Instant) -> f32 {
        if self.previous.is_none() {
            return 0.0;
        }
        let Some(tick_time) = self.tick_time else {
            return 0.0;
        };
        let elapsed = now.saturating_duration_since(tick_time).as_secs_f32();
        (elapsed / TICK.as_secs_f32()).clamp(0.0, 1.0)
    }

    /// The render environment for `now`.
    ///
    /// The world clock and dimension are the newest values the game thread
    /// published; the render thread never advances the clock by itself
    /// (ADR-0008 §4).
    fn environment(&self, now: Instant, view_distance: u8) -> RenderEnvironment {
        RenderEnvironment::new(
            self.dimension,
            self.world_time,
            self.partial_tick(now),
            view_distance,
        )
    }

    /// The interpolated state for `now`.
    ///
    /// The renderer shows the simulation one tick behind and interpolates
    /// between the two newest ticks, so the motion is smooth and independent
    /// of the frame rate (GAM-8).
    fn sample(&self, now: Instant) -> Option<RenderState> {
        let current = self.current?;
        let Some(previous) = self.previous else {
            return Some(current);
        };
        let alpha = self.partial_tick(now);
        Some(RenderState {
            position: previous.position.lerp(current.position, alpha),
            yaw: current.yaw,
            pitch: current.pitch,
            on_ground: current.on_ground,
            effects: previous.effects.lerp(current.effects, alpha),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{self, SyncSender};
    use std::time::Duration;

    use umicraft_world::{BlockPos, BlockState, ChunkPayload, World};

    use super::*;
    use crate::terrain_textures::TerrainTextures;
    use crate::test_support::TextureTarget;

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
                .send(ChunkEvent { pos, snapshot })
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

    #[test]
    fn a_meshed_chunk_is_uploaded_and_its_faces_counted_once() {
        let mut harness = Harness::new();
        harness.send_chunk(ORIGIN, &[BlockPos::new(1, 1, 1)]);
        harness.sync_until(|h| h.target.uploaded.contains_key(&ORIGIN));
        assert_eq!(harness.world_view.face_count(), 6);

        // A new snapshot of the same chunk replaces its faces.
        harness.send_chunk(ORIGIN, &[BlockPos::new(1, 1, 1), BlockPos::new(5, 1, 1)]);
        harness.sync_until(|h| h.target.uploaded.get(&ORIGIN) == Some(&12));
        assert_eq!(harness.world_view.face_count(), 12);
        assert_eq!(harness.world_view.chunk_count(), 1);
    }

    #[test]
    fn a_chunk_that_becomes_empty_is_removed() {
        let mut harness = Harness::new();
        harness.send_chunk(ORIGIN, &[BlockPos::new(1, 1, 1)]);
        harness.sync_until(|h| h.target.uploaded.contains_key(&ORIGIN));

        harness.send_chunk(ORIGIN, &[]);
        harness.sync_until(|h| h.target.uploads == 2);
        assert!(!harness.target.uploaded.contains_key(&ORIGIN));
        assert_eq!(harness.world_view.chunk_count(), 0);
        assert_eq!(harness.world_view.face_count(), 0);
    }

    /// Regression: a mesh of the old world that comes back after a reset
    /// used to be drawn in the new world.
    #[test]
    fn a_reset_drops_the_meshes_still_in_flight() {
        let mut harness = Harness::new();
        harness.send_chunk(ORIGIN, &[BlockPos::new(1, 1, 1)]);
        // This call submits the job; its result can only be collected by a
        // later call, which applies the reset first.
        harness.sync();
        harness
            .frames
            .0
            .send(RenderEvent::Reset)
            .expect("frame queue");

        let deadline = Instant::now() + Duration::from_millis(300);
        while Instant::now() < deadline {
            harness.sync();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(harness.target.clears, 1);
        assert_eq!(harness.target.uploads, 0, "the stale mesh was drawn");
        assert_eq!(harness.world_view.chunk_count(), 0);

        // The new world meshes normally.
        harness.send_chunk(ORIGIN, &[BlockPos::new(2, 1, 2)]);
        harness.sync_until(|h| h.target.uploaded.contains_key(&ORIGIN));
    }

    #[test]
    fn the_hud_the_status_and_the_end_are_kept() {
        let mut harness = Harness::new();
        let hud = HudState {
            health: 7.0,
            ..HudState::default()
        };
        let status = GameStatus {
            chunks: 3,
            ..GameStatus::default()
        };
        for event in [
            RenderEvent::Hud(Box::new(hud)),
            RenderEvent::Status(Box::new(status)),
            RenderEvent::Ended("kicked".to_string()),
        ] {
            harness.frames.0.send(event).expect("frame queue");
        }
        harness.sync();
        assert_eq!(harness.world_view.hud().health, 7.0);
        assert_eq!(harness.world_view.status().chunks, 3);
        assert_eq!(harness.world_view.ended(), Some("kicked"));
    }

    fn frame(position: glam::Vec3, tick_time: Instant, world_time: i64) -> Frame {
        Frame {
            state: RenderState {
                position,
                yaw: 0.0,
                pitch: 0.0,
                on_ground: true,
                effects: umicraft_game::view::ViewEffects::default(),
            },
            tick_time,
            world_time,
            dimension: -1,
            entities: Arc::from([]),
            selection: Arc::from([]),
            cracks: Arc::from([]),
        }
    }

    #[test]
    fn the_camera_and_the_clock_interpolate_between_the_two_newest_frames() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let start = glam::Vec3::new(0.0, 64.0, 0.0);
        harness
            .frames
            .0
            .send(RenderEvent::Frame(frame(start, tick, -6_000)))
            .expect("frame queue");
        harness.sync();
        // One frame is not enough to interpolate: the camera shows it and
        // the clock fraction is zero even when `now` is ahead of the tick.
        let mut camera = Camera::default();
        harness
            .world_view
            .update_camera(&mut camera, tick + TICK * 2);
        assert_eq!(camera.position, start);
        assert_eq!(
            harness
                .world_view
                .environment(tick + TICK * 2, 12)
                .partial_tick,
            0.0
        );

        let later = tick + TICK;
        let end = glam::Vec3::new(2.0, 64.0, 0.0);
        harness
            .frames
            .0
            .send(RenderEvent::Frame(frame(end, later, -5_900)))
            .expect("frame queue");
        harness.sync();
        assert_eq!(harness.world_view.frame_count(), 2);

        let halfway = later + TICK / 2;
        harness.world_view.update_camera(&mut camera, halfway);
        assert!((camera.position.x - 1.0).abs() < 1e-3);
        let environment = harness.world_view.environment(halfway, 12);
        assert!((environment.partial_tick - 0.5).abs() < 1e-3);
        assert_eq!(environment.world_time, -5_900);
        assert_eq!(environment.dimension, -1);
        assert_eq!(environment.view_distance, 12);

        // Before the newest tick and after a full tick the fraction is
        // clamped to the interpolation range.
        assert_eq!(
            harness
                .world_view
                .environment(later - TICK, 12)
                .partial_tick,
            0.0
        );
        assert_eq!(
            harness
                .world_view
                .environment(later + TICK * 3, 12)
                .partial_tick,
            1.0
        );
    }

    #[test]
    fn a_changed_crack_set_rebuilds_the_crack_mesh_once() {
        let mut harness = Harness::new();
        harness.world_view.destroy_layers = Some([5; STAGES]);
        let crack = Crack {
            bounds: OutlineBox {
                min: [0.0, 64.0, 0.0],
                max: [1.0, 65.0, 1.0],
            },
            stage: 4,
        };
        let send = |harness: &mut Harness, cracks: &[Crack]| {
            let mut next = frame(glam::Vec3::ZERO, Instant::now(), 0);
            next.cracks = Arc::from(cracks);
            harness
                .frames
                .0
                .send(RenderEvent::Frame(next))
                .expect("frame queue");
            harness.sync();
        };

        send(&mut harness, &[crack]);
        let (vertices, _) = harness.world_view.take_crack_mesh().expect("a mesh");
        assert_eq!(vertices.len(), 24);
        assert!(harness.world_view.take_crack_mesh().is_none());

        send(&mut harness, &[crack]);
        assert!(
            harness.world_view.take_crack_mesh().is_none(),
            "the same cracks need no new mesh"
        );

        send(&mut harness, &[]);
        let (vertices, indices) = harness.world_view.take_crack_mesh().expect("a clear");
        assert!(vertices.is_empty() && indices.is_empty());
    }

    #[test]
    fn the_procedural_terrain_draws_no_cracks() {
        let mut harness = Harness::new();
        let mut next = frame(glam::Vec3::ZERO, Instant::now(), 0);
        next.cracks = Arc::from([Crack {
            bounds: OutlineBox {
                min: [0.0; 3],
                max: [1.0; 3],
            },
            stage: 0,
        }]);
        harness
            .frames
            .0
            .send(RenderEvent::Frame(next))
            .expect("frame queue");
        harness.sync();
        assert!(harness.world_view.take_crack_mesh().is_none());
    }

    #[test]
    fn the_newest_frame_decides_the_selection_outline() {
        let mut harness = Harness::new();
        let outline = OutlineBox {
            min: [0.0; 3],
            max: [1.0; 3],
        };
        let mut selected = frame(glam::Vec3::ZERO, Instant::now(), 0);
        selected.selection = Arc::from([outline]);
        harness
            .frames
            .0
            .send(RenderEvent::Frame(selected))
            .expect("frame queue");
        harness.sync();
        assert_eq!(harness.world_view.selection(), &[outline]);

        harness
            .frames
            .0
            .send(RenderEvent::Frame(frame(
                glam::Vec3::ZERO,
                Instant::now(),
                0,
            )))
            .expect("frame queue");
        harness.sync();
        assert!(harness.world_view.selection().is_empty());
    }
}
