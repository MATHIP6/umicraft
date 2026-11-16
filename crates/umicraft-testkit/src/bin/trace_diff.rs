// SPDX-License-Identifier: MIT OR Apache-2.0

//! Serverbound tick diff of two captures (ROADMAP.md P0, parity harness).
//!
//! ```text
//! umicraft-trace-diff <vanilla.umcapt> <umicraft.umcapt> [--show N] [--dump]
//! ```
//!
//! Both captures are recorded with `umicraft-packet-logger` against the same
//! offline-mode 1.8.8 server, one play session each (procedure in
//! `docs/parity-harness.md`). The tool prints the packet counts per token,
//! the tick shapes found on one side only and the first `N` aligned ticks
//! that differ. It exits 0 when both sides produce the same tick shapes, 1
//! when they differ, and 2 on a usage or capture error.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use umicraft_testkit::capture::Capture;
use umicraft_testkit::tick_diff::{body_mismatches, render_extras, TickDiff};
use umicraft_testkit::trace::Trace;
use umicraft_testkit::trajectory::Trajectory;

const USAGE: &str = "usage: umicraft-trace-diff <vanilla.umcapt> <umicraft.umcapt> [--show N] [--dump] [--align index|motion] [--motion-nth N] [--anchor TOKEN] [--bodies]";

struct Args {
    reference: PathBuf,
    candidate: PathBuf,
    show: usize,
    dump: bool,
    /// Drop the ticks before the first change of position on each side.
    align_motion: bool,
    /// Which change of position anchors `--align motion` (1-based).
    motion_nth: usize,
    /// Align on the first tick holding this packet token instead.
    anchor: Option<String>,
    /// Also compare packet bytes and report the cadence; a byte difference
    /// then fails the run.
    bodies: bool,
}

/// The trace from its first tick with a new horizontal position (phase E.2).
fn from_nth_motion(trace: Trace, nth: usize) -> Trace {
    let all = Trajectory::from_trace(&trace);
    let start = all.samples.len() - all.from_nth_motion(nth).samples.len();
    Trace {
        ticks: trace.ticks.into_iter().skip(start).collect(),
        ..trace
    }
}

/// The trace from the first tick holding the packet `token` (`0x0B:start_sprint`).
fn from_token(trace: Trace, token: &str) -> Result<Trace, String> {
    let start = trace
        .ticks
        .iter()
        .position(|tick| tick.tokens().iter().any(|t| t.to_string() == token))
        .ok_or_else(|| format!("no tick holds the anchor packet {token}"))?;
    Ok(Trace {
        ticks: trace.ticks.into_iter().skip(start).collect(),
        ..trace
    })
}

fn parse_args(argv: Vec<String>) -> Result<Args, String> {
    let mut positional = Vec::new();
    let mut show = 40;
    let mut dump = false;
    let mut align_motion = false;
    let mut motion_nth = 1;
    let mut anchor = None;
    let mut bodies = false;
    let mut index = 0;
    while index < argv.len() {
        match argv[index].as_str() {
            "--help" | "-h" => return Err(USAGE.to_string()),
            "--anchor" => {
                anchor = Some(argv.get(index + 1).ok_or("missing value after --anchor")?.clone());
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
            "--bodies" => {
                bodies = true;
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
            "--show" => {
                show = argv
                    .get(index + 1)
                    .ok_or("missing value after --show")?
                    .parse()
                    .map_err(|_| "--show must be a number")?;
                index += 2;
            }
            "--dump" => {
                dump = true;
                index += 1;
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
            show,
            dump,
            align_motion,
            motion_nth,
            anchor,
            bodies,
        }),
        Err(_) => Err(format!("need exactly two capture files\n{USAGE}")),
    }
}

fn load_trace(path: &Path) -> Result<Trace, String> {
    let capture = Capture::load(path).map_err(|error| format!("{}: {error}", path.display()))?;
    Trace::from_capture(&capture).map_err(|error| format!("{}: {error}", path.display()))
}

/// Prints every tick of a trace, one signature per line.
fn dump(label: &str, trace: &Trace) {
    println!("--- {label}: {} ticks ---", trace.ticks.len());
    for (index, tick) in trace.ticks.iter().enumerate() {
        println!("{index:>6} {}", tick.signature());
    }
}

fn main() -> ExitCode {
    let result = parse_args(std::env::args().skip(1).collect()).and_then(|args| {
        let mut reference = load_trace(&args.reference)?;
        let mut candidate = load_trace(&args.candidate)?;
        if args.align_motion {
            reference = from_nth_motion(reference, args.motion_nth);
            candidate = from_nth_motion(candidate, args.motion_nth);
        }
        if let Some(token) = &args.anchor {
            reference = from_token(reference, token)?;
            candidate = from_token(candidate, token)?;
        }
        if args.dump {
            dump("reference", &reference);
            dump("candidate", &candidate);
        }
        let diff = TickDiff::new(&reference, &candidate);
        print!("{}", diff.render(args.show));
        if args.bodies {
            print!("{}", render_extras(&reference, &candidate, args.show));
            return Ok(diff.same_shapes() && body_mismatches(&reference, &candidate).is_empty());
        }
        Ok(diff.same_shapes())
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
