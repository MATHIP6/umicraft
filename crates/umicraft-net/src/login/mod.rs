// SPDX-License-Identifier: MIT OR Apache-2.0

//! Login (state `Login`): handshake → `Login Start` → optional
//! `Encryption Request` → optional `Set Compression` → `Login Success`.
//!
//! [`login_offline`] has no session; [`login_online`] answers the encryption
//! request with the launcher [`Session`] ([`encrypt`], NET-4).
//!
//! Clean failure contract (spec NET-2):
//! - `Encryption Request` without a session ⇒ [`NetError::OnlineModeRequired`].
//! - login-state `Disconnect` ⇒ [`NetError::KickedAtLogin`] with readable text.
//! - Any other login-state packet ⇒ [`NetError::UnexpectedPacket`].

use umicraft_auth::{HttpSessionServer, Session, SessionServer};
use umicraft_protocol::packets::login::serverbound;
use umicraft_protocol::{
    Handshake, LoginClientbound, LoginServerbound, ProtocolError, State, PROTOCOL_VERSION_1_8,
};

use crate::connection::Connection;
use crate::error::NetError;
use crate::status::disconnect_reason;

mod encrypt;

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
    login(
        conn,
        username,
        handshake_address,
        None::<Online<'_, HttpSessionServer>>,
    )
    .await
}

/// Runs the login flow with a launcher session: the server's `Encryption
/// Request` is answered, the session server is told about the join and the
/// connection switches to encrypted transport (vanilla
/// `NetHandlerLoginClient.handleEncryptionRequest`).
///
/// A server in offline mode does not ask for encryption, so the flow then
/// ends exactly as [`login_offline`] does, under the session's username.
///
/// # Errors
/// As [`login_offline`], plus [`NetError::Auth`] when the session server
/// refuses the join. Neither the error nor the logs contain the token.
pub async fn login_online<S: SessionServer>(
    conn: &mut Connection,
    session: &Session,
    session_server: &S,
    handshake_address: Option<&str>,
) -> Result<LoginProfile, NetError> {
    let online = Online {
        session,
        session_server,
    };
    login(conn, &session.username, handshake_address, Some(online)).await
}

/// What the encryption step needs: who joins, and where to register it.
struct Online<'a, S> {
    session: &'a Session,
    session_server: &'a S,
}

async fn login<S: SessionServer>(
    conn: &mut Connection,
    username: &str,
    handshake_address: Option<&str>,
    online: Option<Online<'_, S>>,
) -> Result<LoginProfile, NetError> {
    conn.write_packet(&Handshake {
        protocol_version: PROTOCOL_VERSION_1_8,
        server_address: handshake_address.unwrap_or(conn.host()).to_string(),
        server_port: conn.port(),
        next_state: 2,
    })
    .await?;
    let start = serverbound::LoginStart {
        username: username.to_string(),
    };
    conn.write_packet(&LoginServerbound::LoginStart(start))
        .await?;

    loop {
        let (id, body) = conn.read_packet().await?;
        match LoginClientbound::decode(id, &mut body.as_slice()) {
            Ok(LoginClientbound::SetCompression(packet)) => {
                conn.set_compression_threshold(packet.threshold);
            }
            Ok(LoginClientbound::LoginSuccess(packet)) => {
                return Ok(LoginProfile {
                    uuid: packet.uuid,
                    username: packet.username,
                });
            }
            Ok(LoginClientbound::Disconnect(packet)) => {
                return Err(NetError::KickedAtLogin {
                    reason: disconnect_reason(&packet.reason),
                });
            }
            Ok(LoginClientbound::EncryptionRequest(packet)) => match &online {
                Some(online) => encrypt::answer(conn, &packet, online).await?,
                None => {
                    return Err(NetError::OnlineModeRequired {
                        server_id: packet.server_id,
                        public_key: packet.public_key,
                        verify_token: packet.verify_token,
                    });
                }
            },
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
