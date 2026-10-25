// SPDX-License-Identifier: MIT OR Apache-2.0

//! `umicraft-client` entry point: choose the windowed client, the offscreen
//! screenshot, the headless scripted session, the server-less UI demo or the
//! server-less M5 block demo.

use std::process::ExitCode;

use tracing_subscriber::EnvFilter;
use umicraft_client::args::{Args, Mode, USAGE};
use umicraft_client::{block_demo, entity_demo, headless, screenshot, setup_cli, ui_demo, window};

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

    let result = match args.mode() {
        Mode::SetupAssets => setup_cli::run(),
        Mode::UiDemo => ui_demo::run(&args),
        Mode::BlockDemo => block_demo::run(&args),
        Mode::EntityDemo => entity_demo::run(&args),
        Mode::Headless => headless::run(&args),
        Mode::Screenshot => screenshot::run(&args),
        Mode::Windowed => window::run(args.clone()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("umicraft-client: {error}");
            ExitCode::FAILURE
        }
    }
}
