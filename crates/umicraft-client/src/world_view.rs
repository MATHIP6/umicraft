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

use umicraft_game::entity::{EntityFrameData, EntityRenderSnapshot};
#[cfg(test)]
use umicraft_game::entity::{EntityId, EntityKind, EntityRecord};
use umicraft_game::gamemode::GameMode;
use umicraft_game::hand::HandPose;
use umicraft_game::inventory::WindowView;
use umicraft_game::piston::MovingBlocks;
use umicraft_game::server::block_entities::EntityTable;
use umicraft_game::server::block_entities::SignTable;
use umicraft_game::server::chest_lids::ChestLids;
use umicraft_game::server::maps::MapStore;
use umicraft_game::sound::cue::SoundCue;
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
use crate::moving_blocks_draw::mesh_moving_blocks;
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
        while let Ok(event) = frames.try_recv() {
            self.apply(event, target);
        }
        // After the frames, so a look newer than the tick they carry wins.
        self.view.take_look();
        while let Ok(ChunkEvent { pos, snapshot }) = chunks.try_recv() {
            self.queue.queue(pos, snapshot);
        }
        // Collect before dispatching: a job submitted by this call comes back
        // in a later one, after any reset this call applied.
        self.collect(target);
        self.dispatch();
    }

    /// Points `camera` at the player, interpolated for `now` (GAM-8).
    /// `view_bobbing` is the "View Bobbing" option: off, the camera and the
    /// hand keep still while walking (`EntityRenderer.setupCameraTransform`).
    pub fn update_camera(&self, camera: &mut Camera, now: Instant, view_bobbing: bool) {
        let Some(state) = self.view.sample(now) else {
            return;
        };
        camera.set_eye(state.position);
        let (yaw, pitch) = camera_angles(&state);
        camera.yaw = yaw;
        camera.pitch = pitch;
        let bob = if view_bobbing {
            state.effects.bob.matrix()
        } else {
            glam::Mat4::IDENTITY
        };
        camera.set_view_effects(state.effects.fov_modifier, bob);
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

    /// The quads of the newest frame's particles at `partial_ticks` into the
    /// tick, for `Renderer::set_particles`. Block fragments find their sprite
    /// in the bound terrain; the procedural terrain has none, so they are
    /// left out. The centres are relative to `origin`, the render origin of the
    /// camera (F2-1b).
    pub fn particle_sprites_into(
        &self,
        out: &mut Vec<umicraft_render::ParticleSprite>,
        partial_ticks: f32,
        origin: glam::DVec3,
    ) {
        let context = self.mesher.model_context();
        crate::particle_view::sprites_into(
            out,
            &self.view.particles,
            partial_ticks,
            origin,
            &|state| crate::particle_view::block_sprite(context?, state),
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

    /// The simulation frames received so far.
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.frames
    }

    /// Interpolated entity render snapshots for the current render instant.
    ///
    /// Uses the same partial tick fraction as the camera so entities and the
    /// world stay in sync. Every position is relative to `origin`, the render
    /// origin of the camera (F2-1b).
    #[must_use]
    pub fn entities(&self, now: Instant, origin: glam::DVec3) -> Vec<EntityRenderSnapshot> {
        let alpha = self.view.partial_tick(now);
        self.entities
            .iter()
            .map(|entity| {
                let mut snapshot = entity.interpolate(alpha, origin);
                // `Entity.setAngles` shifts the pitch and its previous value
                // together every frame, so what `RendererLivingEntity` draws
                // is the pitch of the mouse, not one interpolated over a tick.
                if snapshot.local {
                    if let Some(pitch) = self.view.body_pitch {
                        snapshot.pitch = pitch;
                    }
                }
                snapshot
            })
            .collect()
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
        self.local_body
            .as_ref()
            .map(|body| body.interpolate(1.0, glam::DVec3::ZERO))
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
            RenderEvent::Hud(hud) => self.hud = *hud,
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
        let Some(context) = self.mesher.model_context() else {
            return Mesh::default();
        };
        let mut mesh = mesh_moving_blocks(&self.moving_blocks, partial_tick, context);
        // The plants of the flower pots ride the same dynamic mesh: their
        // contents come from tile entity data the chunk mesher cannot see.
        push_pot_plants(
            &mut mesh,
            self.block_entities(),
            self.block_entity_data(),
            context,
        );
        mesh
    }

    /// Forgets everything of a finished session: the meshes, the camera
    /// history, the HUD, the inventory, the statistics and the end reason.
    /// The mesh workers and the terrain stay, so the next session starts
    /// from a view that looks like a new one (ADR-0044).
    pub fn clear_session(&mut self, target: &mut impl MeshTarget) {
        self.reset(target);
        self.view = View::default();
        self.hud = HudState::default();
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
        // goes through and the rest wait in the channel for the next frame.
        while uploaded < self.upload_budget {
            let Some(MeshResult {
                pos,
                generation,
                mesh,
            }) = self.pool.try_recv()
            else {
                break;
            };
            if generation != self.generation {
                // A mesh of the world a reset replaced: never drawn.
                continue;
            }
            self.queue.finished(pos);
            if mesh.is_empty() {
                self.faces.remove(&pos);
            } else {
                self.faces.insert(pos, mesh.face_count());
            }
            if mesh.block_entities.is_empty() {
                self.block_entities.remove(&pos);
            } else {
                self.block_entities.insert(pos, mesh.block_entities.clone());
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
    /// The state of the previous tick.
    previous: Option<RenderState>,
    /// The state of the newest tick.
    current: Option<RenderState>,
    /// The instant of the newest tick.
    tick_time: Option<Instant>,
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
    /// Stores a new frame.
    fn push(&mut self, frame: Frame) {
        self.previous = self.current;
        self.current = Some(frame.state);
        self.body_pitch = Some(frame.local_body.pitch);
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
        let Some(look) = self
            .look_slot
            .as_ref()
            .and_then(|slot| slot.lock().ok()?.take())
        else {
            return;
        };
        self.retarget(look.view);
        self.body_pitch = Some(look.body_pitch);
    }

    /// Applies the angles of a look to the view of the newest tick.
    ///
    /// Only the angles and the orbit offset follow the look. The eye, the
    /// effects and the interpolation clock stay those of the tick the render
    /// thread has: the game thread may already be one tick ahead (its frame
    /// not read yet), and taking its eye would make the camera cover two
    /// ticks in one and then stand still, which shows when sprinting.
    fn retarget(&mut self, look: RenderState) {
        let Some(current) = self.current.as_mut() else {
            return;
        };
        current.yaw = look.yaw;
        current.pitch = look.pitch;
        current.orbit_start = look.orbit_start;
        current.position = current.eye + (look.position - look.eye);
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
        RenderEnvironment {
            sky_temperature: self.sky_temperature,
            fog: FogContext {
                light_brightness: self.fog_light.at(self.partial_tick(now)),
                ..self.fog
            },
            rain_strength: self.weather.rain_strength,
            thunder_strength: self.weather.thunder_strength,
            lightning_ticks: self.weather.lightning_ticks,
            flat_world: self.flat_world,
            ..RenderEnvironment::new(
                self.dimension,
                self.world_time,
                self.partial_tick(now),
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
            *slot.lock().expect("slot") = Some(crate::game::Look {
                view: state,
                body_pitch: 7.0,
            });
        }
        *slot.lock().expect("slot") = Some(crate::game::Look {
            view: turned,
            body_pitch: 12.0,
        });
        harness.sync();
        assert_eq!(harness.world_view.view.body_pitch, Some(12.0));
        assert!(slot.lock().expect("slot").is_none(), "the look was taken");

        let halfway = later + TICK / 2;
        let mut camera = Camera::default();
        harness.world_view.update_camera(&mut camera, halfway, true);
        assert!(
            (camera.position().x - 1.0).abs() < 1e-4,
            "{}",
            camera.position().x
        );
        assert!((camera.yaw - (1.0 + std::f32::consts::PI)).abs() < 1e-4);
        assert!((harness.world_view.view.partial_tick(halfway) - 0.5).abs() < 1e-4);
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
        *slot.lock().expect("slot") = Some(crate::game::Look {
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
        *slot.lock().expect("slot") = Some(crate::game::Look {
            view: ahead,
            body_pitch: 0.0,
        });
        harness.sync();

        let mut camera = Camera::default();
        harness
            .world_view
            .update_camera(&mut camera, later + TICK / 2, true);
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
        first.state.position = glam::DVec3::new(0.0, 0.0, -4.0);
        // Tick 2: eye 2 blocks east, the player turned and the camera is now
        // 4 blocks east of the eye.
        let mut second = frame(glam::Vec3::new(2.0, 0.0, 0.0), later, 0);
        second.state.position = glam::DVec3::new(6.0, 0.0, 0.0);
        second.state.orbit_start = glam::Vec3::new(4.0, 0.0, 0.0);
        for event in [RenderEvent::Frame(first), RenderEvent::Frame(second)] {
            harness.frames.0.send(event).expect("frame queue");
        }
        harness.sync();

        let mut camera = Camera::default();
        harness
            .world_view
            .update_camera(&mut camera, later + TICK / 2, true);
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
        let first = frame(glam::Vec3::ZERO, tick, 0);
        // The eye walks 1 block towards a wall behind it: the camera was 4
        // blocks back from the old eye and is only 1 back from the new one.
        let mut second = frame(glam::Vec3::new(0.0, 0.0, 1.0), later, 0);
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
            .update_camera(&mut camera, later + TICK / 2, true);
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
        harness.world_view.update_camera(&mut camera, tick, true);
        assert_ne!(camera.view_bob(), glam::Mat4::IDENTITY, "bobbing on");
        harness.world_view.update_camera(&mut camera, tick, false);
        assert_eq!(camera.view_bob(), glam::Mat4::IDENTITY, "bobbing off");
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
            .update_camera(&mut camera, tick + TICK * 2, true);
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
        harness.world_view.update_camera(&mut camera, halfway, true);
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
}
