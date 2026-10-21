// SPDX-License-Identifier: MIT OR Apache-2.0

//! Physics conformance probe (spec §11: *"physics conformance tests: recorded
//! trajectories compared with expected positions from a real server"*).
//!
//! The probe drives the client-side prediction of `umicraft-game` and the
//! 1.8 server at the same time, then compares the two trajectories:
//!
//! ```text
//! umicraft-movement-probe <host> <port> [--name NAME] [--ticks N]
//!                         [--forward V] [--report] [--lag N] [--scale R]
//! ```
//!
//! * the probe logs in offline, waits for the world chunks, and sends one
//!   `Input` (0x0C) packet per 20 Hz tick with a constant forward value;
//! * the server simulates the movement and answers with `Player Position and
//!   Look` (0x08) resyncs, which carry its authoritative position;
//! * `umicraft-game` predicts the same movement locally;
//! * the probe reports the server's speed, the predicted speed and the
//!   divergence.
//!
//! The two speeds must agree: a constant divergence means the client model
//! and the server disagree on a rule (the input damping, the friction, the
//! speed factor), and the M3 acceptance criterion — a prediction error below
//! 0.05 block — cannot hold.

use std::time::{Duration, Instant};

use umicraft_game::{GameState, InputIntent};
use umicraft_net::login::login_offline;
use umicraft_net::Connection;
use umicraft_protocol::packets::play::clientbound::{
    ChunkData, JoinGame, KeepAlive, MapChunkBulk, PositionLook,
};
use umicraft_protocol::packets::play::serverbound::{
    Brand, ClientSettings, Input, KeepAlive as ServerKeepAlive, PositionLook as ServerPositionLook,
};
use umicraft_protocol::varint::write_var_u32;
use umicraft_protocol::PlayServerbound;
use umicraft_world::World;

const USAGE: &str = "usage: umicraft-movement-probe <host> <port> [--name NAME] [--ticks N] [--forward V] [--report]";

/// Tick period of the 1.8 server and of the client simulation.
const TICK: Duration = Duration::from_millis(50);

/// How long the probe waits for the world chunks before measuring.
const WARMUP: Duration = Duration::from_secs(3);

struct Args {
    host: String,
    port: u16,
    name: String,
    ticks: u32,
    forward: f32,
    report: bool,
    /// How many ticks the reported position lags behind.
    ///
    /// A lag makes the server notice the difference between its own
    /// simulation and what we report, so it resynchronises us (0x08) and
    /// reveals its authoritative position. With a lag of zero the server
    /// accepts everything and reveals nothing.
    lag: u32,
    /// Print the model's trajectory every ten ticks.
    trace: bool,
    /// Scale applied to the reported displacement.
    ///
    /// The server simulates the movement on its own and compares it with
    /// what the client reports. Reporting a *scaled* displacement makes the
    /// two diverge, and the server's anti-cheat eventually rejects the
    /// session. The scale that survives the longest is the server's own
    /// speed, which is what the probe is after.
    scale: f64,
}

fn parse_args(argv: Vec<String>) -> Result<Args, String> {
    let mut host = None;
    let mut port = None;
    let mut name = "umicraft-probe".to_string();
    let mut ticks = 120_u32;
    let mut forward = 1.0_f32;
    let mut report = false;
    let mut lag = 10_u32;
    let mut scale = 1.0_f64;
    let mut trace = false;
    let mut index = 0;
    while index < argv.len() {
        match argv[index].as_str() {
            "--help" | "-h" => return Err(USAGE.to_string()),
            "--name" => {
                name = argv.get(index + 1).ok_or("missing value")?.clone();
                index += 2;
            }
            "--ticks" => {
                ticks = argv
                    .get(index + 1)
                    .ok_or("missing value")?
                    .parse()
                    .map_err(|_| "ticks must be a number")?;
                index += 2;
            }
            "--forward" => {
                forward = argv
                    .get(index + 1)
                    .ok_or("missing value")?
                    .parse()
                    .map_err(|_| "forward must be a number")?;
                index += 2;
            }
            "--report" => {
                report = true;
                index += 1;
            }
            "--trace" => {
                trace = true;
                index += 1;
            }
            "--scale" => {
                scale = argv
                    .get(index + 1)
                    .ok_or("missing value")?
                    .parse()
                    .map_err(|_| "scale must be a number")?;
                index += 2;
            }
            "--lag" => {
                lag = argv
                    .get(index + 1)
                    .ok_or("missing value")?
                    .parse()
                    .map_err(|_| "lag must be a number")?;
                index += 2;
            }
            other if host.is_none() => {
                host = Some(other.to_string());
                index += 1;
            }
            other if port.is_none() => {
                port = Some(other.parse().map_err(|_| "port must be a number")?);
                index += 1;
            }
            other => return Err(format!("unexpected argument {other:?}\n{USAGE}")),
        }
    }
    Ok(Args {
        host: host.ok_or_else(|| format!("missing host\n{USAGE}"))?,
        port: port.ok_or_else(|| format!("missing port\n{USAGE}"))?,
        name,
        ticks,
        forward,
        report,
        lag,
        scale,
        trace,
    })
}

