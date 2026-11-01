// SPDX-License-Identifier: MIT OR Apache-2.0

//! The particles of the three portal-like blocks: the Nether portal, the end
//! portal and the ender chest.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;
use umicraft_world::BlockPos;

use super::emit::{emit, emit_with, float_add, random_sign, Requests};
use crate::particles::ParticleKind;
use crate::shapes::BlockSource;

/// The block id of the Nether portal.
const NETHER_PORTAL: u16 = 90;

/// `(double)rand.nextFloat() - 0.5D` scaled, the sideways drift of a portal
/// particle.
fn drift(rng: &mut JavaRandom, scale: f64) -> f64 {
    (f64::from(rng.next_float()) - 0.5) * scale
}

/// `BlockPortal.randomDisplayTick`: four purple particles drifting through the
/// frame, along the axis the portal faces (MCP `BlockPortal.java:205`). The
/// ambient sound (`nextInt(100)`) is not played.
pub(super) fn nether_portal<S: BlockSource + ?Sized>(
    world: &S,
    pos: BlockPos,
    rng: &mut JavaRandom,
    out: &mut Requests,
) {
    let along_x = world.block(pos.x - 1, pos.y, pos.z).id() == NETHER_PORTAL
        || world.block(pos.x + 1, pos.y, pos.z).id() == NETHER_PORTAL;
    for _ in 0..4 {
        let mut position = DVec3::new(
            float_add(pos.x, rng),
            float_add(pos.y, rng),
            float_add(pos.z, rng),
        );
        let mut velocity = DVec3::new(drift(rng, 0.5), drift(rng, 0.5), drift(rng, 0.5));
        let side = random_sign(rng);
        let rushing = f64::from(rng.next_float() * 2.0 * side as f32);
        if along_x {
            position.z = f64::from(pos.z) + 0.5 + 0.25 * f64::from(side);
            velocity.z = rushing;
        } else {
            position.x = f64::from(pos.x) + 0.5 + 0.25 * f64::from(side);
            velocity.x = rushing;
        }
        emit_with(out, ParticleKind::Portal, position, velocity);
    }
}

/// `BlockEndPortal.randomDisplayTick`: a puff of smoke 0.8 up the block (MCP
/// `BlockEndPortal.java:85`).
pub(super) fn end_portal(pos: BlockPos, rng: &mut JavaRandom, out: &mut Requests) {
    let position = DVec3::new(
        float_add(pos.x, rng),
        f64::from(pos.y as f32 + 0.8),
        float_add(pos.z, rng),
    );
    emit(out, ParticleKind::Smoke, position);
}

/// `BlockEnderChest.randomDisplayTick`: three portal particles around the
/// chest, each thrown away from its corner (MCP `BlockEnderChest.java:132`).
pub(super) fn ender_chest(pos: BlockPos, rng: &mut JavaRandom, out: &mut Requests) {
    for _ in 0..3 {
        let (j, k) = (random_sign(rng), random_sign(rng));
        let position = DVec3::new(
            f64::from(pos.x) + 0.5 + 0.25 * f64::from(j),
            f64::from(pos.y as f32 + rng.next_float()),
            f64::from(pos.z) + 0.5 + 0.25 * f64::from(k),
        );
        let velocity = DVec3::new(
            f64::from(rng.next_float() * j as f32),
            drift(rng, 0.125),
            f64::from(rng.next_float() * k as f32),
        );
        emit_with(out, ParticleKind::Portal, position, velocity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::ambient::testing::{kinds, World};
    use umicraft_world::BlockState;

    #[test]
    fn a_nether_portal_throws_four_particles_across_its_plane() {
        // Another portal block to the east: the plane is along X, so the
        // particles are pinned near the middle of Z and rush along it.
        let mut world = World::default();
        world.set(1, 64, 0, BlockState::new(NETHER_PORTAL, 0));
        let mut out = Vec::new();
        nether_portal(
            &world,
            BlockPos::new(0, 64, 0),
            &mut JavaRandom::new(9),
            &mut out,
        );
        assert_eq!(kinds(&out), [ParticleKind::Portal; 4]);
        for request in &out {
            let offset = request.position.z - 0.5;
            assert!((offset.abs() - 0.25).abs() < 1e-12);
            assert!(request.velocity.z.abs() <= 2.0);
            assert!(request.velocity.x.abs() <= 0.25 && request.velocity.y.abs() <= 0.25);
            assert_eq!(request.velocity.z.signum(), offset.signum());
        }
    }

    #[test]
    fn an_isolated_portal_block_is_pinned_on_x() {
        let mut out = Vec::new();
        nether_portal(
            &World::default(),
            BlockPos::new(0, 64, 0),
            &mut JavaRandom::new(9),
            &mut out,
        );
        assert!(out
            .iter()
            .all(|r| ((r.position.x - 0.5).abs() - 0.25).abs() < 1e-12));
    }

    #[test]
    fn the_end_portal_smokes_at_four_fifths_of_a_block() {
        let mut out = Vec::new();
        end_portal(BlockPos::new(2, 30, 2), &mut JavaRandom::new(1), &mut out);
        assert_eq!(kinds(&out), [ParticleKind::Smoke]);
        assert_eq!(out[0].position.y, f64::from(30.8f32));
        assert!((2.0..3.0).contains(&out[0].position.x));
    }

    #[test]
    fn the_ender_chest_makes_three_portal_particles_at_its_corners() {
        let mut out = Vec::new();
        ender_chest(BlockPos::new(0, 5, 0), &mut JavaRandom::new(1), &mut out);
        assert_eq!(kinds(&out), [ParticleKind::Portal; 3]);
        for request in &out {
            assert!(((request.position.x - 0.5).abs() - 0.25).abs() < 1e-12);
            assert!((5.0..6.0).contains(&request.position.y));
            assert!(request.velocity.y.abs() <= 0.0625);
            assert_eq!(
                request.velocity.x.signum(),
                (request.position.x - 0.5).signum()
            );
        }
    }
}
