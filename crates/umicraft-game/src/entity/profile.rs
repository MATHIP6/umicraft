// SPDX-License-Identifier: MIT OR Apache-2.0

//! The game profile of a remote player, as far as drawing it needs.

use std::sync::Arc;

/// Who a player is and what they wear.
///
/// Built from the tab list entry (`Player List Item`): the name for the name
/// tag and the texture URLs of the profile's `textures` property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerProfile {
    /// The profile name.
    pub name: Arc<str>,
    /// URL of the skin PNG, when the profile has one.
    pub skin_url: Option<Arc<str>>,
    /// URL of the cape PNG, when the profile has one.
    pub cape_url: Option<Arc<str>>,
    /// Slim (3 pixel) arms, when the profile says so; `None` means the
    /// default skin of the UUID decides.
    pub slim: Option<bool>,
}

impl PlayerProfile {
    /// A profile with a name only (no textures).
    #[must_use]
    pub fn named(name: &str) -> Self {
        Self {
            name: Arc::from(name),
            skin_url: None,
            cape_url: None,
            slim: None,
        }
    }
}
