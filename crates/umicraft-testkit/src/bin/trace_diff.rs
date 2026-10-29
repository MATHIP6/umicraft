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
use umicraft_testkit::tick_diff::TickDiff;
use umicraft_testkit::trace::Trace;

const USAGE: &str =
    "usage: umicraft-trace-diff <vanilla.umcapt> <umicraft.umcapt> [--show N] [--dump]";

struct Args {
    reference: PathBuf,
    candidate: PathBuf,
    show: usize,
    dump: bool,
}

fn parse_args(argv: Vec<String>) -> Result<Args, String> {
    let mut positional = Vec::new();
    let mut show = 40;
    let mut dump = false;
    let mut index = 0;
    while index < argv.len() {
        match argv[index].as_str() {
            "--help" | "-h" => return Err(USAGE.to_string()),
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
        let reference = load_trace(&args.reference)?;
        let candidate = load_trace(&args.candidate)?;
        if args.dump {
            dump("reference", &reference);
            dump("candidate", &candidate);
        }
        let diff = TickDiff::new(&reference, &candidate);
        print!("{}", diff.render(args.show));
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