/// One authoritative position reported by the server.
#[derive(Debug, Clone, Copy)]
struct Sample {
    /// Probe tick at which the sample was received.
    tick: u32,
    /// Authoritative feet position.
    x: f64,
    y: f64,
    z: f64,
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let args = parse_args(std::env::args().skip(1).collect())?;
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let mut connection = Connection::connect(&args.host, args.port)
        .await
        .map_err(|error| format!("connect: {error}"))?;
    let profile = login_offline(&mut connection, &args.name, None)
        .await
        .map_err(|error| format!("login: {error}"))?;
    println!("logged in as {}", profile.username);

    let mut world = World::new(0);
    let mut samples: Vec<Sample> = Vec::new();
    let mut placement: Option<(f64, f64, f64)> = None;
    let mut entry_sent = false;
    let mut previous = (0.0_f64, 0.0_f64, 0.0_f64);
    let mut tick = 0_u32;
    let mut model: Option<GameState> = None;
    let mut history: Vec<(f64, f64, f64)> = Vec::new();
    let warmup_until = Instant::now() + WARMUP;
    let intent = InputIntent {
        forward: args.forward,
        strafe: 0.0,
        jump: false,
        sneak: false,
        sprint: false,
        yaw: 0.0,
        pitch: 0.0,
    };

    loop {
        // --- drain the incoming packets -----------------------------------
        while let Ok(Ok((id, body))) =
            tokio::time::timeout(Duration::from_millis(2), connection.read_packet()).await
        {
            match id {
                KeepAlive::ID => {
                    let packet = KeepAlive::decode(&mut &body[..])
                        .map_err(|error| format!("keepalive: {error}"))?;
                    connection
                        .write_packet(&PlayServerbound::KeepAlive(ServerKeepAlive {
                            id: packet.id,
                        }))
                        .await
                        .map_err(|error| format!("keepalive answer: {error}"))?;
                }
                JoinGame::ID => {
                    let packet = JoinGame::decode(&mut &body[..])
                        .map_err(|error| format!("join: {error}"))?;
                    println!(
                        "joined: entity_id={} dimension={} level_type={}",
                        packet.entity_id, packet.dimension, packet.level_type
                    );
                    if !entry_sent {
                        send_entry_packets(&mut connection).await?;
                        entry_sent = true;
                    }
                }
                PositionLook::ID => {
                    let packet = PositionLook::decode(&mut &body[..])
                        .map_err(|error| format!("position: {error}"))?;
                    let resolved = resolve(previous, &packet);
                    previous = resolved;
                    // Acknowledge the teleport the way a real client does.
                    connection
                        .write_packet(&PlayServerbound::PositionLook(ServerPositionLook {
                            x: resolved.0,
                            y: resolved.1,
                            z: resolved.2,
                            yaw: packet.yaw,
                            pitch: packet.pitch,
                            on_ground: true,
                        }))
                        .await
                        .map_err(|error| format!("position answer: {error}"))?;
                    if placement.is_none() {
                        placement = Some(resolved);
                        println!(
                            "placed at ({:.2}, {:.2}, {:.2})",
                            resolved.0, resolved.1, resolved.2
                        );
                        model = Some(GameState::new(glam::DVec3::new(
                            resolved.0, resolved.1, resolved.2,
                        )));
                    } else {
                        samples.push(Sample {
                            tick,
                            x: resolved.0,
                            y: resolved.1,
                            z: resolved.2,
                        });
                    }
                }
                ChunkData::ID => {
                    let packet = ChunkData::decode(&mut &body[..])
                        .map_err(|error| format!("chunk data: {error}"))?;
                    let _ = world.apply_chunk_data(&packet);
                }
                MapChunkBulk::ID => {
                    let packet = MapChunkBulk::decode(&mut &body[..])
                        .map_err(|error| format!("chunk bulk: {error}"))?;
                    let _ = world.apply_map_chunk_bulk(&packet);
                }
                _ => {}
            }
        }

        let Some(state) = model.as_mut() else {
            tokio::time::sleep(Duration::from_millis(10)).await;
            continue;
        };

        // --- one tick -----------------------------------------------------
        if Instant::now() < warmup_until {
            // The world is still loading: the model must not tick yet, or it
            // would fall through the empty world and report a position the
            // server rejects as floating.
            tokio::time::sleep(TICK).await;
            tick += 1;
            continue;
        }

        connection
            .write_packet(&PlayServerbound::Input(Input {
                strafe: intent.strafe,
                forward: intent.forward,
                jumping: intent.jump,
                sneaking: intent.sneak,
            }))
            .await
            .map_err(|error| format!("input: {error}"))?;
        state.tick(&intent, &world);

        history.push((state.position().x, state.position().y, state.position().z));
        if args.trace && tick.is_multiple_of(10) {
            let position = state.position();
            println!(
                "tick {tick:4} model ({:.3}, {:.3}, {:.3}) chunks={} on_ground={}",
                position.x,
                position.y,
                position.z,
                world.chunk_count(),
                state.interpolated(1.0).on_ground
            );
        }
        if args.report {
            // The server expects the feet position, not the eye position.
            // The reported position lags behind so the server resynchronises
            // us and reveals its own authoritative position.
            let index = history.len().saturating_sub(1 + args.lag as usize);
            let (mut px, py, mut pz) = history[index];
            if args.scale != 1.0 {
                // Scale the displacement from the start of the run.
                let (sx, _, sz) = history[0];
                let (cx, _, cz) = history[history.len() - 1];
                px = sx + (px - sx) * args.scale;
                pz = sz + (pz - sz) * args.scale;
                let _ = (cx, cz);
            }
            let position = (px, py, pz);
            connection
                .write_packet(&PlayServerbound::PlayerPosition(
                    umicraft_protocol::packets::play::serverbound::PlayerPosition {
                        x: position.0,
                        y: position.1,
                        z: position.2,
                        on_ground: true,
                    },
                ))
                .await
                .map_err(|error| format!("position report: {error}"))?;
        }

        tick += 1;
        if tick >= args.ticks {
            break;
        }
        tokio::time::sleep(TICK).await;
    }

