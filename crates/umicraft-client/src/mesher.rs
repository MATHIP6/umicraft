// SPDX-License-Identifier: MIT OR Apache-2.0

//! CPU mesh worker pool.
//!
//! Jobs carry a [`ChunkSnapshot`] (nine `Arc` column handles), so workers never
//! touch the mutable [`umicraft_world::World`]. Results return over a channel
//! and are uploaded on the render thread.

use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use umicraft_mesh::{ChunkSnapshot, Mesh};
use umicraft_world::ChunkPos;

use crate::error::ClientError;
use crate::terrain_textures::{InstalledTerrain, TerrainMesher};

/// Bounded job queue; the render thread uses non-blocking `try_send` and
/// retries on a later frame when full.
const JOB_CAPACITY: usize = 512;
/// Bounded result queue; workers may block briefly if the render thread is
/// busy, but the render thread itself only polls.
const RESULT_CAPACITY: usize = 512;

/// One mesher worker's output.
#[derive(Debug)]
pub struct MeshResult {
    /// Chunk column that was meshed.
    pub pos: ChunkPos,
    /// The caller's tag of the job, returned unchanged.
    ///
    /// The render thread tags every job with its world generation, so a mesh
    /// of a world that was replaced while the job ran can be recognised and
    /// dropped instead of drawn in the new world.
    pub generation: u64,
    /// CPU mesh; empty when the column is empty or unloaded.
    pub mesh: Mesh,
}

struct MeshJob {
    pos: ChunkPos,
    generation: u64,
    snapshot: ChunkSnapshot,
}

/// Fixed-size pool of chunk meshing threads.
pub struct MeshPool {
    jobs: Option<SyncSender<MeshJob>>,
    results: Option<Receiver<MeshResult>>,
    workers: Vec<JoinHandle<()>>,
}

impl MeshPool {
    /// Starts `worker_count.max(1)` worker threads.
    ///
    /// The workers mesh with the mesher of `terrain`, so they only write
    /// layers of the texture array the renderer has bound (ADR-0012). Its
    /// state is immutable and shared by all workers, so no job touches the
    /// disk.
    ///
    /// # Errors
    /// [`ClientError::Worker`] when a thread cannot be spawned.
    pub fn new(worker_count: usize, terrain: &InstalledTerrain) -> Result<Self, ClientError> {
        let worker_count = worker_count.max(1);
        let (job_sender, job_receiver) = mpsc::sync_channel::<MeshJob>(JOB_CAPACITY);
        let (result_sender, result_receiver) = mpsc::sync_channel::<MeshResult>(RESULT_CAPACITY);
        let job_receiver = Arc::new(Mutex::new(job_receiver));

        let mut workers = Vec::with_capacity(worker_count);
        for index in 0..worker_count {
            let receiver = Arc::clone(&job_receiver);
            let results = result_sender.clone();
            let mesher = terrain.mesher();
            let builder = std::thread::Builder::new().name(format!("umicraft-mesh-{index}"));
            match builder.spawn(move || worker_loop(&receiver, &results, &mesher)) {
                Ok(handle) => workers.push(handle),
                Err(error) => {
                    drop(job_sender);
                    for worker in workers {
                        let _ = worker.join();
                    }
                    return Err(ClientError::Worker(error.to_string()));
                }
            }
        }

        Ok(Self {
            jobs: Some(job_sender),
            results: Some(result_receiver),
            workers,
        })
    }

    /// Number of worker threads.
    #[must_use]
    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    /// Queues a chunk for meshing; `generation` comes back in its result.
    ///
    /// The snapshot is returned when the job queue is full or when all
    /// workers have gone, so the caller can retry it on a later frame
    /// instead of losing the work.
    pub fn submit(
        &self,
        pos: ChunkPos,
        generation: u64,
        snapshot: ChunkSnapshot,
    ) -> Result<(), ChunkSnapshot> {
        let Some(sender) = self.jobs.as_ref() else {
            return Err(snapshot);
        };
        let job = MeshJob {
            pos,
            generation,
            snapshot,
        };
        match sender.try_send(job) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(job)) => Err(job.snapshot),
            Err(mpsc::TrySendError::Disconnected(job)) => Err(job.snapshot),
        }
    }

    /// Non-blocking result poll for the render loop.
    #[must_use]
    pub fn try_recv(&self) -> Option<MeshResult> {
        self.results.as_ref().and_then(|rx| rx.try_recv().ok())
    }

    /// Blocking result wait used by screenshot mode.
    #[must_use]
    pub fn recv(&self) -> Option<MeshResult> {
        self.results.as_ref().and_then(|rx| rx.recv().ok())
    }
}

impl Drop for MeshPool {
    fn drop(&mut self) {
        // Close the job channel, then drop the result receiver before joining
        // so workers blocked on a full result queue can see the disconnect.
        self.jobs.take();
        self.results.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn worker_loop(
    receiver: &Mutex<Receiver<MeshJob>>,
    results: &SyncSender<MeshResult>,
    mesher: &TerrainMesher,
) {
    loop {
        let job = {
            let guard = match receiver.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            match guard.recv() {
                Ok(job) => job,
                Err(_) => return,
            }
        };
        let mesh = mesher.mesh(job.pos, &job.snapshot);
        let result = MeshResult {
            pos: job.pos,
            generation: job.generation,
            mesh,
        };
        if results.send(result).is_err() {
            return;
        }
    }
}

/// Suggested worker count: all but one core, capped at four to leave the render
/// and network threads room.
#[must_use]
pub fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|count| count.get().saturating_sub(1).clamp(1, 4))
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_world::World;

    use crate::terrain_textures::TerrainTextures;
    use crate::test_support::TextureTarget;

    #[test]
    fn meshes_submitted_chunks() {
        let world = World::new(0);
        let terrain = TerrainTextures::Procedural.install(&mut TextureTarget::default());
        let pool = MeshPool::new(2, &terrain).expect("pool");
        assert_eq!(pool.worker_count(), 2);
        for x in 0..2 {
            let pos = ChunkPos::new(x, 0);
            let generation = 7 + x as u64;
            assert!(pool
                .submit(pos, generation, ChunkSnapshot::from_world(&world, pos))
                .is_ok());
        }
        let mut results = Vec::new();
        while results.len() < 2 {
            let result = pool.recv().expect("mesh result");
            assert!(result.mesh.is_empty());
            results.push((result.pos, result.generation));
        }
        results.sort_unstable();
        // Each result carries the tag of its job back.
        assert_eq!(
            results,
            vec![(ChunkPos::new(0, 0), 7), (ChunkPos::new(1, 0), 8)]
        );
    }

    #[test]
    fn default_worker_count_is_positive() {
        assert!(default_worker_count() >= 1);
    }
}
