// SPDX-License-Identifier: MIT OR Apache-2.0

//! Internal player-state logs and their phase-aligned comparison (phase E.2).
//!
//! Both clients write the same CSV, one line per client tick, taken from the
//! player entity itself and not from the packets:
//!
//! ```text
//! tick,script_tick,phase,x,y,z,mx,my,mz,yaw,pitch,on_ground,sprinting,sneaking,keys_applied,keys_pressed_WASDJSC
//! ```
//!
//! - the vanilla 1.8.9 client writes it through the agent in
//!   `tools/vanilla-agent` (`motionX/Y/Z`, `posX/Y/Z`, `rotationYaw/Pitch`,
//!   read after `EntityPlayerSP.onUpdate`);
//! - Umicraft writes it from `Player` after the physics step (`--state-log`).
//!
//! `mx, my, mz` are the **internal** motion of the entity, not a speed
//! derived from packets. Lines starting with `#` are comments. `script_tick`
//! is -1 before the scenario starts, and the `phase` column holds the name of
//! the scenario phase (`pre` before the start, `end` after the last phase).
//!
//! # Alignment
//!
//! Samples are compared **phase against phase**: the n-th run of consecutive
//! samples of one phase in the reference against the n-th run in the
//! candidate, and inside a run the sample k against the sample k. A shift of
//! the start tick of the whole scenario therefore costs nothing, and a phase
//! that is longer or shorter on one side is reported as such, never absorbed.
//!
//! # Tolerance
//!
//! [`Tolerance::STRICT`] is zero everywhere and is the default. A looser
//! tolerance must be written out on the command line; the diff never widens
//! it by itself, and the report prints the tolerance used.

use std::fmt::Write as _;

use thiserror::Error;

/// Why a state log could not be read.
#[derive(Debug, Error)]
pub enum StateLogError {
    /// The file could not be read.
    #[error("state log: {0}")]
    Io(#[from] std::io::Error),
    /// A line is malformed.
    #[error("state log line {line}: {reason}")]
    Line {
        /// One-based line number.
        line: usize,
        /// What is wrong with it.
        reason: String,
    },
}

/// One client tick of a state log.
#[derive(Debug, Clone, PartialEq)]
pub struct StateSample {
    /// Client tick counter of the log.
    pub tick: u64,
    /// Scenario tick, -1 before the scenario starts.
    pub script_tick: i64,
    /// Phase name (`pre`, `end` or a scenario phase).
    pub phase: String,
    /// Position of the feet.
    pub pos: [f64; 3],
    /// Internal motion of the entity.
    pub motion: [f64; 3],
    /// Yaw in degrees.
    pub yaw: f32,
    /// Pitch in degrees.
    pub pitch: f32,
    /// `onGround`.
    pub on_ground: bool,
    /// `isSprinting`.
    pub sprinting: bool,
    /// `isSneaking`.
    pub sneaking: bool,
    /// Keys the scenario holds this tick (`W+SPRINT`, or `-`).
    pub keys_applied: String,
    /// State of the seven key bindings after the scenario applied them.
    pub keys_pressed: String,
}

/// A whole state log.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StateLog {
    /// Samples in file order.
    pub samples: Vec<StateSample>,
}

/// The header line every state log starts with (after any `#` comments).
pub const HEADER: &str = "tick,script_tick,phase,x,y,z,mx,my,mz,yaw,pitch,on_ground,sprinting,sneaking,keys_applied,keys_pressed_WASDJSC";

/// Whether `text` is a state log (its first non-comment line is [`HEADER`]).
#[must_use]
pub fn is_state_log(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .is_some_and(|line| line == HEADER)
}

