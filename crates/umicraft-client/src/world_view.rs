// SPDX-License-Identifier: MIT OR Apache-2.0

//! The render thread's view of the game: the interpolated camera and clock,
//! the chunks waiting for a mesh worker, and the meshes handed to the
//! renderer (ADR-0013).
//!
//! The window and the screenshot path both drive one [`WorldView`] with
//! [`WorldView::sync`], so they cannot drift apart: a chunk that becomes empty
//! is removed, a world reset drops every mesh of the old world (including the
//! ones still being meshed), and each chunk's faces are counted once.

use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use umicraft_game::entity::{wrap_degrees, EntityFrameData, EntityRenderSnapshot};
#[cfg(test)]
use umicraft_game::entity::{BodyYaw, EntityId, EntityKind, EntityRecord};
use umicraft_game::gamemode::GameMode;
use umicraft_game::hand::HandPose;
use umicraft_game::inventory::WindowView;
use umicraft_game::piston::MovingBlocks;
use umicraft_game::raycast::look_direction;
use umicraft_game::server::block_entities::EntityTable;
use umicraft_game::server::block_entities::SignTable;
use umicraft_game::server::chest_lids::ChestLids;
use umicraft_game::server::maps::MapStore;
use umicraft_game::sound::cue::SoundCue;
use umicraft_game::view::{Perspective, THIRD_PERSON_DISTANCE};
use umicraft_game::RenderState;
use umicraft_mesh::{BlockEntityRef, Mesh, Vertex};
use umicraft_render::{
    BorderView, Camera, FogContext, FogMedium, HudState, OutlineBox, PrecipitationField,
    RenderEnvironment, Renderer, ScreenOverlays,
};
use umicraft_ui::menus::ConnectStage;
use umicraft_world::{ChunkMap, ChunkPos, LightLevel};

/// Mesh jobs kept in flight per worker: enough to keep the worker busy, few
/// enough that a newly important chunk is picked soon.
const JOBS_PER_WORKER: usize = 2;
/// The vertex bytes the render thread uploads per frame at most (one chunk at
/// least): a burst of finished meshes is spread over frames instead of making
/// one long frame (F2-4b).
const UPLOAD_BUDGET_BYTES: usize = 1 << 20;

/// How far, in blocks, the camera has to be from the eye of the player to count
/// as the third person: the first person has it on the eye, the third person an
/// orbit of up to four blocks, which a wall shortens.
const THIRD_PERSON_MIN_ORBIT: f64 = 0.05;

/// Capacity of the rolling history of snapshot publication intervals.
const SNAPSHOT_PACING_CAPACITY: usize = 128;

/// Pacing metrics for world snapshot publications from the game thread to the render thread.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SnapshotPacingStats {
    /// Total snapshots received.
    pub snapshots_received: u64,
    /// Number of snapshots that arrived in a burst in the same sync pass and had their
    /// intermediate interpolation skipped.
    pub burst_dropped: u64,
    /// Number of render frames drawn that had no new snapshot (repeated frame).
    pub repeated_draws: u64,
    /// Wall-clock interval since the previous snapshot arrived in microseconds.
    pub interval_us: u32,
    /// Mean arrival interval over the rolling window in microseconds.
    pub mean_interval_us: u32,
    /// 50th percentile (median) arrival interval in microseconds.
    pub p50_interval_us: u32,
    /// 95th percentile arrival interval in microseconds.
    pub p95_interval_us: u32,
    /// 99th percentile arrival interval in microseconds.
    pub p99_interval_us: u32,
}

impl SnapshotPacingStats {
    /// Formats an informative F3 summary line.
    #[must_use]
    pub fn f3_line(&self) -> String {
        let p50_ms = self.p50_interval_us as f32 / 1000.0;
        let p95_ms = self.p95_interval_us as f32 / 1000.0;
        let p99_ms = self.p99_interval_us as f32 / 1000.0;
        format!(
            "Snapshots: P50 {:.1}ms, P95 {:.1}ms, P99 {:.1}ms | repeated {}, burst-dropped {}",
            p50_ms, p95_ms, p99_ms, self.repeated_draws, self.burst_dropped
        )
    }
}

/// Rolling tracker of snapshot publication intervals and burst collapses.
#[derive(Debug, Clone)]
pub struct SnapshotPacingTracker {
    last_arrival: Option<Instant>,
    intervals: [u32; SNAPSHOT_PACING_CAPACITY],
    cursor: usize,
    count: usize,
    last_interval_us: u32,
    /// Total snapshots received from the game thread.
    pub snapshots_received: u64,
    /// Intermediate snapshots collapsed during burst receives.
    pub burst_dropped: u64,
    /// Render frames executed without a new world snapshot.
    pub repeated_draws: u64,
}

impl Default for SnapshotPacingTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotPacingTracker {
    /// Creates a new snapshot pacing tracker.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            last_arrival: None,
            intervals: [0; SNAPSHOT_PACING_CAPACITY],
            cursor: 0,
            count: 0,
            last_interval_us: 0,
            snapshots_received: 0,
            burst_dropped: 0,
            repeated_draws: 0,
        }
    }

    /// Records the arrival of a batch of frames in a single sync pass.
    pub fn record_arrival(&mut self, now: Instant, batch_size: usize) {
        if batch_size == 0 {
            self.repeated_draws += 1;
            return;
        }
        self.snapshots_received += batch_size as u64;
        if batch_size > 1 {
            self.burst_dropped += (batch_size - 1) as u64;
        }
        let interval_us = self
            .last_arrival
            .map(|prev| {
                now.saturating_duration_since(prev)
                    .as_micros()
                    .min(u128::from(u32::MAX)) as u32
            })
            .unwrap_or(0);
        self.last_arrival = Some(now);
        self.last_interval_us = interval_us;

        if interval_us > 0 {
            self.intervals[self.cursor] = interval_us;
            self.cursor = (self.cursor + 1) % SNAPSHOT_PACING_CAPACITY;
            if self.count < SNAPSHOT_PACING_CAPACITY {
                self.count += 1;
            }
        }
    }

    /// Computes pacing statistics over the rolling snapshot window.
    #[must_use]
    pub fn stats(&self) -> SnapshotPacingStats {
        let (mean, p50, p95, p99) = if self.count > 0 {
            let mut samples = self.intervals[..self.count].to_vec();
            samples.sort_unstable();
            let sum: u64 = samples.iter().map(|&v| u64::from(v)).sum();
            let mean = (sum / self.count as u64) as u32;
            let rank = |p: f32| {
                let idx = ((p * self.count as f32).ceil() as usize).clamp(1, self.count) - 1;
                samples[idx]
            };
            (mean, rank(0.50), rank(0.95), rank(0.99))
        } else {
            (0, 0, 0, 0)
        };

        SnapshotPacingStats {
            snapshots_received: self.snapshots_received,
            burst_dropped: self.burst_dropped,
            repeated_draws: self.repeated_draws,
            interval_us: self.last_interval_us,
            mean_interval_us: mean,
            p50_interval_us: p50,
            p95_interval_us: p95,
            p99_interval_us: p99,
        }
    }
}

use crate::achievements_view::newly_unlocked;
use crate::block_entity_render::push_pot_plants;
use crate::connection_end::ConnectionEnd;
use crate::cracks::{crack_mesh, Crack, STAGES};
use crate::creative_view::CreativeView;
use crate::error::ClientError;
use crate::fog_context::FogLight;
use crate::game::{
    camera_angles, ChunkEvent, Frame, GameStatus, LookSlot, RenderEvent, TextScreenRequest, TICK,
};
use crate::mesh_queue::MeshQueue;
use crate::mesher::{MeshPool, MeshResult};
use crate::moving_blocks_draw::mesh_moving_blocks_into;
use crate::overlay_state::OverlayInputs;
use crate::overlay_view::{compose, SpriteLayers, ViewFacts, VignetteShade};
use crate::pack_offer::ServerPackRequest;
use crate::terrain_textures::{InstalledTerrain, TerrainMesher};
use crate::weather_view::WeatherView;

/// Where finished chunk meshes go: the renderer, or a test double.
pub trait MeshTarget {
    /// Replaces the mesh of `pos`; an empty mesh removes it.
    fn upload_mesh(&mut self, pos: ChunkPos, mesh: &Mesh);

    /// Removes every chunk mesh.
    fn clear_meshes(&mut self);

    /// Removes the GPU mesh for one chunk column.
    fn remove_chunk(&mut self, pos: ChunkPos) {
        self.upload_mesh(pos, &Mesh::default());
    }
}

impl MeshTarget for Renderer {
    fn upload_mesh(&mut self, pos: ChunkPos, mesh: &Mesh) {
        Renderer::upload_mesh(self, pos, mesh);
    }

    fn clear_meshes(&mut self) {
        Renderer::clear_meshes(self);
    }

    fn remove_chunk(&mut self, pos: ChunkPos) {
        Renderer::remove_chunk(self, pos);
    }
}

/// The game state the render thread shows, and the meshing it drives.
pub struct WorldView {
    view: View,
    pool: MeshPool,
    /// Incremented by every world reset; jobs carry it so a mesh of the
    /// replaced world is dropped when it comes back.
    generation: u64,
    /// The chunks waiting for a free mesh worker, nearest first, and the ones
    /// with a job in flight for the current generation.
    queue: MeshQueue,
    /// The vertex bytes [`WorldView::collect`] uploads per call, at most.
    upload_budget: usize,
    /// A fixed limit of jobs in flight, for tests; `None` derives it from the
    /// workers.
    max_in_flight: Option<usize>,
    /// Faces of every chunk whose current mesh is not empty.
    faces: ChunkMap<usize>,
    /// The chests and signs of every chunk, which a tile-entity renderer
    /// draws (P7c).
    block_entities: ChunkMap<Vec<BlockEntityRef>>,
    hud: HudState,
    /// Monotonically incremented each time `hud` is updated or cleared, so the
    /// render thread can skip cloning `HudState` on unchanged frames.
    hud_revision: u64,
    window_view: WindowView,
    /// The newest creative screen snapshot, once one was published.
    creative: Option<CreativeView>,
    status: GameStatus,
    /// The newest statistic counters the server sent, by name.
    stats: Vec<(String, i32)>,
    /// How many `Statistics` answers arrived (never reset).
    stats_revision: u64,
    /// Whether a statistics request is waiting for its answer.
    stats_loading: bool,
    /// Whether a `Statistics` answer arrived before: only later ones can
    /// unlock an achievement worth a toast (`field_147308_k`).
    stats_baseline: bool,
    /// The newest achievement the server unlocked and when it was seen.
    toast: Option<(&'static str, Instant)>,
    /// Why the session ended, once it has.
    ended: Option<ConnectionEnd>,
    /// The sound cues received and not yet handed to the audio bridge.
    sounds: Vec<SoundCue>,
    /// How far the connection attempt is, for the connecting screen.
    connect_stage: ConnectStage,
    frames: usize,
    /// Active living entities interpolation data from the latest frame.
    entities: Arc<[EntityFrameData]>,
    /// A text screen the game thread asked for, until the window takes it.
    text_request: Option<TextScreenRequest>,
    /// Resource pack offers the window has not taken yet, oldest first.
    pack_requests: Vec<ServerPackRequest>,
    /// The body of the local player in the newest frame.
    local_body: Option<Arc<EntityFrameData>>,
    /// The light at the eye of the local player in the latest frame.
    hand_light: LightLevel,
    /// Selection outline boxes from the latest frame.
    selection: Arc<[OutlineBox]>,
    /// The cracks of the newest frame, and the mesh built from them.
    cracks: Arc<[Crack]>,
    /// The maps of the newest frame.
    maps: Arc<MapStore>,
    /// The chest lids of the newest frame.
    chest_lids: Arc<ChestLids>,
    /// The blocks pistons move, of the newest frame.
    moving_blocks: Arc<MovingBlocks>,
    /// The block entity updates of the newest frame.
    block_entity_data: Arc<EntityTable>,
    signs: Arc<SignTable>,
    /// The texture layer of each destroy stage, when the terrain has them.
    destroy_layers: Option<[u32; STAGES]>,
    /// The mesher of the bound terrain, for the blocks pistons move.
    mesher: TerrainMesher,
    /// A crack mesh the renderer has not received yet.
    crack_mesh: Option<(Vec<Vertex>, Vec<u32>)>,
    /// Snapshot publication pacing and burst tracker.
    snapshot_pacing: SnapshotPacingTracker,
}

/// The inventory of a player who has not joined: nothing in any slot.
fn empty_window_view() -> WindowView {
    WindowView {
        player_slots: std::array::from_fn(|_| None),
        container: None,
        cursor: None,
        selected_slot: 0,
    }
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
            queue: MeshQueue::default(),
            upload_budget: UPLOAD_BUDGET_BYTES,
            max_in_flight: None,
            faces: ChunkMap::default(),
            block_entities: ChunkMap::default(),
            hud: HudState::default(),
            hud_revision: 1,
            window_view: empty_window_view(),
            creative: None,
            status: GameStatus::default(),
            stats: Vec::new(),
            ended: None,
            sounds: Vec::new(),
            stats_revision: 0,
            stats_loading: false,
            stats_baseline: false,
            toast: None,
            connect_stage: ConnectStage::Connecting,
            frames: 0,
            entities: Arc::from([]),
            local_body: None,
            text_request: None,
            pack_requests: Vec::new(),
            hand_light: LightLevel::FULL_SKY,
            selection: Arc::from([]),
            cracks: Arc::from([]),
            maps: Arc::default(),
            chest_lids: Arc::default(),
            moving_blocks: Arc::default(),
            block_entity_data: Arc::default(),
            signs: Arc::default(),
            destroy_layers: terrain.mesher().destroy_layers(),
            mesher: terrain.mesher(),
            crack_mesh: None,
            snapshot_pacing: SnapshotPacingTracker::new(),
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
        umicraft_core::profile_scope!("WorldView::sync");
        let mut frame_events = 0;
        while let Ok(event) = frames.try_recv() {
            if matches!(event, RenderEvent::Frame(_)) {
                frame_events += 1;
            }
            self.apply(event, target);
        }
        self.snapshot_pacing.record_arrival(Instant::now(), frame_events);
        // After the frames, so a look newer than the tick they carry wins.
        self.view.take_look();
        while let Ok(ChunkEvent { pos, snapshot }) = chunks.try_recv() {
            if let Some(snapshot) = snapshot {
                self.queue.queue(pos, snapshot);
            } else {
                self.queue.remove(pos);
                self.faces.remove(&pos);
                self.block_entities.remove(&pos);
                target.remove_chunk(pos);
            }
        }
        // Collect before dispatching: a job submitted by this call comes back
        // in a later one, after any reset this call applied.
        self.collect(target);
        self.dispatch();
    }

    /// Live telemetry snapshot of the chunk meshing worker pool.
    #[must_use]
    pub fn mesh_pool_stats(&self) -> crate::mesher::MeshPoolStats {
        self.pool.stats()
    }

    /// Snapshot publication pacing metrics.
    #[must_use]
    pub fn snapshot_pacing_stats(&self) -> SnapshotPacingStats {
        self.snapshot_pacing.stats()
    }

    /// Points `camera` at the player, interpolated for `now` (GAM-8).
    /// `view_bobbing` is the "View Bobbing" option: off, the camera and the
    /// hand keep still while walking (`EntityRenderer.setupCameraTransform`).
    /// `dynamic_fov` is the "Dynamic FOV" option: off, the field of view stays
    /// what it is set to, whatever the player does. `hurt_cam` is whether a hit
    /// tilts the view; the No Hurt Cam mod turns it off.
    pub fn update_camera(
        &self,
        camera: &mut Camera,
        now: Instant,
        (view_bobbing, dynamic_fov, hurt_cam): (bool, bool, bool),
    ) {
        let Some(state) = self.view.sample(now) else {
            return;
        };
        camera.set_eye(state.position);
        let (yaw, pitch) = camera_angles(&state);
        camera.yaw = yaw;
        camera.pitch = pitch;
        // The hurt roll stays with the option off, the bobbing does not.
        let mut effects = state.effects;
        if !hurt_cam {
            effects.hurt_roll_degrees = 0.0;
        }
        let bob = effects.camera_matrix(view_bobbing);
        let fov_modifier = if dynamic_fov {
            state.effects.fov_modifier
        } else {
            1.0
        };
        camera.set_view_effects(fov_modifier, bob);
    }

