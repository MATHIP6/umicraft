// SPDX-License-Identifier: MIT OR Apache-2.0

//! `umicraft-client` entry point: choose the windowed client, the offscreen
//! screenshot or the headless scripted session.

use std::process::ExitCode;

use tracing_subscriber::EnvFilter;
use umicraft_client::args::{Args, USAGE};
use umicraft_client::{headless, screenshot, window};

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let words: Vec<String> = std::env::args().skip(1).collect();
    if words.iter().any(|word| word == "--help" || word == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let args = match Args::parse(&words) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    let result = if args.headless {
        headless::run(&args)
    } else if args.screenshot.is_some() {
        screenshot::run(&args)
    } else {
        window::run(args.clone())
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("umicraft-client: {error}");
            ExitCode::FAILURE
        }
    }
}