impl StateLog {
    /// Parses a state log.
    ///
    /// # Errors
    /// [`StateLogError::Line`] on a line that is not 16 fields of the right type.
    pub fn parse(text: &str) -> Result<Self, StateLogError> {
        let mut samples = Vec::new();
        let mut seen_header = false;
        for (index, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fail = |reason: &str| StateLogError::Line {
                line: index + 1,
                reason: reason.to_string(),
            };
            if !seen_header {
                if line != HEADER {
                    return Err(fail("missing state log header"));
                }
                seen_header = true;
                continue;
            }
            let f: Vec<&str> = line.split(',').map(str::trim).collect();
            if f.len() != 16 {
                return Err(fail("expected 16 fields"));
            }
            let float = |s: &str| s.parse::<f64>().map_err(|_| fail("not a number"));
            let single = |s: &str| s.parse::<f32>().map_err(|_| fail("not a number"));
            let flag = |s: &str| match s {
                "true" | "1" => Ok(true),
                "false" | "0" => Ok(false),
                _ => Err(fail("not a boolean")),
            };
            samples.push(StateSample {
                tick: f[0].parse().map_err(|_| fail("bad tick"))?,
                script_tick: f[1].parse().map_err(|_| fail("bad script_tick"))?,
                phase: f[2].to_string(),
                pos: [float(f[3])?, float(f[4])?, float(f[5])?],
                motion: [float(f[6])?, float(f[7])?, float(f[8])?],
                yaw: single(f[9])?,
                pitch: single(f[10])?,
                on_ground: flag(f[11])?,
                sprinting: flag(f[12])?,
                sneaking: flag(f[13])?,
                keys_applied: f[14].to_string(),
                keys_pressed: f[15].to_string(),
            });
        }
        if !seen_header {
            return Err(StateLogError::Line {
                line: 1,
                reason: "empty state log".to_string(),
            });
        }
        Ok(Self { samples })
    }

    /// Reads a state log from `path`.
    ///
    /// # Errors
    /// [`StateLogError`] when the file cannot be read or parsed.
    pub fn load(path: &std::path::Path) -> Result<Self, StateLogError> {
        Self::parse(&std::fs::read_to_string(path)?)
    }

    /// The runs of consecutive samples of one scenario phase, in order.
    /// Samples before the scenario starts and after it ends are left out.
    #[must_use]
    pub fn runs(&self) -> Vec<Run<'_>> {
        let mut runs: Vec<Run<'_>> = Vec::new();
        for sample in self.samples.iter().filter(|s| s.script_tick >= 0 && s.phase != "end") {
            match runs.last_mut() {
                Some(run) if run.name == sample.phase && sample.script_tick == run.last_script_tick() + 1 => {
                    run.samples.push(sample);
                }
                _ => runs.push(Run {
                    name: &sample.phase,
                    samples: vec![sample],
                }),
            }
        }
        runs
    }
}

/// Consecutive samples of one phase.
#[derive(Debug, Clone)]
pub struct Run<'a> {
    /// Phase name.
    pub name: &'a str,
    /// The samples, in tick order.
    pub samples: Vec<&'a StateSample>,
}

impl Run<'_> {
    fn last_script_tick(&self) -> i64 {
        self.samples.last().map_or(-1, |s| s.script_tick)
    }
}

/// Per-quantity tolerances. Zero means exact equality.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tolerance {
    /// Absolute error per position axis, in blocks.
    pub position: f64,
    /// Absolute error per motion axis, in blocks per tick.
    pub motion: f64,
    /// Absolute error of yaw and pitch, in degrees.
    pub rotation: f32,
}

impl Tolerance {
    /// Exact equality, the default.
    pub const STRICT: Self = Self {
        position: 0.0,
        motion: 0.0,
        rotation: 0.0,
    };

    /// Whether every quantity is exact.
    #[must_use]
    pub fn is_strict(&self) -> bool {
        *self == Self::STRICT
    }
}

/// The largest error of one quantity and where it first left the tolerance.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Worst {
    /// Largest absolute error.
    pub error: f64,
    /// Offset in the phase run of the first sample over the tolerance.
    pub first_over: Option<usize>,
}