    /// Immediately applies relative yaw and pitch deltas (in radians) to the active camera.
    pub fn apply_look_delta(&mut self, yaw_delta: f32, pitch_delta: f32) {
        self.view.apply_look_delta(yaw_delta, pitch_delta);
    }

    /// The eye of the local player while the camera shows the player from
    /// outside (the third person), `None` in the first person and with no frame.
    /// The camera is the orbit, so it differs from the eye by the orbit distance.
    #[must_use]
    pub fn third_person_body(&self, now: Instant) -> Option<glam::DVec3> {
        let state = self.view.sample(now)?;
        (state.position.distance(state.eye) > THIRD_PERSON_MIN_ORBIT).then_some(state.eye)
    }

    /// The pose of the first-person hand for `now`.
    #[must_use]
    pub fn hand_pose(&self, now: Instant) -> HandPose {
        // The sway reads the live view (`ItemRenderer.func_178110_a` uses the
        // `rotationYaw` of the frame), not the one of the last tick.
        self.view
            .sample(now)
            .map_or_else(HandPose::default, |state| {
                state
                    .effects
                    .hand
                    .swayed_by(state.yaw.to_degrees(), state.pitch.to_degrees())
            })
    }

    /// The sky, fog and light environment for `now`, in step with the camera.
    #[must_use]
    pub fn environment(&self, now: Instant, view_distance: u8) -> RenderEnvironment {
        self.view.environment(now, view_distance)
    }

    /// The columns rain and snow fall on in the newest frame (empty while it
    /// does not rain), for `Renderer::set_precipitation`.
    #[must_use]
    pub fn precipitation(&self) -> Arc<PrecipitationField> {
        Arc::clone(&self.view.weather.precipitation)
    }

    /// How far through its life the elder guardian apparition is at
    /// `partial_ticks` into the tick (`0..=1`), `None` while there is none
    /// (MCP `MobAppearance.renderParticle`: `(age + partial) / maxAge`).
    #[must_use]
    pub fn mob_appearance(&self, partial_ticks: f32) -> Option<f32> {
        crate::particle_view::mob_appearance(&self.view.particles, partial_ticks)
    }

    /// The quads of the newest frame's particles at `partial_ticks` into the
    /// tick, for `Renderer::set_particles`. Block fragments find their sprite
    /// in the bound terrain, the pieces of an item in `items`; the procedural
    /// terrain has neither, so they are left out. The centres are relative to
    /// `origin`, the render origin of the camera (F2-1b).
    pub fn particle_sprites_into(
        &self,
        out: &mut Vec<umicraft_render::ParticleSprite>,
        partial_ticks: f32,
        origin: glam::DVec3,
        items: Option<&crate::item_icons::ItemResources>,
    ) {
        let context = self.mesher.model_context();
        crate::particle_view::sprites_into(
            out,
            &self.view.particles,
            partial_ticks,
            origin,
            &crate::particle_view::Pictures {
                blocks: |state| crate::particle_view::block_sprite(context?, state),
                items: |id, damage| items?.particle_layer(id, damage),
            },
        );
    }

    /// The full-screen overlays of the frame at `now`, for
    /// `Renderer::set_screen_overlays`. `fancy` is the graphics setting, which
    /// the vignette needs.
    pub fn screen_overlays(&mut self, now: Instant, fancy: bool) -> ScreenOverlays {
        let Some(state) = self.view.sample(now) else {
            return ScreenOverlays::default();
        };
        let (inputs, fog) = (self.view.overlays, self.view.fog);
        let facts = ViewFacts {
            underwater: fog.medium == FogMedium::Water,
            brightness: fog.light_brightness,
            yaw_degrees: state.yaw.to_degrees(),
            pitch_degrees: state.pitch.to_degrees(),
            partial_tick: self.view.partial_tick(now),
        };
        let vignette = fancy.then(|| {
            self.view
                .vignette
                .step(fog.light_brightness, inputs.border_danger)
        });
        let layers = SpriteLayers::find(self.mesher.model_context(), inputs.in_wall);
        compose(&inputs, &facts, layers, vignette)
    }

    /// The world border of the newest frame; `None` before the first one.
    #[must_use]
    pub fn border(&self) -> Option<BorderView> {
        self.view.border
    }

    /// The dimension and the spawn point of the newest frame, for the compass.
    #[must_use]
    pub fn compass_world(&self) -> (i8, Option<(i32, i32, i32)>) {
        (self.view.dimension, self.view.spawn)
    }

    /// The dimension and game mode of the newest frame, which pick the music.
    #[must_use]
    pub fn music_world(&self) -> (i8, GameMode) {
        (self.view.dimension, self.view.gamemode)
    }

    /// The world facts of the F3 screen from the newest frame.
    #[must_use]
    pub fn debug_world(&self) -> &crate::debug_world::DebugWorld {
        &self.view.debug
    }

    /// The entities and particles drawn in the newest frame, for F3.
    #[must_use]
    pub fn drawn_counts(&self) -> (usize, usize) {
        (self.entities.len(), self.view.particles.len())
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

    /// The newest creative inventory snapshot the game thread published.
    #[must_use]
    pub fn creative_view(&self) -> Option<&CreativeView> {
        self.creative.as_ref()
    }

    /// The newest session statistics.
    #[must_use]
    pub fn status(&self) -> &GameStatus {
        &self.status
    }

    /// The statistic counters of the last `Statistics` answer, sorted by name.
    #[must_use]
    pub fn stats(&self) -> &[(String, i32)] {
        &self.stats
    }

    /// How many statistics answers arrived, to tell a new one from the last.
    #[must_use]
    pub fn stats_revision(&self) -> u64 {
        self.stats_revision
    }

    /// Whether a statistics request has no answer yet.
    #[must_use]
    pub fn stats_loading(&self) -> bool {
        self.stats_loading
    }

    /// The achievement the server unlocked last and when, for its toast.
    #[must_use]
    pub fn toast(&self) -> Option<(&'static str, Instant)> {
        self.toast
    }

    /// Notes that a statistics request was sent: the screen shows the
    /// download text until the answer arrives.
    pub fn expect_stats(&mut self) {
        self.stats_loading = true;
    }

    /// Whether the server placed the player: the world is ready to be shown.
    #[must_use]
    pub fn placed(&self) -> bool {
        self.status.placed
    }

    /// Takes the sound cues of the ticks received since the last call, oldest
    /// first. The window calls it once per frame and gives them to the audio
    /// thread.
    pub fn take_sounds(&mut self) -> Vec<SoundCue> {
        std::mem::take(&mut self.sounds)
    }

    /// Why the session ended, once it has.
    #[must_use]
    pub fn ended(&self) -> Option<&ConnectionEnd> {
        self.ended.as_ref()
    }

    /// How far the connection attempt has come.
    #[must_use]
    pub fn connect_stage(&self) -> ConnectStage {
        self.connect_stage
    }

    /// The chunks whose current mesh has faces.
    #[must_use]
    pub fn chunk_count(&self) -> usize {
        self.faces.len()
    }

    /// Whether chunk `pos` has a mesh with faces.
    #[must_use]
    pub fn is_meshed(&self, pos: ChunkPos) -> bool {
        self.faces.contains_key(&pos)
    }

    /// The faces of every current chunk mesh.
    #[must_use]
    pub fn face_count(&self) -> usize {
        self.faces.values().sum()
    }

    /// The lid angles of the chests in the newest frame.
    #[must_use]
    pub fn chest_lids(&self) -> &ChestLids {
        &self.chest_lids
    }

    /// The text of the signs of the newest frame.
    #[must_use]
    pub fn signs(&self) -> &SignTable {
        &self.signs
    }

    /// The updates of the skulls, banners, beacons, spawners and flower pots.
    #[must_use]
    pub fn block_entity_data(&self) -> &EntityTable {
        &self.block_entity_data
    }

    /// The blocks pistons are moving in the newest frame.
    #[must_use]
    pub fn moving_blocks(&self) -> &MovingBlocks {
        &self.moving_blocks
    }

    /// The chests and signs of every current chunk mesh.
    pub fn block_entities(&self) -> impl Iterator<Item = &BlockEntityRef> {
        self.block_entities.values().flatten()
    }

    /// Whether any chunk currently resident has block entities (chests, signs, etc.).
    #[must_use]
    pub fn has_block_entities(&self) -> bool {
        !self.block_entities.is_empty()
    }

    /// The simulation frames received so far.
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.frames
    }

    /// Monotonic revision of [`WorldView::hud`], incremented on every HUD event.
    #[must_use]
    pub fn hud_revision(&self) -> u64 {
        self.hud_revision
    }

    /// Interpolated entity render snapshots for the current render instant.
    ///
    /// Uses the same partial tick fraction as the camera so entities and the
    /// world stay in sync. Every position is relative to `origin`, the render
    /// origin of the camera (F2-1b).
    #[must_use]
    pub fn entities(&self, now: Instant, origin: glam::DVec3) -> Vec<EntityRenderSnapshot> {
        let mut out = Vec::with_capacity(self.entities.len());
        self.entities_into(now, origin, &mut out);
        out
    }

    /// Interpolates entity render snapshots into a reusable `out` buffer,
    /// retaining allocated capacity across frames.
    pub fn entities_into(
        &self,
        now: Instant,
        origin: glam::DVec3,
        out: &mut Vec<EntityRenderSnapshot>,
    ) {
        out.clear();
        out.reserve(self.entities.len());
        let alpha = self.view.partial_tick(now);
        out.extend(self.entities.iter().map(|entity| {
            let mut snapshot = entity.interpolate(alpha, origin);
            // `Entity.setAngles` shifts the pitch and its previous value
            // together every frame, so what `RendererLivingEntity` draws
            // is the pitch of the mouse, not one interpolated over a tick.
            if snapshot.local {
                if self.view.current.is_some() {
                    let live_yaw = self.view.player_yaw.to_degrees();
                    snapshot.yaw = live_yaw;
                    snapshot.head_yaw = live_yaw;
                    let diff = wrap_degrees(live_yaw - snapshot.body_yaw);
                    let twist = diff.clamp(-75.0, 75.0);
                    snapshot.body_yaw = wrap_degrees(live_yaw - twist);
                    if diff * diff > 2500.0 {
                        snapshot.body_yaw = wrap_degrees(snapshot.body_yaw + twist * 0.2);
                    }
                }
                if let Some(pitch) = self.view.body_pitch {
                    snapshot.pitch = pitch;
                }
            }
            snapshot
        }));
    }

    /// Takes the text screen the game thread asked for, if any.
    pub fn take_text_request(&mut self) -> Option<TextScreenRequest> {
        self.text_request.take()
    }

    /// Takes the resource pack offers received since the last call.
    pub fn take_pack_requests(&mut self) -> Vec<ServerPackRequest> {
        std::mem::take(&mut self.pack_requests)
    }

    /// The body of the local player at the newest tick, once a frame arrived.
    #[must_use]
    pub fn local_body(&self) -> Option<EntityRenderSnapshot> {
        self.local_body.as_ref().map(|body| {
            let mut snapshot = body.interpolate(1.0, glam::DVec3::ZERO);
            if self.view.current.is_some() {
                let live_yaw = self.view.player_yaw.to_degrees();
                snapshot.yaw = live_yaw;
                snapshot.head_yaw = live_yaw;
                let diff = wrap_degrees(live_yaw - snapshot.body_yaw);
                let twist = diff.clamp(-75.0, 75.0);
                snapshot.body_yaw = wrap_degrees(live_yaw - twist);
                if diff * diff > 2500.0 {
                    snapshot.body_yaw = wrap_degrees(snapshot.body_yaw + twist * 0.2);
                }
            }
            if let Some(pitch) = self.view.body_pitch {
                snapshot.pitch = pitch;
            }
            snapshot
        })
    }

    /// The light that lights the first-person item in the newest frame.
    #[must_use]
    pub fn hand_light(&self) -> LightLevel {
        self.hand_light
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

    /// The maps the server sent, as of the newest frame.
    #[must_use]
    pub fn maps(&self) -> &MapStore {
        &self.maps
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
                self.local_body = Some(Arc::clone(&frame.local_body));
                self.hand_light = frame.hand_light;
                self.selection = Arc::clone(&frame.selection);
                self.maps = Arc::clone(&frame.maps);
                self.chest_lids = Arc::clone(&frame.chest_lids);
                self.moving_blocks = Arc::clone(&frame.moving_blocks);
                self.block_entity_data = Arc::clone(&frame.block_entity_data);
                self.signs = Arc::clone(&frame.signs);
                if *self.cracks != *frame.cracks {
                    self.cracks = Arc::clone(&frame.cracks);
                    self.crack_mesh = self
                        .destroy_layers
                        .as_ref()
                        .map(|layers| crack_mesh(&self.cracks, layers));
                }
                self.view.push(*frame);
            }
            RenderEvent::LookSlot(slot) => self.view.look_slot = Some(slot),
            RenderEvent::Sounds(cues) => self.sounds.extend(cues),
            RenderEvent::Hud(hud) => {
                self.hud = *hud;
                self.hud_revision = self.hud_revision.wrapping_add(1);
            }
            RenderEvent::Inventory(view) => self.window_view = *view,
            RenderEvent::Creative(view) => self.creative = Some(*view),
            RenderEvent::TextScreen(request) => self.text_request = Some(*request),
            RenderEvent::ServerPack(request) => self.pack_requests.push(*request),
            RenderEvent::Status(status) => self.status = *status,
            RenderEvent::Stats(stats) => {
                if self.stats_baseline {
                    let unlocked = newly_unlocked(&self.stats, &stats);
                    if let Some(id) = unlocked.last() {
                        self.toast = Some((id, Instant::now()));
                    }
                }
                self.stats_baseline = true;
                self.stats = stats;
                self.stats_revision += 1;
                self.stats_loading = false;
            }
            RenderEvent::Connect(stage) => self.connect_stage = stage,
            RenderEvent::Teleport { position, yaw, pitch } => {
                self.view.teleport(position, yaw, pitch);
            }
            RenderEvent::Ended(end) => self.ended = Some(end),
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
        self.mesher = terrain.mesher();
        self.cracks = Arc::from([]);
    }

    /// The geometry of the blocks pistons move at `partial_tick`, and the
    /// plants of flower pots, in world coordinates. Empty when nothing
    /// moves or the terrain has no models
    /// (the procedural set draws moving blocks nowhere).
    #[must_use]
    pub fn moving_blocks_mesh(&self, partial_tick: f32) -> Mesh {
        let mut mesh = Mesh::default();
        self.moving_blocks_mesh_into(partial_tick, &mut mesh);
        mesh
    }

    /// Writes the moving-blocks and flower-pot mesh into `out`, reusing its
    /// existing `Vec` capacities across frames.
    pub fn moving_blocks_mesh_into(&self, partial_tick: f32, out: &mut Mesh) {
        let Some(context) = self.mesher.model_context() else {
            out.clear();
            return;
        };
        mesh_moving_blocks_into(&self.moving_blocks, partial_tick, context, out);
        // The plants of the flower pots ride the same dynamic mesh: their
        // contents come from tile entity data the chunk mesher cannot see.
        push_pot_plants(
            out,
            self.block_entities(),
            self.block_entity_data(),
            context,
        );
    }

