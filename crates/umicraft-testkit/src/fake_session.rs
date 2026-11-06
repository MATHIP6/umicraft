// SPDX-License-Identifier: MIT OR Apache-2.0

//! A recording stand-in for the Mojang session server (NET-4 tests).
//!
//! Implements [`SessionServer`] without any network access. It keeps the
//! username, UUID and server hash of every join, but never the access token.

use std::sync::Mutex;

use umicraft_auth::{AuthError, Session, SessionServer};

/// One join the client registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinRecord {
    /// Player name of the session.
    pub username: String,
    /// Undashed profile UUID of the session.
    pub uuid: String,
    /// Signed-hex server hash the client computed.
    pub server_hash: String,
}

/// Recording [`SessionServer`] that can be told to refuse joins.
#[derive(Debug, Default)]
pub struct FakeSessionServer {
    reject_status: Option<u16>,
    joins: Mutex<Vec<JoinRecord>>,
}

impl FakeSessionServer {
    /// Accepts every join (like a 204 answer).
    #[must_use]
    pub fn accepting() -> Self {
        Self::default()
    }

    /// Refuses every join with [`AuthError::JoinRejected`] and `status`.
    #[must_use]
    pub fn rejecting(status: u16) -> Self {
        Self {
            reject_status: Some(status),
            ..Self::default()
        }
    }

    /// Joins seen so far, including refused ones, in arrival order.
    #[must_use]
    pub fn joins(&self) -> Vec<JoinRecord> {
        self.joins
            .lock()
            .expect("joins mutex is not poisoned")
            .clone()
    }
}

impl SessionServer for FakeSessionServer {
    async fn join(&self, session: &Session, server_hash: &str) -> Result<(), AuthError> {
        self.joins
            .lock()
            .expect("joins mutex is not poisoned")
            .push(JoinRecord {
                username: session.username.clone(),
                uuid: session.uuid.clone(),
                server_hash: server_hash.to_string(),
            });
        match self.reject_status {
            Some(status) => Err(AuthError::JoinRejected { status }),
            None => Ok(()),
        }
    }
}
