// SPDX-License-Identifier: MIT OR Apache-2.0

//! Online-mode login against the encrypting fake server (NET-4).

use std::time::Duration;

use umicraft_auth::{Secret, Session};
use umicraft_net::login::{login_offline, login_online};
use umicraft_net::{Connection, NetError};
use umicraft_protocol::PlayClientbound;
use umicraft_testkit::fake_online::EncryptionScript;
use umicraft_testkit::fake_server::{start, FakeServer, FakeServerConfig};
use umicraft_testkit::fake_session::FakeSessionServer;

fn session() -> Session {
    Session::new(
        "Notch",
        "069a79f4-44e9-4726-a5be-fca90e38aaf5",
        Secret::new("token"),
    )
    .expect("valid session")
}

async fn encrypting_server(compression_threshold: Option<i32>) -> FakeServer {
    start(FakeServerConfig {
        encryption: Some(EncryptionScript::default()),
        compression_threshold,
        ..FakeServerConfig::default()
    })
    .await
    .expect("fake server binds")
}

async fn connect(server: &FakeServer) -> Connection {
    Connection::connect("127.0.0.1", server.addr.port())
        .await
        .expect("connect works")
}

async fn assert_encrypted_login(compression_threshold: Option<i32>) {
    let server = encrypting_server(compression_threshold).await;
    let sessions = FakeSessionServer::accepting();
    let mut conn = connect(&server).await;

    let profile = login_online(&mut conn, &session(), &sessions, None)
        .await
        .expect("online login succeeds");
    assert_eq!(profile.username, "Notch");

    let (id, body) = tokio::time::timeout(Duration::from_secs(5), conn.read_packet())
        .await
        .expect("join game arrives in time")
        .expect("encrypted play packet reads");
    let packet = PlayClientbound::decode(id, &mut body.as_slice()).expect("play packet decodes");
    assert!(matches!(packet, PlayClientbound::JoinGame(_)));

    let joins = sessions.joins();
    assert_eq!(joins.len(), 1);
    assert_eq!(joins[0].username, "Notch");
    assert_eq!(joins[0].uuid, "069a79f444e94726a5befca90e38aaf5");
    let seen = server.observed().snapshot();
    assert_eq!(seen.encryption_responses, 1);
    assert_eq!(seen.encryption_failures, 0);
    assert_eq!(
        seen.encryption_hash.as_deref(),
        Some(&*joins[0].server_hash)
    );
}

#[tokio::test]
async fn encrypted_login_reaches_play_with_compression() {
    assert_encrypted_login(Some(256)).await;
}

#[tokio::test]
async fn encrypted_login_reaches_play_without_compression() {
    assert_encrypted_login(None).await;
}

#[tokio::test]
async fn rejected_join_surfaces_auth_error_and_sends_no_response() {
    let server = encrypting_server(None).await;
    let sessions = FakeSessionServer::rejecting(403);
    let mut conn = connect(&server).await;

    let err = login_online(&mut conn, &session(), &sessions, None)
        .await
        .expect_err("rejected join fails the login");
    assert!(matches!(err, NetError::Auth(_)), "got {err:?}");

    drop(conn);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(sessions.joins().len(), 1);
    assert_eq!(server.observed().snapshot().encryption_responses, 0);
}

#[tokio::test]
async fn offline_login_against_encrypting_server_needs_online_mode() {
    let server = encrypting_server(None).await;
    let mut conn = connect(&server).await;
    let err = login_offline(&mut conn, "TestBot", None)
        .await
        .expect_err("encryption cannot be answered offline");
    match err {
        NetError::OnlineModeRequired {
            public_key,
            verify_token,
            ..
        } => {
            assert_eq!(public_key.len(), 162);
            assert_eq!(verify_token.len(), 4);
        }
        other => panic!("expected OnlineModeRequired, got {other:?}"),
    }
}
