// SPDX-License-Identifier: MIT OR Apache-2.0

//! The timing of the displayed frames: the pacing, the frame times and the
//! counters of the debug screens.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::debug::fps_meter::FpsMeter;
use crate::debug::metrics::FrameMetricsWindow;

/// The frame times the F3 graph keeps.
const FRAME_HISTORY: usize = 120;

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

impl FrameClock {
    /// A clock whose first frame starts at `now`.
    pub(in crate::window) fn new(now: Instant) -> Self {
        Self {
            last_frame: now,
            next_frame: now,
            started: now,
            frame_times: VecDeque::with_capacity(FRAME_HISTORY),
            frame_ms: 0.0,
            fps: 0.0,
            fps_meter: FpsMeter::new(now),
            metrics_window: FrameMetricsWindow::new(),
            last_alloc_snapshot: umicraft_core::profiling::AllocSnapshot::now(),
        }
    }

    /// Starts a frame at `now`: the time since the last one, the smoothed
    /// frame time, the FPS, the allocation counters and, when the F3 graph
    /// shows, the frame time history. Returns the seconds since the last
    /// frame.
    pub(in crate::window) fn tick(&mut self, now: Instant, keep_history: bool) -> f32 {
        let elapsed = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;
        let frame_ms = elapsed * 1000.0;
        self.frame_ms = if self.frame_ms == 0.0 {
            frame_ms
        } else {
            self.frame_ms * 0.9 + frame_ms * 0.1
        };
        self.fps = self.fps_meter.tick(now);
        let alloc_now = umicraft_core::profiling::AllocSnapshot::now();
        let alloc_delta = alloc_now.delta_since(self.last_alloc_snapshot);
        self.last_alloc_snapshot = alloc_now;
        self.metrics_window.push(now, frame_ms, alloc_delta);
        if keep_history {
            if self.frame_times.len() >= FRAME_HISTORY {
                self.frame_times.pop_front();
            }
            self.frame_times.push_back(frame_ms);
        }
        elapsed
    }

    /// Sets when the next frame may start, for a frame that ended at
    /// `arrived`. With a `budget` (a frame-rate cap) the deadlines follow
    /// each other by it, without piling up behind a late frame; without one
    /// the next frame may start at once.
    pub(in crate::window) fn schedule_next(&mut self, budget: Option<Duration>, arrived: Instant) {
        let Some(budget) = budget else {
            self.next_frame = arrived;
            return;
        };
        if self.next_frame < arrived {
            self.next_frame = arrived + budget;
        } else {
            self.next_frame += budget;
            if self.next_frame < arrived {
                self.next_frame = arrived;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tick_measures_the_time_since_the_last_frame() {
        let start = Instant::now();
        let mut clock = FrameClock::new(start);
        let elapsed = clock.tick(start + Duration::from_millis(20), true);
        assert!((elapsed - 0.020).abs() < 1e-6);
        assert!((clock.frame_ms - 20.0).abs() < 1e-3);
        assert_eq!(clock.frame_times.len(), 1);
    }

    #[test]
    fn the_frame_history_is_kept_only_while_shown_and_is_bounded() {
        let start = Instant::now();
        let mut clock = FrameClock::new(start);
        clock.tick(start + Duration::from_millis(16), false);
        assert!(clock.frame_times.is_empty());
        for frame in 0..=FRAME_HISTORY as u64 {
            clock.tick(start + Duration::from_millis(32 + frame * 16), true);
        }
        assert_eq!(clock.frame_times.len(), FRAME_HISTORY);
    }

    #[test]
    fn a_capped_frame_follows_the_last_deadline_by_one_budget() {
        let start = Instant::now();
        let mut clock = FrameClock::new(start);
        let budget = Duration::from_millis(10);
        clock.next_frame = start + Duration::from_millis(5);
        clock.schedule_next(Some(budget), start);
        assert_eq!(clock.next_frame, start + Duration::from_millis(15));
    }

    #[test]
    fn a_late_frame_restarts_the_deadlines_from_its_end() {
        let start = Instant::now();
        let mut clock = FrameClock::new(start);
        let arrived = start + Duration::from_millis(50);
        clock.schedule_next(Some(Duration::from_millis(10)), arrived);
        assert_eq!(clock.next_frame, arrived + Duration::from_millis(10));
    }

    #[test]
    fn without_a_cap_the_next_frame_may_start_at_once() {
        let start = Instant::now();
        let mut clock = FrameClock::new(start);
        let arrived = start + Duration::from_millis(3);
        clock.schedule_next(None, arrived);
        assert_eq!(clock.next_frame, arrived);
    }
}
