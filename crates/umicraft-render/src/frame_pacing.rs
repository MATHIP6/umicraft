// SPDX-License-Identifier: MIT OR Apache-2.0

//! Frame pacing, swapchain presentation metrics, and surface diagnostics.

use std::time::Instant;

/// Capacity of the rolling history of presentation intervals.
const ROLLING_PACING_CAPACITY: usize = 256;

/// Detailed surface and swapchain configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceInfo {
    /// The active WGPU graphics backend, when known.
    pub backend: Option<wgpu::Backend>,
    /// Backend and driver info string (e.g. "Dx12" or "Vulkan Mesa 25").
    pub backend_name: String,
    /// The GPU device name reported by the adapter.
    pub adapter_name: String,
    /// GPU driver or vendor string.
    pub adapter_vendor: String,
    /// The swapchain framebuffer format.
    pub format: wgpu::TextureFormat,
    /// The swapchain presentation mode.
    pub present_mode: wgpu::PresentMode,
    /// Maximum allowed frames in flight.
    pub desired_maximum_frame_latency: u32,
    /// Physical width of the surface in pixels.
    pub width: u32,
    /// Physical height of the surface in pixels.
    pub height: u32,
    /// Enabled texture usage flags.
    pub usages: wgpu::TextureUsages,
}

/// Snapshot of presentation timing, intervals, and swapchain diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FramePacingStats {
    /// Time spent acquiring the surface texture (`surface.get_current_texture()`) in microseconds.
    pub acquire_us: u32,
    /// Time spent encoding render passes into the command buffer in microseconds.
    pub encode_us: u32,
    /// Time spent submitting command buffers to the GPU queue in microseconds.
    pub submit_us: u32,
    /// Time spent presenting the frame (`queue.present()`) in microseconds.
    pub present_us: u32,
    /// Wall-clock interval since the preceding presentation in microseconds.
    pub interval_us: u32,
    /// Arithmetic mean of presentation intervals over the rolling window in microseconds.
    pub mean_interval_us: u32,
    /// 50th percentile (median) presentation interval in microseconds.
    pub p50_interval_us: u32,
    /// 95th percentile presentation interval in microseconds.
    pub p95_interval_us: u32,
    /// 99th percentile presentation interval in microseconds.
    pub p99_interval_us: u32,
    /// Total frames presented successfully.
    pub presented_count: u64,
    /// Surface acquire timeouts.
    pub timeout_count: u64,
    /// Suboptimal surface acquisitions.
    pub suboptimal_count: u64,
    /// Outdated surface acquisitions.
    pub outdated_count: u64,
    /// Occluded surface frames.
    pub occluded_count: u64,
    /// Surface lost occurrences.
    pub lost_count: u64,
    /// Surface reconfigurations triggered.
    pub reconfigure_count: u64,
}

impl FramePacingStats {
    /// Format an informative summary string for debug displays.
    #[must_use]
    pub fn summary_line(&self) -> String {
        let p50_ms = self.p50_interval_us as f32 / 1000.0;
        let p95_ms = self.p95_interval_us as f32 / 1000.0;
        let p99_ms = self.p99_interval_us as f32 / 1000.0;
        let acq_ms = self.acquire_us as f32 / 1000.0;
        let sub_ms = self.submit_us as f32 / 1000.0;
        let pres_ms = self.present_us as f32 / 1000.0;
        format!(
            "Present: P50 {:.2}ms, P95 {:.2}ms, P99 {:.2}ms | acq {:.2}ms, sub {:.2}ms, pres {:.2}ms",
            p50_ms, p95_ms, p99_ms, acq_ms, sub_ms, pres_ms
        )
    }
}

/// Rolling tracker of frame presentation pacing.
#[derive(Debug, Clone)]
pub struct FramePacingTracker {
    last_present: Option<Instant>,
    intervals: [u32; ROLLING_PACING_CAPACITY],
    cursor: usize,
    count: usize,
    last_acquire_us: u32,
    last_encode_us: u32,
    last_submit_us: u32,
    last_present_us: u32,
    last_interval_us: u32,
    /// Total frames presented successfully.
    pub presented_count: u64,
    /// Number of surface acquire timeouts.
    pub timeout_count: u64,
    /// Number of suboptimal surface acquisitions.
    pub suboptimal_count: u64,
    /// Number of outdated surface acquisitions.
    pub outdated_count: u64,
    /// Number of occluded surface acquisitions.
    pub occluded_count: u64,
    /// Number of lost surface acquisitions.
    pub lost_count: u64,
    /// Number of surface reconfigurations performed.
    pub reconfigure_count: u64,
}

