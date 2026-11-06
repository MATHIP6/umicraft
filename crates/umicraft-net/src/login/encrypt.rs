// SPDX-License-Identifier: MIT OR Apache-2.0

//! The online-mode key exchange (NET-4), the client half of MCP
//! `NetHandlerLoginClient.handleEncryptionRequest`.
//!
//! The order matters and mirrors vanilla: draw the shared secret, register
//! the join with the session server, and only then send the `Encryption
//! Response` and switch the socket to AES/CFB8. A refused join therefore
//! never reaches the game server.

use umicraft_auth::SessionServer;
use umicraft_protocol::encryption::{random_shared_secret, rsa_encrypt, server_id_hash};
use umicraft_protocol::packets::login::clientbound::EncryptionRequest;
use umicraft_protocol::packets::login::serverbound::EncryptionResponse;
use umicraft_protocol::LoginServerbound;

use super::Online;
use crate::connection::Connection;
use crate::error::NetError;

/// Answers one `Encryption Request` and enables encryption on `conn`.
///
/// # Errors
/// [`NetError::Auth`] when the session server refuses the join,
/// [`NetError::Protocol`] on a malformed server key, transport errors as is.
pub(super) async fn answer<S: SessionServer>(
    conn: &mut Connection,
    request: &EncryptionRequest,
    online: &Online<'_, S>,
) -> Result<(), NetError> {
    let secret = random_shared_secret();
    let hash = server_id_hash(&request.server_id, &secret, &request.public_key);
    online.session_server.join(online.session, &hash).await?;

    let response = EncryptionResponse {
        shared_secret: rsa_encrypt(&request.public_key, &secret)?,
        nonce: rsa_encrypt(&request.public_key, &request.verify_token)?,
    };
    conn.write_packet(&LoginServerbound::EncryptionResponse(response))
        .await?;
    conn.enable_encryption(&secret)?;
    tracing::debug!("login: transport encrypted");
    Ok(())
}
