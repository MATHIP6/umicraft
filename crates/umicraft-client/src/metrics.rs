// SPDX-License-Identifier: MIT OR Apache-2.0

//! Frame-time distribution, 1% / 0.1% Low FPS, per-frame allocation and thread
//! utilization metrics.
//!
//! Both the reproducible benchmark scene (`--bench-scene`) and the live F3
//! overlay use [`FrameDistribution`] so offline and in-game numbers share the
//! exact same statistical definitions:
//! - Percentiles (`p50`, `p95`, `p99`, `p99.9`) use the nearest-rank method.
//! - **1% Low FPS** is `1000.0 / mean(slowest 1% of frames)` (with a minimum of
//!   1 sample), the standard gaming benchmark definition that captures frame
//!   pacing spikes more accurately than a single percentile.
//! - **0.1% Low FPS** is `1000.0 / mean(slowest 0.1% of frames)`.

use std::time::{Duration, Instant};

use umicraft_core::profiling::AllocDelta;

/// Capacity of the rolling in-game frame history (~1 second at 1000 FPS, ~16 s at 60 FPS).
const ROLLING_CAPACITY: usize = 1000;

/// How often the rolling F3 distribution is recomputed to avoid sorting every frame.
const REFRESH_INTERVAL: Duration = Duration::from_millis(500);

/// Complete statistical summary of a sequence of frame times (in milliseconds).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FrameDistribution {
    /// Arithmetic mean frame time in milliseconds.
    pub mean_ms: f32,
    /// Median (50th percentile) frame time in milliseconds.
    pub p50_ms: f32,
    /// 95th percentile frame time in milliseconds.
    pub p95_ms: f32,
    /// 99th percentile frame time in milliseconds.
    pub p99_ms: f32,
    /// 99.9th percentile frame time in milliseconds.
    pub p99_9_ms: f32,
    /// Worst (maximum) frame time in milliseconds.
    pub max_ms: f32,
    /// Mean frames per second (`1000.0 / mean_ms`).
    pub mean_fps: f32,
    /// 1% Low FPS (`1000.0 / mean_of_slowest_1_percent`).
    pub fps_1_low: f32,
    /// 0.1% Low FPS (`1000.0 / mean_of_slowest_0_1_percent`).
    pub fps_0_1_low: f32,
}

impl FrameDistribution {
    /// Computes the distribution of `samples` (in milliseconds); returns `None` when empty.
    #[must_use]
    pub fn of(samples: &[f32]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(f32::total_cmp);
        let len = sorted.len();
        let rank = |p: f32| sorted[((p * len as f32).ceil() as usize).clamp(1, len) - 1];
        let mean_ms = sorted.iter().sum::<f32>() / len as f32;

        // Slowest 1% tail average (at least 1 sample).
        let tail_1_count = (len as f32 * 0.01).ceil().max(1.0) as usize;
        let tail_1_mean =
            sorted[len - tail_1_count..].iter().sum::<f32>() / tail_1_count as f32;

        // Slowest 0.1% tail average (at least 1 sample).
        let tail_0_1_count = (len as f32 * 0.001).ceil().max(1.0) as usize;
        let tail_0_1_mean =
            sorted[len - tail_0_1_count..].iter().sum::<f32>() / tail_0_1_count as f32;

        let to_fps = |ms: f32| {
            if ms <= f32::EPSILON {
                0.0
            } else {
                1000.0 / ms
            }
        };

        Some(Self {
            mean_ms,
            p50_ms: rank(0.50),
            p95_ms: rank(0.95),
            p99_ms: rank(0.99),
            p99_9_ms: rank(0.999),
            max_ms: sorted[len - 1],
            mean_fps: to_fps(mean_ms),
            fps_1_low: to_fps(tail_1_mean),
            fps_0_1_low: to_fps(tail_0_1_mean),
        })
    }

    /// Formats a compact F3 line summarizing frame time percentiles and 1% / 0.1% Low FPS.
    #[must_use]
    pub fn f3_line(&self) -> String {
        format!(
            "Frame: {:.2} ms (P95 {:.2}, P99 {:.2}) | 1% {:.0} fps, 0.1% {:.0} fps",
            self.mean_ms, self.p95_ms, self.p99_ms, self.fps_1_low, self.fps_0_1_low
        )
    }
}

/// Detailed latency metrics for the input subsystem, tailored for competitive PvP profiling.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct InputLatencyMetrics {
    /// Time spent executing input handlers on the game thread (microseconds).
    pub input_processing_us: u32,
    /// Time an input event spent waiting in the channel before being dequeued (microseconds).
    pub event_queue_latency_us: u32,
    /// Duration from physical input sampling to completion of the rendered frame (milliseconds).
    pub input_to_frame_ms: f32,
    /// Surface swapchain presentation duration (`queue.present`) in milliseconds.
    pub presentation_time_ms: f32,
}

impl InputLatencyMetrics {
    /// Formats a compact summary line suitable for the F3 debug screen.
    #[must_use]
    pub fn f3_line(&self) -> String {
        format!(
            "Input: proc {} us, queue {} us | in-to-frame {:.2} ms | present {:.2} ms",
            self.input_processing_us,
            self.event_queue_latency_us,
            self.input_to_frame_ms,
            self.presentation_time_ms
        )
    }
}

