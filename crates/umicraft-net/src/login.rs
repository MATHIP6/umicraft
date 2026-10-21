// SPDX-License-Identifier: MIT OR Apache-2.0

//! Offline-mode login (state `Login`): handshake → `Login Start` →
//! `Set Compression` (optional) → `Login Success`.
//!
//! Clean failure contract (spec NET-2):
//! - `Encryption Request` ⇒ [`NetError::OnlineModeRequired`] (premium auth is
//!   the M5 flow; payload is preserved for it).
//! - login-state `Disconnect` ⇒ [`NetError::KickedAtLogin`] with readable text.
//! - Any other login-state packet ⇒ [`NetError::UnexpectedPacket`].

use umicraft_protocol::{
    Handshake, LoginClientbound, LoginServerbound, ProtocolError, State, PROTOCOL_VERSION_1_8,
};

use crate::connection::Connection;
use crate::error::NetError;
use crate::status::disconnect_reason;

/// Profile granted by a successful offline login.
#[derive(Debug, Clone)]
pub struct LoginProfile {
    /// UUID string the offline server computed for the player.
    pub uuid: String,
    /// Username actually registered (server may trim).
    pub username: String,
}

/// Runs the offline-mode login flow over an open connection.
///
/// `handshake_address` overrides the address string inside the `Handshake`
/// (use case: proxy formats); it defaults to the connected host.
///
/// # Errors
/// [`NetError`] variants according to the module contract; transport errors
/// surface untouched.
pub async fn login_offline(
    conn: &mut Connection,
    username: &str,
    handshake_address: Option<&str>,
) -> Result<LoginProfile, NetError> {
    conn.write_packet(&Handshake {
        protocol_version: PROTOCOL_VERSION_1_8,
        server_address: handshake_address.unwrap_or(conn.host()).to_string(),
        server_port: conn.port(),
        next_state: 2,
    })
    .await?;
    conn.write_packet(&LoginServerbound::LoginStart {
        username: username.to_string(),
    })
    .await?;

    loop {
        let (id, body) = conn.read_packet().await?;
        match LoginClientbound::decode(id, &mut body.as_slice()) {
            Ok(LoginClientbound::SetCompression { threshold }) => {
                conn.set_compression_threshold(threshold);
            }
            Ok(LoginClientbound::LoginSuccess { uuid, username }) => {
                return Ok(LoginProfile { uuid, username });
            }
            Ok(LoginClientbound::Disconnect { reason }) => {
                return Err(NetError::KickedAtLogin {
                    reason: disconnect_reason(&reason),
                });
            }
            Ok(LoginClientbound::EncryptionRequest {
                server_id,
                public_key,
                verify_token,
            }) => {
                return Err(NetError::OnlineModeRequired {
                    server_id,
                    public_key,
                    verify_token,
                });
            }
            Err(ProtocolError::UnknownPacket { .. }) => {
                return Err(NetError::UnexpectedPacket {
                    state: State::Login,
                    id,
                });
            }
            Err(other) => return Err(other.into()),
        }
    }
}
