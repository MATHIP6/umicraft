// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chunks waiting for a mesh worker, nearest first (F2-4a).
//!
//! A burst of chunks (the first load, a teleport) used to go to the workers in
//! arrival order, so what stood next to the player could wait behind hundreds of
//! far columns; and the job queue took everything at once, so the order could not
//! be corrected afterwards. The queue now keeps the chunks in a map (at most one
//! snapshot per chunk, the newest) and hands out the best one whenever a worker
//! has room, with a small number of jobs in flight so that a chunk the player
//! walks towards still overtakes the others.
//!
//! The priority is the squared distance of the chunk centre to the player, plus a
//! penalty for chunks behind the view. It is recomputed at every pick, so it
//! follows the player; a pick scans the waiting chunks, which costs about a
//! microsecond per hundred of them and happens a few times per frame.

use glam::DVec3;
use umicraft_mesh::ChunkSnapshot;
use umicraft_world::{ChunkMap, ChunkPos, ChunkSet};

/// Added to the squared distance of a chunk behind the player: it then ranks as
/// if it were this much farther (128 blocks = 8 chunks).
const BEHIND_PENALTY: f64 = 128.0 * 128.0;
/// A chunk centre this far (in blocks) behind the eye or less is not "behind":
/// the chunks under and beside the player load first.
const BEHIND_MARGIN: f64 = 24.0;

/// Where the player is and where they look.
#[derive(Debug, Clone, Copy)]
struct Focus {
    x: f64,
    z: f64,
    /// The horizontal view direction, a unit vector (`x`, `z`).
    forward: (f64, f64),
}

/// The meshing queue of one world.
#[derive(Default)]
pub(crate) struct MeshQueue {
    /// The newest snapshot of every chunk that waits for a worker.
    waiting: ChunkMap<ChunkSnapshot>,
    /// The chunks with a job in flight.
    pending: ChunkSet,
    focus: Option<Focus>,
}

impl MeshQueue {
    /// Remembers where the player is (`yaw` in radians, 0 looks to −Z, as the
    /// camera) for the next picks.
    pub(crate) fn focus_on(&mut self, eye: DVec3, yaw: f32) {
        let yaw = f64::from(yaw);
        self.focus = Some(Focus {
            x: eye.x,
            z: eye.z,
            forward: (yaw.sin(), -yaw.cos()),
        });
    }

    /// Queues a snapshot; a newer snapshot of a waiting chunk replaces it.
    pub(crate) fn queue(&mut self, pos: ChunkPos, snapshot: ChunkSnapshot) {
        self.waiting.insert(pos, snapshot);
    }

    /// Takes the best waiting chunk that has no job in flight, if fewer than
    /// `max_in_flight` jobs are in flight; it counts as in flight until
    /// [`MeshQueue::finished`] (or [`MeshQueue::give_back`]).
    ///
    /// A chunk with a job in flight waits for its result first, so meshes arrive
    /// in snapshot order.
    pub(crate) fn next(&mut self, max_in_flight: usize) -> Option<(ChunkPos, ChunkSnapshot)> {
        if self.pending.len() >= max_in_flight {
            return None;
        }
        let focus = self.focus;
        let best = self
            .waiting
            .keys()
            .filter(|pos| !self.pending.contains(pos))
            .map(|pos| (priority(*pos, focus), *pos))
            .min_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)))?
            .1;
        let snapshot = self.waiting.remove(&best)?;
        self.pending.insert(best);
        Some((best, snapshot))
    }

    /// Puts a chunk back after the workers refused its job (the job queue was
    /// full). A newer snapshot that arrived meanwhile wins.
    pub(crate) fn give_back(&mut self, pos: ChunkPos, snapshot: ChunkSnapshot) {
        self.pending.remove(&pos);
        self.waiting.entry(pos).or_insert(snapshot);
    }

    /// The result of the job of `pos` came back.
    pub(crate) fn finished(&mut self, pos: ChunkPos) {
        self.pending.remove(&pos);
    }

    /// Forgets everything (a world reset).
    pub(crate) fn clear(&mut self) {
        self.waiting.clear();
        self.pending.clear();
    }

    /// How many chunks wait for a worker.
    #[cfg(test)]
    pub(crate) fn waiting(&self) -> usize {
        self.waiting.len()
    }
}