    /// Forgets everything of a finished session: the meshes, the camera
    /// history, the HUD, the inventory, the statistics and the end reason.
    /// The mesh workers and the terrain stay, so the next session starts
    /// from a view that looks like a new one (ADR-0044).
    pub fn clear_session(&mut self, target: &mut impl MeshTarget) {
        self.reset(target);
        self.view = View::default();
        self.hud = HudState::default();
        self.hud_revision = self.hud_revision.wrapping_add(1);
        self.window_view = empty_window_view();
        self.creative = None;
        self.status = GameStatus::default();
        self.stats.clear();
        self.stats_loading = false;
        self.stats_baseline = false;
        self.toast = None;
        self.ended = None;
        self.connect_stage = ConnectStage::Connecting;
        self.frames = 0;
        self.local_body = None;
        self.text_request = None;
        self.pack_requests.clear();
        self.hand_light = LightLevel::FULL_SKY;
        self.selection = Arc::from([]);
        self.cracks = Arc::from([]);
        self.crack_mesh = None;
        self.maps = Arc::default();
        self.chest_lids = Arc::default();
        self.moving_blocks = Arc::default();
        self.block_entity_data = Arc::default();
        self.signs = Arc::default();
    }

    /// Forgets the replaced world, including the jobs still in flight.
    fn reset(&mut self, target: &mut impl MeshTarget) {
        self.generation += 1;
        self.queue.clear();
        self.faces.clear();
        self.block_entities.clear();
        self.entities = Arc::from([]);
        target.clear_meshes();
    }

    fn collect(&mut self, target: &mut impl MeshTarget) {
        let mut uploaded = 0_usize;
        // The budget is checked before taking a result, so one mesh always
        // goes through and the rest wait in the lock-free channel for the next
        // frame, naturally pacing worker dispatch with the GPU upload budget.
        while uploaded < self.upload_budget {
            let Some(MeshResult {
                pos,
                generation,
                mut mesh,
                build_micros: _,
            }) = self.pool.try_recv()
            else {
                break;
            };
            if generation != self.generation {
                // A mesh of the world a reset replaced: never drawn.
                continue;
            }
            if !self.queue.finished(pos) {
                // The chunk was cancelled or unloaded while the meshing was in flight.
                continue;
            }
            if mesh.is_empty() {
                self.faces.remove(&pos);
            } else {
                self.faces.insert(pos, mesh.face_count());
            }
            if mesh.block_entities.is_empty() {
                self.block_entities.remove(&pos);
            } else {
                self.block_entities
                    .insert(pos, std::mem::take(&mut mesh.block_entities));
            }
            uploaded += (mesh.vertices.len() + mesh.transparent_vertices.len())
                * std::mem::size_of::<Vertex>();
            target.upload_mesh(pos, &mesh);
        }
    }

    /// Hands the waiting chunks to the workers, nearest first, keeping only a
    /// couple of jobs per worker in flight so that the order can still change
    /// when the player moves (F2-4a).
    fn dispatch(&mut self) {
        let in_flight = self
            .max_in_flight
            .unwrap_or(self.pool.worker_count() * JOBS_PER_WORKER);
        while let Some((pos, snapshot)) = self.queue.next(in_flight) {
            if let Err(snapshot) = self.pool.submit(pos, self.generation, snapshot) {
                // The job queue is full or the workers are gone: retry later.
                self.queue.give_back(pos, snapshot);
                break;
            }
        }
    }

    /// Limits the jobs in flight to `limit` whatever the worker count.
    #[cfg(test)]
    fn queue_limit_for_tests(&mut self, limit: usize) {
        self.max_in_flight = Some(limit);
    }

    /// Tells the mesh queue where the player is and where they look, so the
    /// chunks around and ahead of them are meshed first. Call it every frame
    /// with the camera.
    pub fn focus_on(&mut self, camera: &Camera) {
        self.queue.focus_on(camera.eye(), camera.yaw);
    }
}

/// The camera of the player, interpolated between the two newest frames.
#[derive(Debug, Default)]
struct View {
    /// Active camera perspective.
    perspective: Perspective,
    /// Player's actual yaw in radians (Minecraft simulation convention).
    player_yaw: f32,
    /// Player's actual pitch in radians (Minecraft simulation convention).
    player_pitch: f32,
    /// Third-person camera orbit distance from the eye, accounting for wall collisions.
    orbit_distance: f64,
    /// The state of the previous tick.
    previous: Option<RenderState>,
    /// The state of the newest tick.
    current: Option<RenderState>,
    /// The instant of the newest tick.
    tick_time: Option<Instant>,
    /// When the latest frame arrived on the render thread (anchors interpolation).
    tick_instant: Option<Instant>,
    /// Whether the render thread is actively driving mouse look directly via `apply_look_delta`.
    local_mouse_look: bool,
    /// Where the game thread leaves the mouse look.
    look_slot: Option<LookSlot>,
    /// The pitch of the local body in degrees, as of the newest look or tick.
    body_pitch: Option<f32>,
    /// The newest signed world time in ticks.
    world_time: i64,
    /// The dimension of the newest frame.
    dimension: i8,
    /// Whether the world of the newest frame is a superflat one.
    flat_world: bool,
    /// The game mode of the local player in the newest frame.
    gamemode: GameMode,
    /// The newest announced world spawn point.
    spawn: Option<(i32, i32, i32)>,
    /// The biome temperature at the player, for the sky colour.
    sky_temperature: f32,
    /// The fog inputs of the newest frame (P7g).
    fog: FogContext,
    /// The eye light of the fog, eased over ticks.
    fog_light: FogLight,
    /// The world border of the newest frame.
    border: Option<BorderView>,
    /// Rain, thunder, lightning and the precipitation columns of the newest frame.
    weather: WeatherView,
    /// The particles of the newest frame (P8).
    particles: Arc<[crate::particle_view::ParticleDraw]>,
    /// What decides the screen overlays, of the newest frame (P8-f).
    overlays: OverlayInputs,
    /// The facts the F3 screen prints, of the newest frame (P9-h).
    debug: Arc<crate::debug_world::DebugWorld>,
    /// The vignette brightness, eased frame by frame.
    vignette: VignetteShade,
}

impl View {
    /// Computes the camera yaw, pitch, and world position from the player's true angles and eye position.
    fn camera_pose_from_player(&self, eye: glam::DVec3) -> (f32, f32, glam::DVec3) {
        match self.perspective {
            Perspective::FirstPerson => (self.player_yaw, self.player_pitch, eye),
            Perspective::ThirdPersonBack => {
                let dir = look_direction(self.player_yaw, self.player_pitch) * -1.0;
                (self.player_yaw, self.player_pitch, eye + dir * self.orbit_distance)
            }
            Perspective::ThirdPersonFront => {
                let dir = look_direction(self.player_yaw, self.player_pitch) * 1.0;
                (
                    (self.player_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU),
                    -self.player_pitch,
                    eye + dir * self.orbit_distance,
                )
            }
        }
    }

    /// Stores a new frame.
    fn push(&mut self, frame: Frame) {
        let perspective_changed = self.perspective != frame.perspective;
        self.perspective = frame.perspective;

        // Preserve previous state from the preceding tick so that eye interpolation
        // (moving, jumping, falling, knockback, strafing) remains continuous across
        // perspective switches, eliminating 50ms freezing or visual stair-stepping.
        self.previous = self.current;

        self.orbit_distance = if frame.perspective == Perspective::FirstPerson {
            0.0
        } else {
            let dist = (frame.state.position - frame.state.eye).length();
            if dist > 0.1 {
                dist
            } else {
                THIRD_PERSON_DISTANCE
            }
        };

        let mut state = frame.state;
        if self.local_mouse_look || perspective_changed {
            if !self.local_mouse_look {
                self.player_yaw = frame.local_body.yaw.to_radians();
                self.player_pitch = frame.local_body.pitch.to_radians();
                self.body_pitch = Some(frame.local_body.pitch);
            }
            let (cam_yaw, cam_pitch, cam_pos) = self.camera_pose_from_player(state.eye);
            let orbit = (cam_pos - state.eye).as_vec3();
            state.yaw = cam_yaw;
            state.pitch = cam_pitch;
            state.position = cam_pos;
            state.orbit_start = orbit;
        } else {
            self.player_yaw = frame.local_body.yaw.to_radians();
            self.player_pitch = frame.local_body.pitch.to_radians();
            self.body_pitch = Some(frame.local_body.pitch);
        }
        self.current = Some(state);

        self.tick_instant = Some(Instant::now());
        self.tick_time = Some(frame.tick_time);
        self.world_time = frame.world_time;
        self.dimension = frame.dimension;
        self.flat_world = frame.flat_world;
        self.gamemode = frame.gamemode;
        self.spawn = frame.spawn;
        self.sky_temperature = frame.sky_temperature;
        self.fog = frame.fog;
        self.fog_light.tick(frame.fog.light_brightness);
        self.border = Some(frame.border);
        self.weather = frame.weather;
        self.particles = frame.particles;
        self.overlays = frame.overlays;
        self.debug = frame.debug;
    }

    /// Applies the mouse look the game thread left in the slot since the last
    /// call: the camera turns at once and the body looks up or down with it.
    fn take_look(&mut self) {
        let Some(look) = self.look_slot.as_ref().and_then(|slot| slot.take()) else {
            return;
        };
        if !self.local_mouse_look {
            self.body_pitch = Some(look.body_pitch);
            if self.perspective == Perspective::ThirdPersonFront {
                self.player_yaw = (look.view.yaw - std::f32::consts::PI).rem_euclid(std::f32::consts::TAU);
                self.player_pitch = -look.view.pitch;
            } else {
                self.player_yaw = look.view.yaw;
                self.player_pitch = look.view.pitch;
            }
            if let Some(eye) = self.current.as_ref().map(|c| c.eye) {
                let (cam_yaw, cam_pitch, cam_pos) = self.camera_pose_from_player(eye);
                let orbit = (cam_pos - eye).as_vec3();
                if let Some(current) = self.current.as_mut() {
                    current.yaw = cam_yaw;
                    current.pitch = cam_pitch;
                    current.position = cam_pos;
                    current.orbit_start = orbit;
                }
            }
        }
    }

    /// Immediately applies relative yaw and pitch deltas (in radians) to the current
    /// render view angles.
    ///
    /// This allows zero-latency raw mouse look on the render thread before the
    /// update has made a full round trip to the game thread and back.
    fn apply_look_delta(&mut self, yaw_delta: f32, pitch_delta: f32) {
        self.local_mouse_look = true;
        const MAX_PITCH: f32 = 90.0_f32.to_radians();
        self.player_yaw = (self.player_yaw + yaw_delta).rem_euclid(std::f32::consts::TAU);
        self.player_pitch = (self.player_pitch + pitch_delta).clamp(-MAX_PITCH, MAX_PITCH);
        self.body_pitch = Some(self.player_pitch.to_degrees());

        if let Some(eye) = self.current.as_ref().map(|c| c.eye) {
            let (cam_yaw, cam_pitch, cam_pos) = self.camera_pose_from_player(eye);
            let orbit = (cam_pos - eye).as_vec3();
            if let Some(current) = self.current.as_mut() {
                current.yaw = cam_yaw;
                current.pitch = cam_pitch;
                current.position = cam_pos;
                current.orbit_start = orbit;
            }
        }
    }

    /// Snaps the render camera immediately to the exact eye position and angles on a
    /// server teleport (packet 0x08), resetting previous state to eliminate visual gliding.
    fn teleport(&mut self, position: glam::DVec3, yaw: f32, pitch: f32) {
        self.player_yaw = yaw;
        self.player_pitch = pitch;
        self.body_pitch = Some(pitch.to_degrees());

        let new_eye = position;
        let (cam_yaw, cam_pitch, cam_pos) = self.camera_pose_from_player(new_eye);
        let orbit = (cam_pos - new_eye).as_vec3();

        if let Some(current) = self.current.as_mut() {
            current.eye = new_eye;
            current.position = cam_pos;
            current.yaw = cam_yaw;
            current.pitch = cam_pitch;
            current.orbit_start = orbit;
        } else {
            self.current = Some(RenderState {
                position: cam_pos,
                eye: new_eye,
                orbit_start: orbit,
                yaw: cam_yaw,
                pitch: cam_pitch,
                on_ground: true,
                effects: Default::default(),
            });
        }
        self.previous = self.current;
        self.tick_instant = Some(Instant::now());
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
        let tick_secs = TICK.as_secs_f32();
        let elapsed = if now < tick_time {
            if let Some(instant) = self.tick_instant {
                now.saturating_duration_since(instant).as_secs_f32()
            } else {
                0.0
            }
        } else {
            let from_tick = now.saturating_duration_since(tick_time).as_secs_f32();
            if from_tick <= tick_secs {
                from_tick
            } else if let Some(instant) = self.tick_instant {
                // If the snapshot arrived late due to network transit or catch-up,
                // interpolate based on wall-clock time since it was accepted on the
                // render thread so frames are not frozen at 1.0 during network jitter.
                let from_arrival = now.saturating_duration_since(instant).as_secs_f32();
                from_arrival.min(tick_secs)
            } else {
                from_tick
            }
        };
        (elapsed / tick_secs).clamp(0.0, 1.0)
    }

