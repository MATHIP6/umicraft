// SPDX-License-Identifier: MIT OR Apache-2.0

//! The headless session: a scripted run without a window (spec §11).
//!
//! Spec §11 asks for an integration suite that "spins up a local Paper 1.8
//! server and runs scripted scenarios (login, receive chunks, move, chat)",
//! and for "a headless mode of the client". This module is that mode: it
//! joins a server, plays a scripted session through the very same game
//! thread the windowed client uses, and prints a report that the acceptance
//! run can assert on.

use std::time::{Duration, Instant};

use crate::args::Args;
use crate::error::ClientError;
use crate::game::{GameEvent, GameStatus, RenderEvent, Session, TICK};
use crate::input::{default_script, idle_script, Script, ScriptPhase};
use crate::network::NetworkConfig;

/// The summary of a headless session.
#[derive(Debug, Default, Clone)]
pub struct Report {
    /// The statistics the game thread published.
    pub status: GameStatus,
    /// The number of simulated ticks.
    pub ticks: u64,
    /// The number of position reports the client sent.
    pub position_reports: u64,
    /// The number of `Player Position and Look` packets the server sent after
    /// the initial placement.
    pub resyncs: u64,
    /// The number of block breaks the server confirmed.
    pub breaks_acknowledged: u64,
    /// The number of block placements the server confirmed.
    pub placements_acknowledged: u64,
    /// The position of the player at the end of the session.
    pub position: (f64, f64, f64),
    /// Whether the session ended with an error.
    pub error: Option<String>,
    /// The chat lines the server sent.
    pub chat: Vec<String>,
    /// The health the server reported.
    pub health: f32,
    /// The food the server reported.
    pub food: f32,
    /// The armour points of the player.
    pub armor: f32,
    /// The experience level of the player.
    pub level: i32,
    /// The selected hotbar slot.
    pub selected_slot: u8,
}

/// Runs a scripted session and prints the report.
///
/// # Errors
/// The session could not be started, or the server ended the session with an
/// error.
pub fn run(args: &Args) -> Result<(), ClientError> {
    let session = Session::start(NetworkConfig {
        host: args.host.clone(),
        port: args.port,
        name: args.name.clone(),
        view_distance: args.view_distance,
    })?;

    let mut script = Script::new(phases(&args.script));
    let mut report = Report::default();
    let mut chat_sent = false;
    let started = Instant::now();
    let deadline = started + args.play;
    let mut next_input = Instant::now();

    while Instant::now() < deadline {
        let now = Instant::now();
        if now >= next_input {
            let update = script.update(now);
            let _ = session.events().try_send(GameEvent::Input(update));
            next_input = now + TICK;
        }
        while let Ok(event) = session.frames().try_recv() {
            match event {
                RenderEvent::Frame(frame) => {
                    report.ticks = report.status.ticks.max(1);
                    report.position = (
                        f64::from(frame.state.position.x),
                        f64::from(frame.state.position.y),
                        f64::from(frame.state.position.z),
                    );
                    // The chat line goes out once the player is placed.
                    if let (Some(message), false) = (args.chat.as_ref(), chat_sent) {
                        chat_sent = true;
                        let _ = session.events().try_send(GameEvent::Chat(message.clone()));
                    }
                }
                RenderEvent::Hud(hud) => {
                    if !hud.chat_lines.is_empty() {
                        report.chat = hud.chat_lines.clone();
                    }
                    report.health = hud.health;
                    report.food = hud.food;
                    report.armor = hud.armor;
                    report.level = hud.level;
                    report.selected_slot = hud.selected_slot;
                }
                RenderEvent::Status(status) => {
                    report.status = *status;
                    report.ticks = report.status.ticks;
                    report.position_reports = report.status.position_reports;
                    report.resyncs = report.status.resyncs;
                    report.breaks_acknowledged = report.status.breaks_acknowledged;
                    report.placements_acknowledged = report.status.placements_acknowledged;
                }
                RenderEvent::Ended(reason) => {
                    report.error = Some(reason);
                }
                RenderEvent::Reset => {}
            }
        }
        // The chunk snapshots are not needed without a window, but the queue
        // must be drained so the game thread never blocks on it.
        while session.chunks().try_recv().is_ok() {}
        std::thread::sleep(Duration::from_millis(2));
    }

    let elapsed = started.elapsed();
    drop(session);

    if let Some(error) = report.error.as_ref() {
        eprintln!("umicraft-client: the session ended: {error}");
        return Err(ClientError::Network(error.clone()));
    }

    println!("--- umicraft headless session ---");
    println!("server            : {}:{}", args.host, args.port);
    println!("script            : {}", args.script);
    println!("duration          : {:.1} s", elapsed.as_secs_f32());
    println!("ticks             : {}", report.ticks);
    println!(
        "position          : {:.2} {:.2} {:.2}",
        report.position.0, report.position.1, report.position.2
    );
    println!("position reports  : {}", report.position_reports);
    println!("server resyncs    : {}", report.resyncs);
    println!("chunk packets     : {}", report.status.chunk_packets);
    println!("block updates     : {}", report.status.block_updates);
    println!("chat messages     : {}", report.status.chat_messages);
    println!("breaks confirmed  : {}", report.breaks_acknowledged);
    println!("places confirmed  : {}", report.placements_acknowledged);
    println!("gamemode          : {}", report.status.gamemode);
    println!(
        "health / food     : {:.1} / {:.1} (armour {:.0}, level {})",
        report.health, report.food, report.armor, report.level
    );
    println!("hotbar slot       : {}", report.selected_slot);
    println!("status            : {}", report.status.status);
    for line in &report.chat {
        println!("chat              : {line}");
    }
    Ok(())
}

/// The phases of the requested script.
fn phases(name: &str) -> Vec<ScriptPhase> {
    match name {
        "idle" => idle_script(),
        _ => default_script(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_known_scripts_are_built() {
        assert!(!phases("default").is_empty());
        assert_eq!(phases("idle").len(), 1);
        assert_eq!(phases("idle")[0].step, crate::input::ScriptStep::Idle);
    }
}
