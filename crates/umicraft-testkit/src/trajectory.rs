// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-tick trajectories and their comparison (ROADMAP.md P0, parity
//! harness).
//!
//! A trajectory is the position, look and on-ground flag a client reported
//! at the end of each tick. It is read from a [`Trace`] (the movement packet
//! that ends every tick) or from a CSV file, which is what
//! `umicraft-movement-probe --trajectory FILE` writes for the client-side
//! prediction. Comparing the trajectory of the vanilla client with the one of
//! Umicraft for the same deterministic input script measures the movement
//! error per tick, where the probe alone only compares average speeds.
//!
//! The vanilla client reports a position only when it moved by more than
//! `sqrt(9.0E-4)` = 0.03 block or every 20 ticks, and a look only when it
//! changed (MCP `EntityPlayerSP.onUpdateWalkingPlayer`); a tick without one
//! carries the last reported value forward, as the server does.

use std::fmt::Write as _;
use std::path::Path;

use thiserror::Error;
use umicraft_protocol::PlayServerbound;

use crate::trace::{Trace, TracePacket};

/// The tolerance proposed in ADR-0019, pending owner approval (ROADMAP §8.1):
/// 0.01 block of position error per tick.
pub const PROPOSED_TOLERANCE: f64 = 0.01;

/// Why a trajectory could not be read.
#[derive(Debug, Error)]
pub enum TrajectoryError {
    /// The file could not be read or written.
    #[error("trajectory file: {0}")]
    Io(#[from] std::io::Error),
    /// A CSV line is malformed.
    #[error("trajectory line {line}: {reason}")]
    Csv {
        /// One-based line number.
        line: usize,
        /// What is wrong with it.
        reason: String,
    },
}

/// The state a client reported at the end of one tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Feet X.
    pub x: f64,
    /// Feet Y.
    pub y: f64,
    /// Feet Z.
    pub z: f64,
    /// Yaw in degrees.
    pub yaw: f32,
    /// Pitch in degrees.
    pub pitch: f32,
    /// Reported on-ground flag.
    pub on_ground: bool,
}

impl Sample {
    /// Euclidean distance between the two positions, in blocks.
    #[must_use]
    pub fn distance(&self, other: &Self) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2) + (self.z - other.z).powi(2))
            .sqrt()
    }

    /// Applies one movement packet (0x03–0x06) to the sample; other packets
    /// and malformed bodies leave it unchanged.
    fn apply(&mut self, packet: &TracePacket) {
        if packet.id == 0x03 {
            // `Player` (not decoded by the protocol crate yet): one bool.
            if let Some(&flag) = packet.body.first() {
                self.on_ground = flag != 0;
            }
            return;
        }
        match PlayServerbound::decode(packet.id, &mut packet.body.as_slice()) {
            Ok(PlayServerbound::PlayerPosition(moved)) => {
                (self.x, self.y, self.z, self.on_ground) =
                    (moved.x, moved.y, moved.z, moved.on_ground);
            }
            Ok(PlayServerbound::PlayerLook(look)) => {
                (self.yaw, self.pitch, self.on_ground) = (look.yaw, look.pitch, look.on_ground);
            }
            Ok(PlayServerbound::PositionLook(both)) => {
                *self = Self {
                    x: both.x,
                    y: both.y,
                    z: both.z,
                    yaw: both.yaw,
                    pitch: both.pitch,
                    on_ground: both.on_ground,
                };
            }
            _ => {}
        }
    }
}

/// One sample per tick.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Trajectory {
    /// The samples, tick 0 first.
    pub samples: Vec<Sample>,
}

impl Trajectory {
    /// Reads the trajectory a trace reports, starting from the answer to the
    /// initial placement (the last movement packet of the preamble).
    #[must_use]
    pub fn from_trace(trace: &Trace) -> Self {
        let mut current = Sample {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            on_ground: false,
        };
        for packet in trace.preamble.iter().filter(|packet| packet.is_movement()) {
            current.apply(packet);
        }
        let samples = trace
            .ticks
            .iter()
            .map(|tick| {
                for packet in tick.packets.iter().filter(|packet| packet.is_movement()) {
                    current.apply(packet);
                }
                current
            })
            .collect();
        Self { samples }
    }

    /// The trajectory with every position made relative to the first sample,
    /// for runs that did not start at the same spot.
    #[must_use]
    pub fn relative(&self) -> Self {
        let Some(origin) = self.samples.first().copied() else {
            return Self::default();
        };
        let samples = self
            .samples
            .iter()
            .map(|sample| Sample {
                x: sample.x - origin.x,
                y: sample.y - origin.y,
                z: sample.z - origin.z,
                ..*sample
            })
            .collect();
        Self { samples }
    }

