// SPDX-License-Identifier: MIT OR Apache-2.0

//! The vitals of the local player: what the HUD bars and the death screen show.

/// The health, food, armour and experience of the local player.
pub(in crate::game) struct Vitals {
    /// The health points.
    pub(in crate::game) health: f32,
    /// Whether a health update has come since the player was created: the first
    /// one only sets the health, a later drop hurts (MCP
    /// `EntityPlayerSP.hasValidHealth`).
    pub(in crate::game) health_known: bool,
    /// The food points.
    pub(in crate::game) food: f32,
    /// The armour points.
    pub(in crate::game) armor: f32,
    /// The absorption hearts of the local player (entry 17).
    pub(in crate::game) absorption: f32,
    /// The experience bar fill, `0..=1`.
    pub(in crate::game) experience: f32,
    /// The experience level.
    pub(in crate::game) level: i32,
    /// The score of the death screen (data watcher 18 of the local player).
    pub(in crate::game) score: i32,
    /// The ticks since the death, up to [`DEATH_TICKS`](crate::game::tick::DEATH_TICKS)
    /// (MCP `EntityLivingBase.deathTime`).
    pub(in crate::game) death_time: u32,
    /// Whether the world is hardcore (`Join Game`).
    pub(in crate::game) hardcore: bool,
}

impl Default for Vitals {
    fn default() -> Self {
        Self {
            health: 20.0,
            health_known: false,
            food: 20.0,
            armor: 0.0,
            absorption: 0.0,
            experience: 0.0,
            level: 0,
            score: 0,
            death_time: 0,
            hardcore: false,
        }
    }
}
