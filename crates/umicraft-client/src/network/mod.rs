// SPDX-License-Identifier: MIT OR Apache-2.0

//! The network thread: the owner of the socket during play.
//!
//! The thread runs a Tokio current-thread runtime, logs in and hands the
//! connection to [`umicraft_net::play::drive`], which answers the keepalives,
//! sends the entry packets and decodes the clientbound play packets
//! (ADR-0010). Every event is forwarded to the game thread over its bounded
//! inbox. The game thread answers with [`ClientCommand`]s, which it converts
//! into wire packets before they reach this thread.
//!
//! - [`command`] — the commands and their wire packets.
//! - [`thread`] — the thread, the runtime and the connect/login/play flow.

mod command;
mod thread;

pub use command::ClientCommand;
pub use thread::spawn;

/// The number of serverbound packets that may be queued for the socket.
///
/// The game thread produces two or three packets per tick, so a small queue
/// is enough; when it is full the game thread keeps the command and retries
/// on the next tick instead of blocking the simulation.
pub const COMMAND_CAPACITY: usize = 64;

/// Server and identity settings for the network thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkConfig {
    /// Server host name or IP.
    pub host: String,
    /// Server port.
    pub port: u16,
    /// Username of an offline login; an online login uses the session's.
    pub name: String,
    /// The launcher session; `None` logs in offline (ADR-0053).
    pub session: Option<umicraft_auth::Session>,
    /// Client Settings view distance.
    pub view_distance: u8,
    /// Client Settings chat visibility: `0` all, `1` commands, `2` hidden.
    pub chat_mode: u8,
    /// Client Settings chat colours.
    pub chat_colors: bool,
    /// Client Settings skin-parts byte (`SkinOptions::parts_mask`).
    pub skin_parts: u8,
    /// The address of the `Handshake` when it is not `host`.
    pub handshake_host: Option<String>,
    /// Answer every server resource pack offer with `Declined` at once
    /// (the saved server's pack mode is Disabled, or no window can ask).
    pub decline_server_packs: bool,
    /// Look up the skin and the cape of the account at Mojang, so they show
    /// even when the server sends none (an offline server, or an offline
    /// account). Off for the scripted sessions, which must not use the network
    /// beyond their server.
    pub account_skin: bool,
}

impl NetworkConfig {
    /// The name the player is logged in as: the session's for an online login,
    /// `name` for an offline one.
    ///
    /// The tab list, the scoreboard and the teams know the player by this name,
    /// so it is what the game looks itself up with. (`name` alone is only the
    /// offline name: with a launcher session it is the default `umicraft`, which
    /// no server has in its tab list.)
    #[must_use]
    pub fn login_name(&self) -> &str {
        self.session
            .as_ref()
            .map_or(self.name.as_str(), |session| session.username.as_str())
    }
}

#[cfg(test)]
mod tests {
    use umicraft_auth::{Secret, Session};

    use super::*;

    fn config(session: Option<Session>) -> NetworkConfig {
        NetworkConfig {
            host: "localhost".to_owned(),
            port: 25565,
            name: "umicraft".to_owned(),
            session,
            view_distance: 8,
            chat_mode: 0,
            chat_colors: true,
            skin_parts: 0,
            handshake_host: None,
            decline_server_packs: false,
            account_skin: false,
        }
    }

    #[test]
    fn an_offline_login_uses_the_offline_name() {
        assert_eq!(config(None).login_name(), "umicraft");
    }

    /// Regression: the game looked itself up in the tab list as `name`, the
    /// default `umicraft`, instead of the name of the launcher session it logged
    /// in with, so the ping mod never found the player's entry.
    #[test]
    fn a_launcher_session_logs_in_with_its_own_name() {
        let session = Session::new(
            "Pixel",
            "069a79f444e94726a5befca90e38aaf5",
            Secret::new("token".to_owned()),
        )
        .expect("a valid session");
        assert_eq!(config(Some(session)).login_name(), "Pixel");
    }
}
