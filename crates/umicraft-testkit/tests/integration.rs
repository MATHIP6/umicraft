// SPDX-License-Identifier: MIT OR Apache-2.0

//! M1 integration coverage: full ping/login/play session against the fake
//! 1.8 server, scripted failure paths, and the decoder fuzz targets (spec
//! acceptance: "fuzz tests on decoder run clean").

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use umicraft_net::login::login_offline;
use umicraft_net::play::{drive, PlayEvent, PlayOptions, PlaySession, SessionCounters, SessionEnd};
use umicraft_net::{ping::ping, Connection, NetError};
use umicraft_protocol::framing::MAX_PACKET_LEN;
use umicraft_protocol::packets::play::serverbound::Chat;
use umicraft_protocol::{Framing, PlayServerbound, StatusClientbound};
use umicraft_testkit::capture::{Capture, Direction};
use umicraft_testkit::fake_server::{start, FakeServerConfig, FAKE_UUID};

/// Deterministic xorshift64* so failures are reproducible.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        self.0 = state;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

/// What a test client observed while it held the play state.
struct Played {
    events: Vec<PlayEvent>,
    /// `None` when the deadline stopped the session.
    end: Option<SessionEnd>,
    counters: SessionCounters,
}

/// Drives a logged-in connection like a client without a simulation: every
/// placement is acknowledged. `stay` stops the session; dropping `drive` is
/// safe because the test drops the connection afterwards (ADR-0010).
async fn play(conn: &mut Connection, stay: Duration) -> Result<Played, NetError> {
    let mut session = PlaySession::new(PlayOptions::default());
    let (acks, mut commands) = mpsc::channel(4);
    let mut played = Played {
        events: Vec::new(),
        end: None,
        counters: SessionCounters::default(),
    };
    let run = drive(
        conn,
        &mut session,
        &mut commands,
        &played.counters,
        |event| {
            if let PlayEvent::Placement(placement) = &event {
                let _ = acks.try_send(placement.acknowledgement(true));
            }
            played.events.push(event);
        },
    );
    played.end = match tokio::time::timeout(stay, run).await {
        Ok(end) => Some(end?),
        Err(_) => None,
    };
    Ok(played)
}

async fn logged_in(
    config: FakeServerConfig,
    name: &str,
) -> (Connection, umicraft_testkit::fake_server::FakeServer) {
    let server = start(config).await.expect("fake server binds");
    let mut conn = Connection::connect("127.0.0.1", server.addr.port())
        .await
        .expect("connect works");
    login_offline(&mut conn, name, None)
        .await
        .expect("login works");
    (conn, server)
}

