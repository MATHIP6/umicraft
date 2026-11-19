// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where the CPU time of a frame goes, for the F3 screen (perf 2, step A2).
//!
//! The window times the stages of `redraw` with [`CpuSplit::lap`]; the F3 screen
//! shows them smoothed, so the owner can read what costs in a real game without a
//! profiler. The same stages carry puffin spans (`profiling` feature).

use std::time::Instant;

/// The stages of a frame, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Mods, menus, the world snapshot, the music: everything before the HUD.
    World,
    /// The HUD and the screens: building the draw list.
    Hud,
    /// What is handed to the renderer: entities, items, particles, the portrait.
    Scene,
    /// The renderer: culling, encoding, submitting, presenting.
    Render,
}

/// The names of the stages, in the order of [`Stage`].
pub const STAGE_NAMES: [&str; 4] = ["world", "hud", "scene", "render"];

/// Weight of the newest frame in the smoothed values.
const SMOOTHING: f32 = 0.05;

/// The stage times of the frame being timed and their smoothed values.
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuSplit {
    last: Option<Instant>,
    frame: [f32; 4],
    smooth: [f32; 4],
    seen: bool,
}

impl CpuSplit {
    /// Starts timing a frame at `now`.
    pub fn start(&mut self, now: Instant) {
        self.last = Some(now);
        self.frame = [0.0; 4];
    }

    /// Ends `stage`: the time since the previous lap or the start is its.
    pub fn lap(&mut self, stage: Stage) {
        let now = Instant::now();
        if let Some(last) = self.last.replace(now) {
            self.frame[stage as usize] += now.duration_since(last).as_secs_f32() * 1e6;
        }
    }

    /// Ends the frame: its times join the smoothed ones.
    pub fn finish(&mut self) {
        if self.last.take().is_none() {
            return;
        }
        if self.seen {
            for (smooth, frame) in self.smooth.iter_mut().zip(self.frame) {
                *smooth += (frame - *smooth) * SMOOTHING;
            }
        } else {
            self.smooth = self.frame;
            self.seen = true;
        }
    }

    /// The smoothed times for the F3 screen, `None` before the first frame.
    #[must_use]
    pub fn reading(&self) -> Option<CpuReading> {
        self.seen.then_some(CpuReading {
            micros: self.smooth,
        })
    }
}

/// The smoothed time of each stage, in microseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpuReading {
    /// Per stage, in the order of [`STAGE_NAMES`].
    pub micros: [f32; 4],
}

impl CpuReading {
    /// The F3 line, e.g. `CPU 1.20 ms: world 0.10, hud 0.31, scene 0.45, render 0.34`.
    #[must_use]
    pub fn line(reading: Option<Self>) -> String {
        let Some(reading) = reading else {
            return "CPU time: ...".to_owned();
        };
        let total: f32 = reading.micros.iter().sum();
        let stages: Vec<String> = STAGE_NAMES
            .iter()
            .zip(reading.micros)
            .map(|(name, micros)| format!("{name} {:.2}", micros / 1000.0))
            .collect();
        format!("CPU {:.2} ms: {}", total / 1000.0, stages.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_frame_no_reading_and_a_pending_line() {
        let split = CpuSplit::default();
        assert!(split.reading().is_none());
        assert_eq!(CpuReading::line(None), "CPU time: ...");
    }

    #[test]
    fn a_lap_goes_to_its_stage_and_the_first_frame_sets_the_average() {
        let mut split = CpuSplit::default();
        split.start(Instant::now());
        std::thread::sleep(std::time::Duration::from_millis(2));
        split.lap(Stage::Hud);
        split.finish();
        let reading = split.reading().expect("a frame");
        assert!(reading.micros[Stage::Hud as usize] >= 2000.0);
        assert!(reading.micros[Stage::World as usize] < 500.0);
    }

    #[test]
    fn the_average_moves_slowly_towards_a_new_cost() {
        let mut split = CpuSplit {
            smooth: [1000.0; 4],
            seen: true,
            ..CpuSplit::default()
        };
        split.start(Instant::now());
        split.finish();
        // A frame of nearly zero pulls the average down by a twentieth only.
        let reading = split.reading().expect("seen");
        assert!((reading.micros[0] - 950.0).abs() < 60.0, "{reading:?}");
    }

    #[test]
    fn the_line_names_every_stage_and_totals_them() {
        let reading = CpuReading {
            micros: [100.0, 310.0, 450.0, 340.0],
        };
        assert_eq!(
            CpuReading::line(Some(reading)),
            "CPU 1.20 ms: world 0.10, hud 0.31, scene 0.45, render 0.34"
        );
    }
}