/// The comparison of one phase run.
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseDiff {
    /// Phase name.
    pub name: String,
    /// Samples in the reference run and in the candidate run.
    pub lengths: (usize, usize),
    /// Per-axis position errors (x, y, z).
    pub position: [Worst; 3],
    /// Euclidean distance error.
    pub distance: Worst,
    /// Per-axis motion errors (mx, my, mz).
    pub motion: [Worst; 3],
    /// Yaw error.
    pub yaw: Worst,
    /// Pitch error.
    pub pitch: Worst,
    /// Samples whose `onGround` differs.
    pub on_ground: usize,
    /// Samples whose sprinting flag differs.
    pub sprinting: usize,
    /// Samples whose sneaking flag differs.
    pub sneaking: usize,
    /// Samples whose pressed key bindings differ (a scenario mismatch).
    pub keys: usize,
}

impl PhaseDiff {
    fn passes(&self) -> bool {
        self.lengths.0 == self.lengths.1
            && self.position.iter().all(|w| w.first_over.is_none())
            && self.distance.first_over.is_none()
            && self.motion.iter().all(|w| w.first_over.is_none())
            && self.yaw.first_over.is_none()
            && self.pitch.first_over.is_none()
            && self.on_ground == 0
            && self.sprinting == 0
            && self.sneaking == 0
            && self.keys == 0
    }
}

/// The comparison of two state logs.
#[derive(Debug, Clone, PartialEq)]
pub struct StateDiff {
    /// Tolerance the comparison used (printed, never adjusted).
    pub tolerance: Tolerance,
    /// Phase sequences differ (names or number of runs).
    pub sequence_mismatch: Option<String>,
    /// One entry per compared phase run.
    pub phases: Vec<PhaseDiff>,
    /// Scenario ticks of both logs (reference, candidate).
    pub scenario_ticks: (usize, usize),
}

fn track(worst: &mut Worst, error: f64, tolerance: f64, offset: usize) {
    if error > worst.error {
        worst.error = error;
    }
    if error > tolerance && worst.first_over.is_none() {
        worst.first_over = Some(offset);
    }
}

