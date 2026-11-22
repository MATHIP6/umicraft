// SPDX-License-Identifier: MIT OR Apache-2.0

//! The smoke and the flames of an active mob spawner (MCP 1.8.9
//! `MobSpawnerBaseLogic.updateSpawner`, client side).
//!
//! A spawner block and a spawner minecart run the same logic: while a player
//! is within 16 blocks of the middle of its block, it puts one smoke puff and
//! one flame at the same random point of the block every tick.

use glam::DVec3;
use umicraft_world::BlockPos;

use super::engine::ParticleEngine;
use super::kind::ParticleKind;
use super::spawn::SpawnFilter;

/// `MobSpawnerBaseLogic.activatingRangeFromPlayer`.
const ACTIVATING_RANGE: f64 = 16.0;

/// Whether a player whose feet are at `player` wakes the spawner at `block`
/// (`isActivated`: `getDistanceSq` to the middle of the block under 16²).
#[must_use]
pub fn spawner_active(block: BlockPos, player: DVec3) -> bool {
    let middle = DVec3::new(
        f64::from(block.x) + 0.5,
        f64::from(block.y) + 0.5,
        f64::from(block.z) + 0.5,
    );
    middle.distance_squared(player) < ACTIVATING_RANGE * ACTIVATING_RANGE
}

impl ParticleEngine {
    /// One tick of the particles of the active spawner at `block`.
    pub fn spawn_spawner_puff(&mut self, block: BlockPos, filter: &SpawnFilter) {
        // `(double)((float)x + rand.nextFloat())`: the sum rounds as a float.
        let rng = self.rng();
        let x = f64::from(block.x as f32 + rng.next_float());
        let y = f64::from(block.y as f32 + rng.next_float());
        let z = f64::from(block.z as f32 + rng.next_float());
        let at = DVec3::new(x, y, z);
        self.emit(ParticleKind::Smoke, at, DVec3::ZERO, filter);
        self.emit(ParticleKind::Flame, at, DVec3::ZERO, filter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::ParticleSetting;

    #[test]
    fn a_spawner_wakes_within_sixteen_blocks_of_its_middle() {
        let block = BlockPos::new(0, 64, 0);
        assert!(spawner_active(block, DVec3::new(0.5, 64.5, 15.9)));
        assert!(!spawner_active(block, DVec3::new(0.5, 64.5, 16.6)));
    }

    #[test]
    fn an_active_spawner_puffs_smoke_and_a_flame_inside_its_block() {
        let mut engine = ParticleEngine::new(3);
        let filter = SpawnFilter {
            viewer: DVec3::new(0.0, 64.0, 0.0),
            setting: ParticleSetting::All,
        };
        engine.spawn_spawner_puff(BlockPos::new(2, 64, -3), &filter);
        assert_eq!(engine.len(), 2);
    }
}
