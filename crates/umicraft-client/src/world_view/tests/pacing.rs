// SPDX-License-Identifier: MIT OR Apache-2.0

//! The pacing of the snapshots and the interpolation of a late one.

use super::*;

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
        .send(RenderEvent::Frame(frame(
            glam::Vec3::new(1.0, 0.0, 0.0),
            delayed_tick,
            0,
        )))
        .unwrap();
    harness.sync();

    // At arrival time, partial_tick should start around 0.0 (based on tick_instant fallback)
    let alpha_at_arrival = harness.world_view.view.partial_tick(now);
    assert!(
        alpha_at_arrival < 0.2,
        "delayed snapshot must not instantly clamp to 1.0 (got {alpha_at_arrival})"
    );

    // At 25ms after arrival, partial_tick should interpolate smoothly around 0.5
    let alpha_halfway = harness
        .world_view
        .view
        .partial_tick(now + Duration::from_millis(25));
    assert!(
        (alpha_halfway - 0.5).abs() < 0.1,
        "delayed snapshot must interpolate smoothly over 50ms window (got {alpha_halfway})"
    );
}