impl StateDiff {
    /// Compares `candidate` against `reference`, phase run against phase run.
    #[must_use]
    pub fn new(reference: &StateLog, candidate: &StateLog, tolerance: Tolerance) -> Self {
        let (left, right) = (reference.runs(), candidate.runs());
        let mut diff = Self {
            tolerance,
            sequence_mismatch: None,
            phases: Vec::new(),
            scenario_ticks: (
                left.iter().map(|r| r.samples.len()).sum(),
                right.iter().map(|r| r.samples.len()).sum(),
            ),
        };
        let names = |runs: &[Run<'_>]| runs.iter().map(|r| r.name.to_string()).collect::<Vec<_>>();
        if names(&left) != names(&right) {
            diff.sequence_mismatch = Some(format!(
                "reference phases [{}] against candidate phases [{}]",
                names(&left).join(" "),
                names(&right).join(" ")
            ));
        }
        for (a, b) in left.iter().zip(&right) {
            if a.name != b.name {
                break;
            }
            diff.phases.push(compare_run(a, b, &tolerance));
        }
        diff
    }

    /// Whether the logs agree within the tolerance in every phase.
    #[must_use]
    pub fn passes(&self) -> bool {
        self.sequence_mismatch.is_none() && !self.phases.is_empty() && self.phases.iter().all(PhaseDiff::passes)
    }

    /// A human-readable report.
    #[must_use]
    pub fn render(&self) -> String {
        let t = &self.tolerance;
        let mut out = String::from("--- state diff (reference = vanilla, internal player state) ---\n");
        let _ = writeln!(
            out,
            "tolerance : {} (position {} blocks/axis, motion {} blocks/tick, rotation {} deg)",
            if t.is_strict() { "STRICT" } else { "EXPLICIT" },
            t.position,
            t.motion,
            t.rotation
        );
        let _ = writeln!(
            out,
            "scenario ticks: reference {}, candidate {}",
            self.scenario_ticks.0, self.scenario_ticks.1
        );
        if let Some(why) = &self.sequence_mismatch {
            let _ = writeln!(out, "PHASE SEQUENCE DIFFERS: {why}");
        }
        let _ = writeln!(
            out,
            "\n{:<16} {:>9} {:>11} {:>11} {:>11} {:>11} {:>11} {:>7} {:>4} {:>4} {:>4}",
            "phase", "ticks", "max|dx|", "max|dy|", "max|dz|", "max|dm|", "max|dyaw|", "ground", "spr", "snk", "keys"
        );
        for p in &self.phases {
            let motion = p.motion.iter().map(|w| w.error).fold(0.0, f64::max);
            let lengths = if p.lengths.0 == p.lengths.1 {
                p.lengths.0.to_string()
            } else {
                format!("{}!={}", p.lengths.0, p.lengths.1)
            };
            let _ = writeln!(
                out,
                "{:<16} {:>9} {:>11.3e} {:>11.3e} {:>11.3e} {:>11.3e} {:>11.3e} {:>7} {:>4} {:>4} {:>4}  {}",
                p.name,
                lengths,
                p.position[0].error,
                p.position[1].error,
                p.position[2].error,
                motion,
                p.yaw.error,
                p.on_ground,
                p.sprinting,
                p.sneaking,
                p.keys,
                if p.passes() { "ok" } else { "DIFFERS" }
            );
        }
        for p in self.phases.iter().filter(|p| !p.passes()) {
            let first = [
                ("x", p.position[0].first_over),
                ("y", p.position[1].first_over),
                ("z", p.position[2].first_over),
                ("mx", p.motion[0].first_over),
                ("my", p.motion[1].first_over),
                ("mz", p.motion[2].first_over),
                ("yaw", p.yaw.first_over),
                ("pitch", p.pitch.first_over),
            ];
            let list: Vec<String> = first
                .iter()
                .filter_map(|(name, at)| at.map(|offset| format!("{name}@+{offset}")))
                .collect();
            let _ = writeln!(out, "  {}: first divergence (tick offset in phase): {}", p.name, list.join(" "));
        }
        let verdict = if self.passes() {
            "IDENTICAL WITHIN TOLERANCE"
        } else {
            "DIFFERENCES FOUND"
        };
        let _ = writeln!(out, "\nverdict: {verdict}");
        out
    }
}

fn compare_run(a: &Run<'_>, b: &Run<'_>, t: &Tolerance) -> PhaseDiff {
    let mut diff = PhaseDiff {
        name: a.name.to_string(),
        lengths: (a.samples.len(), b.samples.len()),
        position: [Worst::default(); 3],
        distance: Worst::default(),
        motion: [Worst::default(); 3],
        yaw: Worst::default(),
        pitch: Worst::default(),
        on_ground: 0,
        sprinting: 0,
        sneaking: 0,
        keys: 0,
    };
    for (offset, (x, y)) in a.samples.iter().zip(&b.samples).enumerate() {
        let mut squared = 0.0;
        for axis in 0..3 {
            let error = (x.pos[axis] - y.pos[axis]).abs();
            squared += error * error;
            track(&mut diff.position[axis], error, t.position, offset);
            track(&mut diff.motion[axis], (x.motion[axis] - y.motion[axis]).abs(), t.motion, offset);
        }
        track(&mut diff.distance, squared.sqrt(), t.position, offset);
        track(&mut diff.yaw, f64::from((x.yaw - y.yaw).abs()), f64::from(t.rotation), offset);
        track(&mut diff.pitch, f64::from((x.pitch - y.pitch).abs()), f64::from(t.rotation), offset);
        diff.on_ground += usize::from(x.on_ground != y.on_ground);
        diff.sprinting += usize::from(x.sprinting != y.sprinting);
        diff.sneaking += usize::from(x.sneaking != y.sneaking);
        diff.keys += usize::from(x.keys_pressed != y.keys_pressed);
    }
    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(tick: u64, script: i64, phase: &str, x: f64, mx: f64, ground: bool) -> String {
        format!("{tick},{script},{phase},{x},4,0.5,{mx},0,0,0,0,{ground},false,false,-,0000000\n")
    }

    fn log(lines: &[String]) -> StateLog {
        StateLog::parse(&format!("# comment\n{HEADER}\n{}", lines.concat())).expect("valid log")
    }

    /// Two phases of three ticks, the scenario starting `shift` ticks late.
    fn scenario(shift: u64, dx: f64, dm: f64) -> StateLog {
        let mut lines = Vec::new();
        for pre in 0..shift {
            lines.push(line(pre, -1, "pre", 0.0, 0.0, true));
        }
        for k in 0..3 {
            lines.push(line(shift + k, k as i64, "walk", 0.2 * k as f64 + dx, 0.2 + dm, true));
        }
        for k in 0..3 {
            lines.push(line(shift + 3 + k, 3 + k as i64, "stop", 0.6, 0.0, true));
        }
        log(&lines)
    }

    #[test]
    fn detects_the_state_log_header() {
        assert!(is_state_log(&format!("# c\n{HEADER}\n")));
        assert!(!is_state_log("tick,x,y,z,yaw,pitch,on_ground\n"));
    }

    #[test]
    fn a_shifted_start_does_not_matter_but_a_value_does() {
        let same = StateDiff::new(&scenario(0, 0.0, 0.0), &scenario(7, 0.0, 0.0), Tolerance::STRICT);
        assert!(same.passes(), "{}", same.render());
        let moved = StateDiff::new(&scenario(0, 0.0, 0.0), &scenario(3, 1e-9, 0.0), Tolerance::STRICT);
        assert!(!moved.passes());
        assert_eq!(moved.phases[0].position[0].first_over, Some(0));
    }

    #[test]
    fn motion_is_compared_from_the_logged_values() {
        let diff = StateDiff::new(&scenario(0, 0.0, 0.0), &scenario(0, 0.0, 1e-12), Tolerance::STRICT);
        assert!(!diff.passes());
        assert!(diff.phases[0].motion[0].first_over.is_some());
    }

    #[test]
    fn an_explicit_tolerance_is_applied_and_reported() {
        let loose = Tolerance {
            position: 0.01,
            motion: 0.0,
            rotation: 0.0,
        };
        let diff = StateDiff::new(&scenario(0, 0.0, 0.0), &scenario(0, 0.005, 0.0), loose);
        assert!(diff.passes(), "{}", diff.render());
        assert!(diff.render().contains("EXPLICIT"));
        let strict = StateDiff::new(&scenario(0, 0.0, 0.0), &scenario(0, 0.005, 0.0), Tolerance::STRICT);
        assert!(!strict.passes());
        assert!(strict.render().contains("STRICT"));
    }

    #[test]
    fn a_longer_phase_is_reported_not_absorbed() {
        let mut longer = scenario(0, 0.0, 0.0);
        longer.samples.insert(
            3,
            StateSample {
                tick: 99,
                script_tick: 3,
                ..longer.samples[2].clone()
            },
        );
        for sample in longer.samples.iter_mut().skip(4) {
            sample.script_tick += 1;
        }
        let diff = StateDiff::new(&scenario(0, 0.0, 0.0), &longer, Tolerance::STRICT);
        assert!(!diff.passes());
    }

    #[test]
    fn different_phase_sequences_fail() {
        let mut renamed = scenario(0, 0.0, 0.0);
        for sample in renamed.samples.iter_mut().filter(|s| s.phase == "stop") {
            sample.phase = "sprint".to_string();
        }
        let diff = StateDiff::new(&scenario(0, 0.0, 0.0), &renamed, Tolerance::STRICT);
        assert!(diff.sequence_mismatch.is_some());
        assert!(!diff.passes());
    }

    #[test]
    fn flag_and_key_differences_are_counted() {
        let mut other = scenario(0, 0.0, 0.0);
        other.samples[1].on_ground = false;
        other.samples[1].sprinting = true;
        other.samples[2].keys_pressed = "1000000".to_string();
        let diff = StateDiff::new(&scenario(0, 0.0, 0.0), &other, Tolerance::STRICT);
        assert_eq!(diff.phases[0].on_ground, 1);
        assert_eq!(diff.phases[0].sprinting, 1);
        assert_eq!(diff.phases[0].keys, 1);
    }

    #[test]
    fn a_malformed_line_is_an_error() {
        let text = format!("{HEADER}\n1,2,3\n");
        assert!(StateLog::parse(&text).is_err());
        assert!(StateLog::parse("tick,x\n").is_err());
    }
}
