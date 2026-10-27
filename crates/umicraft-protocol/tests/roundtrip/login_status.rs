//! Round-trip tests for the handshake, status and login packets.

use proptest::collection::vec;
use proptest::prelude::*;
use umicraft_protocol::packets::login::{clientbound as login_cb, serverbound as login_sb};
use umicraft_protocol::packets::status::{clientbound as status_cb, serverbound as status_sb};
use umicraft_protocol::{
    Handshake, LoginClientbound, LoginServerbound, StatusClientbound, StatusServerbound,
};

use crate::common::*;

/// `next_state` is 1 (status) or 2 (login); the decoder rejects anything else.
fn handshake() -> impl Strategy<Value = Handshake> {
    (any::<i32>(), text(255), any::<u16>(), 1i32..=2).prop_map(
        |(protocol_version, server_address, server_port, next_state)| Handshake {
            protocol_version,
            server_address,
            server_port,
            next_state,
        },
    )
}

roundtrip_case!(
    HandshakePacket,
    DEFAULT,
    std::convert::identity,
    handshake()
);

roundtrip_case!(
    StatusResponse,
    DEFAULT,
    status_response,
    text(256).prop_map(|json| status_cb::Response { json })
);
roundtrip_case!(
    StatusPong,
    DEFAULT,
    status_pong,
    any::<i64>().prop_map(|time| status_cb::Pong { time })
);
roundtrip_case!(
    StatusRequest,
    DEFAULT,
    status_request,
    Just(status_sb::Request)
);
roundtrip_case!(
    StatusPing,
    DEFAULT,
    status_ping,
    any::<i64>().prop_map(|time| status_sb::Ping { time })
);

roundtrip_case!(
    LoginDisconnect,
    DEFAULT,
    login_disconnect,
    text(256).prop_map(|reason| login_cb::Disconnect { reason })
);
roundtrip_case!(
    LoginEncryptionRequest,
    DEFAULT,
    login_encryption_request,
    (text(20), bytes(162), bytes(128)).prop_map(|(server_id, public_key, verify_token)| {
        login_cb::EncryptionRequest {
            server_id,
            public_key,
            verify_token,
        }
    })
);
roundtrip_case!(
    LoginSuccess,
    DEFAULT,
    login_success,
    (text(36), text(16)).prop_map(|(uuid, username)| login_cb::LoginSuccess { uuid, username })
);
roundtrip_case!(
    LoginSetCompression,
    DEFAULT,
    login_set_compression,
    any::<i32>().prop_map(|threshold| login_cb::SetCompression { threshold })
);
roundtrip_case!(
    LoginStart,
    DEFAULT,
    login_start,
    text(16).prop_map(|username| login_sb::LoginStart { username })
);
roundtrip_case!(
    LoginEncryptionResponse,
    DEFAULT,
    login_encryption_response,
    (vec(any::<u8>(), 0..=128), vec(any::<u8>(), 0..=128)).prop_map(|(shared_secret, nonce)| {
        login_sb::EncryptionResponse {
            shared_secret,
            nonce,
        }
    })
);

fn status_response(value: status_cb::Response) -> StatusClientbound {
    StatusClientbound::Response(value)
}
fn status_pong(value: status_cb::Pong) -> StatusClientbound {
    StatusClientbound::Pong(value)
}
fn status_request(value: status_sb::Request) -> StatusServerbound {
    StatusServerbound::StatusRequest(value)
}
fn status_ping(value: status_sb::Ping) -> StatusServerbound {
    StatusServerbound::Ping(value)
}
fn login_disconnect(value: login_cb::Disconnect) -> LoginClientbound {
    LoginClientbound::Disconnect(value)
}
fn login_encryption_request(value: login_cb::EncryptionRequest) -> LoginClientbound {
    LoginClientbound::EncryptionRequest(value)
}
fn login_success(value: login_cb::LoginSuccess) -> LoginClientbound {
    LoginClientbound::LoginSuccess(value)
}
fn login_set_compression(value: login_cb::SetCompression) -> LoginClientbound {
    LoginClientbound::SetCompression(value)
}
fn login_start(value: login_sb::LoginStart) -> LoginServerbound {
    LoginServerbound::LoginStart(value)
}
fn login_encryption_response(value: login_sb::EncryptionResponse) -> LoginServerbound {
    LoginServerbound::EncryptionResponse(value)
}
