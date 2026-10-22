// SPDX-License-Identifier: MIT OR Apache-2.0

//! Typed errors for network operations.

use umicraft_protocol::State;

/// Error type for `umicraft-net` connection, ping, login, and play flows.
#[derive(Debug, thiserror::Error)]
pub enum NetError {
    /// Transport-level failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Protocol-level failure (framing, decode, crypto helpers).
    #[error(transparent)]
    Protocol(#[from] umicraft_protocol::ProtocolError),

    /// The peer closed the socket (EOF happened mid-flow).
    #[error("server closed the connection")]
    ConnectionClosed,

    /// An awaited operation did not finish inside its budget.
    #[error("operation `{operation}` timed out")]
    TimedOut {
        /// Human-readable operation name for the message.
        operation: &'static str,
    },

    /// The server demanded encryption + premium authentication, which lands
    /// in M13; carried payloads let the auth crate answer the same handshake.
    #[error("server is online-mode; premium auth (M13) required (server_id {server_id:?})")]
    OnlineModeRequired {
        /// `Encryption Request` server id string (usually empty on vanilla).
        server_id: String,
        /// Server RSA public key, SPKI DER, for the M13 flow.
        public_key: Vec<u8>,
        /// Verify token to echo back encrypted.
        verify_token: Vec<u8>,
    },

    /// The server sent a login-state `Disconnect`.
    #[error("kicked during login: {reason}")]
    KickedAtLogin {
        /// Kick reason, best-effort readable text from chat JSON.
        reason: String,
    },

    /// The server sent a play-state `Disconnect`.
    #[error("kicked during play: {reason}")]
    KickedAtPlay {
        /// Kick reason, best-effort readable text from chat JSON.
        reason: String,
    },

    /// A packet id the given state must never produce.
    #[error("unexpected packet id {id:#04x} in {state:?} state")]
    UnexpectedPacket {
        /// Protocol state the packet arrived in.
        state: State,
        /// Raw packet id.
        id: u8,
    },

    /// Status response was not parseable JSON of the expected shape.
    #[error("status response is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
}
