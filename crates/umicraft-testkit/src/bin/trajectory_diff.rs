// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-tick trajectory diff (ROADMAP.md P0, parity harness; phase E.2).
//!
//! ```text
//! umicraft-trajectory-diff <reference> <candidate> [--strict] [--tolerance BLOCKS]
//!                          [--motion-tolerance V] [--rot-tolerance DEG]
//!                          [--align index|motion] [--relative] [--csv-out FILE]
//! ```
//!
//! **State logs (phase E.2).** When both inputs are state logs
//! (`umicraft_testkit::state_log`, written by the vanilla agent and by
//! `umicraft-client --state-log`) the tool compares the internal player
//! state phase against phase: position per axis, Euclidean distance, motion
//! per axis, yaw, pitch, `onGround`, sprint and sneak. The default is STRICT
//! (every tolerance zero); `--tolerance`, `--motion-tolerance` and
//! `--rot-tolerance` set an explicit one and are echoed in the report. Nothing
//! widens a tolerance on its own.
//!
//! **Packet trajectories.** Each input is either a `.umcapt` capture (its
//! trajectory is read from the movement packets of the trace) or a `.csv`
//! trajectory, such as the one `umicraft-movement-probe --trajectory FILE`
//! writes. The position tolerance defaults to the ADR-0019 proposal;
//! `--strict` makes it zero. `--align motion` drops the ticks before the
//! first change of position on each side, so a scenario started a few ticks
//! apart still lines up. `--relative` compares the displacements from each
//! run's first tick. `--csv-out` writes the reference trajectory as CSV.
//! Packet trajectories carry no velocity: nothing here derives one.
//!
//! Exits 0 within tolerance, 1 outside, 2 on a usage or input error.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use umicraft_testkit::capture::Capture;
use umicraft_testkit::state_log::{is_state_log, StateDiff, StateLog, Tolerance};
use umicraft_testkit::trace::Trace;
use umicraft_testkit::trajectory::{Trajectory, TrajectoryDiff, PROPOSED_TOLERANCE};

const USAGE: &str = "usage: umicraft-trajectory-diff <reference.umcapt|csv> <candidate.umcapt|csv> [--strict] [--tolerance BLOCKS] [--motion-tolerance V] [--rot-tolerance DEG] [--align index|motion] [--motion-nth N] [--relative] [--csv-out FILE]";

struct Args {
    reference: PathBuf,
    candidate: PathBuf,
    /// Position tolerance as typed; `None` picks the mode's default.
    tolerance: Option<f64>,
    motion_tolerance: Option<f64>,
    rot_tolerance: Option<f32>,
    strict: bool,
    align_motion: bool,
    /// Which change of position anchors `--align motion` (1-based).
    motion_nth: usize,
    relative: bool,
    csv_out: Option<PathBuf>,
}

/// Parses the number after a flag; a negative tolerance is an error.
fn number(argv: &[String], index: usize, flag: &str) -> Result<f64, String> {
    let value: f64 = argv
        .get(index + 1)
        .ok_or(format!("missing value after {flag}"))?
        .parse()
        .map_err(|_| format!("{flag} must be a number"))?;
    if value < 0.0 || !value.is_finite() {
        return Err(format!("{flag} must be zero or positive"));
    }
    Ok(value)
}

fn parse_args(argv: Vec<String>) -> Result<Args, String> {
    let mut positional = Vec::new();
    let mut tolerance = None;
    let mut motion_tolerance = None;
    let mut rot_tolerance = None;
    let mut strict = false;
    let mut align_motion = false;
    let mut motion_nth = 1;
    let mut relative = false;
    let mut csv_out = None;
    let mut index = 0;
    while index < argv.len() {
        match argv[index].as_str() {
            "--help" | "-h" => return Err(USAGE.to_string()),
            "--tolerance" => {
                tolerance = Some(number(&argv, index, "--tolerance")?);
                index += 2;
            }
            "--motion-tolerance" => {
                motion_tolerance = Some(number(&argv, index, "--motion-tolerance")?);
                index += 2;
            }
            "--rot-tolerance" => {
                rot_tolerance = Some(number(&argv, index, "--rot-tolerance")? as f32);
                index += 2;
            }
            "--strict" => {
                strict = true;
                index += 1;
            }
            "--align" => {
                align_motion = match argv.get(index + 1).map(String::as_str) {
                    Some("motion") => true,
                    Some("index") => false,
                    _ => return Err("--align must be index or motion".to_string()),
                };
                index += 2;
            }
            "--motion-nth" => {
                motion_nth = argv
                    .get(index + 1)
                    .and_then(|value| value.parse().ok())
                    .filter(|value| *value > 0)
                    .ok_or("--motion-nth must be a positive number")?;
                index += 2;
            }
            "--relative" => {
                relative = true;
                index += 1;
            }
            "--csv-out" => {
                csv_out = Some(PathBuf::from(
                    argv.get(index + 1).ok_or("missing value after --csv-out")?,
                ));
                index += 2;
            }
            other => {
                positional.push(PathBuf::from(other));
                index += 1;
            }
        }
    }
    if strict && (tolerance.is_some() || motion_tolerance.is_some() || rot_tolerance.is_some()) {
        return Err("--strict cannot be combined with an explicit tolerance".to_string());
    }
    match <[PathBuf; 2]>::try_from(positional) {
        Ok([reference, candidate]) => Ok(Args {
            reference,
            candidate,
            tolerance,
            motion_tolerance,
            rot_tolerance,
            strict,
            align_motion,
            motion_nth,
            relative,
            csv_out,
        }),
        Err(_) => Err(format!("need exactly two inputs\n{USAGE}")),
    }
}

