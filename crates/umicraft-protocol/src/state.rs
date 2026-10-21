// SPDX-License-Identifier: MIT OR Apache-2.0

//! Protocol connection states and their transition points.
//!
//! A connection walks `Handshake` → `Status` or `Login` → `Play`; the state
//! selects which packet ids are legal. `Handshake` packets themselves are
//! readable while still in `Handshake` (dispatch is what changes state).

/// Wire identity of the 1.8.x protocol line.
pub const PROTOCOL_VERSION_1_8: i32 = 47;

/// Selection of server-side protocol states for one connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Outgoing connection intent (`Handshake`); also used for the initial read.
    Handshake,
    /// Server list ping round-trip (`Status Request/Response`, `Ping/Pong`).
    Status,
    /// Authentication (`Login Start/Success`, `Encryption`, `Set Compression`).
    Login,
    /// In-game packets after `Login Success`.
    Play,
}

impl State {
    /// The next state a `Handshake` packet asks for, if valid.
    #[must_use]
    pub fn from_handshake_next_state(next: i32) -> Option<Self> {
        match next {
            1 => Some(State::Status),
            2 => Some(State::Login),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_state_mapping_matches_protocol() {
        assert_eq!(State::from_handshake_next_state(1), Some(State::Status));
        assert_eq!(State::from_handshake_next_state(2), Some(State::Login));
        assert_eq!(State::from_handshake_next_state(3), None);
        assert_eq!(State::from_handshake_next_state(0), None);
    }
}
