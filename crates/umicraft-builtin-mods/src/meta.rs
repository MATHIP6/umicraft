// SPDX-License-Identifier: MIT OR Apache-2.0
//! The static description of a mod, shown in the mods menu.

use crate::id::{ModCategory, ModId};

/// What the menu needs to list a mod. All strings are language keys, not text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModMeta {
    /// Stable identifier, also the config key.
    pub id: ModId,
    /// Language key of the display name.
    pub name_key: &'static str,
    /// Language key of the one-line description.
    pub description_key: &'static str,
    /// Where the mod is listed.
    pub category: ModCategory,
    /// Whether a player who never touched the menu has the mod on.
    pub default_enabled: bool,
}
