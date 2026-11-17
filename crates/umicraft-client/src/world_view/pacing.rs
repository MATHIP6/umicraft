// SPDX-License-Identifier: MIT OR Apache-2.0

//! The pacing of the world snapshots the game thread publishes: how often
//! they arrive and how many arrive together.

use std::time::Instant;

use super::WorldView;

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
    pub(super) last_arrival: Option<Instant>,
    pub(super) intervals: [u32; SNAPSHOT_PACING_CAPACITY],
    pub(super) cursor: usize,
    pub(super) count: usize,
    pub(super) last_interval_us: u32,
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

impl WorldView {
    /// Snapshot publication pacing metrics.
    #[must_use]
    pub fn snapshot_pacing_stats(&self) -> SnapshotPacingStats {
        self.snapshot_pacing.stats()
    }
}
