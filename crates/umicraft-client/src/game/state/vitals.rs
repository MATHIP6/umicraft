// SPDX-License-Identifier: MIT OR Apache-2.0

//! The vitals of the local player: what the HUD bars and the death screen show.

/// The ticks the death animation runs before it stops
/// (MCP `EntityLivingBase.onDeathUpdate`: `deathTime == 20`).
pub(in crate::game) const DEATH_TICKS: u32 = 20;

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
    /// The ticks since the death, up to [`DEATH_TICKS`]
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

impl Vitals {
    /// Whether the local player is dead (MCP `EntityLivingBase.getHealth() <= 0`).
    pub(in crate::game) fn dead(&self) -> bool {
        self.health <= 0.0
    }

    /// Advances the death animation by one tick and tells whether the dead
    /// player entity was already removed before this tick.
    ///
    /// Phase E.9: `EntityLivingBase.onDeathUpdate` (bytecode `pr.aZ`) counts the
    /// dead ticks and, at 20, calls `setDead()`: the dead player entity then
    /// stops updating, so the client sends no movement packet any more
    /// (measured on the vanilla 1.8.9 client: exactly 20 movement packets after
    /// the death, then silence). A living player counts nothing.
    pub(in crate::game) fn tick_death(&mut self) -> bool {
        let removed = self.dead() && self.death_time >= DEATH_TICKS;
        self.death_time = if self.dead() {
            (self.death_time + 1).min(DEATH_TICKS)
        } else {
            0
        };
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dead_player_entity_is_removed_after_twenty_ticks() {
        let mut vitals = Vitals {
            health: 0.0,
            ..Vitals::default()
        };
        let removed: Vec<bool> = (0..=DEATH_TICKS).map(|_| vitals.tick_death()).collect();
        assert!(removed[..DEATH_TICKS as usize].iter().all(|&gone| !gone));
        assert!(removed[DEATH_TICKS as usize]);
        assert_eq!(vitals.death_time, DEATH_TICKS);
    }

    #[test]
    fn a_living_player_counts_no_death_time() {
        let mut vitals = Vitals {
            death_time: 7,
            ..Vitals::default()
        };
        assert!(!vitals.tick_death());
        assert_eq!(vitals.death_time, 0);
    }
}