/// Whether the file is a state log (`tick,script_tick,phase,...`).
fn is_state_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "csv")
        && std::fs::read_to_string(path).is_ok_and(|text| is_state_log(&text))
}

/// The strict-by-default comparison of two state logs.
fn run_state(args: &Args) -> Result<bool, String> {
    let load = |path: &Path| StateLog::load(path).map_err(|error| format!("{}: {error}", path.display()));
    let tolerance = Tolerance {
        position: args.tolerance.unwrap_or(0.0),
        motion: args.motion_tolerance.unwrap_or(0.0),
        rotation: args.rot_tolerance.unwrap_or(0.0),
    };
    let diff = StateDiff::new(&load(&args.reference)?, &load(&args.candidate)?, tolerance);
    print!("{}", diff.render());
    Ok(diff.passes())
}

/// Reads a trajectory from a CSV file or from the trace of a capture.
fn load(path: &Path) -> Result<Trajectory, String> {
    let describe = |error: &dyn std::fmt::Display| format!("{}: {error}", path.display());
    if path.extension().is_some_and(|extension| extension == "csv") {
        return Trajectory::load_csv(path).map_err(|error| describe(&error));
    }
    let capture = Capture::load(path).map_err(|error| describe(&error))?;
    let trace = Trace::from_capture(&capture).map_err(|error| describe(&error))?;
    Ok(Trajectory::from_trace(&trace))
}

fn main() -> ExitCode {
    let result = parse_args(std::env::args().skip(1).collect()).and_then(|args| {
        match (is_state_file(&args.reference), is_state_file(&args.candidate)) {
            (true, true) => return run_state(&args),
            (false, false) => {}
            _ => {
                return Err(
                    "one input is a state log and the other is not: compare like with like".to_string(),
                )
            }
        }
        let mut reference = load(&args.reference)?;
        let mut candidate = load(&args.candidate)?;
        if args.align_motion {
            reference = reference.from_nth_motion(args.motion_nth);
            candidate = candidate.from_nth_motion(args.motion_nth);
        }
        if let Some(path) = &args.csv_out {
            reference
                .save_csv(path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
        }
        if args.relative {
            reference = reference.relative();
            candidate = candidate.relative();
        }
        let tolerance = if args.strict {
            0.0
        } else {
            args.tolerance.unwrap_or(PROPOSED_TOLERANCE)
        };
        let diff = TrajectoryDiff::new(&reference, &candidate, tolerance);
        print!("{}", diff.render());
        Ok(diff.within_tolerance())
    });
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Result<Args, String> {
        parse_args(list.iter().map(|s| (*s).to_string()).collect())
    }

    #[test]
    fn tolerances_default_to_unset_and_strict_excludes_them() {
        let plain = args(&["a", "b"]).expect("valid");
        assert!(plain.tolerance.is_none() && !plain.strict);
        assert!(args(&["a", "b", "--strict", "--tolerance", "0.1"]).is_err());
        assert!(args(&["a", "b", "--strict"]).expect("valid").strict);
    }

    #[test]
    fn a_negative_or_missing_tolerance_is_refused() {
        assert!(args(&["a", "b", "--tolerance", "-1"]).is_err());
        assert!(args(&["a", "b", "--rot-tolerance"]).is_err());
        assert!(args(&["a", "b", "--align", "random"]).is_err());
        assert!(args(&["a", "b", "--align", "motion"]).expect("valid").align_motion);
    }
}
