// SPDX-License-Identifier: MIT OR Apache-2.0

//! The particles of an explosion (MCP `NetHandlerPlayClient.handleExplosion`
//! and `Explosion.doExplosionB`).

use glam::DVec3;

use super::engine::ParticleEngine;
use super::kind::ParticleKind;
use super::spawn::SpawnFilter;

/// From this strength the flash becomes the huge explosion, MCP
/// `Explosion.java:178`.
const HUGE_SIZE: f32 = 2.0;

impl ParticleEngine {
    /// Spawns the particles of an explosion of `size` at `center` that
    /// destroyed the blocks `affected` (absolute positions, see
    /// `Explosion::affected_blocks` of the packet).
    ///
    /// A flash first (the huge emitter from size 2, else one large flash, both
    /// shown from any distance), then for every destroyed block a grey puff
    /// half-way to the centre and a smoke wisp at a random point of the
    /// block, both flying away from the centre. The client always sets
    /// `isSmoking`, so the particles are always made. The block removal, the
    /// sound and the push of the player belong to the caller.
    pub fn explode(
        &mut self,
        center: DVec3,
        size: f32,
        affected: &[(i32, i32, i32)],
        filter: &SpawnFilter,
    ) {
        let flash = if size >= HUGE_SIZE {
            ParticleKind::HugeExplosion
        } else {
            ParticleKind::LargeExplode
        };
        self.emit(flash, center, DVec3::X, filter);
        for &(x, y, z) in affected {
            self.block_debris(
                center,
                size,
                DVec3::new(x.into(), y.into(), z.into()),
                filter,
            );
        }
    }

    /// The puff and the wisp of one destroyed block (`doExplosionB` lines
    /// 193-212): the point is the block corner plus three float draws, the
    /// speed `0.5 / (distance / size + 0.1)` times `r * r + 0.3`.
    fn block_debris(&mut self, center: DVec3, size: f32, corner: DVec3, filter: &SpawnFilter) {
        let rng = self.rng();
        let mut point = |base: f64| f64::from(base as f32 + rng.next_float());
        let at = DVec3::new(point(corner.x), point(corner.y), point(corner.z));
        let away = at - center;
        let distance = away.length();
        let draw = f64::from(rng.next_float() * rng.next_float() + 0.3);
        let speed = 0.5 / (distance / f64::from(size) + 0.1) * draw;
        let velocity = away / distance * speed;
        self.emit(ParticleKind::Explode, (at + center) / 2.0, velocity, filter);
        self.emit(ParticleKind::Smoke, at, velocity, filter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::{Layer, ParticleSetting};
    use umicraft_world::World;

    fn filter(viewer: DVec3) -> SpawnFilter {
        SpawnFilter {
            viewer,
            setting: ParticleSetting::All,
        }
    }

    const CENTER: DVec3 = DVec3::new(0.5, 64.5, 0.5);

    #[test]
    fn a_small_explosion_is_one_large_flash() {
        let mut engine = ParticleEngine::new(1);
        engine.explode(CENTER, 1.0, &[], &filter(CENTER));
        assert_eq!(engine.particles(Layer::Lit, false).len(), 1);
        assert_eq!(engine.len(), 1);
    }

    #[test]
    fn a_big_explosion_spawns_the_huge_emitter() {
        let mut engine = ParticleEngine::new(1);
        engine.explode(CENTER, 4.0, &[], &filter(CENTER));
        engine.tick(&World::new(0));
        assert_eq!(engine.particles(Layer::Lit, false).len(), 6);
    }

    #[test]
    fn the_flash_and_puffs_are_seen_from_far_away_but_the_smoke_is_not() {
        let mut engine = ParticleEngine::new(1);
        let far = filter(DVec3::new(500.0, 64.0, 0.0));
        engine.explode(CENTER, 1.0, &[(1, 64, 0), (2, 64, 0)], &far);
        // The flash and the two `explode` puffs ignore the range; the two
        // smoke wisps do not.
        assert_eq!(engine.len(), 3);
    }

    #[test]
    fn every_destroyed_block_makes_a_puff_and_a_wisp_flying_outwards() {
        let mut engine = ParticleEngine::new(1);
        engine.explode(CENTER, 1.0, &[(3, 64, 0), (-3, 64, 0)], &filter(CENTER));
        assert_eq!(engine.len(), 5);
        let puffs = engine.particles(Layer::Misc, false);
        assert_eq!(puffs.len(), 4);
        for puff in puffs {
            let outwards = (puff.position - CENTER).normalize();
            assert!(puff.motion.dot(outwards) > 0.0);
        }
    }
}
