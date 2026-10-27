// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-tick trajectory diff (ROADMAP.md P0, parity harness).
//!
//! ```text
//! umicraft-trajectory-diff <reference> <candidate> [--tolerance BLOCKS]
//!                          [--relative] [--csv-out FILE]
//! ```
//!
//! Each input is either a `.umcapt` capture (its trajectory is read from the
//! movement packets of the trace) or a `.csv` trajectory, such as the one
//! `umicraft-movement-probe --trajectory FILE` writes. `--relative` compares
//! the displacements from each run's first tick instead of the absolute
//! positions. `--csv-out` writes the reference trajectory as CSV, to inspect
//! or plot it. The default tolerance is the one proposed in ADR-0019. Exits 0
//! within tolerance, 1 outside, 2 on a usage or input error.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use umicraft_testkit::capture::Capture;
use umicraft_testkit::trace::Trace;
use umicraft_testkit::trajectory::{Trajectory, TrajectoryDiff, PROPOSED_TOLERANCE};

const USAGE: &str = "usage: umicraft-trajectory-diff <reference.umcapt|csv> <candidate.umcapt|csv> [--tolerance BLOCKS] [--relative] [--csv-out FILE]";

struct Args {
    reference: PathBuf,
    candidate: PathBuf,
    tolerance: f64,
    relative: bool,
    csv_out: Option<PathBuf>,
}

fn parse_args(argv: Vec<String>) -> Result<Args, String> {
    let mut positional = Vec::new();
    let mut tolerance = PROPOSED_TOLERANCE;
    let mut relative = false;
    let mut csv_out = None;
    let mut index = 0;
    while index < argv.len() {
        match argv[index].as_str() {
            "--help" | "-h" => return Err(USAGE.to_string()),
            "--tolerance" => {
                tolerance = argv
                    .get(index + 1)
                    .ok_or("missing value after --tolerance")?
                    .parse()
                    .map_err(|_| "--tolerance must be a number")?;
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
    match <[PathBuf; 2]>::try_from(positional) {
        Ok([reference, candidate]) => Ok(Args {
            reference,
            candidate,
            tolerance,
            relative,
            csv_out,
        }),
        Err(_) => Err(format!("need exactly two inputs\n{USAGE}")),
    }
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
        let mut reference = load(&args.reference)?;
        let mut candidate = load(&args.candidate)?;
        if let Some(path) = &args.csv_out {
            reference
                .save_csv(path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
        }
        if args.relative {
            reference = reference.relative();
            candidate = candidate.relative();
        }
        let diff = TrajectoryDiff::new(&reference, &candidate, args.tolerance);
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
