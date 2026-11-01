// SPDX-License-Identifier: MIT OR Apache-2.0

//! The fragments of a broken or hit block (MCP `EffectRenderer`,
//! `addBlockDestroyEffects` and `addBlockHitEffects`).

use glam::DVec3;
use umicraft_world::{BlockPos, BlockState};

use super::behaviours::digging;
use super::engine::ParticleEngine;
use crate::raycast::Face;
use crate::shapes::Aabb;

/// Fragments per axis of a broken block (`i = 4`, so 64 in all).
const GRID: u32 = 4;
/// How far a hit fragment sits from the face it comes out of (`f = 0.1F`).
const HIT_MARGIN: f64 = 0.100_000_001_490_116_12;
/// Hit fragments move at a fifth of the usual speed and are 0.6 as big.
const HIT_SPEED_FACTOR: f32 = 0.2;
/// See [`HIT_SPEED_FACTOR`].
const HIT_SCALE_FACTOR: f32 = 0.6;

impl ParticleEngine {
    /// Throws the 64 fragments of a block that just broke: one per cell of a
    /// 4 x 4 x 4 grid, each flying away from the centre (MCP
    /// `EffectRenderer.addBlockDestroyEffects`, line 361). Air breaks into
    /// nothing.
    pub fn add_block_destroy_effects(&mut self, pos: BlockPos, state: BlockState, tint: [f32; 3]) {
        if state.id() == 0 {
            return;
        }
        let origin = DVec3::new(pos.x.into(), pos.y.into(), pos.z.into());
        let cell = |index: u32| (f64::from(index) + 0.5) / f64::from(GRID);
        for x in 0..GRID {
            for y in 0..GRID {
                for z in 0..GRID {
                    let offset = DVec3::new(cell(x), cell(y), cell(z));
                    let particle = digging::fragment(
                        state,
                        tint,
                        origin + offset,
                        offset - DVec3::splat(0.5),
                        self.rng(),
                    );
                    self.add(particle);
                }
            }
        }
    }

    /// Chips one fragment off the face being dug (MCP
    /// `EffectRenderer.addBlockHitEffects`, line 387). `bounds` is the block's
    /// selection box relative to the block, as the reference reads
    /// `getBlockBounds*`.
    pub fn add_block_hit_effects(
        &mut self,
        pos: BlockPos,
        state: BlockState,
        bounds: Aabb,
        face: Face,
        tint: [f32; 3],
    ) {
        if state.id() == 0 {
            return;
        }
        let origin = DVec3::new(pos.x.into(), pos.y.into(), pos.z.into());
        let size = bounds.max - bounds.min;
        let rng = self.rng();
        let mut at = DVec3::new(
            origin.x + rng.next_double() * (size.x - HIT_MARGIN * 2.0) + HIT_MARGIN + bounds.min.x,
            origin.y + rng.next_double() * (size.y - HIT_MARGIN * 2.0) + HIT_MARGIN + bounds.min.y,
            origin.z + rng.next_double() * (size.z - HIT_MARGIN * 2.0) + HIT_MARGIN + bounds.min.z,
        );
        match face {
            Face::Down => at.y = origin.y + bounds.min.y - HIT_MARGIN,
            Face::Up => at.y = origin.y + bounds.max.y + HIT_MARGIN,
            Face::North => at.z = origin.z + bounds.min.z - HIT_MARGIN,
            Face::South => at.z = origin.z + bounds.max.z + HIT_MARGIN,
            Face::West => at.x = origin.x + bounds.min.x - HIT_MARGIN,
            Face::East => at.x = origin.x + bounds.max.x + HIT_MARGIN,
        }
        let particle = digging::fragment(state, tint, at, DVec3::ZERO, self.rng())
            .multiply_velocity(HIT_SPEED_FACTOR)
            .scale_by(HIT_SCALE_FACTOR);
        self.add(particle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::Layer;

    const STONE: BlockState = BlockState::new(1, 0);

    #[test]
    fn a_broken_block_throws_sixty_four_fragments() {
        let mut engine = ParticleEngine::new(1);
        engine.add_block_destroy_effects(BlockPos::new(3, 64, 3), STONE, [1.0; 3]);
        assert_eq!(engine.len(), 64);
        assert_eq!(engine.particles(Layer::Terrain, false).len(), 64);
    }

    #[test]
    fn air_breaks_into_nothing() {
        let mut engine = ParticleEngine::new(1);
        engine.add_block_destroy_effects(BlockPos::new(0, 0, 0), BlockState::AIR, [1.0; 3]);
        assert!(engine.is_empty());
    }

    #[test]
    fn fragments_start_inside_the_block() {
        let mut engine = ParticleEngine::new(1);
        engine.add_block_destroy_effects(BlockPos::new(3, 64, 3), STONE, [1.0; 3]);
        for piece in engine.particles(Layer::Terrain, false) {
            let inside = |value: f64, base: f64| (base..=base + 1.0).contains(&value);
            assert!(inside(piece.position.x, 3.0));
            assert!(inside(piece.position.y, 64.0));
            assert!(inside(piece.position.z, 3.0));
        }
    }

    #[test]
    fn a_hit_chips_one_small_fragment_off_the_face() {
        let mut engine = ParticleEngine::new(1);
        let cube = Aabb::new(DVec3::ZERO, DVec3::ONE);
        engine.add_block_hit_effects(BlockPos::new(0, 10, 0), STONE, cube, Face::Up, [1.0; 3]);
        let chips = engine.particles(Layer::Terrain, false);
        assert_eq!(chips.len(), 1);
        assert!((chips[0].position.y - (11.0 + HIT_MARGIN)).abs() < 1e-9);
        assert!(chips[0].size < 0.2);
    }
}
