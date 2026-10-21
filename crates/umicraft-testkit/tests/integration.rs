// SPDX-License-Identifier: MIT OR Apache-2.0

//! M1 integration coverage: full ping/login/play session against the fake
//! 1.8 server, scripted failure paths, and the decoder fuzz targets (spec
//! acceptance: "fuzz tests on decoder run clean").

use std::sync::{Arc, Mutex};
use std::time::Duration;

use umicraft_net::client::PlayOptions;
use umicraft_net::login::login_offline;
use umicraft_net::{ping::ping, Connection, NetError};
use umicraft_protocol::framing::MAX_PACKET_LEN;
use umicraft_protocol::{Framing, StatusClientbound};
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
    let summary = conn
        .run_play(Some(Duration::from_millis(700)), &PlayOptions::default())
        .await
        .expect("play survives");
    assert!(
        summary.keepalives_answered >= 2,
        "keepalives: {:?}",
        summary.keepalives_answered
    );
    let join = summary.join.expect("join game decoded");
    assert_eq!(join.entity_id, 42);
    assert_eq!(join.level_type, "flat");
    assert_eq!(join.dimension, 0);
    let placement = summary.placement.expect("teleport answered");
    assert!((placement.y - 64.0).abs() < 1e-9);
    assert!(summary.stayed >= Duration::from_millis(600));

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
    let err = conn
        .run_play(None, &PlayOptions::default())
        .await
        .expect_err("kicked during play");
    assert!(matches!(err, NetError::KickedAtPlay { ref reason } if reason == "session over"));
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
    let summary = conn
        .run_play(Some(Duration::from_millis(400)), &PlayOptions::default())
        .await
        .expect("play works");
    assert!(summary.keepalives_answered >= 2);
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
    let summary = conn
        .run_play(Some(Duration::from_millis(300)), &PlayOptions::default())
        .await
        .expect("play works");
    assert!(summary.keepalives_answered >= 1);
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
