// SPDX-License-Identifier: MIT OR Apache-2.0

//! The meshing the view drives: chunks queued for the mesh workers, finished
//! meshes handed to a [`MeshTarget`] within an upload budget.

use std::sync::Arc;

use umicraft_mesh::{BlockEntityRef, Mesh, Vertex};
use umicraft_render::{Camera, Renderer};
use umicraft_world::ChunkPos;

use super::WorldView;
use crate::meshing::pool::{MeshPool, MeshResult};
use crate::resources::terrain::InstalledTerrain;
use crate::scene::block_entity::push_pot_plants;
use crate::scene::moving_blocks::mesh_moving_blocks_into;

/// Mesh jobs kept in flight per worker: enough to keep the worker busy, few
/// enough that a newly important chunk is picked soon.
const JOBS_PER_WORKER: usize = 2;

/// The vertex bytes the render thread uploads per frame at most (one chunk at
/// least): a burst of finished meshes is spread over frames instead of making
/// one long frame (F2-4b).
pub(super) const UPLOAD_BUDGET_BYTES: usize = 1 << 20;

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

impl WorldView {
    /// Live telemetry snapshot of the chunk meshing worker pool.
    #[must_use]
    pub fn mesh_pool_stats(&self) -> crate::meshing::pool::MeshPoolStats {
        self.pool.stats()
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

    /// The chests and signs of every current chunk mesh.
    pub fn block_entities(&self) -> impl Iterator<Item = &BlockEntityRef> {
        self.block_entities.values().flatten()
    }

    /// Whether any chunk currently resident has block entities (chests, signs, etc.).
    #[must_use]
    pub fn has_block_entities(&self) -> bool {
        !self.block_entities.is_empty()
    }

    /// The crack mesh built since the last call, for the renderer; empty
    /// vectors remove the cracks.
    pub fn take_crack_mesh(&mut self) -> Option<(Vec<Vertex>, Vec<u32>)> {
        self.crack_mesh.take()
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

    pub(super) fn collect(&mut self, target: &mut impl MeshTarget) {
        let mut uploaded = 0_usize;
        // The budget is checked before taking a result, so one mesh always
        // goes through and the rest wait in the lock-free channel for the next
        // frame, naturally pacing worker dispatch with the GPU upload budget.
        while uploaded < self.upload_budget {
            let Some(MeshResult {
                pos,
                generation,
                mut mesh,
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
    pub(super) fn dispatch(&mut self) {
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
    pub(super) fn queue_limit_for_tests(&mut self, limit: usize) {
        self.max_in_flight = Some(limit);
    }

    /// Tells the mesh queue where the player is and where they look, so the
    /// chunks around and ahead of them are meshed first. Call it every frame
    /// with the camera.
    pub fn focus_on(&mut self, camera: &Camera) {
        self.queue.focus_on(camera.eye(), camera.yaw);
    }
}