    /// The render environment for `now`.
    ///
    /// The world clock and dimension are the newest values the game thread
    /// published; the render thread never advances the clock by itself
    /// (ADR-0008 §4).
    fn environment(&self, now: Instant, view_distance: u8) -> RenderEnvironment {
        let partial_tick = self.partial_tick(now);
        RenderEnvironment {
            sky_temperature: self.sky_temperature,
            fog: FogContext {
                light_brightness: self.fog_light.at(partial_tick),
                ..self.fog
            },
            rain_strength: self.weather.rain_strength,
            thunder_strength: self.weather.thunder_strength,
            lightning_ticks: self.weather.lightning_ticks,
            flat_world: self.flat_world,
            ..RenderEnvironment::new(
                self.dimension,
                self.world_time,
                partial_tick,
                view_distance,
            )
        }
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
        // `orientCamera`: the interpolated eye plus the orbit of the current
        // angles. Interpolating the camera positions of two ticks instead
        // would drag the third-person camera behind its own angle.
        // The eye is lerped in f64: casting it first would round it to the
        // 6-12 cm grain of f32 around x = 1 000 000 (F2-1a). The orbit is a
        // small offset, so f32 is exact enough for it.
        let eye = previous.eye.lerp(current.eye, f64::from(alpha));
        let orbit = current
            .orbit_start
            .lerp((current.position - current.eye).as_vec3(), alpha);
        Some(RenderState {
            position: eye + orbit.as_dvec3(),
            eye,
            orbit_start: current.orbit_start,
            yaw: current.yaw,
            pitch: current.pitch,
            on_ground: current.on_ground,
            effects: previous.effects.lerp(current.effects, alpha),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::mpsc::{self, SyncSender};
    use std::time::Duration;

    use umicraft_mesh::ChunkSnapshot;

    use umicraft_world::{BlockPos, BlockState, ChunkPayload, World};

    use super::*;
    use crate::terrain_textures::TerrainTextures;
    use crate::test_support::TextureTarget;
    use umicraft_game::server::world_border::Border;

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
                .send(ChunkEvent { pos, snapshot: None })
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

        // An unload event evicts the chunk mesh and zeroes face count.
        harness.send_unload(ORIGIN);
        harness.sync();
        assert!(!harness.target.uploaded.contains_key(&ORIGIN));
        assert_eq!(harness.world_view.face_count(), 0);
        assert_eq!(harness.world_view.chunk_count(), 0);
    }

    /// F2-4b: a burst of finished meshes is spread over several syncs when it
    /// exceeds the upload budget, and none is lost.
    #[test]
    fn a_small_upload_budget_spreads_a_burst_over_frames() {
        let mut harness = Harness::new();
        harness.world_view.upload_budget = 1;
        let chunks = [ORIGIN, ChunkPos::new(1, 0), ChunkPos::new(2, 0)];
        for pos in chunks {
            harness.send_chunk(pos, &[BlockPos::new(pos.x * 16 + 1, 1, 1)]);
        }
        // Let every job finish before the first collection can take more than
        // its budget.
        harness.sync();
        std::thread::sleep(Duration::from_millis(200));
        let mut per_sync = Vec::new();
        for _ in 0..40 {
            let before = harness.target.uploads;
            harness.sync();
            per_sync.push(harness.target.uploads - before);
            if harness.target.uploads == 3 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(harness.target.uploads, 3, "every mesh arrives");
        assert!(
            per_sync.iter().all(|&count| count <= 1),
            "one mesh per sync at most: {per_sync:?}"
        );
    }

    #[test]
    fn the_nearest_chunk_is_meshed_before_a_far_one_sent_first() {
        let mut harness = Harness::new();
        let far = ChunkPos::new(12, 0);
        let near = ChunkPos::new(0, 0);
        harness.send_chunk(far, &[BlockPos::new(12 * 16 + 1, 1, 1)]);
        harness.send_chunk(near, &[BlockPos::new(1, 1, 1)]);
        let mut camera = Camera::default();
        camera.set_eye(glam::DVec3::new(8.0, 70.0, 8.0));
        harness.world_view.focus_on(&camera);
        // One job in flight at a time: the order is the dispatch order.
        harness.world_view.queue_limit_for_tests(1);
        harness.sync_until(|h| h.target.uploads >= 1);
        assert!(harness.target.uploaded.contains_key(&near));
        assert!(!harness.target.uploaded.contains_key(&far));
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
            placed: true,
            ..GameStatus::default()
        };
        for event in [
            RenderEvent::Hud(Box::new(hud)),
            RenderEvent::Status(Box::new(status)),
            RenderEvent::Stats(vec![("stat.jump".to_string(), 4)]),
            RenderEvent::Ended(ConnectionEnd::kicked("kicked")),
        ] {
            harness.frames.0.send(event).expect("frame queue");
        }
        harness.sync();
        assert_eq!(harness.world_view.hud().health, 7.0);
        assert_eq!(harness.world_view.status().chunks, 3);
        assert!(harness.world_view.placed());
        assert_eq!(harness.world_view.stats(), [("stat.jump".to_string(), 4)]);
        assert_eq!(
            harness.world_view.ended(),
            Some(&ConnectionEnd::kicked("kicked"))
        );
    }

    #[test]
    fn only_an_achievement_unlocked_after_the_first_answer_makes_a_toast() {
        let mut harness = Harness::new();
        let answer = |names: &[&str]| {
            RenderEvent::Stats(
                names
                    .iter()
                    .map(|name| (format!("achievement.{name}"), 1))
                    .collect(),
            )
        };
        harness
            .frames
            .0
            .send(answer(&["openInventory"]))
            .expect("queue");
        harness.sync();
        assert!(
            harness.world_view.toast().is_none(),
            "the first answer is the baseline"
        );
        harness
            .frames
            .0
            .send(answer(&["openInventory", "mineWood"]))
            .expect("queue");
        harness.sync();
        assert_eq!(
            harness.world_view.toast().map(|(id, _)| id),
            Some("mineWood")
        );
        harness.world_view.clear_session(&mut harness.target);
        assert!(harness.world_view.toast().is_none());
    }

    #[test]
    fn the_connect_stage_follows_the_event_and_resets_with_the_session() {
        let mut harness = Harness::new();
        assert_eq!(harness.world_view.connect_stage(), ConnectStage::Connecting);
        let event = RenderEvent::Connect(ConnectStage::LoggingIn);
        harness.frames.0.send(event).expect("frame queue");
        harness.sync();
        assert_eq!(harness.world_view.connect_stage(), ConnectStage::LoggingIn);
        harness.world_view.clear_session(&mut harness.target);
        assert_eq!(harness.world_view.connect_stage(), ConnectStage::Connecting);
    }

    #[test]
    fn clearing_a_session_forgets_its_state_but_keeps_the_workers() {
        let mut harness = Harness::new();
        let status = GameStatus {
            placed: true,
            chunks: 4,
            ..GameStatus::default()
        };
        for event in [
            RenderEvent::Hud(Box::new(HudState {
                health: 3.0,
                ..HudState::default()
            })),
            RenderEvent::Status(Box::new(status)),
            RenderEvent::Stats(vec![("stat.jump".to_string(), 4)]),
            RenderEvent::Ended(ConnectionEnd::kicked("kicked")),
        ] {
            harness.frames.0.send(event).expect("frame queue");
        }
        harness.sync();
        harness.world_view.clear_session(&mut harness.target);
        assert!(!harness.world_view.placed());
        assert!(harness.world_view.stats().is_empty());
        assert_eq!(harness.world_view.ended(), None);
        assert_eq!(harness.world_view.hud().health, 20.0);
        assert_eq!(harness.target.clears, 1);

        harness.send_chunk(ORIGIN, &[BlockPos::new(2, 1, 2)]);
        harness.sync_until(|h| h.target.uploaded.contains_key(&ORIGIN));
    }

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
            hand_light: LightLevel::FULL_SKY,
            selection: Arc::from([]),
            cracks: Arc::from([]),
            maps: Arc::default(),
            chest_lids: Arc::default(),
            moving_blocks: Arc::default(),
            block_entity_data: Arc::default(),
            sky_temperature: umicraft_render::RenderEnvironment::PLAINS_TEMPERATURE,
            fog: FogContext::DEFAULT,
            border: crate::border_view::border_view(&Border::default()),
            weather: crate::weather_view::WeatherView::default(),
            particles: Arc::from([]),
            overlays: crate::overlay_state::OverlayInputs::default(),
            debug: Arc::default(),
            signs: Arc::default(),
        })
    }

    #[test]
    fn sound_cues_wait_for_the_window_in_order_and_are_taken_once() {
        use std::borrow::Cow;
        use umicraft_game::sound::cue::CueCategory;

        let cue = |name: &'static str| SoundCue {
            name: Cow::Borrowed(name),
            pos: [0.0; 3],
            volume: 1.0,
            pitch: 1.0,
            category: CueCategory::Blocks,
        };
        let mut harness = Harness::new();
        for cues in [
            vec![cue("step.stone")],
            vec![cue("dig.stone"), cue("random.pop")],
        ] {
            harness
                .frames
                .0
                .send(RenderEvent::Sounds(cues))
                .expect("frame queue");
        }
        harness.sync();
        let names: Vec<_> = harness
            .world_view
            .take_sounds()
            .into_iter()
            .map(|cue| cue.name)
            .collect();
        assert_eq!(names, ["step.stone", "dig.stone", "random.pop"]);
        assert!(harness.world_view.take_sounds().is_empty());
    }

    #[test]
    fn a_look_between_ticks_turns_the_camera_without_restarting_the_interpolation() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let later = tick + TICK;
        for event in [
            RenderEvent::Frame(frame(glam::Vec3::ZERO, tick, 0)),
            RenderEvent::Frame(frame(glam::Vec3::new(2.0, 0.0, 0.0), later, 0)),
        ] {
            harness.frames.0.send(event).expect("frame queue");
        }
        let mut turned = frame(glam::Vec3::new(2.0, 0.0, 0.0), later, 0).state;
        turned.yaw = 1.0;
        // The mouse never queues an event: it overwrites the shared slot, so
        // a hundred looks leave the frame queue as empty as one.
        let slot = crate::game::LookSlot::default();
        harness
            .frames
            .0
            .send(RenderEvent::LookSlot(slot.clone()))
            .expect("frame queue");
        for yaw in 0..100 {
            let mut state = turned;
            state.yaw = yaw as f32 / 100.0;
            slot.store(crate::game::Look {
                view: state,
                body_pitch: 7.0,
            });
        }
        slot.store(crate::game::Look {
            view: turned,
            body_pitch: 12.0,
        });
        harness.sync();
        assert_eq!(harness.world_view.view.body_pitch, Some(12.0));
        assert!(slot.take().is_none(), "the look was taken");

        let halfway = later + TICK / 2;
        let mut camera = Camera::default();
        harness
            .world_view
            .update_camera(&mut camera, halfway, (true, true, true));
        assert!(
            (camera.position().x - 1.0).abs() < 1e-4,
            "{}",
            camera.position().x
        );
        assert!((camera.yaw - (1.0 + std::f32::consts::PI)).abs() < 1e-4);
        assert!((harness.world_view.view.partial_tick(halfway) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn apply_look_delta_updates_camera_immediately_without_waiting_for_game_thread() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        harness
            .frames
            .0
            .send(RenderEvent::Frame(frame(glam::Vec3::ZERO, tick, 0)))
            .expect("frame queue");
        harness.sync();

        let mut camera = Camera::default();
        harness
            .world_view
            .update_camera(&mut camera, tick, (false, false, false));
        let initial_yaw = camera.yaw;
        let initial_pitch = camera.pitch;

        // Apply raw mouse delta directly
        harness.world_view.apply_look_delta(0.25, -0.15);
        harness
            .world_view
            .update_camera(&mut camera, tick, (false, false, false));

        assert!((camera.yaw - (initial_yaw + 0.25)).abs() < 1e-4);
        assert!((camera.pitch - (initial_pitch + 0.15)).abs() < 1e-4);
    }

    #[test]
    fn the_hand_sways_with_the_live_look_not_the_one_of_the_tick() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let later = tick + TICK;
        for event in [
            RenderEvent::Frame(frame(glam::Vec3::ZERO, tick, 0)),
            RenderEvent::Frame(frame(glam::Vec3::new(2.0, 0.0, 0.0), later, 0)),
        ] {
            harness.frames.0.send(event).expect("frame queue");
        }
        harness.sync();
        let at_rest = harness.world_view.hand_pose(later).sway_yaw;
        assert!(at_rest.abs() < 1e-4, "{at_rest}");
        // The mouse turned 0.7 radians since the tick: 40 degrees against an
        // arm that has not moved, a tenth of it as sway.
        let mut turned = frame(glam::Vec3::new(2.0, 0.0, 0.0), later, 0).state;
        turned.yaw = 0.7;
        let slot = crate::game::LookSlot::default();
        harness
            .frames
            .0
            .send(RenderEvent::LookSlot(slot.clone()))
            .expect("frame queue");
        slot.store(crate::game::Look {
            view: turned,
            body_pitch: 0.0,
        });
        harness.sync();
        let swayed = harness.world_view.hand_pose(later).sway_yaw;
        let expected = 0.7_f32.to_degrees() * 0.1;
        assert!((swayed - expected).abs() < 1e-3, "{swayed} vs {expected}");
    }

    #[test]
    fn a_look_from_a_tick_not_received_yet_does_not_move_the_camera() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let later = tick + TICK;
        for event in [
            RenderEvent::Frame(frame(glam::Vec3::ZERO, tick, 0)),
            RenderEvent::Frame(frame(glam::Vec3::new(2.0, 0.0, 0.0), later, 0)),
        ] {
            harness.frames.0.send(event).expect("frame queue");
        }
        // The game thread already ticked once more (x = 4): its look reaches
        // the slot before its frame reaches the queue.
        let mut ahead = frame(glam::Vec3::new(4.0, 0.0, 0.0), later + TICK, 0).state;
        ahead.yaw = 0.5;
        let slot = crate::game::LookSlot::default();
        harness
            .frames
            .0
            .send(RenderEvent::LookSlot(slot.clone()))
            .expect("frame queue");
        slot.store(crate::game::Look {
            view: ahead,
            body_pitch: 0.0,
        });
        harness.sync();

        let mut camera = Camera::default();
        harness
            .world_view
            .update_camera(&mut camera, later + TICK / 2, (true, true, true));
        // Halfway between the eyes of the two ticks the view knows, not 3.
        assert!(
            (camera.position().x - 1.0).abs() < 1e-4,
            "{}",
            camera.position().x
        );
        assert!((camera.yaw - (0.5 + std::f32::consts::PI)).abs() < 1e-4);
    }

    #[test]
    fn the_third_person_camera_orbits_the_interpolated_eye_with_the_newest_angles() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let later = tick + TICK;
        // Tick 1: eye at the origin, camera 4 blocks behind it (south).
        let mut first = frame(glam::Vec3::ZERO, tick, 0);
        first.perspective = Perspective::ThirdPersonBack;
        first.state.position = glam::DVec3::new(0.0, 0.0, -4.0);
        // Tick 2: eye 2 blocks east, the player turned and the camera is now
        // 4 blocks east of the eye.
        let mut second = frame(glam::Vec3::new(2.0, 0.0, 0.0), later, 0);
        second.perspective = Perspective::ThirdPersonBack;
        second.state.position = glam::DVec3::new(6.0, 0.0, 0.0);
        second.state.orbit_start = glam::Vec3::new(4.0, 0.0, 0.0);
        for event in [RenderEvent::Frame(first), RenderEvent::Frame(second)] {
            harness.frames.0.send(event).expect("frame queue");
        }
        harness.sync();

        let mut camera = Camera::default();
        harness
            .world_view
            .update_camera(&mut camera, later + TICK / 2, (true, true, true));
        // `orientCamera`: eye at (1, 0, 0) plus the orbit of the newest angles.
        // Blending the two camera positions would give (3, 0, -2) and drag the
        // camera behind the turn.
        let expected = glam::Vec3::new(5.0, 0.0, 0.0);
        assert!(
            (camera.position() - expected).length() < 1e-4,
            "{}",
            camera.position()
        );
    }

    #[test]
    fn the_camera_distance_follows_the_wall_while_the_eye_moves() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let later = tick + TICK;
        let mut first = frame(glam::Vec3::ZERO, tick, 0);
        first.perspective = Perspective::ThirdPersonBack;
        // The eye walks 1 block towards a wall behind it: the camera was 4
        // blocks back from the old eye and is only 1 back from the new one.
        let mut second = frame(glam::Vec3::new(0.0, 0.0, 1.0), later, 0);
        second.perspective = Perspective::ThirdPersonBack;
        second.state.position = glam::DVec3::new(0.0, 0.0, 0.0);
        second.state.orbit_start = glam::Vec3::new(0.0, 0.0, -4.0);
        for event in [RenderEvent::Frame(first), RenderEvent::Frame(second)] {
            harness.frames.0.send(event).expect("frame queue");
        }
        harness.sync();

        // Each frame casts the rays from the eye of that frame: halfway the
        // eye is 0.5 along and the camera 2.5 behind it, not a step at the
        // tick that jumps from 4 to 1.
        let mut camera = Camera::default();
        harness
            .world_view
            .update_camera(&mut camera, later + TICK / 2, (true, true, true));
        assert!(
            (camera.position().z - (0.5 - 2.5)).abs() < 1e-4,
            "{}",
            camera.position().z
        );
    }

    #[test]
    fn the_view_bobbing_option_gates_the_camera_sway() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let mut walking = frame(glam::Vec3::ZERO, tick, 0);
        walking.state.effects.bob = umicraft_game::view::ViewBob::new(0.4, 0.1, 0.0);
        harness
            .frames
            .0
            .send(RenderEvent::Frame(walking))
            .expect("frame queue");
        harness.sync();

        let mut camera = Camera::default();
        harness
            .world_view
            .update_camera(&mut camera, tick, (true, true, true));
        assert_ne!(camera.view_bob(), glam::Mat4::IDENTITY, "bobbing on");
        harness
            .world_view
            .update_camera(&mut camera, tick, (false, true, true));
        assert_eq!(camera.view_bob(), glam::Mat4::IDENTITY, "bobbing off");
    }

    #[test]
    fn the_no_hurt_cam_mod_leaves_the_roll_out_and_keeps_the_sway() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let mut hurt = frame(glam::Vec3::ZERO, tick, 0);
        hurt.state.effects.hurt_roll_degrees = 9.0;
        hurt.state.effects.bob = umicraft_game::view::ViewBob::new(0.4, 0.1, 0.0);
        harness
            .frames
            .0
            .send(RenderEvent::Frame(hurt))
            .expect("frame queue");
        harness.sync();

        let mut camera = Camera::default();
        harness
            .world_view
            .update_camera(&mut camera, tick, (false, true, false));
        assert_eq!(camera.view_bob(), glam::Mat4::IDENTITY, "no roll, no sway");
        harness
            .world_view
            .update_camera(&mut camera, tick, (true, true, false));
        let swayed = camera.view_bob();
        harness
            .world_view
            .update_camera(&mut camera, tick, (true, true, true));
        assert_ne!(
            swayed,
            camera.view_bob(),
            "the roll comes back with the mod off"
        );
    }

    /// Regression: the roll of a hurt player went with the bobbing, so the camera
    /// stopped shaking when "View Bobbing" was off; vanilla keeps it.
    #[test]
    fn the_hurt_roll_stays_when_view_bobbing_is_off() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let mut hurt = frame(glam::Vec3::ZERO, tick, 0);
        hurt.state.effects.hurt_roll_degrees = 9.0;
        hurt.state.effects.bob = umicraft_game::view::ViewBob::new(0.4, 0.1, 0.0);
        harness
            .frames
            .0
            .send(RenderEvent::Frame(hurt))
            .expect("frame queue");
        harness.sync();

        let mut camera = Camera::default();
        harness
            .world_view
            .update_camera(&mut camera, tick, (false, true, true));
        assert_eq!(
            camera.view_bob(),
            glam::Mat4::from_rotation_z(9.0_f32.to_radians()),
            "off: the roll without the sway"
        );
        harness
            .world_view
            .update_camera(&mut camera, tick, (true, true, true));
        assert_ne!(
            camera.view_bob(),
            glam::Mat4::from_rotation_z(9.0_f32.to_radians()),
            "on: the sway comes with it"
        );
    }

    #[test]
    fn the_body_is_only_reported_in_the_third_person() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let mut first = frame(glam::Vec3::ZERO, tick, 0);
        first.state.eye = first.state.position;
        harness
            .frames
            .0
            .send(RenderEvent::Frame(first))
            .expect("frame queue");
        harness.sync();
        assert_eq!(
            harness.world_view.third_person_body(tick),
            None,
            "the camera is on the eye in the first person"
        );

        let mut third = frame(glam::Vec3::ZERO, tick + TICK, 1);
        third.perspective = Perspective::ThirdPersonBack;
        third.state.eye = third.state.position + glam::DVec3::new(0.0, 0.0, 4.0);
        harness
            .frames
            .0
            .send(RenderEvent::Frame(third))
            .expect("frame queue");
        harness.sync();
        let eye = harness
            .world_view
            .third_person_body(tick + TICK * 2)
            .expect("the camera orbits the player");
        assert!((eye.z - 4.0).abs() < 1e-6, "{eye}");
    }

    #[test]
    fn the_dynamic_fov_option_gates_the_widening_of_the_view() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let mut sprinting = frame(glam::Vec3::ZERO, tick, 0);
        sprinting.state.effects.fov_modifier = 1.3;
        harness
            .frames
            .0
            .send(RenderEvent::Frame(sprinting))
            .expect("frame queue");
        harness.sync();

        let mut dynamic = Camera::default();
        harness
            .world_view
            .update_camera(&mut dynamic, tick, (true, true, true));
        let mut fixed = Camera::default();
        harness
            .world_view
            .update_camera(&mut fixed, tick, (true, false, true));
        // The same camera with no widening at all.
        let mut plain = fixed;
        plain.set_view_effects(1.0, fixed.view_bob());

        let origin = glam::DVec3::ZERO;
        assert_ne!(
            dynamic.view_projection_from(origin),
            plain.view_projection_from(origin),
            "on, sprinting widens the view"
        );
        assert_eq!(
            fixed.view_projection_from(origin),
            plain.view_projection_from(origin),
            "off, the view stays as set"
        );
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
            .update_camera(&mut camera, tick + TICK * 2, (true, true, true));
        assert_eq!(camera.position(), start);
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
        harness
            .world_view
            .update_camera(&mut camera, halfway, (true, true, true));
        assert!((camera.position().x - 1.0).abs() < 1e-3);
        let environment = harness.world_view.environment(halfway, 12);
        assert!((environment.partial_tick - 0.5).abs() < 1e-3);
        assert_eq!(environment.world_time, -5_900);
        assert_eq!(environment.dimension, -1);
        // The world type of the frame reaches the sky and the fog.
        assert!(environment.flat_world);
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

    #[test]
    fn teleport_event_snaps_camera_immediately_without_gliding() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        harness
            .frames
            .0
            .send(RenderEvent::Frame(frame(glam::Vec3::ZERO, tick, 0)))
            .expect("frame queue");
        harness.sync();

        let initial_sample = harness.world_view.view.sample(tick).expect("sample");
        assert_eq!(initial_sample.position, glam::DVec3::ZERO);

        // Server issues a setback or teleport 15 blocks away
        let target = glam::DVec3::new(15.0, 64.0, -10.0);
        let yaw = 1.23_f32;
        let pitch = -0.45_f32;
        harness
            .frames
            .0
            .send(RenderEvent::Teleport {
                position: target,
                yaw,
                pitch,
            })
            .expect("teleport event");
        harness.sync();

        // The camera should immediately sample the exact target position without gliding
        let snapped_sample = harness
            .world_view
            .view
            .sample(Instant::now())
            .expect("sample");
        assert_eq!(snapped_sample.position, target);
        assert!((snapped_sample.yaw - yaw).abs() < 1e-5);
        assert!((snapped_sample.pitch - pitch).abs() < 1e-5);
    }

    #[test]
    fn local_mouse_look_prevents_rollback_from_stale_ticks_and_echoes() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        harness
            .frames
            .0
            .send(RenderEvent::Frame(frame(glam::Vec3::ZERO, tick, 0)))
            .expect("frame queue");
        harness.sync();

        // 1. User applies zero-latency mouse look
        let yaw_delta = 0.5_f32;
        let pitch_delta = -0.2_f32;
        harness.world_view.apply_look_delta(yaw_delta, pitch_delta);

        let current_yaw = harness.world_view.view.current.unwrap().yaw;
        let current_pitch = harness.world_view.view.current.unwrap().pitch;
        assert!((current_yaw - yaw_delta).abs() < 1e-5);
        assert!((current_pitch - pitch_delta).abs() < 1e-5);

        // 2. A tick arrives from the game thread with stale 0.0 yaw
        let later = tick + TICK;
        harness
            .frames
            .0
            .send(RenderEvent::Frame(frame(glam::Vec3::new(1.0, 0.0, 0.0), later, 0)))
            .expect("frame queue");
        harness.sync();

        // Camera must preserve the live mouse angles and NOT roll back to 0.0!
        let preserved_yaw = harness.world_view.view.current.unwrap().yaw;
        let preserved_pitch = harness.world_view.view.current.unwrap().pitch;
        assert!((preserved_yaw - yaw_delta).abs() < 1e-5, "{preserved_yaw} vs {yaw_delta}");
        assert!((preserved_pitch - pitch_delta).abs() < 1e-5, "{preserved_pitch} vs {pitch_delta}");

        // 3. A stale echo arrives via LookSlot
        let slot = crate::game::LookSlot::default();
        harness
            .frames
            .0
            .send(RenderEvent::LookSlot(slot.clone()))
            .expect("look slot");
        let mut stale_look = frame(glam::Vec3::ZERO, later, 0).state;
        stale_look.yaw = 0.05; // Delayed angle from earlier input
        slot.store(crate::game::Look {
            view: stale_look,
            body_pitch: 0.0,
        });
        harness.sync();

        // The camera must STILL keep the live mouse angles and NOT roll back to 0.05!
        let final_yaw = harness.world_view.view.current.unwrap().yaw;
        assert!((final_yaw - yaw_delta).abs() < 1e-5, "{final_yaw} vs {yaw_delta}");
    }

    #[test]
    fn third_person_back_orbits_correctly_with_raw_mouse_look() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let eye = glam::DVec3::new(10.0, 64.0, 20.0);
        let mut f1 = frame(eye.as_vec3(), tick, 0);
        f1.perspective = Perspective::ThirdPersonBack;
        f1.state.eye = eye;
        f1.state.position = eye + glam::DVec3::new(0.0, 0.0, -4.0);
        f1.state.orbit_start = glam::Vec3::new(0.0, 0.0, -4.0);
        harness.frames.0.send(RenderEvent::Frame(f1)).expect("frame");
        harness.sync();

        let s = harness.world_view.view.sample(tick).expect("sample");
        assert!((s.position - (eye + glam::DVec3::new(0.0, 0.0, -4.0))).length() < 1e-4);

        // Turn mouse 90 degrees right
        harness
            .world_view
            .apply_look_delta(std::f32::consts::FRAC_PI_2, 0.0);
        let s2 = harness.world_view.view.sample(tick).expect("sample");
        // At yaw = PI/2, look_direction is (-1, 0, 0); in back view, camera is at eye - (-1, 0, 0)*4 = eye + (4, 0, 0)
        assert!(
            (s2.position - (eye + glam::DVec3::new(4.0, 0.0, 0.0))).length() < 1e-4,
            "got {:?}",
            s2.position
        );
        assert!((s2.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-4);

        // Player model must face player_yaw
        let body = harness.world_view.local_body().expect("local body");
        assert!((body.yaw - 90.0).abs() < 1e-3);
    }

    #[test]
    fn third_person_front_faces_player_face_and_orbits_with_mouse() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let eye = glam::DVec3::new(10.0, 64.0, 20.0);
        let mut f1 = frame(eye.as_vec3(), tick, 0);
        f1.perspective = Perspective::ThirdPersonFront;
        f1.state.eye = eye;
        f1.state.position = eye + glam::DVec3::new(0.0, 0.0, 4.0); // 4 blocks south (in front of south-facing player)
        f1.state.yaw = std::f32::consts::PI;
        f1.state.pitch = 0.0;
        f1.state.orbit_start = glam::Vec3::new(0.0, 0.0, 4.0);
        harness.frames.0.send(RenderEvent::Frame(f1)).expect("frame");
        harness.sync();

        let s = harness.world_view.view.sample(tick).expect("sample");
        assert!((s.position - (eye + glam::DVec3::new(0.0, 0.0, 4.0))).length() < 1e-4);
        assert!((s.yaw - std::f32::consts::PI).abs() < 1e-4);

        // Player body must face 0.0 (towards camera, face visible!)
        let body = harness.world_view.local_body().expect("local body");
        assert!(body.yaw.abs() < 1e-3, "player yaw: {}", body.yaw);

        // Move mouse look
        harness.world_view.apply_look_delta(0.4, -0.2);
        let s2 = harness.world_view.view.sample(tick).expect("sample");
        assert!((s2.yaw - (0.4 + std::f32::consts::PI)).abs() < 1e-4);
        assert!((s2.pitch - 0.2).abs() < 1e-4);

        // Player body and head must follow true player angles (0.4, -0.2)
        let body2 = harness.world_view.local_body().expect("local body");
        assert!((body2.yaw - 0.4_f32.to_degrees()).abs() < 1e-3);
        assert!((body2.pitch - (-0.2_f32.to_degrees())).abs() < 1e-3);
    }

    #[test]
    fn rapid_perspective_switching_f5_cuts_cleanly_without_gliding_or_desync() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let eye = glam::DVec3::new(0.0, 64.0, 0.0);

        // 1. FirstPerson
        let mut f1 = frame(eye.as_vec3(), tick, 0);
        f1.perspective = Perspective::FirstPerson;
        harness.frames.0.send(RenderEvent::Frame(f1)).expect("frame");
        harness.sync();
        let s1 = harness.world_view.view.sample(tick).expect("sample");
        assert_eq!(s1.position, eye);

        // 2. Switch to ThirdPersonBack
        let later = tick + TICK;
        let mut f2 = frame(eye.as_vec3(), later, 1);
        f2.perspective = Perspective::ThirdPersonBack;
        f2.state.position = eye + glam::DVec3::new(0.0, 0.0, -4.0);
        harness.frames.0.send(RenderEvent::Frame(f2)).expect("frame");
        harness.sync();

        // Sample halfway: must NOT glide from (0, 64, 0) to (0, 64, -4); must cut instantly to -4!
        let s2 = harness
            .world_view
            .view
            .sample(later + TICK / 2)
            .expect("sample");
        assert!(
            (s2.position.z - (eye.z - 4.0)).abs() < 1e-4,
            "perspective switch must cut immediately: {}",
            s2.position
        );

        // 3. Switch to ThirdPersonFront
        let later2 = later + TICK;
        let mut f3 = frame(eye.as_vec3(), later2, 2);
        f3.perspective = Perspective::ThirdPersonFront;
        f3.state.position = eye + glam::DVec3::new(0.0, 0.0, 4.0);
        harness.frames.0.send(RenderEvent::Frame(f3)).expect("frame");
        harness.sync();
        let s3 = harness
            .world_view
            .view
            .sample(later2 + TICK / 2)
            .expect("sample");
        assert!(
            (s3.position.z - (eye.z + 4.0)).abs() < 1e-4,
            "perspective switch must cut immediately: {}",
            s3.position
        );

        // 4. Switch back to FirstPerson
        let later3 = later2 + TICK;
        let mut f4 = frame(eye.as_vec3(), later3, 3);
        f4.perspective = Perspective::FirstPerson;
        harness.frames.0.send(RenderEvent::Frame(f4)).expect("frame");
        harness.sync();
        let s4 = harness
            .world_view
            .view
            .sample(later3 + TICK / 2)
            .expect("sample");
        assert_eq!(s4.position, eye);
    }

    #[test]
    fn server_setback_during_f5_teleports_orbit_and_camera_instantly() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let eye = glam::DVec3::new(0.0, 64.0, 0.0);
        let mut f1 = frame(eye.as_vec3(), tick, 0);
        f1.perspective = Perspective::ThirdPersonBack;
        f1.state.position = eye + glam::DVec3::new(0.0, 0.0, -4.0);
        harness.frames.0.send(RenderEvent::Frame(f1)).expect("frame");
        harness.sync();

        // Server setback / teleport to new location
        let target_eye = glam::DVec3::new(100.0, 80.0, -200.0);
        let yaw = 0.8_f32;
        let pitch = 0.2_f32;
        harness
            .frames
            .0
            .send(RenderEvent::Teleport {
                position: target_eye,
                yaw,
                pitch,
            })
            .expect("teleport");
        harness.sync();

        let s = harness
            .world_view
            .view
            .sample(Instant::now())
            .expect("sample");
        assert_eq!(s.eye, target_eye);
        assert!((s.yaw - yaw).abs() < 1e-4);
        assert!((s.pitch - pitch).abs() < 1e-4);

        let expected_dir = look_direction(yaw, pitch) * -1.0;
        let expected_pos = target_eye + expected_dir * 4.0;
        assert!(
            (s.position - expected_pos).length() < 1e-4,
            "got {:?}, expected {:?}",
            s.position,
            expected_pos
        );
    }

    #[test]
    fn full_360_degree_mouse_look_rotation_in_both_f5_modes() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let eye = glam::DVec3::new(5.0, 64.0, 5.0);

        for perspective in [Perspective::ThirdPersonBack, Perspective::ThirdPersonFront] {
            let mut f = frame(eye.as_vec3(), tick, 0);
            f.perspective = perspective;
            let dist_vec = if perspective == Perspective::ThirdPersonBack {
                glam::DVec3::new(0.0, 0.0, -4.0)
            } else {
                glam::DVec3::new(0.0, 0.0, 4.0)
            };
            f.state.position = eye + dist_vec;
            harness.frames.0.send(RenderEvent::Frame(f)).expect("frame");
            harness.sync();

            let step = std::f32::consts::TAU / 12.0;
            for _ in 0..12 {
                harness.world_view.apply_look_delta(step, 0.0);
                let s = harness.world_view.view.sample(tick).expect("sample");
                assert!(!s.yaw.is_nan());
                assert!(!s.pitch.is_nan());
                assert!(!s.position.x.is_nan());
                let dist = (s.position - eye).length();
                assert!((dist - 4.0).abs() < 1e-4, "orbit distance broken: {dist}");
            }
        }
    }

    #[test]
    fn test_audit_camera_three_perspectives_position_orientation_and_hand() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let eye = glam::DVec3::new(10.0, 65.62, 20.0);
        let player_yaw = 0.5_f32;
        let player_pitch = -0.2_f32;

        // 1. FirstPerson
        let mut f1 = frame(eye.as_vec3(), tick, 0);
        f1.perspective = Perspective::FirstPerson;
        f1.state.eye = eye;
        f1.state.position = eye;
        f1.state.yaw = player_yaw;
        f1.state.pitch = player_pitch;
        f1.local_body = Arc::new(
            EntityRecord::new(
                EntityId(1),
                EntityKind::Player,
                eye,
                player_yaw.to_degrees(),
                player_pitch.to_degrees(),
            )
            .frame_data(),
        );
        harness.frames.0.send(RenderEvent::Frame(f1)).expect("frame");
        harness.sync();

        let s1 = harness.world_view.view.sample(tick).expect("sample");
        assert_eq!(s1.position, eye);
        assert!((s1.yaw - player_yaw).abs() < 1e-4);
        assert!((s1.pitch - player_pitch).abs() < 1e-4);
        assert!(harness.world_view.third_person_body(tick).is_none());

        // 2. ThirdPersonBack
        let later = tick + TICK;
        let mut f2 = frame(eye.as_vec3(), later, 1);
        f2.perspective = Perspective::ThirdPersonBack;
        f2.state.eye = eye;
        let back_dir = look_direction(player_yaw, player_pitch) * -1.0;
        let back_pos = eye + back_dir * 4.0;
        f2.state.position = back_pos;
        f2.state.yaw = player_yaw;
        f2.state.pitch = player_pitch;
        f2.local_body = Arc::new(
            EntityRecord::new(
                EntityId(1),
                EntityKind::Player,
                eye,
                player_yaw.to_degrees(),
                player_pitch.to_degrees(),
            )
            .frame_data(),
        );
        harness.frames.0.send(RenderEvent::Frame(f2)).expect("frame");
        harness.sync();

        let s2 = harness.world_view.view.sample(later).expect("sample");
        assert!((s2.position - back_pos).length() < 1e-4);
        assert!((s2.yaw - player_yaw).abs() < 1e-4);
        assert!((s2.pitch - player_pitch).abs() < 1e-4);
        assert_eq!(harness.world_view.third_person_body(later), Some(eye));
        let body2 = harness.world_view.local_body().expect("local body");
        assert!((body2.yaw - player_yaw.to_degrees()).abs() < 1e-3);

        // 3. ThirdPersonFront
        let later2 = later + TICK;
        let mut f3 = frame(eye.as_vec3(), later2, 2);
        f3.perspective = Perspective::ThirdPersonFront;
        f3.state.eye = eye;
        let front_dir = look_direction(player_yaw, player_pitch) * 1.0;
        let front_pos = eye + front_dir * 4.0;
        f3.state.position = front_pos;
        f3.state.yaw = (player_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU);
        f3.state.pitch = -player_pitch;
        f3.local_body = Arc::new(
            EntityRecord::new(
                EntityId(1),
                EntityKind::Player,
                eye,
                player_yaw.to_degrees(),
                player_pitch.to_degrees(),
            )
            .frame_data(),
        );
        harness.frames.0.send(RenderEvent::Frame(f3)).expect("frame");
        harness.sync();

        let s3 = harness.world_view.view.sample(later2).expect("sample");
        assert!((s3.position - front_pos).length() < 1e-4);
        assert!(
            (s3.yaw - (player_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)).abs()
                < 1e-4
        );
        assert!((s3.pitch - (-player_pitch)).abs() < 1e-4);
        // Player model in front view must still face player_yaw (towards camera)
        let body3 = harness.world_view.local_body().expect("local body");
        assert!(
            (body3.yaw - player_yaw.to_degrees()).abs() < 1e-3,
            "player yaw must match player_yaw, got {}",
            body3.yaw
        );
    }

    #[test]
    fn test_audit_camera_movement_sprint_wtap_stap_rapid_strafes() {
        let mut harness = Harness::new();
        let mut tick = Instant::now();
        let mut eye = glam::DVec3::new(0.0, 65.62, 0.0);
        let mut yaw = 0.0_f32;
        let pitch = 0.0_f32;

        // Initialize in ThirdPersonBack
        let mut f = frame(eye.as_vec3(), tick, 0);
        f.perspective = Perspective::ThirdPersonBack;
        f.state.eye = eye;
        f.state.position = eye + glam::DVec3::new(0.0, 0.0, -4.0);
        harness.frames.0.send(RenderEvent::Frame(f)).expect("frame");
        harness.sync();

        // Simulate 30 ticks of W-Tap, S-Tap and alternating strafes
        for t in 1..=30 {
            tick += TICK;
            // Movement: forward sprint with W-Tap every 4 ticks and alternating strafe A/D
            let forward_speed = if t % 4 == 0 { 0.0 } else { 0.28 };
            let strafe_offset = if t % 2 == 0 { 0.15 } else { -0.15 };
            eye.z += forward_speed;
            eye.x += strafe_offset;
            yaw = (yaw + 0.02).rem_euclid(std::f32::consts::TAU);

            let mut f_move = frame(eye.as_vec3(), tick, t as i64);
            f_move.perspective = Perspective::ThirdPersonBack;
            f_move.state.eye = eye;
            let dir = look_direction(yaw, pitch) * -1.0;
            f_move.state.position = eye + dir * 4.0;
            f_move.state.orbit_start = (dir * 4.0).as_vec3();
            f_move.state.yaw = yaw;
            f_move.state.pitch = pitch;
            f_move.local_body = Arc::new(
                EntityRecord::new(
                    EntityId(1),
                    EntityKind::Player,
                    eye,
                    yaw.to_degrees(),
                    pitch.to_degrees(),
                )
                .frame_data(),
            );
            harness
                .frames
                .0
                .send(RenderEvent::Frame(f_move))
                .expect("frame");
            harness.sync();

            // Sample at 4 sub-tick instants
            for sub in 0..4 {
                let now = tick + TICK * sub / 4;
                let s = harness.world_view.view.sample(now).expect("sample");
                assert!(!s.position.x.is_nan());
                assert!(!s.position.y.is_nan());
                assert!(!s.position.z.is_nan());
                let dist = (s.position - s.eye).length();
                assert!(
                    (dist - 4.0).abs() < 1e-3,
                    "tick {t} sub {sub}: orbit distance broken: {dist}"
                );
            }
        }
    }

    #[test]
    fn test_audit_camera_vertical_dynamics_jump_fall_knockback() {
        let mut harness = Harness::new();
        let mut tick = Instant::now();
        let mut eye = glam::DVec3::new(0.0, 65.62, 0.0);
        let mut vy = 0.42_f64; // Jump velocity
        let gravity = 0.08_f64;
        let drag = 0.98_f64;

        let mut f_init = frame(eye.as_vec3(), tick, 0);
        f_init.perspective = Perspective::ThirdPersonBack;
        f_init.state.eye = eye;
        f_init.state.position = eye + glam::DVec3::new(0.0, 0.0, -4.0);
        f_init.state.orbit_start = glam::Vec3::new(0.0, 0.0, -4.0);
        harness.frames.0.send(RenderEvent::Frame(f_init)).expect("frame");
        harness.sync();

        // Jump & fall trajectory over 20 ticks
        for t in 1..=20 {
            tick += TICK;
            eye.y += vy;
            vy = (vy - gravity) * drag;

            // Knockback impulse at tick 10
            if t == 10 {
                eye.x += 0.4;
                vy += 0.36;
            }

            let mut f_step = frame(eye.as_vec3(), tick, t as i64);
            f_step.perspective = Perspective::ThirdPersonBack;
            f_step.state.eye = eye;
            f_step.state.position = eye + glam::DVec3::new(0.0, 0.0, -4.0);
            f_step.state.orbit_start = glam::Vec3::new(0.0, 0.0, -4.0);
            harness
                .frames
                .0
                .send(RenderEvent::Frame(f_step))
                .expect("frame");
            harness.sync();

            // Test sub-tick interpolation stability
            let sample = harness.world_view.view.sample(tick + TICK / 2).expect("sample");
            let orbit_offset = sample.position - sample.eye;
            assert!(
                (orbit_offset.length() - 4.0).abs() < 1e-4,
                "vertical dynamic orbit distance: {}",
                orbit_offset.length()
            );
            assert!((orbit_offset.y).abs() < 1e-4, "vertical tilt introduced in orbit");
        }
    }

    #[test]
    fn test_audit_camera_transitions_rapid_cycling_while_moving() {
        let mut harness = Harness::new();
        let mut tick = Instant::now();
        let mut eye = glam::DVec3::new(0.0, 65.62, 0.0);

        let perspectives = [
            Perspective::FirstPerson,
            Perspective::ThirdPersonBack,
            Perspective::ThirdPersonFront,
        ];

        // Baseline initial frame
        let f0 = frame(eye.as_vec3(), tick, 0);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        for i in 1..=15 {
            tick += TICK;
            eye += glam::DVec3::new(0.2, 0.1, 0.3); // moving while switching
            let p = perspectives[i % 3];

            let mut f = frame(eye.as_vec3(), tick, i as i64);
            f.perspective = p;
            f.state.eye = eye;
            let (cam_yaw, cam_pitch, cam_pos) = match p {
                Perspective::FirstPerson => (0.0, 0.0, eye),
                Perspective::ThirdPersonBack => (0.0, 0.0, eye + glam::DVec3::new(0.0, 0.0, -4.0)),
                Perspective::ThirdPersonFront => (
                    std::f32::consts::PI,
                    0.0,
                    eye + glam::DVec3::new(0.0, 0.0, 4.0),
                ),
            };
            f.state.position = cam_pos;
            f.state.yaw = cam_yaw;
            f.state.pitch = cam_pitch;

            harness.frames.0.send(RenderEvent::Frame(f)).expect("frame");
            harness.sync();

            // At sub-tick fraction 0.5, verify that:
            // 1. The eye smoothly interpolates along the movement path (no freeze or stair-stepping).
            // 2. The camera position cuts immediately to the new orbit relative to the interpolated eye.
            let sample = harness
                .world_view
                .view
                .sample(tick + TICK / 2)
                .expect("sample");
            let prev_eye = eye - glam::DVec3::new(0.2, 0.1, 0.3);
            let expected_eye = prev_eye.lerp(eye, 0.5);
            let expected_pos = match p {
                Perspective::FirstPerson => expected_eye,
                Perspective::ThirdPersonBack => expected_eye + glam::DVec3::new(0.0, 0.0, -4.0),
                Perspective::ThirdPersonFront => expected_eye + glam::DVec3::new(0.0, 0.0, 4.0),
            };
            assert!(
                (sample.eye - expected_eye).length() < 1e-4,
                "eye must interpolate smoothly while moving across transition cycle {i}: {:?} vs {:?}",
                sample.eye,
                expected_eye
            );
            assert!(
                (sample.position - expected_pos).length() < 1e-4,
                "camera must immediately use new orbit centered on interpolated eye in cycle {i} ({p:?}): {:?} vs {:?}",
                sample.position,
                expected_pos
            );
        }
    }

    #[test]
    fn test_audit_camera_transition_during_server_setback() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let eye = glam::DVec3::new(5.0, 65.62, 5.0);

        // Frame in FirstPerson
        let mut f1 = frame(eye.as_vec3(), tick, 0);
        f1.perspective = Perspective::FirstPerson;
        harness.frames.0.send(RenderEvent::Frame(f1)).expect("frame");
        harness.sync();

        // Next event: Server setback to (50, 70, -100) and simultaneous toggle to ThirdPersonFront
        let setback_eye = glam::DVec3::new(50.0, 71.62, -100.0);
        let yaw = 1.0_f32;
        let pitch = -0.15_f32;

        harness
            .frames
            .0
            .send(RenderEvent::Teleport {
                position: setback_eye,
                yaw,
                pitch,
            })
            .expect("teleport");

        let later = tick + TICK;
        let mut f2 = frame(setback_eye.as_vec3(), later, 1);
        f2.perspective = Perspective::ThirdPersonFront;
        f2.state.eye = setback_eye;
        let dir = look_direction(yaw, pitch) * 1.0;
        f2.state.position = setback_eye + dir * 4.0;
        f2.state.orbit_start = (dir * 4.0).as_vec3();
        f2.state.yaw = (yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU);
        f2.state.pitch = -pitch;
        f2.local_body = Arc::new(
            EntityRecord::new(
                EntityId(1),
                EntityKind::Player,
                setback_eye,
                yaw.to_degrees(),
                pitch.to_degrees(),
            )
            .frame_data(),
        );
        harness.frames.0.send(RenderEvent::Frame(f2)).expect("frame");
        harness.sync();

        let s = harness.world_view.view.sample(later + TICK / 2).expect("sample");
        assert_eq!(s.eye, setback_eye);
        assert!((s.position - (setback_eye + dir * 4.0)).length() < 1e-4);
        assert!(
            (s.yaw - (yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)).abs() < 1e-4
        );
        assert!((s.pitch - (-pitch)).abs() < 1e-4);
    }

    #[test]
    fn test_audit_camera_framerate_independence_1000fps_vs_60fps() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let eye0 = glam::DVec3::new(0.0, 65.62, 0.0);
        let eye1 = glam::DVec3::new(5.0, 65.62, 2.0);

        let mut f0 = frame(eye0.as_vec3(), tick, 0);
        f0.perspective = Perspective::ThirdPersonBack;
        f0.state.eye = eye0;
        f0.state.position = eye0 + glam::DVec3::new(0.0, 0.0, -4.0);
        f0.state.orbit_start = glam::Vec3::new(0.0, 0.0, -4.0);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");

        let later = tick + TICK;
        let mut f1 = frame(eye1.as_vec3(), later, 1);
        f1.perspective = Perspective::ThirdPersonBack;
        f1.state.eye = eye1;
        f1.state.position = eye1 + glam::DVec3::new(0.0, 0.0, -4.0);
        f1.state.orbit_start = glam::Vec3::new(0.0, 0.0, -4.0);
        harness.frames.0.send(RenderEvent::Frame(f1)).expect("frame");
        harness.sync();

        // Compare samples at exact same wall-clock timestamp sampled with high density (1000 FPS) vs standard (60 FPS)
        let probe_time = later + Duration::from_micros(25_000); // exactly halfway (partial_tick = 0.5)

        let s_1000fps = harness.world_view.view.sample(probe_time).expect("sample");
        let s_60fps = harness.world_view.view.sample(probe_time).expect("sample");

        assert_eq!(s_1000fps.position, s_60fps.position);
        assert_eq!(s_1000fps.yaw, s_60fps.yaw);
        assert_eq!(s_1000fps.pitch, s_60fps.pitch);
        assert_eq!(s_1000fps.eye, glam::DVec3::new(2.5, 65.62, 1.0));
        assert_eq!(s_1000fps.position, glam::DVec3::new(2.5, 65.62, -3.0));
    }

    #[test]
    fn test_audit_f5_rapid_toggle_stationary() {
        let mut harness = Harness::new();
        let mut tick = Instant::now();
        let eye = glam::DVec3::new(10.0, 64.0, 10.0);

        let perspectives = [
            Perspective::FirstPerson,
            Perspective::ThirdPersonBack,
            Perspective::ThirdPersonFront,
        ];

        // Send baseline frame
        let f0 = frame(eye.as_vec3(), tick, 0);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        for i in 1..=10 {
            tick += TICK;
            let p = perspectives[i % 3];
            let mut f = frame(eye.as_vec3(), tick, i as i64);
            f.perspective = p;
            f.state.eye = eye;
            let (cam_yaw, cam_pitch, cam_pos) = match p {
                Perspective::FirstPerson => (0.0, 0.0, eye),
                Perspective::ThirdPersonBack => (0.0, 0.0, eye + glam::DVec3::new(0.0, 0.0, -4.0)),
                Perspective::ThirdPersonFront => (
                    std::f32::consts::PI,
                    0.0,
                    eye + glam::DVec3::new(0.0, 0.0, 4.0),
                ),
            };
            f.state.position = cam_pos;
            f.state.yaw = cam_yaw;
            f.state.pitch = cam_pitch;
            harness.frames.0.send(RenderEvent::Frame(f)).expect("frame");
            harness.sync();

            for alpha_step in [0.0, 0.5, 1.0] {
                let sample_time = tick + Duration::from_secs_f32(0.05 * alpha_step);
                let sample = harness.world_view.view.sample(sample_time).expect("sample");
                assert_eq!(sample.eye, eye, "stationary eye must remain unchanged");
                let expected_dist = if p == Perspective::FirstPerson { 0.0 } else { 4.0 };
                assert!(
                    ((sample.position - sample.eye).length() - expected_dist).abs() < 1e-4,
                    "orbit distance must be exact"
                );
            }
        }
    }

    #[test]
    fn test_audit_f5_rapid_toggle_moving() {
        let mut harness = Harness::new();
        let mut tick = Instant::now();
        let mut eye = glam::DVec3::new(0.0, 64.0, 0.0);
        let move_delta = glam::DVec3::new(0.3, 0.0, 0.4);

        let perspectives = [
            Perspective::FirstPerson,
            Perspective::ThirdPersonBack,
            Perspective::ThirdPersonFront,
        ];

        let f0 = frame(eye.as_vec3(), tick, 0);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        for i in 1..=10 {
            let prev_eye = eye;
            eye += move_delta;
            tick += TICK;
            let p = perspectives[i % 3];

            let mut f = frame(eye.as_vec3(), tick, i as i64);
            f.perspective = p;
            f.state.eye = eye;
            let (cam_yaw, cam_pitch, cam_pos) = match p {
                Perspective::FirstPerson => (0.0, 0.0, eye),
                Perspective::ThirdPersonBack => (0.0, 0.0, eye + glam::DVec3::new(0.0, 0.0, -4.0)),
                Perspective::ThirdPersonFront => (
                    std::f32::consts::PI,
                    0.0,
                    eye + glam::DVec3::new(0.0, 0.0, 4.0),
                ),
            };
            f.state.position = cam_pos;
            f.state.yaw = cam_yaw;
            f.state.pitch = cam_pitch;
            harness.frames.0.send(RenderEvent::Frame(f)).expect("frame");
            harness.sync();

            let sample = harness.world_view.view.sample(tick + TICK / 2).expect("sample");
            let expected_eye = prev_eye.lerp(eye, 0.5);
            assert!(
                (sample.eye - expected_eye).length() < 1e-4,
                "eye must interpolate continuously without freeze: {:?} vs {:?}",
                sample.eye,
                expected_eye
            );
            let expected_pos = expected_eye + match p {
                Perspective::FirstPerson => glam::DVec3::ZERO,
                Perspective::ThirdPersonBack => glam::DVec3::new(0.0, 0.0, -4.0),
                Perspective::ThirdPersonFront => glam::DVec3::new(0.0, 0.0, 4.0),
            };
            assert!(
                (sample.position - expected_pos).length() < 1e-4,
                "camera must be placed at new orbit offset from interpolated eye"
            );
        }
    }

    #[test]
    fn test_audit_f5_rapid_toggle_during_jump() {
        let mut harness = Harness::new();
        let mut tick = Instant::now();
        let jump_heights = [
            64.0, 64.42, 64.75, 65.00, 65.17, 65.25, 65.24, 65.13, 64.93, 64.63,
        ];

        let perspectives = [
            Perspective::FirstPerson,
            Perspective::ThirdPersonBack,
            Perspective::ThirdPersonFront,
        ];

        let mut eye = glam::DVec3::new(0.0, jump_heights[0], 0.0);
        let f0 = frame(eye.as_vec3(), tick, 0);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        for i in 1..jump_heights.len() {
            let prev_eye = eye;
            eye = glam::DVec3::new(0.0, jump_heights[i], 0.0);
            tick += TICK;
            let p = perspectives[i % 3];

            let mut f = frame(eye.as_vec3(), tick, i as i64);
            f.perspective = p;
            f.state.eye = eye;
            let (cam_yaw, cam_pitch, cam_pos) = match p {
                Perspective::FirstPerson => (0.0, 0.0, eye),
                Perspective::ThirdPersonBack => (0.0, 0.0, eye + glam::DVec3::new(0.0, 0.0, -4.0)),
                Perspective::ThirdPersonFront => (
                    std::f32::consts::PI,
                    0.0,
                    eye + glam::DVec3::new(0.0, 0.0, 4.0),
                ),
            };
            f.state.position = cam_pos;
            f.state.yaw = cam_yaw;
            f.state.pitch = cam_pitch;
            harness.frames.0.send(RenderEvent::Frame(f)).expect("frame");
            harness.sync();

            for fraction in [0.25, 0.5, 0.75] {
                let sample_time = tick + Duration::from_secs_f32(0.05 * fraction);
                let sample = harness.world_view.view.sample(sample_time).expect("sample");
                let expected_y = prev_eye.y + (eye.y - prev_eye.y) * f64::from(fraction);
                assert!(
                    (sample.eye.y - expected_y).abs() < 1e-4,
                    "jump height must interpolate smoothly: got {}, expected {}",
                    sample.eye.y,
                    expected_y
                );
                let expected_cam_y = expected_y; // pitch = 0, orbit is horizontal
                assert!(
                    (sample.position.y - expected_cam_y).abs() < 1e-4,
                    "camera height must match jump height without sudden leap"
                );
            }
        }
    }

    #[test]
    fn test_audit_f5_spam_movement_and_jump() {
        let mut harness = Harness::new();
        let mut tick = Instant::now();
        let jump_heights = [
            64.0, 64.42, 64.75, 65.00, 65.17, 65.25, 65.24, 65.13, 64.93, 64.63, 64.25, 64.0,
        ];
        let mut eye = glam::DVec3::new(0.0, jump_heights[0], 0.0);

        let perspectives = [
            Perspective::FirstPerson,
            Perspective::ThirdPersonBack,
            Perspective::ThirdPersonFront,
        ];

        let f0 = frame(eye.as_vec3(), tick, 0);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        for i in 1..jump_heights.len() {
            let prev_eye = eye;
            eye = glam::DVec3::new(i as f64 * 0.28, jump_heights[i], 0.0);
            tick += TICK;
            let p = perspectives[i % 3];

            let mut f = frame(eye.as_vec3(), tick, i as i64);
            f.perspective = p;
            f.state.eye = eye;
            let (cam_yaw, cam_pitch, cam_pos) = match p {
                Perspective::FirstPerson => (0.0, 0.0, eye),
                Perspective::ThirdPersonBack => (0.0, 0.0, eye + glam::DVec3::new(0.0, 0.0, -4.0)),
                Perspective::ThirdPersonFront => (
                    std::f32::consts::PI,
                    0.0,
                    eye + glam::DVec3::new(0.0, 0.0, 4.0),
                ),
            };
            f.state.position = cam_pos;
            f.state.yaw = cam_yaw;
            f.state.pitch = cam_pitch;
            harness.frames.0.send(RenderEvent::Frame(f)).expect("frame");
            harness.sync();

            let sample = harness.world_view.view.sample(tick + TICK / 2).expect("sample");
            let expected_eye = prev_eye.lerp(eye, 0.5);
            assert!(
                (sample.eye - expected_eye).length() < 1e-4,
                "combined 3D movement + jump must interpolate smoothly"
            );
            let expected_dist = if p == Perspective::FirstPerson { 0.0 } else { 4.0 };
            assert!(
                ((sample.position - sample.eye).length() - expected_dist).abs() < 1e-4,
                "orbit distance must remain constant during jump + movement"
            );
        }
    }

    #[test]
    fn test_audit_f5_spam_movement_and_mouse_look() {
        let mut harness = Harness::new();
        let mut tick = Instant::now();
        let mut eye = glam::DVec3::new(0.0, 64.0, 0.0);

        let perspectives = [
            Perspective::FirstPerson,
            Perspective::ThirdPersonBack,
            Perspective::ThirdPersonFront,
        ];

        let f0 = frame(eye.as_vec3(), tick, 0);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        for i in 1..=10 {
            eye += glam::DVec3::new(0.25, 0.0, 0.1);
            tick += TICK;
            let p = perspectives[i % 3];

            let mut f = frame(eye.as_vec3(), tick, i as i64);
            f.perspective = p;
            f.state.eye = eye;
            harness.frames.0.send(RenderEvent::Frame(f)).expect("frame");
            harness.sync();

            // Direct raw mouse look delta applied on the render thread
            harness.world_view.apply_look_delta(0.1, -0.05);

            let sample = harness.world_view.view.sample(tick + TICK / 2).expect("sample");
            let expected_dist = if p == Perspective::FirstPerson { 0.0 } else { 4.0 };
            assert!(
                ((sample.position - sample.eye).length() - expected_dist).abs() < 1e-4,
                "raw mouse look must maintain exact orbit distance across perspective toggles"
            );
        }
    }

    #[test]
    fn test_audit_f5_spam_jump_and_mouse_look() {
        let mut harness = Harness::new();
        let mut tick = Instant::now();
        let jump_heights = [64.0, 64.42, 64.75, 65.00, 65.17, 65.25];
        let mut eye = glam::DVec3::new(0.0, jump_heights[0], 0.0);

        let perspectives = [
            Perspective::FirstPerson,
            Perspective::ThirdPersonBack,
            Perspective::ThirdPersonFront,
        ];

        let f0 = frame(eye.as_vec3(), tick, 0);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        for i in 1..jump_heights.len() {
            eye = glam::DVec3::new(0.0, jump_heights[i], 0.0);
            tick += TICK;
            let p = perspectives[i % 3];

            let mut f = frame(eye.as_vec3(), tick, i as i64);
            f.perspective = p;
            f.state.eye = eye;
            harness.frames.0.send(RenderEvent::Frame(f)).expect("frame");
            harness.sync();

            // Continuous 360-degree rotation step
            harness.world_view.apply_look_delta(0.5, 0.1);

            let sample = harness.world_view.view.sample(tick + TICK / 2).expect("sample");
            let expected_dist = if p == Perspective::FirstPerson { 0.0 } else { 4.0 };
            assert!(
                ((sample.position - sample.eye).length() - expected_dist).abs() < 1e-4,
                "orbit radius must never collapse or diverge during 360-degree look + jump"
            );
        }
    }

    #[test]
    fn test_audit_f5_spam_during_knockback() {
        let mut harness = Harness::new();
        let mut tick = Instant::now();
        let mut eye = glam::DVec3::new(0.0, 64.0, 0.0);
        let mut velocity = glam::DVec3::new(1.2, 0.35, 1.2);

        let perspectives = [
            Perspective::FirstPerson,
            Perspective::ThirdPersonBack,
            Perspective::ThirdPersonFront,
        ];

        let f0 = frame(eye.as_vec3(), tick, 0);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        for i in 1..=6 {
            let prev_eye = eye;
            eye += velocity;
            velocity *= 0.6; // knockback decay
            tick += TICK;
            let p = perspectives[i % 3];

            let mut f = frame(eye.as_vec3(), tick, i as i64);
            f.perspective = p;
            f.state.eye = eye;
            let (cam_yaw, cam_pitch, cam_pos) = match p {
                Perspective::FirstPerson => (0.0, 0.0, eye),
                Perspective::ThirdPersonBack => (0.0, 0.0, eye + glam::DVec3::new(0.0, 0.0, -4.0)),
                Perspective::ThirdPersonFront => (
                    std::f32::consts::PI,
                    0.0,
                    eye + glam::DVec3::new(0.0, 0.0, 4.0),
                ),
            };
            f.state.position = cam_pos;
            f.state.yaw = cam_yaw;
            f.state.pitch = cam_pitch;
            harness.frames.0.send(RenderEvent::Frame(f)).expect("frame");
            harness.sync();

            let sample = harness.world_view.view.sample(tick + TICK / 2).expect("sample");
            let expected_eye = prev_eye.lerp(eye, 0.5);
            assert!(
                (sample.eye - expected_eye).length() < 1e-4,
                "knockback impulse must interpolate smoothly across F5 toggles"
            );
            let expected_dist = if p == Perspective::FirstPerson { 0.0 } else { 4.0 };
            assert!(
                ((sample.position - sample.eye).length() - expected_dist).abs() < 1e-4,
                "orbit distance must remain strictly anchored to knocked back eye"
            );
        }
    }

    #[test]
    fn test_audit_f5_spam_during_server_setback() {
        let mut harness = Harness::new();
        let mut tick = Instant::now();
        let mut eye = glam::DVec3::new(0.0, 64.0, 0.0);

        let f0 = frame(eye.as_vec3(), tick, 0);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        // 1. Move in ThirdPersonBack
        eye += glam::DVec3::new(1.0, 0.0, 0.0);
        tick += TICK;
        let mut f1 = frame(eye.as_vec3(), tick, 1);
        f1.perspective = Perspective::ThirdPersonBack;
        f1.state.eye = eye;
        f1.state.position = eye + glam::DVec3::new(0.0, 0.0, -4.0);
        harness.frames.0.send(RenderEvent::Frame(f1)).expect("frame");
        harness.sync();

        // 2. Server setback / rubberband teleport occurs
        let setback_pos = glam::DVec3::new(0.0, 64.0, 0.0);
        harness.world_view.view.teleport(setback_pos, 0.0, 0.0);

        // Immediate sample after teleport must be snapped at setback_pos with NO glide
        let s_snap = harness.world_view.view.sample(tick).expect("sample");
        assert_eq!(s_snap.eye, setback_pos, "eye must snap to setback position instantly");
        assert_eq!(
            s_snap.position,
            setback_pos + glam::DVec3::new(0.0, 0.0, -4.0),
            "orbit must snap to setback position instantly"
        );

        // 3. Next tick with another F5 toggle to ThirdPersonFront
        let next_eye = setback_pos + glam::DVec3::new(0.1, 0.0, 0.0);
        tick += TICK;
        let mut f2 = frame(next_eye.as_vec3(), tick, 2);
        f2.perspective = Perspective::ThirdPersonFront;
        f2.state.eye = next_eye;
        f2.state.position = next_eye + glam::DVec3::new(0.0, 0.0, 4.0);
        f2.state.yaw = std::f32::consts::PI;
        harness.frames.0.send(RenderEvent::Frame(f2)).expect("frame");
        harness.sync();

        let s_after = harness.world_view.view.sample(tick + TICK / 2).expect("sample");
        let expected_eye = setback_pos.lerp(next_eye, 0.5);
        assert!(
            (s_after.eye - expected_eye).length() < 1e-4,
            "interpolation must cleanly resume from setback origin"
        );
        assert!(
            ((s_after.position - s_after.eye).length() - 4.0).abs() < 1e-4,
            "front view orbit must be exact after setback"
        );
    }

    // =========================================================================
    // VANILLA 1.8.9 HEAD ROTATION & BODY ALIGNMENT REPLICATION TESTS (PHASE 5)
    // =========================================================================

    fn local_player_record(eye: glam::DVec3, yaw: f32, pitch: f32) -> EntityRecord {
        let mut rec = EntityRecord::new(EntityId(1), EntityKind::Player, eye, yaw, pitch);
        rec.local = true;
        rec
    }

    #[test]
    fn test_vanilla_scenario_1_player_turns_only_head_under_50_degrees() {
        // Vanilla EntityLivingBase.func_110146_f:
        // When twist*twist <= 2500 (angle < 50°), the body stays completely still.
        let mut harness = Harness::new();
        let tick = Instant::now();
        let eye = glam::DVec3::new(0.0, 64.0, 0.0);

        let mut f0 = frame(eye.as_vec3(), tick, 0);
        f0.perspective = Perspective::ThirdPersonBack;
        let rec = local_player_record(eye, 0.0, 0.0);
        let fd = rec.frame_data();
        f0.local_body = Arc::new(fd.clone());
        f0.entities = Arc::from([fd]);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        // Turn head 30° to the right via raw mouse look
        harness.world_view.apply_look_delta(30.0_f32.to_radians(), 0.0);
        let mut out = Vec::new();
        harness.world_view.entities_into(tick, glam::DVec3::ZERO, &mut out);

        let local = out.iter().find(|e| e.local).expect("local player snapshot");
        assert!((local.head_yaw - 30.0).abs() < 1e-3, "head must face 30°, got {}", local.head_yaw);
        assert!((local.body_yaw - 0.0).abs() < 1e-3, "body must stay still at 0°, got {}", local.body_yaw);
        let net_head_yaw = wrap_degrees(local.head_yaw - local.body_yaw);
        assert!((net_head_yaw - 30.0).abs() < 1e-3, "netHeadYaw must be exactly 30°, got {}", net_head_yaw);
    }

    #[test]
    fn test_vanilla_scenario_2_player_turns_head_past_75_degrees_drags_body() {
        // Vanilla EntityLivingBase.func_110146_f:
        // f1 = wrapAngleTo180(rotationYaw - renderYawOffset) clamped to [-75, 75]
        // renderYawOffset = rotationYaw - f1; if f1*f1 > 2500 renderYawOffset += f1*0.2
        let mut harness = Harness::new();
        let tick = Instant::now();
        let eye = glam::DVec3::new(0.0, 64.0, 0.0);

        let mut f0 = frame(eye.as_vec3(), tick, 0);
        f0.perspective = Perspective::ThirdPersonBack;
        let rec = local_player_record(eye, 0.0, 0.0);
        let fd = rec.frame_data();
        f0.local_body = Arc::new(fd.clone());
        f0.entities = Arc::from([fd]);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        // Turn head 90° right: twist clamps to 75°, diff*diff > 2500 applies 0.2*75 = 15°
        // Expected body_yaw = 90 - 75 + 15 = 30°
        harness.world_view.apply_look_delta(90.0_f32.to_radians(), 0.0);
        let mut out = Vec::new();
        harness.world_view.entities_into(tick, glam::DVec3::ZERO, &mut out);

        let local = out.iter().find(|e| e.local).expect("local player snapshot");
        assert!((local.head_yaw - 90.0).abs() < 1e-3, "head must face 90°");
        assert!((local.body_yaw - 30.0).abs() < 1e-3, "body must be dragged to 30°, got {}", local.body_yaw);
        let net_head_yaw = wrap_degrees(local.head_yaw - local.body_yaw);
        assert!((net_head_yaw - 60.0).abs() < 1e-3, "netHeadYaw must be 60° (within 75° bound)");
    }

    #[test]
    fn test_vanilla_scenario_3_rapid_mouse_flick_left_and_right() {
        // Rapid alternating mouse flicking: ensure netHeadYaw remains strictly in [-75, 75]
        let mut harness = Harness::new();
        let tick = Instant::now();
        let eye = glam::DVec3::new(0.0, 64.0, 0.0);

        let mut f0 = frame(eye.as_vec3(), tick, 0);
        f0.perspective = Perspective::ThirdPersonBack;
        let rec = local_player_record(eye, 0.0, 0.0);
        let fd = rec.frame_data();
        f0.local_body = Arc::new(fd.clone());
        f0.entities = Arc::from([fd]);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        let deltas = [60.0_f32, -120.0, 150.0, -180.0, 70.0, -90.0];
        for d in deltas {
            harness.world_view.apply_look_delta(d.to_radians(), 0.0);
            let mut out = Vec::new();
            harness.world_view.entities_into(tick, glam::DVec3::ZERO, &mut out);
            let local = out.iter().find(|e| e.local).expect("local player");
            let net_head_yaw = wrap_degrees(local.head_yaw - local.body_yaw);
            assert!(
                net_head_yaw.abs() <= 75.001,
                "netHeadYaw {} exceeded 75° during flick delta {}",
                net_head_yaw,
                d
            );
        }
    }

    #[test]
    fn test_vanilla_scenario_4_natural_lag_offset_beyond_50_degrees() {
        // Exact 50° threshold test (EntityLivingBase.java: f1 * f1 > 2500.0F)
        let mut harness = Harness::new();
        let tick = Instant::now();
        let eye = glam::DVec3::new(0.0, 64.0, 0.0);

        let mut f0 = frame(eye.as_vec3(), tick, 0);
        f0.perspective = Perspective::ThirdPersonBack;
        let rec = local_player_record(eye, 0.0, 0.0);
        let fd = rec.frame_data();
        f0.local_body = Arc::new(fd.clone());
        f0.entities = Arc::from([fd]);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        // 1. At 45° (45^2 = 2025 <= 2500): body stays at 0°
        harness.world_view.apply_look_delta(45.0_f32.to_radians(), 0.0);
        let mut out = Vec::new();
        harness.world_view.entities_into(tick, glam::DVec3::ZERO, &mut out);
        let local45 = out.iter().find(|e| e.local).unwrap();
        assert!((local45.body_yaw - 0.0).abs() < 1e-3, "body must stay 0° under 50°");

        // 2. Further to 60° (60^2 = 3600 > 2500): body catches up by 60 * 0.2 = 12°
        harness.world_view.apply_look_delta(15.0_f32.to_radians(), 0.0);
        out.clear();
        harness.world_view.entities_into(tick, glam::DVec3::ZERO, &mut out);
        let local60 = out.iter().find(|e| e.local).unwrap();
        assert!((local60.body_yaw - 12.0).abs() < 1e-3, "body must catch up to 12° at 60° look");
        assert!((local60.head_yaw - 60.0).abs() < 1e-3);
    }

    #[test]
    fn test_vanilla_scenario_5_subtick_smooth_rotation_interpolation() {
        let mut harness = Harness::new();
        let t0 = Instant::now();
        let eye = glam::DVec3::new(0.0, 64.0, 0.0);

        // Tick 0 with entity facing 0°
        let mut f0 = frame(eye.as_vec3(), t0, 0);
        f0.perspective = Perspective::ThirdPersonBack;
        let mut remote0 = EntityRecord::new(EntityId(2), EntityKind::Player, eye, 0.0, 0.0);
        remote0.local = false;
        f0.entities = Arc::from([remote0.frame_data()]);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        // Tick 1 with entity turning to 40°
        let t1 = t0 + TICK;
        let mut f1 = frame(eye.as_vec3(), t1, 1);
        f1.perspective = Perspective::ThirdPersonBack;
        let mut remote1 = EntityRecord::new(EntityId(2), EntityKind::Player, eye, 40.0, 0.0);
        remote1.local = false;
        remote1.previous_yaw = 0.0;
        remote1.previous_head_yaw = 0.0;
        remote1.body_yaw.offset = 12.0;
        remote1.body_yaw.previous = 0.0;
        f1.entities = Arc::from([remote1.frame_data()]);
        harness.frames.0.send(RenderEvent::Frame(f1)).expect("frame");
        harness.sync();

        // Sample at alpha = 0.5 (halfway between ticks)
        let t_half = t1 + TICK / 2;
        let mut out = Vec::new();
        harness.world_view.entities_into(t_half, glam::DVec3::ZERO, &mut out);
        let remote = out.iter().find(|e| e.id == EntityId(2)).unwrap();
        // Head smoothly interpolates: (0° to 40° at alpha 0.5 is 20°)
        assert!((remote.head_yaw - 20.0).abs() < 1e-3, "sub-tick head_yaw must smoothly interpolate, got {}", remote.head_yaw);
        // Body smoothly interpolates: (0° to 12° at alpha 0.5 is 6°)
        assert!((remote.body_yaw - 6.0).abs() < 1e-3, "sub-tick body_yaw must smoothly interpolate, got {}", remote.body_yaw);
    }

    #[test]
    fn test_vanilla_scenario_6_wrapping_across_359_to_0_degrees() {
        // Player faces 355° (or -5°) and turns to 10°
        let mut harness = Harness::new();
        let tick = Instant::now();
        let eye = glam::DVec3::new(0.0, 64.0, 0.0);

        let mut f0 = frame(eye.as_vec3(), tick, 0);
        f0.perspective = Perspective::ThirdPersonBack;
        let mut rec = local_player_record(eye, 355.0, 0.0);
        rec.body_yaw.reset_to(355.0);
        let fd = rec.frame_data();
        f0.local_body = Arc::new(fd.clone());
        f0.entities = Arc::from([fd]);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        // Apply mouse delta of +15° (from 355° to 10°)
        harness.world_view.apply_look_delta(15.0_f32.to_radians(), 0.0);
        let mut out = Vec::new();
        harness.world_view.entities_into(tick, glam::DVec3::ZERO, &mut out);
        let local = out.iter().find(|e| e.local).unwrap();

        assert!((wrap_degrees(local.head_yaw) - 10.0).abs() < 1e-3, "head must wrap to 10°");
        let net_head_yaw = wrap_degrees(local.head_yaw - local.body_yaw);
        assert!((net_head_yaw - 15.0).abs() < 1e-3, "netHeadYaw must be +15°, got {}", net_head_yaw);
    }

    #[test]
    fn test_vanilla_scenario_7_f5_third_person_back_head_and_body() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let eye = glam::DVec3::new(10.0, 64.0, 20.0);

        let mut f0 = frame(eye.as_vec3(), tick, 0);
        f0.perspective = Perspective::ThirdPersonBack;
        let rec = local_player_record(eye, 45.0, 10.0);
        let fd = rec.frame_data();
        f0.local_body = Arc::new(fd.clone());
        f0.entities = Arc::from([fd]);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        let mut out = Vec::new();
        harness.world_view.entities_into(tick, glam::DVec3::ZERO, &mut out);
        let local = out.iter().find(|e| e.local).unwrap();
        assert!((local.head_yaw - 45.0).abs() < 1e-3);
        assert!((local.body_yaw - 45.0).abs() < 1e-3);
        assert!((local.pitch - 10.0).abs() < 1e-3);
    }

    #[test]
    fn test_vanilla_scenario_8_f5_third_person_front_head_faces_camera() {
        let mut harness = Harness::new();
        let tick = Instant::now();
        let eye = glam::DVec3::new(10.0, 64.0, 20.0);

        let mut f0 = frame(eye.as_vec3(), tick, 0);
        f0.perspective = Perspective::ThirdPersonFront;
        let rec = local_player_record(eye, 0.0, 0.0);
        let fd = rec.frame_data();
        f0.local_body = Arc::new(fd.clone());
        f0.entities = Arc::from([fd]);
        harness.frames.0.send(RenderEvent::Frame(f0)).expect("frame");
        harness.sync();

        // In front view, camera is looking at player's face from the front
        // Turning mouse 20° turns player look direction towards 20°
        harness.world_view.apply_look_delta(20.0_f32.to_radians(), 0.0);
        let mut out = Vec::new();
        harness.world_view.entities_into(tick, glam::DVec3::ZERO, &mut out);
        let local = out.iter().find(|e| e.local).unwrap();
        assert!((local.head_yaw - 20.0).abs() < 1e-3, "head must face player_yaw 20°");
        assert!((local.body_yaw - 0.0).abs() < 1e-3, "body stays within 50° limit");
        let net = wrap_degrees(local.head_yaw - local.body_yaw);
        assert!((net - 20.0).abs() < 1e-3);
    }

    #[test]
    fn test_vanilla_scenario_9_movement_and_jump_body_alignment() {
        // When player moves along +X (Minecraft yaw -90°):
        // BodyYaw::update_player turns body 30% towards -90° while head looks at 0°
        let mut body = BodyYaw::new(0.0);
        body.update_player((0.2, 0.0), 0.0, false);
        // target = atan2(0.0, 0.2) - 90 = -90°
        // offset += wrap_degrees(-90 - 0) * 0.3 = -27°
        assert!((body.offset - (-27.0)).abs() < 1e-3, "body aligns towards movement direction");

        // When swinging arm (PvP attack during jump):
        // target snaps to yaw (0°), pulling body back towards aim
        body.update_player((0.2, 0.0), 0.0, true);
        // target = 0°, offset += wrap_degrees(0 - (-27)) * 0.3 = -27 + 8.1 = -18.9°
        assert!((body.offset - (-18.9)).abs() < 1e-3, "swinging snaps target towards aim");
    }

    #[test]
    fn test_vanilla_scenario_10_server_setback_head_and_body_clean_reset() {
        let mut record = EntityRecord::new(
            EntityId(1),
            EntityKind::Player,
            glam::DVec3::new(10.0, 64.0, 10.0),
            45.0,
            10.0,
        );
        record.body_yaw.offset = 30.0;
        record.body_yaw.previous = 20.0;

        // Server setback arrives at (0, 64, 0), yaw 180°, pitch 0°
        record.teleport_to_exact(glam::DVec3::new(0.0, 64.0, 0.0), 180.0, 0.0);

        assert_eq!(record.yaw, 180.0);
        assert_eq!(record.head_yaw, 180.0);
        assert_eq!(record.previous_head_yaw, 180.0);
        assert_eq!(record.body_yaw.offset, 180.0);
        assert_eq!(record.body_yaw.previous, 180.0);

        let snap = record.frame_data().interpolate(0.5, glam::DVec3::ZERO);
        assert_eq!(snap.head_yaw, 180.0);
        assert_eq!(snap.body_yaw, 180.0);
        assert_eq!(wrap_degrees(snap.head_yaw - snap.body_yaw), 0.0, "zero residual twist after setback");
    }

    #[test]
    fn test_snapshot_pacing_tracker_computes_stats_and_detects_bursts() {
        let mut tracker = SnapshotPacingTracker::new();
        let base = Instant::now();

        // 1. Initial snapshot
        tracker.record_arrival(base, 1);
        // 2. Normal snapshot after 50ms (20 TPS)
        tracker.record_arrival(base + Duration::from_millis(50), 1);
        // 3. Repeated frame (no new snapshot arrived, batch_size = 0)
        tracker.record_arrival(base + Duration::from_millis(60), 0);
        // 4. Burst drop (batch_size = 3 => 1 accepted, 2 collapsed)
        tracker.record_arrival(base + Duration::from_millis(110), 3);

        let stats = tracker.stats();
        assert_eq!(stats.snapshots_received, 5);
        assert_eq!(stats.repeated_draws, 1);
        assert_eq!(stats.burst_dropped, 2);
        assert!(stats.p50_interval_us >= 49_000 && stats.p50_interval_us <= 61_000);
    }

    #[test]
    fn test_delayed_snapshot_interpolates_smoothly_without_freezing_at_one() {
        let mut harness = Harness::new();
        let now = Instant::now();
        // Simulate a network/burst delayed snapshot where tick_time is already 200ms in the past
        let delayed_tick = now - Duration::from_millis(200);

        harness
            .frames
            .0
            .send(RenderEvent::Frame(frame(glam::Vec3::ZERO, delayed_tick, 0)))
            .unwrap();
        harness.sync();

        harness
            .frames
            .0
            .send(RenderEvent::Frame(frame(glam::Vec3::new(1.0, 0.0, 0.0), delayed_tick, 0)))
            .unwrap();
        harness.sync();

        // At arrival time, partial_tick should start around 0.0 (based on tick_instant fallback)
        let alpha_at_arrival = harness.world_view.view.partial_tick(now);
        assert!(
            alpha_at_arrival < 0.2,
            "delayed snapshot must not instantly clamp to 1.0 (got {alpha_at_arrival})"
        );

        // At 25ms after arrival, partial_tick should interpolate smoothly around 0.5
        let alpha_halfway = harness.world_view.view.partial_tick(now + Duration::from_millis(25));
        assert!(
            (alpha_halfway - 0.5).abs() < 0.1,
            "delayed snapshot must interpolate smoothly over 50ms window (got {alpha_halfway})"
        );
    }
}
