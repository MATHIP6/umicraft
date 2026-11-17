// SPDX-License-Identifier: MIT OR Apache-2.0

//! The meshing: uploads, the upload budget, the order of the queue and resets.

use super::*;

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
