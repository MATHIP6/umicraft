// SPDX-License-Identifier: MIT OR Apache-2.0

//! A scriptable minimal 1.8 server used by tests and the packet logger.
//!
//! Implements the three protocol flows a client needs in M1: server list
//! ping, offline-mode login (with configurable compression, online-mode
//! simulation, and scripted kicks), and a play phase that sends `Join Game`,
//! optional teleport + chat, periodic keepalives, and a scripted delayed
//! kick — while recording everything it observes for test assertions.
//!
//! It reuses [`umicraft_net::Connection`] on the accept side so both ends of
//! test sockets exercise the same production framing code paths.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::net::TcpListener;
use umicraft_net::Connection;
use umicraft_protocol::packets::login::clientbound as login_clientbound;
use umicraft_protocol::packets::status::clientbound as status_clientbound;
use umicraft_protocol::{
    EncodablePacket, Handshake, LoginClientbound, LoginServerbound, PlayClientbound,
    PlayServerbound, StatusClientbound, StatusServerbound,
};

use crate::capture::{Capture, Direction};

/// Offline UUID string handed out by the fake server.
pub const FAKE_UUID: &str = "00000000-0000-0000-0000-000000000001";

/// Play-state `Join Game` script values.
#[derive(Debug, Clone)]
pub struct JoinScript {
    /// Player entity id.
    pub entity_id: i32,
    /// Raw gamemode byte.
    pub gamemode: u8,
    /// Dimension byte.
    pub dimension: i8,
    /// Difficulty byte.
    pub difficulty: u8,
    /// Player cap.
    pub max_players: u8,
    /// World type string.
    pub level_type: String,
    /// 1.8 trailing reduced-debug flag.
    pub reduced_debug_info: bool,
}

impl Default for JoinScript {
    fn default() -> Self {
        Self {
            entity_id: 42,
            gamemode: 0,
            dimension: 0,
            difficulty: 1,
            max_players: 20,
            level_type: "flat".to_string(),
            reduced_debug_info: false,
        }
    }
}

/// Everything the fake server can be scripted to do.
#[derive(Debug, Clone)]
pub struct FakeServerConfig {
    /// Status response JSON (raw body of `Response`).
    pub status_json: String,
    /// `Set Compression` threshold; `None` never sends the packet (raw
    /// framing), `Some(t <= 0)` disables compression per the protocol rule.
    pub compression_threshold: Option<i32>,
    /// Sends an `Encryption Request` instead of logging the player in.
    pub online_mode: bool,
    /// Kicks with this reason during login instead of succeeding.
    pub kick_at_login: Option<String>,
    /// Play-state `Join Game` values.
    pub join: JoinScript,
    /// Teleports via `Player Position and Look` after join at this position.
    pub placement: Option<(f64, f64, f64, f32, f32)>,
    /// Server chat sent after join (chat JSON body).
    pub chat_json: Option<String>,
    /// Keepalive send interval; `None` disables server keepalives.
    pub keepalive_interval: Option<Duration>,
    /// After this long in play, kick with the reason.
    pub kick_after_play: Option<(Duration, String)>,
    /// Decoded-packet (id + body) recorder.
    pub capture: Option<Arc<Mutex<Capture>>>,
}

impl Default for FakeServerConfig {
    fn default() -> Self {
        Self {
            status_json: r#"{"version":{"name":"UmicraftFake 1.8","protocol":47},"players":{"online":0,"max":20},"description":{"text":"Umicraft Test Server"}}"#.to_string(),
            compression_threshold: Some(256),
            online_mode: false,
            kick_at_login: None,
            join: JoinScript::default(),
            placement: Some((0.5, 64.0, 0.5, 0.0, 0.0)),
            chat_json: Some(r#"{"text":"welcome to the fake server"}"#.to_string()),
            keepalive_interval: Some(Duration::from_millis(200)),
            kick_after_play: None,
            capture: None,
        }
    }
}

/// What the fake server observed about its connections (for assertions).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Observed {
    /// Successful `Status Request` packets.
    pub status_requests: u32,
    /// `Ping` packets answered.
    pub pings: u32,
    /// Usernames from `Login Start`.
    pub login_usernames: Vec<String>,
    /// Any player reached the play state.
    pub play_started: bool,
    /// Keepalive ids echoed back by the client.
    pub keepalive_echoes: Vec<i32>,
    /// Chat messages the client sent.
    pub chat_messages: Vec<String>,
    /// Client settings (locale, view distance, chat mode, colors, skin parts).
    pub client_settings: Option<(String, u8, u8, bool, u8)>,
    /// `MC|Brand` payload without the channel.
    pub brand: Option<Vec<u8>>,
    /// Teleport answers as `(x, y, z, yaw, pitch, on_ground)`.
    pub placements: Vec<(f64, f64, f64, f32, f32, bool)>,
}
/// Shared observation state handed to tests.
#[derive(Clone, Debug)]
pub struct ObservedHandle {
    shared: Arc<Mutex<Observed>>,
}

