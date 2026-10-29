// SPDX-License-Identifier: MIT OR Apache-2.0

//! The visual flags of an entity (metadata index 0 and the player skin parts).

/// Bit flags representing active entity visual states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EntityFlags {
    /// Raw byte flags (index 0 in 1.8 data watchers).
    pub flags: u8,
    /// Player skin parts bitmask (index 10 in 1.8 data watchers).
    pub skin_parts: u8,
}

impl EntityFlags {
    /// Creates flags from raw data watcher bytes.
    #[must_use]
    pub const fn new(flags: u8, skin_parts: u8) -> Self {
        Self { flags, skin_parts }
    }

    /// Returns true if the entity is burning.
    #[must_use]
    pub const fn on_fire(self) -> bool {
        self.flags & 0x01 != 0
    }

    /// Returns true if the entity is crouching / sneaking.
    #[must_use]
    pub const fn is_sneaking(self) -> bool {
        self.flags & 0x02 != 0
    }

    /// Returns true if the entity is sprinting.
    #[must_use]
    pub const fn is_sprinting(self) -> bool {
        self.flags & 0x08 != 0
    }

    /// Returns true while the entity eats, drinks, blocks or draws a bow
    /// (MCP `Entity.isEating`, flag 4: the use of the held item).
    #[must_use]
    pub const fn is_using_item(self) -> bool {
        self.flags & 0x10 != 0
    }

    /// Returns true if the entity is invisible.
    #[must_use]
    pub const fn is_invisible(self) -> bool {
        self.flags & 0x20 != 0
    }
}
