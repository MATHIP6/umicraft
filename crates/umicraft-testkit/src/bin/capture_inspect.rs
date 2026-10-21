// SPDX-License-Identifier: MIT OR Apache-2.0

//! Offline inspector for `.umcapt` files recorded by
//! `umicraft-packet-logger` (capture format: `umicraft_testkit::capture`).
//!
//! ```text
//! umicraft-capture-inspect FILE [--limit N] [--summary]
//! umicraft-capture-inspect FILE --cat --direction c2s|s2c > BYTES
//! ```
//!
//! `--cat` prints a direction's frames concatenated (the exact byte stream
//! `umicraft-capture-replay` sends over TCP).
//!
//! Per-frame interpretation lives in
//! [`umicraft_testkit::capture::describe_frame`], which flips from raw to
//! compression-framed parsing when the login-state `Set Compression` frame
//! passes by. Encrypted captures can only be listed by frame length.

use std::io::Write as _;
use std::path::PathBuf;

use umicraft_testkit::capture::{Capture, Direction};
use umicraft_testkit::replay;

const USAGE: &str =
    "usage: umicraft-capture-inspect FILE [--limit N] [--summary] | [--cat --direction c2s|s2c]";

struct Args {
    file: PathBuf,
    limit: usize,
    summary: bool,
    cat: bool,
    direction: Option<Direction>,
}

fn parse_args(argv: Vec<String>) -> Result<Args, String> {
    let mut positional = Vec::new();
    let mut limit = 48;
    let mut summary = false;
    let mut cat = false;
    let mut direction = None;
    let mut index = 0;
    while index < argv.len() {
        match argv[index].as_str() {
            "--help" | "-h" => return Err(USAGE.to_string()),
            "--summary" => {
                summary = true;
                index += 1;
            }
            "--cat" => {
                cat = true;
                index += 1;
            }
            "--direction" => {
                let token = argv
                    .get(index + 1)
                    .ok_or("missing value after --direction")?;
                direction = Some(replay::parse_direction(token)?);
                index += 2;
            }
            "--limit" => {
                let value = argv.get(index + 1).ok_or("missing value after --limit")?;
                limit = value
                    .parse()
                    .map_err(|_| format!("--limit needs a number, got {value}"))?;
                index += 2;
            }
            other => {
                positional.push(other.to_string());
                index += 1;
            }
        }
    }
    match positional.len() {
        1 => Ok(Args {
            file: positional[0].clone().into(),
            limit,
            summary,
            cat,
            direction,
        }),
        _ => Err(format!("need exactly one file\n{USAGE}")),
    }
}

fn direction_label(direction: Direction) -> &'static str {
    match direction {
        Direction::ClientToServer => "C→S",
        Direction::ServerToClient => "S→C",
    }
}

/// Loads the capture and prints the summary and frame listing.
///
/// # Errors
/// Whatever loading the file (missing, unreadable, bad format) reports.
fn run(args: &Args) -> Result<(), std::io::Error> {
    let capture = Capture::load(&args.file)?;
    let entries: Vec<_> = capture.entries.iter().collect();

    let mut per_direction = [0usize; 2];
    for entry in &entries {
        per_direction[entry.direction as usize] += 1;
    }
    let span_ms = entries.last().map_or(0, |entry| {
        entry.timestamp_ms - entries.first().map_or(0, |first| first.timestamp_ms)
    });
    println!(
        "{}: {} frames (C→S {}, S→C {}) over {span_ms} ms",
        args.file.display(),
        entries.len(),
        per_direction[0],
        per_direction[1],
    );
    if args.summary {
        return Ok(());
    }

    let mut compressed = false;
    let mut previous_ms = entries.first().map_or(0, |entry| entry.timestamp_ms);
    for (index, entry) in entries.iter().enumerate().take(args.limit) {
        let delta = entry.timestamp_ms.saturating_sub(previous_ms);
        previous_ms = entry.timestamp_ms;
        println!(
            "{:>5} {} +{:>6} ms {}",
            index,
            direction_label(entry.direction),
            delta,
            umicraft_testkit::capture::describe_frame(entry, &mut compressed),
        );
    }
    if entries.len() > args.limit {
        println!(
            "... {} more frames (raise --limit)",
            entries.len() - args.limit
        );
    }
    Ok(())
}

/// Dumps the selected direction's frames concatenated to stdout — the
/// same byte stream `umicraft-capture-replay` sends over TCP. This gives
/// byte-exact reference material for inspecting or re-cutting captures.
///
/// # Errors
/// Whatever loading the file or writing to stdout reports.
fn cat(args: &Args, direction: Direction) -> Result<(), std::io::Error> {
    let capture = Capture::load(&args.file)?;
    for entry in replay::select_direction(&capture.entries, direction) {
        std::io::stdout().write_all(&entry.data)?;
    }
    Ok(())
}

fn main() {
    let args = match parse_args(std::env::args().skip(1).collect()) {
        Ok(args) => args,
        Err(usage) => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    };
    let outcome = match (args.cat, args.direction) {
        (true, Some(direction)) => cat(&args, direction),
        (true, None) => {
            eprintln!("--cat needs --direction c2s|s2c\n{USAGE}");
            std::process::exit(2);
        }
        (false, _) => run(&args),
    };
    if let Err(err) = outcome {
        eprintln!("failed to inspect capture: {err}");
        std::process::exit(1);
    }
}