impl Default for FramePacingTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl FramePacingTracker {
    /// Creates a new frame pacing tracker.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            last_present: None,
            intervals: [0; ROLLING_PACING_CAPACITY],
            cursor: 0,
            count: 0,
            last_acquire_us: 0,
            last_encode_us: 0,
            last_submit_us: 0,
            last_present_us: 0,
            last_interval_us: 0,
            presented_count: 0,
            timeout_count: 0,
            suboptimal_count: 0,
            outdated_count: 0,
            occluded_count: 0,
            lost_count: 0,
            reconfigure_count: 0,
        }
    }

    /// Records the presentation of a frame.
    pub fn record_present(
        &mut self,
        now: Instant,
        acquire_us: u32,
        encode_us: u32,
        submit_us: u32,
        present_us: u32,
    ) {
        let interval_us = self
            .last_present
            .map(|prev| {
                now.saturating_duration_since(prev)
                    .as_micros()
                    .min(u128::from(u32::MAX)) as u32
            })
            .unwrap_or(0);
        self.last_present = Some(now);
        self.last_acquire_us = acquire_us;
        self.last_encode_us = encode_us;
        self.last_submit_us = submit_us;
        self.last_present_us = present_us;
        self.last_interval_us = interval_us;
        self.presented_count += 1;

        if interval_us > 0 {
            self.intervals[self.cursor] = interval_us;
            self.cursor = (self.cursor + 1) % ROLLING_PACING_CAPACITY;
            if self.count < ROLLING_PACING_CAPACITY {
                self.count += 1;
            }
        }
    }

    /// Computes summary statistics over the current rolling window.
    #[must_use]
    pub fn stats(&self) -> FramePacingStats {
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

        FramePacingStats {
            acquire_us: self.last_acquire_us,
            encode_us: self.last_encode_us,
            submit_us: self.last_submit_us,
            present_us: self.last_present_us,
            interval_us: self.last_interval_us,
            mean_interval_us: mean,
            p50_interval_us: p50,
            p95_interval_us: p95,
            p99_interval_us: p99,
            presented_count: self.presented_count,
            timeout_count: self.timeout_count,
            suboptimal_count: self.suboptimal_count,
            outdated_count: self.outdated_count,
            occluded_count: self.occluded_count,
            lost_count: self.lost_count,
            reconfigure_count: self.reconfigure_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn frame_pacing_tracker_computes_percentiles_accurately() {
        let mut tracker = FramePacingTracker::new();
        let base = Instant::now();

        // Feed 100 frames with varying intervals
        for i in 1..=100 {
            let now = base + Duration::from_millis(i * 10);
            tracker.record_present(now, 50, 100, 20, 30);
        }

        let stats = tracker.stats();
        assert_eq!(stats.presented_count, 100);
        assert_eq!(stats.acquire_us, 50);
        assert_eq!(stats.encode_us, 100);
        assert_eq!(stats.submit_us, 20);
        assert_eq!(stats.present_us, 30);
        // Each interval is 10ms = 10,000 us
        assert!(stats.p50_interval_us >= 9_900 && stats.p50_interval_us <= 10_100);
        assert!(stats.p95_interval_us >= 9_900 && stats.p95_interval_us <= 10_100);
        assert!(stats.p99_interval_us >= 9_900 && stats.p99_interval_us <= 10_100);
    }

    #[test]
    fn frame_pacing_tracker_empty_returns_zeros() {
        let tracker = FramePacingTracker::new();
        let stats = tracker.stats();
        assert_eq!(stats.presented_count, 0);
        assert_eq!(stats.mean_interval_us, 0);
        assert_eq!(stats.p50_interval_us, 0);
        assert_eq!(stats.p95_interval_us, 0);
        assert_eq!(stats.p99_interval_us, 0);
    }
}