#[tokio::test]
async fn full_offline_session_ping_login_play() {
    let capture = Arc::new(Mutex::new(Capture::default()));
    let config = FakeServerConfig {
        capture: Some(Arc::clone(&capture)),
        ..FakeServerConfig::default()
    };
    let server = start(config).await.expect("fake server binds");
    let port = server.addr.port();

    // Server list ping (spec NET-1).
    let result = ping("127.0.0.1", port, Duration::from_secs(5))
        .await
        .expect("ping works");
    assert_eq!(result.status.motd_text(), "Umicraft Test Server");
    assert_eq!(
        result.status.version.as_ref().and_then(|v| v.name.clone()),
        Some("UmicraftFake 1.8".to_string())
    );
    assert_eq!(result.status.players.as_ref().and_then(|p| p.max), Some(20));
    assert!(result.latency < Duration::from_secs(2));

    // Offline login (spec NET-2).
    let mut conn = Connection::connect("127.0.0.1", port)
        .await
        .expect("connect works");
    let profile = login_offline(&mut conn, "TestBot", None)
        .await
        .expect("login works");
    assert_eq!(profile.uuid, FAKE_UUID);
    assert_eq!(profile.username, "TestBot");

    // Play-held connection with keepalive answering (spec NET-6).
    let started = Instant::now();
    let played = play(&mut conn, Duration::from_millis(700))
        .await
        .expect("play survives");
    assert_eq!(played.end, None, "the deadline ends the session");
    assert!(started.elapsed() >= Duration::from_millis(600));
    let keepalives = played.counters.keepalives_answered();
    assert!(keepalives >= 2, "keepalives: {keepalives}");
    let join = played
        .events
        .iter()
        .find_map(|event| match event {
            PlayEvent::JoinGame(info) => Some(info),
            _ => None,
        })
        .expect("join game decoded");
    assert_eq!(join.entity_id, 42);
    assert_eq!(join.level_type, "flat");
    assert_eq!(join.dimension, 0);
    let placement = played
        .events
        .iter()
        .find_map(|event| match event {
            PlayEvent::Placement(placement) => Some(placement),
            _ => None,
        })
        .expect("placement decoded");
    assert!((placement.y - 64.0).abs() < 1e-9);

    // Server-side observations agree with the client-side summary.
    let observed = server.observed().snapshot();
    assert_eq!(observed.status_requests, 1);
    assert_eq!(observed.pings, 1);
    assert_eq!(observed.login_usernames, vec!["TestBot".to_string()]);
    assert!(observed.play_started);
    assert!(
        observed.keepalive_echoes.starts_with(&[90_000, 90_001]),
        "echo list: {:?}",
        observed.keepalive_echoes
    );
    let (locale, view, chat_mode, colors, skins) =
        observed.client_settings.expect("client settings arrived");
    assert_eq!(
        (locale.as_str(), view, chat_mode, colors, skins),
        ("en_us", 8, 0, true, 0b0111_1111)
    );
    assert_eq!(
        observed.brand,
        Some(vec![8, b'u', b'm', b'i', b'c', b'r', b'a', b'f', b't'])
    );
    assert!(observed
        .placements
        .iter()
        .any(|(x, y, _z, _yaw, _pitch, ground)| {
            (*y - 64.0).abs() < 1e-9 && *ground && (*x - 0.5).abs() < 1e-9
        }));
    assert!(observed.chat_messages.is_empty());

    // Both flow directions appear in the capture (logger/replay basis).
    let entries = capture.lock().expect("capture lock").entries.clone();
    assert!(entries
        .iter()
        .any(|e| e.direction == Direction::ClientToServer));
    assert!(entries
        .iter()
        .any(|e| e.direction == Direction::ServerToClient));
}

#[tokio::test]
async fn play_session_streams_join_placement_chat() {
    let config = FakeServerConfig {
        keepalive_interval: Some(Duration::from_millis(100)),
        ..FakeServerConfig::default()
    };
    let (mut conn, server) = logged_in(config, "Streamer").await;

    let played = play(&mut conn, Duration::from_millis(450))
        .await
        .expect("play works");
    match &played.events[..3] {
        [PlayEvent::JoinGame(info), PlayEvent::Placement(placement), PlayEvent::Chat(json)] => {
            assert_eq!(info.entity_id, 42);
            assert!((placement.y - 64.0).abs() < 1e-9);
            assert!(json.contains("welcome"));
        }
        other => panic!("expected join, placement, chat; got {other:?}"),
    }

    // Keepalives never become events; they are answered and counted.
    assert!(played.counters.keepalives_answered() >= 1);
    let observed = server.observed().snapshot();
    assert!(!observed.keepalive_echoes.is_empty());
    let (locale, view, _, _, _) = observed.client_settings.expect("entry packets");
    assert_eq!((locale.as_str(), view), ("en_us", 8));
}

