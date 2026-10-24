// SPDX-License-Identifier: MIT OR Apache-2.0

//! Headless Umicraft client: ping the server list, log in offline, and hold
//! the play connection for a chosen duration, printing what it observes.
//!
//! Intended both as the M1 acceptance driver and as a minimal working CLI:
//!
//! ```text
//! umicraft-headless <host> <port> [--name NAME] [--stay-secs N] [--no-ping]
//! ```

use std::process::ExitCode;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;
use umicraft_net::login::login_offline;
use umicraft_net::ping::ping;
use umicraft_net::play::{
    drive, JoinGameInfo, PlayEvent, PlayOptions, PlaySession, PlayerPlacement, SessionCounters,
    SessionEnd,
};
use umicraft_net::{Connection, NetError};
use umicraft_protocol::ProtocolError;

/// Placement acknowledgements waiting for the socket; the server sends one
/// placement at login and rarely more.
const ACK_CAPACITY: usize = 4;

const USAGE: &str = "\
usage: umicraft-headless <host> <port> [--name NAME] [--stay-secs N] [--no-ping]
  <host> <port>   server address (1.8, protocol 47)
  --name NAME     offline username (default: `umicraft`)
  --stay-secs N   seconds to stay in play state (default: 60)
  --no-ping       skip the server list ping step";

struct Args {
    host: String,
    port: u16,
    name: String,
    stay: Duration,
    do_ping: bool,
}

fn parse_args(argv: impl Iterator<Item = String>) -> Result<Args, String> {
    parse_words(&argv.into_iter().collect::<Vec<_>>())
}

fn parse_words(words: &[String]) -> Result<Args, String> {
    let help = || format!("missing/invalid arguments\n{USAGE}");
    let mut host = None;
    let mut port = None;
    let mut name = "umicraft".to_string();
    let mut stay_secs = 60_u64;
    let mut do_ping = true;

    let mut index = 0;
    while index < words.len() {
        match words[index].as_str() {
            "--help" | "-h" => return Err(USAGE.to_string()),
            "--name" => {
                name = words.get(index + 1).ok_or_else(help)?.clone();
                index += 2;
            }
            "--stay-secs" => {
                stay_secs = words
                    .get(index + 1)
                    .ok_or_else(help)?
                    .parse()
                    .map_err(|_| format!("--stay-secs expects a number\n{USAGE}"))?;
                index += 2;
            }
            "--no-ping" => {
                do_ping = false;
                index += 1;
            }
            other if host.is_none() => {
                host = Some(other.to_string());
                index += 1;
            }
            other if port.is_none() => {
                port = Some(
                    other
                        .parse()
                        .map_err(|_| format!("port must be 1-65535, got {other:?}\n{USAGE}"))?,
                );
                index += 1;
            }
            other => return Err(format!("unexpected argument {other:?}\n{USAGE}")),
        }
    }

    Ok(Args {
        host: host.ok_or_else(help)?,
        port: port.ok_or_else(help)?,
        name,
        stay: Duration::from_secs(stay_secs),
        do_ping,
    })
}

