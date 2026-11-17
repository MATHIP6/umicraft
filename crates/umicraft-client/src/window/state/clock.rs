// SPDX-License-Identifier: MIT OR Apache-2.0

//! The timing of the displayed frames: the pacing, the frame times and the
//! counters of the debug screens.

use std::collections::VecDeque;
use std::time::Instant;

use crate::fps_meter::FpsMeter;
use crate::metrics::FrameMetricsWindow;

/// The timing of the displayed frames.
pub(in crate::window) struct FrameClock {
    pub(in crate::window) last_frame: Instant,
    /// When the frame-rate cap wants the next frame to start.
    pub(in crate::window) next_frame: Instant,
    /// Origin of the clock that scrolls the enchantment glint.
    pub(in crate::window) started: Instant,
    pub(in crate::window) frame_times: VecDeque<f32>,
    pub(in crate::window) frame_ms: f32,
    pub(in crate::window) fps: f32,
    /// Refreshes `fps` once per second for the F3 screen.
    pub(in crate::window) fps_meter: FpsMeter,
    /// Rolling frametime percentiles, 1%/0.1% low FPS, and per-frame allocations.
    pub(in crate::window) metrics_window: FrameMetricsWindow,
    /// Heap allocation snapshot taken at the start of the previous frame.
    pub(in crate::window) last_alloc_snapshot: umicraft_core::profiling::AllocSnapshot,
}