    /// Serialises to CSV: a `tick,x,y,z,yaw,pitch,on_ground` header, then one
    /// line per sample.
    #[must_use]
    pub fn to_csv(&self) -> String {
        let mut out = String::from("tick,x,y,z,yaw,pitch,on_ground\n");
        for (tick, sample) in self.samples.iter().enumerate() {
            // `write!` into a `String` cannot fail.
            let _ = writeln!(
                out,
                "{tick},{},{},{},{},{},{}",
                sample.x,
                sample.y,
                sample.z,
                sample.yaw,
                sample.pitch,
                u8::from(sample.on_ground)
            );
        }
        out
    }

    /// Parses the CSV format of [`Trajectory::to_csv`]; the tick column is
    /// informative and the samples are taken in file order.
    ///
    /// # Errors
    /// [`TrajectoryError::Csv`] on a line without seven numeric fields.
    pub fn from_csv(text: &str) -> Result<Self, TrajectoryError> {
        let mut samples = Vec::new();
        for (index, line) in text.lines().enumerate().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            let csv_error = |reason: &str| TrajectoryError::Csv {
                line: index + 1,
                reason: reason.to_string(),
            };
            let fields: Vec<&str> = line.split(',').map(str::trim).collect();
            let [_, x, y, z, yaw, pitch, on_ground] = fields.as_slice() else {
                return Err(csv_error("expected 7 fields"));
            };
            let float = |field: &str| field.parse::<f64>().map_err(|_| csv_error("not a number"));
            samples.push(Sample {
                x: float(x)?,
                y: float(y)?,
                z: float(z)?,
                yaw: float(yaw)? as f32,
                pitch: float(pitch)? as f32,
                on_ground: match *on_ground {
                    "0" => false,
                    "1" => true,
                    _ => return Err(csv_error("on_ground must be 0 or 1")),
                },
            });
        }
        Ok(Self { samples })
    }

    /// Writes [`Trajectory::to_csv`] to `path`.
    ///
    /// # Errors
    /// [`TrajectoryError::Io`] when the file cannot be written.
    pub fn save_csv(&self, path: &Path) -> Result<(), TrajectoryError> {
        Ok(std::fs::write(path, self.to_csv())?)
    }

    /// Reads a CSV trajectory from `path`.
    ///
    /// # Errors
    /// [`TrajectoryError`] when the file cannot be read or parsed.
    pub fn load_csv(path: &Path) -> Result<Self, TrajectoryError> {
        Self::from_csv(&std::fs::read_to_string(path)?)
    }
}

/// The per-tick comparison of two trajectories.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrajectoryDiff {
    /// Number of ticks compared (the shorter trajectory).
    pub compared: usize,
    /// Ticks present in only one trajectory.
    pub length_difference: usize,
    /// Largest position error, in blocks.
    pub max_error: f64,
    /// Tick of the largest error.
    pub worst_tick: usize,
    /// Mean position error, in blocks.
    pub mean_error: f64,
    /// First tick whose error exceeds the tolerance.
    pub first_over: Option<usize>,
    /// Ticks whose on-ground flags differ.
    pub on_ground_mismatches: usize,
    /// The tolerance used, in blocks.
    pub tolerance: f64,
}

impl TrajectoryDiff {
    /// Compares `candidate` against `reference` with a position `tolerance`
    /// in blocks.
    #[must_use]
    pub fn new(reference: &Trajectory, candidate: &Trajectory, tolerance: f64) -> Self {
        let pairs = reference.samples.iter().zip(&candidate.samples);
        let mut diff = Self {
            compared: 0,
            length_difference: reference.samples.len().abs_diff(candidate.samples.len()),
            max_error: 0.0,
            worst_tick: 0,
            mean_error: 0.0,
            first_over: None,
            on_ground_mismatches: 0,
            tolerance,
        };
        let mut total = 0.0;
        for (tick, (expected, actual)) in pairs.enumerate() {
            let error = expected.distance(actual);
            total += error;
            diff.compared += 1;
            if error > diff.max_error {
                diff.max_error = error;
                diff.worst_tick = tick;
            }
            if error > tolerance && diff.first_over.is_none() {
                diff.first_over = Some(tick);
            }
            diff.on_ground_mismatches += usize::from(expected.on_ground != actual.on_ground);
        }
        if diff.compared > 0 {
            diff.mean_error = total / diff.compared as f64;
        }
        diff
    }

    /// Whether every compared tick is within the tolerance and the on-ground
    /// flags agree.
    #[must_use]
    pub fn within_tolerance(&self) -> bool {
        self.first_over.is_none() && self.on_ground_mismatches == 0
    }