#[derive(Debug, thiserror::Error)]
enum SessionError {
    /// Protocol-level failure (transparently shown).
    #[error(transparent)]
    Net(#[from] umicraft_net::NetError),

    /// Protocol errors that are not net errors (e.g. encode on our side).
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
}

async fn execute(args: &Args) -> Result<(), SessionError> {
    println!("connecting to {}:{}", args.host, args.port);

    if args.do_ping {
        let result = ping(&args.host, args.port, Duration::from_secs(10)).await?;
        let status = &result.status;
        println!(
            "ping ok: version={:?} protocol={:?} players={:?}/{} lat={} ms",
            status
                .version
                .as_ref()
                .and_then(|version| version.name.clone()),
            status.version.as_ref().and_then(|version| version.protocol),
            status.players.as_ref().and_then(|players| players.online),
            status
                .players
                .as_ref()
                .and_then(|players| players.max)
                .map(|max| max.to_string())
                .unwrap_or_else(|| "?".to_string()),
            result.latency.as_millis(),
        );
        println!("MOTD: {}", status.motd_text());
    }

    let mut conn = Connection::connect(&args.host, args.port).await?;
    let profile = login_offline(&mut conn, &args.name, None).await?;
    println!(
        "login ok: uuid={} username={}",
        profile.uuid, profile.username
    );

    let started = Instant::now();
    let held = hold_play(&mut conn, args.stay).await?;
    if let Some(join) = held.join {
        println!(
            "joined: entity_id={} dimension={} difficulty={} max_players={} level_type={}",
            join.entity_id, join.dimension, join.difficulty, join.max_players, join.level_type
        );
    }
    if let Some(place) = held.placement {
        println!(
            "placed at ({:.2}, {:.2}, {:.2}) yaw={:.1} pitch={:.1}",
            place.x, place.y, place.z, place.yaw, place.pitch
        );
    }
    println!(
        "held connection {}s: keepalives answered={}, chat observed={}, packets sent={}",
        started.elapsed().as_secs_f64(),
        held.counters.keepalives_answered(),
        held.chat_messages,
        held.counters.packets_sent(),
    );
    Ok(())
}

/// What the client observed while it held the play state.
struct Held {
    join: Option<JoinGameInfo>,
    placement: Option<PlayerPlacement>,
    chat_messages: u64,
    counters: SessionCounters,
}

/// Drives the play state for `stay`, acknowledging every placement.
///
/// The deadline drops the `drive` future, which is safe here because the
/// connection is dropped right after (ADR-0010).
async fn hold_play(conn: &mut Connection, stay: Duration) -> Result<Held, SessionError> {
    let mut session = PlaySession::new(PlayOptions::default());
    let (acks, mut commands) = mpsc::channel(ACK_CAPACITY);
    let mut held = Held {
        join: None,
        placement: None,
        chat_messages: 0,
        counters: SessionCounters::default(),
    };
    let play = drive(conn, &mut session, &mut commands, &held.counters, |event| {
        match event {
            PlayEvent::JoinGame(info) => held.join = Some(info),
            PlayEvent::Placement(placement) => {
                // A full queue only delays the answer to the next placement.
                let _ = acks.try_send(placement.acknowledgement(true));
                held.placement = Some(placement);
            }
            PlayEvent::Chat(_) => held.chat_messages += 1,
            _ => {}
        }
    });
    match tokio::time::timeout(stay, play).await {
        Ok(Ok(SessionEnd::Kicked(reason))) => Err(NetError::KickedAtPlay { reason }.into()),
        Ok(Ok(SessionEnd::CommandsClosed)) | Err(_) => Ok(held),
        Ok(Err(err)) => Err(err.into()),
    }
}

fn main() -> ExitCode {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();

    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(args_error) => {
            eprintln!("{args_error}");
            return ExitCode::from(2);
        }
    };

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("failed to start async runtime: {err}");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(execute(&args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("session failed: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word_list<'a>(words: &'a [&'a str]) -> impl Iterator<Item = String> + 'a {
        words.iter().map(|word| (*word).to_string())
    }

    #[test]
    fn parses_defaults_and_flags() {
        let args = parse_args(word_list(&["localhost", "25565"])).unwrap();
        assert_eq!(args.host, "localhost");
        assert_eq!(args.port, 25565);
        assert_eq!(args.name, "umicraft");
        assert_eq!(args.stay, Duration::from_secs(60));
        assert!(args.do_ping);

        let args = parse_args(word_list(&[
            "10.0.0.1",
            "25565",
            "--name",
            "Bob",
            "--stay-secs",
            "600",
            "--no-ping",
        ]))
        .unwrap();
        assert_eq!(args.name, "Bob");
        assert_eq!(args.stay, Duration::from_secs(600));
        assert!(!args.do_ping);
    }

    #[test]
    fn rejects_bad_shapes() {
        assert!(parse_args(word_list(&[])).is_err());
        assert!(parse_args(word_list(&["127.0.0.1", "65536"])).is_err());
        assert!(parse_args(word_list(&["127.0.0.1", "25565", "--stay-secs", "abc"])).is_err());
        assert!(parse_args(word_list(&["127.0.0.1", "25565", "--name"])).is_err());
    }
}
