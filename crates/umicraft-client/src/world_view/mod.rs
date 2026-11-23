// SPDX-License-Identifier: MIT OR Apache-2.0

//! The render thread's view of the game: the interpolated camera and clock,
//! the chunks waiting for a mesh worker, and the meshes handed to the
//! renderer (ADR-0013).
//!
//! The window and the screenshot path both drive one [`WorldView`] with
//! [`WorldView::sync`], so they cannot drift apart: a chunk that becomes empty
//! is removed, a world reset drops every mesh of the old world (including the
//! ones still being meshed), and each chunk's faces are counted once.
//!
//! # Layout
//!
//! * `mod.rs` holds [`WorldView`], its construction, [`WorldView::sync`] and
//!   the events it applies, and what clearing a session resets.
//! * `meshing`: the mesh queue, the workers, the upload budget and
//!   [`MeshTarget`].
//! * `view` is the interpolated camera; `camera` reads it for a frame.
//! * `scene`: what the newest frame shows (entities, particles, overlays,
//!   block entities); `session`: what the screens read (HUD, inventory,
//!   status, statistics, sounds).
//! * `pacing`: when the snapshots of the game thread arrive.

use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use umicraft_game::entity::EntityFrameData;
use umicraft_game::inventory::WindowView;
use umicraft_game::piston::MovingBlocks;
use umicraft_game::server::block_entities::EntityTable;
use umicraft_game::server::block_entities::SignTable;
use umicraft_game::server::chest_lids::ChestLids;
use umicraft_game::server::maps::MapStore;
use umicraft_game::sound::cue::SoundCue;
use umicraft_mesh::{BlockEntityRef, Vertex};
use umicraft_render::{HudState, OutlineBox};
use umicraft_ui::menus::ConnectStage;
use umicraft_world::{ChunkMap, LightLevel};

use crate::error::ClientError;
use crate::game::{ChunkEvent, GameStatus, RenderEvent, TextScreenRequest};
use crate::meshing::pool::MeshPool;
use crate::meshing::queue::MeshQueue;
use crate::mods::entity_facts::ModEntity;
use crate::pack_offer::ServerPackRequest;
use crate::resources::terrain::{InstalledTerrain, TerrainMesher};
use crate::scene::cracks::{crack_mesh, Crack, STAGES};
use crate::screens::achievements::newly_unlocked;
use crate::screens::connection_end::ConnectionEnd;
use crate::screens::creative::view::CreativeView;

mod camera;
mod meshing;
mod pacing;
mod scene;
mod session;
#[cfg(test)]
mod tests;
mod view;

pub use meshing::MeshTarget;
use meshing::UPLOAD_BUDGET_BYTES;
pub use pacing::{SnapshotPacingStats, SnapshotPacingTracker};
use view::View;

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
    /// What the external mods may read about each entity, from the latest frame.
    mod_entities: Arc<[ModEntity]>,
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
            mod_entities: Arc::from([]),
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
        self.snapshot_pacing
            .record_arrival(Instant::now(), frame_events);
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

    fn apply(&mut self, event: RenderEvent, target: &mut impl MeshTarget) {
        match event {
            RenderEvent::Frame(frame) => {
                self.frames += 1;
                self.entities = Arc::clone(&frame.entities);
                self.mod_entities = Arc::clone(&frame.mod_entities);
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
            RenderEvent::Teleport {
                position,
                yaw,
                pitch,
                placement,
            } => {
                self.view.placement = placement;
                self.view.teleport(position, yaw, pitch);
            }
            RenderEvent::Ended(end) => self.ended = Some(end),
            RenderEvent::Reset => self.reset(target),
        }
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
    pub(super) fn reset(&mut self, target: &mut impl MeshTarget) {
        self.generation += 1;
        self.queue.clear();
        self.faces.clear();
        self.block_entities.clear();
        self.entities = Arc::from([]);
        self.mod_entities = Arc::from([]);
        target.clear_meshes();
    }
}
