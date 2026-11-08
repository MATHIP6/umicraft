// SPDX-License-Identifier: MIT OR Apache-2.0

//! The frame rate the F3 screen shows, refreshed once per second.
//!
//! A value recomputed every frame flickers too fast to read. The meter counts
//! the frames of a one-second window and publishes their mean rate when the
//! window closes; until the first window closes it shows the rate of the frames
//! so far.

use std::time::{Duration, Instant};

/// How long a window lasts.
const WINDOW: Duration = Duration::from_secs(1);

/// Counts frames and publishes the rate once per window.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FpsMeter {
    window_start: Instant,
    frames: u32,
    shown: f32,
}

impl FpsMeter {
    /// A meter whose first window starts at `now`.
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            window_start: now,
            frames: 0,
            shown: 0.0,
        }
    }

    /// Counts the frame drawn at `now` and returns the rate to show.
    pub(crate) fn tick(&mut self, now: Instant) -> f32 {
        self.frames += 1;
        let elapsed = now.saturating_duration_since(self.window_start);
        if elapsed >= WINDOW {
            self.shown = self.frames as f32 / elapsed.as_secs_f32();
            self.window_start = now;
            self.frames = 0;
        } else if self.shown == 0.0 && elapsed > Duration::ZERO {
            // Before the first window closes: the rate so far, so the screen is
            // not empty for a second.
            return self.frames as f32 / elapsed.as_secs_f32();
        }
        self.shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ticks `frames` frames spaced `step` apart from `start`; returns the last
    /// shown value and the instant after the last frame.
    fn run(meter: &mut FpsMeter, start: Instant, frames: u32, step: Duration) -> (f32, Instant) {
        let mut now = start;
        let mut shown = 0.0;
        for _ in 0..frames {
            now += step;
            shown = meter.tick(now);
        }
        (shown, now)
    }

    #[test]
    fn the_rate_is_published_once_per_second() {
        let start = Instant::now();
        let mut meter = FpsMeter::new(start);
        // 100 frames of 10 ms fill the first second: 100 fps.
        let (shown, now) = run(&mut meter, start, 100, Duration::from_millis(10));
        assert!((shown - 100.0).abs() < 0.5, "{shown}");
        // The next second runs at 50 fps; half-way the old value stays.
        let (halfway, now) = run(&mut meter, now, 25, Duration::from_millis(20));
        assert!((halfway - 100.0).abs() < 0.5, "{halfway}");
        let (shown, _) = run(&mut meter, now, 25, Duration::from_millis(20));
        assert!((shown - 50.0).abs() < 0.5, "{shown}");
    }

    #[test]
    fn the_first_second_shows_the_rate_so_far() {
        let start = Instant::now();
        let mut meter = FpsMeter::new(start);
        let (shown, _) = run(&mut meter, start, 10, Duration::from_millis(20));
        assert!((shown - 50.0).abs() < 0.5, "{shown}");
    }
}
