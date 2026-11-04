// SPDX-License-Identifier: MIT OR Apache-2.0

//! The multiplayer option: the address of the last direct connection
//! (`GameSettings.lastServer`).

use serde::{Deserialize, Serialize};

/// The longest address kept; the direct-connect field takes 128 characters.
const MAX_ADDRESS_CHARS: usize = 128;

/// The `[multiplayer]` section.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MultiplayerOptions {
    /// What the player typed on the direct-connect screen last time, so the
    /// field starts filled (MCP `GuiMultiplayer` keeps `lastServer`).
    pub last_server: String,
}

impl MultiplayerOptions {
    /// Shortens an address that is longer than the direct-connect field takes.
    pub fn clamp(&mut self) {
        if self.last_server.chars().count() > MAX_ADDRESS_CHARS {
            self.last_server = self.last_server.chars().take(MAX_ADDRESS_CHARS).collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_address_is_empty() {
        assert_eq!(MultiplayerOptions::default().last_server, "");
    }

    #[test]
    fn a_very_long_address_is_cut() {
        let mut options = MultiplayerOptions {
            last_server: "a".repeat(500),
        };
        options.clamp();
        assert_eq!(options.last_server.chars().count(), MAX_ADDRESS_CHARS);
    }
}
