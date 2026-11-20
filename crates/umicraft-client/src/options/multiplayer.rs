// SPDX-License-Identifier: MIT OR Apache-2.0

//! The multiplayer options: the address of the last direct connection
//! (`GameSettings.lastServer`), the client brand announced to servers and the
//! server-compatible mode of the external mods.

use serde::{Deserialize, Serialize};

/// The longest address kept; the direct-connect field takes 128 characters.
const MAX_ADDRESS_CHARS: usize = 128;

/// The longest brand kept. The vanilla brand is 7 characters; the cap only
/// keeps a hand-edited file from sending an absurd plugin message.
const MAX_BRAND_CHARS: usize = 32;

/// The `[multiplayer]` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MultiplayerOptions {
    /// What the player typed on the direct-connect screen last time, so the
    /// field starts filled (MCP `GuiMultiplayer` keeps `lastServer`).
    pub last_server: String,
    /// `GameSettings.snooperEnabled`. Stored only: the client never collects
    /// or sends usage data, whatever this says. Off by default.
    pub snooper: bool,
    /// The client brand sent on the `MC|Brand` channel when joining a
    /// server (spec §10: realistic and configurable). Servers and
    /// anti-cheats log it; set it to `vanilla` to announce what the vanilla
    /// client does. Empty or blank falls back to the default.
    pub brand: String,
    /// Server-compatible mode: the external mods that declare the
    /// `network_packets` or `movement` permission stay off, so the client
    /// behaves like an unmodified one on a server with an anti-cheat
    /// (ADR-0086). Off by default.
    pub mod_server_compat: bool,
}

impl Default for MultiplayerOptions {
    fn default() -> Self {
        Self {
            last_server: String::new(),
            snooper: false,
            brand: umicraft_net::play::DEFAULT_BRAND.to_owned(),
            mod_server_compat: false,
        }
    }
}

impl MultiplayerOptions {
    /// Shortens an address that is longer than the direct-connect field takes.
    pub fn clamp(&mut self) {
        if self.last_server.chars().count() > MAX_ADDRESS_CHARS {
            self.last_server = self.last_server.chars().take(MAX_ADDRESS_CHARS).collect();
        }
        let brand: String = self
            .brand
            .trim()
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_BRAND_CHARS)
            .collect();
        self.brand = if brand.is_empty() {
            umicraft_net::play::DEFAULT_BRAND.to_owned()
        } else {
            brand
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_snooper_is_off_by_default() {
        assert!(!MultiplayerOptions::default().snooper);
    }

    #[test]
    fn the_server_compatible_mode_is_off_by_default_and_kept() {
        assert!(!MultiplayerOptions::default().mod_server_compat);
        let options: MultiplayerOptions =
            toml::from_str("mod_server_compat = true").expect("parses");
        assert!(options.mod_server_compat);
    }

    #[test]
    fn the_default_address_is_empty() {
        assert_eq!(MultiplayerOptions::default().last_server, "");
    }

    #[test]
    fn a_very_long_address_is_cut() {
        let mut options = MultiplayerOptions {
            last_server: "a".repeat(500),
            ..MultiplayerOptions::default()
        };
        options.clamp();
        assert_eq!(options.last_server.chars().count(), MAX_ADDRESS_CHARS);
    }

    #[test]
    fn the_default_brand_is_umicraft() {
        assert_eq!(MultiplayerOptions::default().brand, "umicraft");
    }

    #[test]
    fn a_chosen_brand_is_kept_trimmed() {
        let mut options = MultiplayerOptions {
            brand: "  vanilla \n".to_owned(),
            ..MultiplayerOptions::default()
        };
        options.clamp();
        assert_eq!(options.brand, "vanilla");
    }

    #[test]
    fn a_blank_brand_falls_back_to_the_default() {
        let mut options = MultiplayerOptions {
            brand: " \t ".to_owned(),
            ..MultiplayerOptions::default()
        };
        options.clamp();
        assert_eq!(options.brand, "umicraft");
    }

    #[test]
    fn a_very_long_brand_is_cut() {
        let mut options = MultiplayerOptions {
            brand: "b".repeat(200),
            ..MultiplayerOptions::default()
        };
        options.clamp();
        assert_eq!(options.brand.chars().count(), MAX_BRAND_CHARS);
    }

    #[test]
    fn a_file_without_a_brand_keeps_the_default() {
        let options: MultiplayerOptions = toml::from_str("last_server = \"a\"").expect("parses");
        assert_eq!(options.brand, "umicraft");
    }
}