/// Rolling fixed-capacity ring buffer of recent frame times and allocation deltas.
#[derive(Debug, Clone)]
pub struct FrameMetricsWindow {
    samples_ms: [f32; ROLLING_CAPACITY],
    alloc_ops: [u64; ROLLING_CAPACITY],
    alloc_bytes: [u64; ROLLING_CAPACITY],
    cursor: usize,
    len: usize,
    last_refresh: Option<Instant>,
    cached: Option<FrameDistribution>,
    cached_allocs: (u64, u64),
    last_input_latency: InputLatencyMetrics,
}

impl Default for FrameMetricsWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameMetricsWindow {
    /// Creates an empty rolling window.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            samples_ms: [0.0; ROLLING_CAPACITY],
            alloc_ops: [0; ROLLING_CAPACITY],
            alloc_bytes: [0; ROLLING_CAPACITY],
            cursor: 0,
            len: 0,
            last_refresh: None,
            cached: None,
            cached_allocs: (0, 0),
            last_input_latency: InputLatencyMetrics {
                input_processing_us: 0,
                event_queue_latency_us: 0,
                input_to_frame_ms: 0.0,
                presentation_time_ms: 0.0,
            },
        }
    }

    /// Records a completed frame's duration (`frame_ms`), its heap allocation
    /// delta (`alloc`), and updates the cached distribution at most once every
    /// [`REFRESH_INTERVAL`].
    pub fn push(&mut self, now: Instant, frame_ms: f32, alloc: AllocDelta) {
        self.samples_ms[self.cursor] = frame_ms;
        self.alloc_ops[self.cursor] = alloc.total_ops();
        self.alloc_bytes[self.cursor] = alloc.bytes_allocated;
        self.cursor = (self.cursor + 1) % ROLLING_CAPACITY;
        if self.len < ROLLING_CAPACITY {
            self.len += 1;
        }
        let should_refresh = self
            .last_refresh
            .is_none_or(|last| now.saturating_duration_since(last) >= REFRESH_INTERVAL);
        if should_refresh {
            self.last_refresh = Some(now);
            self.cached = FrameDistribution::of(&self.samples_ms[..self.len]);
            if self.len > 0 {
                let ops = self.alloc_ops[..self.len].iter().sum::<u64>() / self.len as u64;
                let bytes = self.alloc_bytes[..self.len].iter().sum::<u64>() / self.len as u64;
                self.cached_allocs = (ops, bytes);
            }
        }
    }

    /// Latest computed frame-time distribution.
    #[must_use]
    pub fn distribution(&self) -> Option<FrameDistribution> {
        self.cached
    }

    /// Mean `(alloc_ops_per_frame, bytes_allocated_per_frame)` over the window.
    #[must_use]
    pub fn mean_allocs_per_frame(&self) -> (u64, u64) {
        self.cached_allocs
    }

    /// Records the latest input latency measurements.
    pub fn set_input_latency(&mut self, latency: InputLatencyMetrics) {
        self.last_input_latency = latency;
    }

    /// Returns the latest input latency measurements.
    #[must_use]
    pub const fn input_latency(&self) -> InputLatencyMetrics {
        self.last_input_latency
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_and_low_fps_follow_sorted_tail() {
        // 99 frames of 1.0 ms (1000 FPS) and 1 spike frame of 10.0 ms (100 FPS).
        let mut samples = vec![1.0_f32; 99];
        samples.push(10.0);
        let dist = FrameDistribution::of(&samples).expect("non-empty");
        assert!((dist.p50_ms - 1.0).abs() < 1e-5);
        assert!((dist.p95_ms - 1.0).abs() < 1e-5);
        assert!((dist.p99_ms - 1.0).abs() < 1e-5);
        assert!((dist.max_ms - 10.0).abs() < 1e-5);
        // Slowest 1% of 100 frames is the single 10.0 ms frame -> 100.0 FPS.
        assert!((dist.fps_1_low - 100.0).abs() < 1e-3);
        assert!((dist.fps_0_1_low - 100.0).abs() < 1e-3);
    }

    #[test]
    fn rolling_window_publishes_distribution_and_alloc_means() {
        let mut win = FrameMetricsWindow::new();
        let now = Instant::now();
        win.push(
            now,
            2.0,
            AllocDelta {
                allocs: 40,
                deallocs: 38,
                reallocs: 2,
                bytes_allocated: 4096,
                bytes_deallocated: 4000,
            },
        );
        let dist = win.distribution().expect("cached on first push");
        assert!((dist.mean_ms - 2.0).abs() < 1e-5);
        assert!((dist.mean_fps - 500.0).abs() < 1e-2);
        assert_eq!(win.mean_allocs_per_frame(), (42, 4096));
    }

    #[test]
    fn input_latency_metrics_record_and_format() {
        let mut win = FrameMetricsWindow::new();
        let latency = InputLatencyMetrics {
            input_processing_us: 45,
            event_queue_latency_us: 12,
            input_to_frame_ms: 1.65,
            presentation_time_ms: 0.22,
        };
        win.set_input_latency(latency);
        assert_eq!(win.input_latency(), latency);
        let f3 = latency.f3_line();
        assert!(f3.contains("proc 45 us"));
        assert!(f3.contains("queue 12 us"));
        assert!(f3.contains("in-to-frame 1.65 ms"));
        assert!(f3.contains("present 0.22 ms"));
    }
}