impl ObservedHandle {
    /// Snapshot of everything observed so far.
    #[must_use]
    pub fn snapshot(&self) -> Observed {
        self.shared
            .lock()
            .expect("observed mutex is not poisoned")
            .clone()
    }
}

/// A running fake server.
#[derive(Debug)]
pub struct FakeServer {
    /// Bound loopback address; connect tests to this.
    pub addr: SocketAddr,
    observed: ObservedHandle,
}

impl FakeServer {
    /// Snapshot accessor for observed values.
    #[must_use]
    pub fn observed(&self) -> ObservedHandle {
        self.observed.clone()
    }
}

type Shared = Arc<Mutex<Observed>>;

/// Starts the fake server bound to `127.0.0.1` on an ephemeral port.
///
/// # Errors
/// [`std::io::Error`] when binding fails.
pub async fn start(config: FakeServerConfig) -> std::io::Result<FakeServer> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let addr = listener.local_addr()?;
    let observed: Shared = Arc::new(Mutex::new(Observed::default()));
    let observed_for_server = Arc::clone(&observed);

    let config = Arc::new(config);
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _peer)) => {
                    let config = Arc::clone(&config);
                    let observed = Arc::clone(&observed);
                    tokio::spawn(async move {
                        run_connection(
                            Connection::adopt(stream, "127.0.0.1", addr.port()),
                            config,
                            observed,
                        )
                        .await;
                    });
                }
                Err(err) => {
                    tracing::warn!(?err, "fake server accept failed");
                    break;
                }
            }
        }
    });

    Ok(FakeServer {
        addr,
        observed: ObservedHandle {
            shared: observed_for_server,
        },
    })
}

/// Applies one mutation to the shared observation state.
fn observe(shared: &Shared, mutate: impl FnOnce(&mut Observed)) {
    mutate(&mut shared.lock().expect("observed mutex is not poisoned"));
}

/// Capture-friendly timestamp (milliseconds; 0 on any clock weirdness).
fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |span| span.as_millis() as u64)
}

/// Records a decoded logical packet (id + body) into the capture, if any.
fn record(capture: &Option<Arc<Mutex<Capture>>>, direction: Direction, id: u8, body: &[u8]) {
    if let Some(capture) = capture {
        let mut data = Vec::with_capacity(body.len() + 1);
        data.push(id);
        data.extend_from_slice(body);
        capture
            .lock()
            .expect("capture mutex is not poisoned")
            .add(direction, unix_ms(), data);
    }
}

/// Sends one packet and records it (direction: server to client). Socket
/// failures surface on the next read, so this returns plain success.
async fn send_packet<PKT: EncodablePacket>(
    conn: &mut Connection,
    capture: &Option<Arc<Mutex<Capture>>>,
    packet: &PKT,
) -> bool {
    // Build the logical id+body locally for the capture; the connection
    // re-encodes the same packet inside its own framing layers.
    let mut body = Vec::with_capacity(64);
    body.push(packet.packet_id());
    if EncodablePacket::encode_body(packet, &mut body).is_ok() {
        record(
            capture,
            Direction::ServerToClient,
            packet.packet_id(),
            &body[1..],
        );
    }
    conn.write_packet(packet).await.is_ok()
}