    /// A human-readable report.
    #[must_use]
    pub fn render(&self) -> String {
        let first_over = self
            .first_over
            .map_or_else(|| "none".to_string(), |tick| tick.to_string());
        let verdict = if self.within_tolerance() {
            "WITHIN TOLERANCE"
        } else {
            "OUT OF TOLERANCE"
        };
        format!(
            "--- trajectory diff (reference = vanilla) ---\n\
             ticks compared    : {}\n\
             length difference : {}\n\
             max error         : {:.5} blocks at tick {}\n\
             mean error        : {:.5} blocks\n\
             tolerance         : {:.5} blocks, first tick over: {first_over}\n\
             on-ground differs : {} ticks\n\
             verdict: {verdict}\n",
            self.compared,
            self.length_difference,
            self.max_error,
            self.worst_tick,
            self.mean_error,
            self.tolerance,
            self.on_ground_mismatches,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{Tick, TracePacket};
    use umicraft_protocol::packets::play::serverbound::{PlayerLook, PlayerPosition, PositionLook};

    fn packet(body: &PlayServerbound) -> TracePacket {
        let mut bytes = Vec::new();
        body.encode_body(&mut bytes).unwrap();
        TracePacket {
            id: body.packet_id(),
            timestamp_ms: 0,
            body: bytes,
        }
    }

    fn sample(x: f64, on_ground: bool) -> Sample {
        Sample {
            x,
            y: 4.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            on_ground,
        }
    }

    #[test]
    fn a_trace_trajectory_carries_the_last_report_forward() {
        let start = packet(&PlayServerbound::PositionLook(PositionLook {
            x: 1.0,
            y: 4.0,
            z: 2.0,
            yaw: 90.0,
            pitch: 0.0,
            on_ground: true,
        }));
        let moved = packet(&PlayServerbound::PlayerPosition(PlayerPosition {
            x: 1.5,
            y: 4.0,
            z: 2.0,
            on_ground: true,
        }));
        let looked = packet(&PlayServerbound::PlayerLook(PlayerLook {
            yaw: 10.0,
            pitch: 5.0,
            on_ground: false,
        }));
        let idle = TracePacket {
            id: 0x03,
            timestamp_ms: 0,
            body: vec![1],
        };
        let trace = Trace {
            preamble: vec![start],
            ticks: [moved, looked, idle]
                .into_iter()
                .map(|packet| Tick {
                    packets: vec![packet],
                })
                .collect(),
            ..Trace::default()
        };
        let trajectory = Trajectory::from_trace(&trace);
        assert_eq!(trajectory.samples.len(), 3);
        assert_eq!(trajectory.samples[0].x, 1.5);
        assert_eq!(trajectory.samples[0].yaw, 90.0);
        assert_eq!(trajectory.samples[1].x, 1.5);
        assert_eq!(trajectory.samples[1].yaw, 10.0);
        assert!(!trajectory.samples[1].on_ground);
        assert!(trajectory.samples[2].on_ground);
    }

    #[test]
    fn csv_round_trips() {
        let trajectory = Trajectory {
            samples: vec![sample(0.25, true), sample(-3.125, false)],
        };
        assert_eq!(
            Trajectory::from_csv(&trajectory.to_csv()).unwrap(),
            trajectory
        );
    }

    #[test]
    fn malformed_csv_is_an_error() {
        let error = Trajectory::from_csv("header\n1,2,3\n").unwrap_err();
        assert!(matches!(error, TrajectoryError::Csv { line: 2, .. }));
    }

    #[test]
    fn the_diff_finds_the_first_tick_over_tolerance() {
        let reference = Trajectory {
            samples: vec![sample(0.0, true), sample(1.0, true), sample(2.0, true)],
        };
        let candidate = Trajectory {
            samples: vec![sample(0.0, true), sample(1.005, true), sample(2.1, false)],
        };
        let diff = TrajectoryDiff::new(&reference, &candidate, PROPOSED_TOLERANCE);
        assert_eq!(diff.compared, 3);
        assert_eq!(diff.first_over, Some(2));
        assert_eq!(diff.worst_tick, 2);
        assert_eq!(diff.on_ground_mismatches, 1);
        assert!(!diff.within_tolerance());
        assert!(diff.render().contains("OUT OF TOLERANCE"));
    }

    #[test]
    fn identical_trajectories_are_within_tolerance() {
        let trajectory = Trajectory {
            samples: vec![sample(0.0, true), sample(0.2, true)],
        };
        let diff = TrajectoryDiff::new(&trajectory, &trajectory, PROPOSED_TOLERANCE);
        assert!(diff.within_tolerance());
        assert_eq!(diff.max_error, 0.0);
    }

    #[test]
    fn relative_trajectories_start_at_the_origin() {
        let trajectory = Trajectory {
            samples: vec![sample(5.0, true), sample(6.0, true)],
        }
        .relative();
        assert_eq!(trajectory.samples[0].x, 0.0);
        assert_eq!(trajectory.samples[1].x, 1.0);
        assert_eq!(trajectory.samples[1].y, 0.0);
    }
}