/// The server sends a keepalive every 5 ms while the client writes a steady
/// stream of compressed commands: both directions stay ordered and complete.
/// The stalled-socket case of ADR-0010 is covered by the unit tests of
/// `umicraft_net::play::drive`, which control the socket buffers.
#[tokio::test]
async fn commands_and_keepalives_interleave_under_compression() {
    const MESSAGES: usize = 300;
    let config = FakeServerConfig {
        keepalive_interval: Some(Duration::from_millis(5)),
        compression_threshold: Some(64),
        ..FakeServerConfig::default()
    };
    let (mut conn, server) = logged_in(config, "Busy").await;

    let (sender, mut commands) = mpsc::channel(8);
    let producer = tokio::spawn(async move {
        for index in 0..MESSAGES {
            // 80 characters: above the threshold, so every frame is compressed.
            let message = format!("{index:04}{}", "x".repeat(76));
            let chat = PlayServerbound::Chat(Chat { message });
            if sender.send(chat).await.is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    });

    let mut session = PlaySession::new(PlayOptions::default());
    let counters = SessionCounters::default();
    let end = tokio::time::timeout(
        Duration::from_secs(10),
        drive(&mut conn, &mut session, &mut commands, &counters, |_| {}),
    )
    .await
    .expect("the session ends when the producer is done")
    .expect("the stream stays valid");
    assert_eq!(end, SessionEnd::CommandsClosed);
    producer.await.expect("producer task");

    // Every command and every answer was written before `drive` returned,
    // and its counters no longer move; give the server the time to read the
    // tail of the stream.
    let answered = counters.keepalives_answered() as usize;
    let deadline = Instant::now() + Duration::from_secs(2);
    let observed = loop {
        let observed = server.observed().snapshot();
        let caught_up =
            observed.chat_messages.len() == MESSAGES && observed.keepalive_echoes.len() == answered;
        if caught_up || Instant::now() > deadline {
            break observed;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let expected: Vec<String> = (0..MESSAGES)
        .map(|index| format!("{index:04}{}", "x".repeat(76)))
        .collect();
    assert_eq!(observed.chat_messages, expected);
    let echoes = &observed.keepalive_echoes;
    assert!(echoes.len() >= 20, "keepalives echoed: {}", echoes.len());
    let consecutive: Vec<i32> = (90_000..90_000 + echoes.len() as i32).collect();
    assert_eq!(echoes, &consecutive, "every keepalive answered, in order");
    assert_eq!(counters.keepalives_answered(), echoes.len() as u64);
}

#[tokio::test]
async fn online_mode_produces_typed_payload_for_m5() {
    let config = FakeServerConfig {
        online_mode: true,
        ..FakeServerConfig::default()
    };
    let server = start(config).await.expect("fake server binds");

    let mut conn = Connection::connect("127.0.0.1", server.addr.port())
        .await
        .expect("connect works");
    let err = login_offline(&mut conn, "TestBot", None)
        .await
        .expect_err("online mode must not silently succeed");
    match err {
        NetError::OnlineModeRequired {
            server_id,
            public_key,
            verify_token,
        } => {
            assert_eq!(server_id, "");
            assert_eq!(public_key.len(), 162);
            assert_eq!(verify_token.len(), 4);
        }
        other => panic!("expected OnlineModeRequired, got {other:?}"),
    }
}

#[tokio::test]
async fn login_kick_is_clean_with_reason() {
    let config = FakeServerConfig {
        kick_at_login: Some(r#"{"text":"you are banned"}"#.to_string()),
        ..FakeServerConfig::default()
    };
    let server = start(config).await.expect("fake server binds");

    let mut conn = Connection::connect("127.0.0.1", server.addr.port())
        .await
        .expect("connect works");
    let err = login_offline(&mut conn, "Banned", None)
        .await
        .expect_err("kicked");
    assert!(matches!(err, NetError::KickedAtLogin { ref reason } if reason == "you are banned"));
}

#[tokio::test]
async fn play_kick_is_clean_with_reason() {
    let config = FakeServerConfig {
        kick_after_play: Some((
            Duration::from_millis(350),
            r#"{"text":"session over"}"#.to_string(),
        )),
        ..FakeServerConfig::default()
    };
    let server = start(config).await.expect("fake server binds");

    let mut conn = Connection::connect("127.0.0.1", server.addr.port())
        .await
        .expect("connect works");
    login_offline(&mut conn, "Short", None)
        .await
        .expect("login works");
    let played = play(&mut conn, Duration::from_secs(5))
        .await
        .expect("a kick is a clean end");
    assert_eq!(
        played.end,
        Some(SessionEnd::Kicked("session over".to_string()))
    );
}

#[tokio::test]
async fn raw_framing_session_works() {
    let config = FakeServerConfig {
        compression_threshold: None,
        keepalive_interval: Some(Duration::from_millis(100)),
        ..FakeServerConfig::default()
    };
    let server = start(config).await.expect("fake server binds");

    let mut conn = Connection::connect("127.0.0.1", server.addr.port())
        .await
        .expect("connect works");
    login_offline(&mut conn, "RawOk", None)
        .await
        .expect("login works");
    let played = play(&mut conn, Duration::from_millis(400))
        .await
        .expect("play works");
    assert!(played.counters.keepalives_answered() >= 2);
}

#[tokio::test]
async fn threshold_zero_disables_compression_symmetric() {
    let config = FakeServerConfig {
        compression_threshold: Some(0),
        ..FakeServerConfig::default()
    };
    let server = start(config).await.expect("fake server binds");

    let mut conn = Connection::connect("127.0.0.1", server.addr.port())
        .await
        .expect("connect works");
    login_offline(&mut conn, "ZeroThr", None)
        .await
        .expect("login works");
    let played = play(&mut conn, Duration::from_millis(300))
        .await
        .expect("play works");
    assert!(played.counters.keepalives_answered() >= 1);
}

#[test]
fn decoder_fuzz_survives_random_garbage() {
    let mut rng = Rng(0xC0FF_EE01);
    for _ in 0..2_000 {
        let len = (rng.next() % 96 + 1) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| (rng.next() & 0xFF) as u8).collect();

        let _ = Framing::Raw.unwrap_body(&bytes);
        let threshold = (rng.next() % 1024 + 1) as u32;
        let _ = Framing::Compressed { threshold }.unwrap_body(&bytes);

        let mut slice = bytes.as_slice();
        let _ = umicraft_protocol::varint::read_var_u32(&mut slice);
        let mut slice = bytes.as_slice();
        let _ = umicraft_protocol::types::read_string_capped(&mut slice);
        let mut slice = bytes.as_slice();
        let _ = umicraft_protocol::packets::read_byte_array(&mut slice, 64);

        for state_id in 0..8_u8 {
            let mut slice = bytes.as_slice();
            let _ = StatusClientbound::decode(state_id, &mut slice);
            let mut status_in: &[u8] = &bytes[..];
            let _ = umicraft_protocol::StatusServerbound::decode(state_id, &mut status_in);
            let mut login_in: &[u8] = &bytes[..];
            let _ = umicraft_protocol::LoginClientbound::decode(state_id, &mut login_in);
            let mut login_serverbound: &[u8] = &bytes[..];
            let _ = umicraft_protocol::LoginServerbound::decode(state_id, &mut login_serverbound);
            let mut play_in: &[u8] = &bytes[..];
            let _ = umicraft_protocol::PlayClientbound::decode(state_id, &mut play_in);
            let mut play_serverbound: &[u8] = &bytes[..];
            let _ = umicraft_protocol::PlayServerbound::decode(state_id, &mut play_serverbound);
        }

        let mut framing_bytes: &[u8] = &bytes[..];
        let _ = Framing::Raw.read_frame(&mut framing_bytes);
        let mut logger_bytes = bytes.clone();
        let _ = umicraft_testkit::capture::take_raw_frame(&mut logger_bytes);
    }
}

#[test]
fn frame_peeling_is_split_resistant() {
    let mut rng = Rng(0xFEED_ADDE);
    for _ in 0..300 {
        let payload_len = (rng.next() % 300) as usize + 1; // a packet always has an id byte
        let payload: Vec<u8> = (0..payload_len)
            .map(|_| (rng.next() & 0xFF) as u8)
            .collect();
        let mut framed = Vec::new();
        Framing::Raw
            .write_frame(&mut framed, &payload)
            .expect("in-bounds framing");

        // Split the wire bytes at awkward sizes; the peeler must see exactly
        // one frame back, untouched.
        for chunk_size in [1usize, 2, 3, 7] {
            let mut buffer = Vec::new();
            let mut collected = Vec::new();
            for chunk in framed.chunks(chunk_size.max(1)) {
                buffer.extend_from_slice(chunk);
                if let Ok(Some(frame)) = umicraft_testkit::capture::take_raw_frame(&mut buffer) {
                    collected.push(frame);
                }
            }
            assert_eq!(collected, vec![framed.clone()], "chunk size {chunk_size}");
            assert!(buffer.is_empty());
        }
    }
}

#[test]
fn hostile_length_declarations_stay_bounded() {
    // A frame declaring the max allowed length over untrusted TCP must not
    // allocate beyond the cap *or* wrap the peeler into a spin.
    let mut buffer = Vec::new();
    umicraft_protocol::varint::write_var_u32(&mut buffer, MAX_PACKET_LEN as u32).expect("varint");
    buffer.extend_from_slice(b"we never deliver the promised bytes");
    // take_raw_frame returns None (needs more bytes) rather than looping.
    assert!(umicraft_testkit::capture::take_raw_frame(&mut buffer)
        .expect("peel attempt is an Ok")
        .is_none());
}