/// Reads one packet; `None` ends the current phase (socket closed or broken).
async fn read_packet(
    conn: &mut Connection,
    capture: &Option<Arc<Mutex<Capture>>>,
) -> Option<(u8, Vec<u8>)> {
    match conn.read_packet().await {
        Ok((id, body)) => {
            record(capture, Direction::ClientToServer, id, &body);
            Some((id, body))
        }
        Err(err) => {
            tracing::trace!(?err, "fake server: read ended");
            None
        }
    }
}

/// Per-connection server task: `Handshake` selects the phase; anything else
/// closes the connection since that is a protocol violation anyway.
async fn run_connection(mut conn: Connection, config: Arc<FakeServerConfig>, observed: Shared) {
    let capture = config.capture.clone();
    let Some((id, body)) = read_packet(&mut conn, &capture).await else {
        return;
    };
    if id != 0x00 {
        tracing::trace!(
            packet_id = id,
            "fake server: first packet was not a handshake"
        );
        return;
    }
    let Ok(handshake) = Handshake::decode(&mut body.as_slice()) else {
        tracing::trace!("fake server: undecodable handshake");
        return;
    };

    match handshake.next_state {
        1 => run_status(&mut conn, &config, &capture, &observed).await,
        2 => run_login_then_play(&mut conn, &config, &capture, &observed).await,
        n => tracing::trace!(state = n, "fake server: unknown handshake state"),
    }
}

/// Status loop: answer requests and pings until the client closes the socket.
async fn run_status(
    conn: &mut Connection,
    config: &FakeServerConfig,
    capture: &Option<Arc<Mutex<Capture>>>,
    observed: &Shared,
) {
    loop {
        let Some((id, body)) = read_packet(conn, capture).await else {
            return;
        };
        match StatusServerbound::decode(id, &mut body.as_slice()) {
            Ok(StatusServerbound::StatusRequest(_)) => {
                observe(observed, |state| state.status_requests += 1);
                let response = StatusClientbound::Response(status_clientbound::Response {
                    json: config.status_json.clone(),
                });
                if !send_packet(conn, capture, &response).await {
                    return;
                }
            }
            Ok(StatusServerbound::Ping(ping)) => {
                observe(observed, |state| state.pings += 1);
                let pong = StatusClientbound::Pong(status_clientbound::Pong { time: ping.time });
                if !send_packet(conn, capture, &pong).await {
                    return;
                }
            }
            Err(err) => {
                tracing::trace!(?err, packet_id = id, "fake server: odd status packet");
                return;
            }
        }
    }
}

/// Login phase, and on success the play loop until either side ends.
async fn run_login_then_play(
    conn: &mut Connection,
    config: &FakeServerConfig,
    capture: &Option<Arc<Mutex<Capture>>>,
    observed: &Shared,
) {
    // Login phase: exactly one `Login Start` is expected; anything else is a
    // protocol violation and closes the session.
    let Some((id, body)) = read_packet(conn, capture).await else {
        return;
    };
    let Ok(LoginServerbound::LoginStart(start)) =
        LoginServerbound::decode(id, &mut body.as_slice())
    else {
        tracing::trace!(packet_id = id, "fake server: odd login packet");
        return;
    };
    observe(observed, |state| {
        state.login_usernames.push(start.username.clone())
    });

    // Scripted login outcomes.
    if config.online_mode {
        let request = LoginClientbound::EncryptionRequest(login_clientbound::EncryptionRequest {
            server_id: String::new(),
            public_key: vec![0x55; 162],
            verify_token: vec![0x77; 4],
        });
        let _ = send_packet(conn, capture, &request).await;
        return;
    }
    if let Some(reason) = &config.kick_at_login {
        let kick = LoginClientbound::Disconnect(login_clientbound::Disconnect {
            reason: reason.clone(),
        });
        let _ = send_packet(conn, capture, &kick).await;
        return;
    }

    // Compression must be applied to the framing *after* the packet itself
    // is sent with the old layer (vanilla ordering, community docs).
    if let Some(threshold) = config.compression_threshold {
        if !send_packet(
            conn,
            capture,
            &LoginClientbound::SetCompression(login_clientbound::SetCompression { threshold }),
        )
        .await
        {
            return;
        }
        conn.set_compression_threshold(threshold);
    }
    let success = LoginClientbound::LoginSuccess(login_clientbound::LoginSuccess {
        uuid: FAKE_UUID.to_string(),
        username: start.username,
    });
    if !send_packet(conn, capture, &success).await {
        return;
    }

    run_play(conn, config, capture, observed).await;
}

