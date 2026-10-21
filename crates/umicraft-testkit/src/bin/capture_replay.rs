// SPDX-License-Identifier: MIT OR Apache-2.0

//! Replays one recorded direction of a `.umcapt` capture over TCP (spec M1
//! capture + replay in testkit).
//!
//! ```text
//! umicraft-capture-replay FILE LISTEN --direction c2s|s2c [--pace]
//! ```
//!
//! Each accepted connection receives the recorded frames as one raw byte
//! stream (every captured frame already carries its length varint), then
//! the writer half closes cleanly. Replaying the server-to-client frames
//! of a capture is the natural input for a client under test; the
//! client-to-server frames feed a fake server.
//!
//! Without `--pace` the frames stream back-to-back. With `--pace` the
//! recorded gaps between frames are honored up to a small cap, because the
//! wall-clock timestamps of a real session minutes long.

use std::path::PathBuf;

use tokio::net::{TcpListener, TcpStream};
use umicraft_testkit::capture::{Capture, Direction};
use umicraft_testkit::replay;

const USAGE: &str = "usage: umicraft-capture-replay FILE LISTEN --direction c2s|s2c [--pace]";

const PACE_CAP_MS: u64 = 250;

struct Args {
    file: PathBuf,
    listen: String,
    direction: Direction,
    pace: bool,
}

fn parse_args(argv: Vec<String>) -> Result<Args, String> {
    let mut positional = Vec::new();
    let mut direction = None;
    let mut pace = false;
    let mut index = 0;
    while index < argv.len() {
        match argv[index].as_str() {
            "--help" | "-h" => return Err(USAGE.to_string()),
            "--pace" => {
                pace = true;
                index += 1;
            }
            "--direction" => {
                let token = argv
                    .get(index + 1)
                    .ok_or("missing value after --direction")?;
                direction = Some(replay::parse_direction(token)?);
                index += 2;
            }
            other => {
                positional.push(other.to_string());
                index += 1;
            }
        }
    }
    let Some(direction) = direction else {
        return Err(format!("missing --direction c2s|s2c\n{USAGE}"));
    };
    match positional.len() {
        2 => Ok(Args {
            file: positional[0].clone().into(),
            listen: positional[1].clone(),
            direction,
            pace,
        }),
        _ => Err(format!("need FILE and LISTEN\n{USAGE}")),
    }
}

/// Serves replayed streams to every connection until the process ends.
async fn serve(args: &Args, frames: Vec<umicraft_testkit::capture::CaptureEntry>) {
    let listener = match TcpListener::bind(&args.listen).await {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("failed to bind {}: {err}", args.listen);
            std::process::exit(1);
        }
    };
    println!(
        "replaying {} {} frames on {} (pace = {})",
        args.file.display(),
        if args.direction == Direction::ServerToClient {
            "server-to-client"
        } else {
            "client-to-server"
        },
        args.listen,
        if args.pace { "capped" } else { "off" },
    );
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(err) => {
                eprintln!("accept failed: {err}");
                continue;
            }
        };
        if let Err(err) = replay_connection(stream, &frames, args.pace).await {
            eprintln!("replay to {peer} aborted: {err}");
        } else {
            println!("replayed {} frames to {peer}", frames.len());
        }
    }
}

async fn replay_connection(
    stream: TcpStream,
    frames: &[umicraft_testkit::capture::CaptureEntry],
    pace: bool,
) -> Result<(), umicraft_protocol::ProtocolError> {
    let (read_half, mut write_half) = stream.into_split();
    drop(read_half);
    replay::replay_to(&mut write_half, frames, pace.then_some(PACE_CAP_MS)).await
}

fn main() {
    let args = match parse_args(std::env::args().skip(1).collect()) {
        Ok(args) => args,
        Err(usage) => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    };
    let capture = match Capture::load(&args.file) {
        Ok(capture) => capture,
        Err(err) => {
            eprintln!("failed to load capture {}: {err}", args.file.display());
            std::process::exit(1);
        }
    };
    let frames = replay::select_direction(&capture.entries, args.direction);
    if frames.is_empty() {
        eprintln!("capture holds no frames for the requested direction");
        std::process::exit(1);
    }

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("failed to start async runtime: {err}");
            std::process::exit(1);
        }
    };
    runtime.block_on(serve(&args, frames));
}