/// Lower is picked first. Without a focus every chunk ties and the position
/// order decides.
fn priority(pos: ChunkPos, focus: Option<Focus>) -> f64 {
    let Some(focus) = focus else {
        return 0.0;
    };
    let dx = f64::from(pos.x) * 16.0 + 8.0 - focus.x;
    let dz = f64::from(pos.z) * 16.0 + 8.0 - focus.z;
    let ahead = dx * focus.forward.0 + dz * focus.forward.1;
    let behind = if ahead < -BEHIND_MARGIN {
        BEHIND_PENALTY
    } else {
        0.0
    };
    dx * dx + dz * dz + behind
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(pos: ChunkPos) -> ChunkSnapshot {
        ChunkSnapshot::from_columns(pos, [])
    }

    fn queue_all(queue: &mut MeshQueue, positions: &[ChunkPos]) {
        for pos in positions {
            queue.queue(*pos, snapshot(*pos));
        }
    }

    fn drain(queue: &mut MeshQueue) -> Vec<ChunkPos> {
        let mut order = Vec::new();
        while let Some((pos, _)) = queue.next(usize::MAX) {
            queue.finished(pos);
            order.push(pos);
        }
        order
    }

    #[test]
    fn the_nearest_chunk_goes_first() {
        let mut queue = MeshQueue::default();
        queue.focus_on(DVec3::new(8.0, 70.0, 8.0), 0.0);
        let far = ChunkPos::new(9, 9);
        let near = ChunkPos::new(0, 1);
        queue_all(&mut queue, &[far, ChunkPos::new(-5, 3), near]);
        assert_eq!(queue.next(8).map(|(pos, _)| pos), Some(near));
    }

    #[test]
    fn chunks_behind_the_view_wait_for_those_ahead() {
        let mut queue = MeshQueue::default();
        // Yaw 0 looks to -Z: the chunk at z = -4 is ahead, z = +4 behind, at the
        // same distance.
        queue.focus_on(DVec3::new(8.0, 70.0, 8.0), 0.0);
        let ahead = ChunkPos::new(0, -4);
        let behind = ChunkPos::new(0, 4);
        queue_all(&mut queue, &[behind, ahead]);
        assert_eq!(drain(&mut queue), [ahead, behind]);
    }

    #[test]
    fn the_chunks_around_the_player_are_never_behind() {
        let mut queue = MeshQueue::default();
        queue.focus_on(DVec3::new(8.0, 70.0, 8.0), 0.0);
        // The chunk right behind (z = +1) is within the margin; it beats a chunk
        // ahead but five chunks away.
        let close_behind = ChunkPos::new(0, 1);
        let far_ahead = ChunkPos::new(0, -5);
        queue_all(&mut queue, &[far_ahead, close_behind]);
        assert_eq!(drain(&mut queue), [close_behind, far_ahead]);
    }

    #[test]
    fn the_order_follows_the_player() {
        let mut queue = MeshQueue::default();
        let west = ChunkPos::new(-6, 0);
        let east = ChunkPos::new(6, 0);
        queue_all(&mut queue, &[west, east]);
        queue.focus_on(DVec3::new(-90.0, 70.0, 8.0), 0.0);
        assert_eq!(queue.next(8).map(|(pos, _)| pos), Some(west));
        queue.give_back(west, snapshot(west));
        queue.focus_on(DVec3::new(100.0, 70.0, 8.0), 0.0);
        assert_eq!(queue.next(8).map(|(pos, _)| pos), Some(east));
    }

    #[test]
    fn without_a_focus_the_position_order_is_stable() {
        let mut queue = MeshQueue::default();
        queue_all(
            &mut queue,
            &[
                ChunkPos::new(2, 0),
                ChunkPos::new(0, 5),
                ChunkPos::new(0, 1),
            ],
        );
        assert_eq!(
            drain(&mut queue),
            [
                ChunkPos::new(0, 1),
                ChunkPos::new(0, 5),
                ChunkPos::new(2, 0)
            ]
        );
    }

    #[test]
    fn at_most_the_given_number_of_jobs_are_in_flight() {
        let mut queue = MeshQueue::default();
        queue_all(
            &mut queue,
            &[
                ChunkPos::new(0, 0),
                ChunkPos::new(1, 0),
                ChunkPos::new(2, 0),
            ],
        );
        assert!(queue.next(2).is_some());
        assert!(queue.next(2).is_some());
        assert!(queue.next(2).is_none(), "two are in flight");
        queue.finished(ChunkPos::new(0, 0));
        assert!(queue.next(2).is_some());
    }

    #[test]
    fn a_chunk_with_a_job_in_flight_waits_for_its_result() {
        let mut queue = MeshQueue::default();
        let pos = ChunkPos::new(0, 0);
        queue.queue(pos, snapshot(pos));
        assert!(queue.next(8).is_some());
        // A newer snapshot arrives while the first is being meshed.
        queue.queue(pos, snapshot(pos));
        assert!(queue.next(8).is_none(), "it must not overtake its own job");
        queue.finished(pos);
        assert!(queue.next(8).is_some());
    }

    #[test]
    fn a_newer_snapshot_replaces_a_waiting_one() {
        let mut queue = MeshQueue::default();
        let pos = ChunkPos::new(3, 3);
        queue.queue(pos, snapshot(pos));
        queue.queue(pos, snapshot(pos));
        assert_eq!(queue.waiting(), 1);
    }

    #[test]
    fn a_refused_job_comes_back_unless_a_newer_snapshot_arrived() {
        let mut queue = MeshQueue::default();
        let pos = ChunkPos::new(0, 0);
        queue.queue(pos, snapshot(pos));
        let (_, old) = queue.next(8).expect("a job");
        queue.queue(pos, snapshot(pos));
        queue.give_back(pos, old);
        assert_eq!(queue.waiting(), 1);
        assert!(queue.next(8).is_some(), "no longer in flight");
    }
}