/// Play loop: scripted intro, then reads interleaved with keepalive ticks and
/// a scripted delayed kick.
async fn run_play(
    conn: &mut Connection,
    config: &FakeServerConfig,
    capture: &Option<Arc<Mutex<Capture>>>,
    observed: &Shared,
) {
    let join = PlayClientbound::JoinGame {
        entity_id: config.join.entity_id,
        gamemode: config.join.gamemode,
        dimension: config.join.dimension,
        difficulty: config.join.difficulty,
        max_players: config.join.max_players,
        level_type: config.join.level_type.clone(),
        reduced_debug_info: config.join.reduced_debug_info,
    };
    let placement = config
        .placement
        .map(|(x, y, z, yaw, pitch)| PlayClientbound::PositionLook {
            x,
            y,
            z,
            yaw,
            pitch,
            flags: 0,
        });
    let chat = config.chat_json.as_ref().map(|json| PlayClientbound::Chat {
        json: json.clone(),
        position: 0,
    });

    observe(observed, |state| state.play_started = true);

    for packet in [Some(join), placement, chat].into_iter().flatten() {
        if !send_packet(conn, capture, &packet).await {
            return;
        }
    }

    let interval = config.keepalive_interval;
    let mut next_keepalive = interval.map(|span| tokio::time::Instant::now() + span);
    let kick_deadline = config
        .kick_after_play
        .as_ref()
        .map(|(delay, _)| tokio::time::Instant::now() + *delay);
    let mut keepalive_id: i32 = 90_000;

    loop {
        tokio::select! {
            read = conn.read_packet() => {
                let Some((id, body)) = read.ok() else {
                    // Client went away or socket broke — normal phase end.
                    return;
                };
                record(capture, Direction::ClientToServer, id, &body);
                match PlayServerbound::decode(id, &mut body.as_slice()) {
                    Ok(PlayServerbound::KeepAlive { id }) => {
                        observe(observed, |state| state.keepalive_echoes.push(id));
                    }
                    Ok(PlayServerbound::Chat { message }) => {
                        observe(observed, |state| state.chat_messages.push(message));
                    }
                    Ok(PlayServerbound::PositionLook { x, y, z, yaw, pitch, on_ground }) => {
                        observe(observed, |state| {
                            state.placements.push((x, y, z, yaw, pitch, on_ground));
                        });
                    }
                    Ok(PlayServerbound::ClientSettings {
                        locale,
                        view_distance,
                        chat_mode,
                        chat_colors,
                        skin_parts,
                    }) => {
                        observe(observed, |state| {
                            state.client_settings =
                                Some((locale, view_distance, chat_mode, chat_colors, skin_parts));
                        });
                    }
                    Ok(PlayServerbound::Brand { data, .. }) => {
                        observe(observed, |state| state.brand = Some(data));
                    }
                    Err(err) => {
                        // Play state is full of packets outside the M1 set;
                        // tolerate and trace rather than end the connection.
                        tracing::trace!(?err, packet_id = id, "fake server: skipped play packet");
                    }
                }
            }
            _ = tokio::time::sleep_until(next_keepalive.unwrap_or(
                tokio::time::Instant::now(),
            )), if next_keepalive.is_some() => {
                if !send_packet(conn, capture, &PlayClientbound::KeepAlive { id: keepalive_id }).await {
                    return;
                }
                keepalive_id += 1;
                next_keepalive = interval.map(|span| tokio::time::Instant::now() + span);
            }
            _ = tokio::time::sleep_until(kick_deadline.unwrap_or(
                tokio::time::Instant::now(),
            )), if kick_deadline.is_some() => {
                let reason = config
                    .kick_after_play
                    .as_ref()
                    .map_or("timed kick".to_string(), |(_, why)| why.clone());
                let kick = PlayClientbound::Disconnect { reason };
                let _ = send_packet(conn, capture, &kick).await;
                return;
            }
        }
    }
}