    report(&samples, model.as_ref().expect("model"), args.ticks);
    Ok(())
}

/// Sends the two entry packets a real client sends after `Join Game`.
///
/// The order matters: sending them before the server switches to the play
/// state makes it log `Bad packet id 21`.
async fn send_entry_packets(connection: &mut Connection) -> Result<(), String> {
    connection
        .write_packet(&PlayServerbound::ClientSettings(ClientSettings {
            locale: "en_us".to_string(),
            view_distance: 8,
            chat_mode: 0,
            chat_colors: true,
            skin_parts: 0b0111_1111,
        }))
        .await
        .map_err(|error| format!("client settings: {error}"))?;
    let mut data = Vec::with_capacity(12);
    write_var_u32(&mut data, "umicraft".len() as u32).map_err(|error| format!("brand: {error}"))?;
    data.extend_from_slice(b"umicraft");
    connection
        .write_packet(&PlayServerbound::Brand(Brand {
            channel: "MC|Brand".to_string(),
            data,
        }))
        .await
        .map_err(|error| format!("brand: {error}"))?;
    Ok(())
}

/// Resolves the 1.8 relative flags of a `Player Position and Look` packet.
fn resolve(previous: (f64, f64, f64), packet: &PositionLook) -> (f64, f64, f64) {
    let x = if packet.flags & 0x01 != 0 {
        previous.0 + packet.x
    } else {
        packet.x
    };
    let y = if packet.flags & 0x02 != 0 {
        previous.1 + packet.y
    } else {
        packet.y
    };
    let z = if packet.flags & 0x04 != 0 {
        previous.2 + packet.z
    } else {
        packet.z
    };
    (x, y, z)
}

/// Prints the comparison between the server's trajectory and the prediction.
fn report(samples: &[Sample], model: &GameState, ticks: u32) {
    println!("--- conformance ---");
    println!("server resyncs: {}", samples.len());
    if samples.len() < 2 {
        println!("not enough server samples to measure the speed");
        return;
    }
    let first = samples[0];
    let last = samples[samples.len() - 1];
    let server_ticks = (last.tick - first.tick).max(1) as f64;
    let server_distance = ((last.x - first.x).powi(2) + (last.z - first.z).powi(2)).sqrt();
    let server_speed = server_distance / server_ticks;
    println!(
        "server : {:.4} blocks/tick over {} ticks ({:.3} blocks total, y {:.2} -> {:.2})",
        server_speed, server_ticks as u64, server_distance, first.y, last.y
    );

    let predicted = model.position();
    let start = samples[0];
    let predicted_distance =
        ((predicted.x - start.x).powi(2) + (predicted.z - start.z).powi(2)).sqrt();
    let predicted_speed = predicted_distance / server_ticks;
    println!(
        "model  : {:.4} blocks/tick ({:.3} blocks total)",
        predicted_speed, predicted_distance
    );
    let ratio = if server_speed > 0.0 {
        predicted_speed / server_speed
    } else {
        f64::NAN
    };
    println!("ratio model/server: {ratio:.4}");
    if (ratio - 1.0).abs() > 0.02 {
        println!(
            "MISMATCH: the prediction is {:.2}% off the server",
            (ratio - 1.0) * 100.0
        );
    } else {
        println!("MATCH: the prediction agrees with the server within 2%");
    }
    println!("ticks requested: {ticks}");
}
