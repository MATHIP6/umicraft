// SPDX-License-Identifier: MIT OR Apache-2.0

//! `BlockFire.randomDisplayTick`: smoke rising from the flammable side of a
//! fire, or from the whole block when it floats on nothing.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;
use umicraft_world::BlockPos;

use super::emit::{emit, Requests};
use super::surface::{can_catch_fire, has_solid_top};
use crate::particles::ParticleKind;
use crate::shapes::BlockSource;

/// The depth of the smoke slab along a burning side (`0.10000000149011612D`,
/// the float `0.1F` widened).
const SIDE_DEPTH: f64 = 0.100_000_001_490_116_12;

/// The five neighbours checked, as `(dx, dy, dz)`, in the order of the
/// reference: west, east, north, south, up.
const SIDES: [(i32, i32, i32); 5] = [(-1, 0, 0), (1, 0, 0), (0, 0, -1), (0, 0, 1), (0, 1, 0)];

/// Fire on a block it can stand on or burn: `BlockFire.randomDisplayTick`
/// (MCP `BlockFire.java:397`). The crackle sound (`nextInt(24)`) is not played.
pub(super) fn fire<S: BlockSource + ?Sized>(
    world: &S,
    pos: BlockPos,
    rng: &mut JavaRandom,
    out: &mut Requests,
) {
    let below = world.block(pos.x, pos.y - 1, pos.z);
    if has_solid_top(below) || can_catch_fire(below) {
        for _ in 0..3 {
            let position = DVec3::new(
                f64::from(pos.x) + rng.next_double(),
                f64::from(pos.y) + rng.next_double() * 0.5 + 0.5,
                f64::from(pos.z) + rng.next_double(),
            );
            emit(out, ParticleKind::LargeSmoke, position);
        }
        return;
    }
    for side in SIDES {
        let (dx, dy, dz) = side;
        if can_catch_fire(world.block(pos.x + dx, pos.y + dy, pos.z + dz)) {
            for _ in 0..2 {
                let position = side_position(pos, side, rng);
                emit(out, ParticleKind::LargeSmoke, position);
            }
        }
    }
}

/// A random point of the slab `SIDE_DEPTH` deep along `side` of the cell.
fn side_position(pos: BlockPos, side: (i32, i32, i32), rng: &mut JavaRandom) -> DVec3 {
    let (dx, dy, dz) = side;
    let axis = |centre: i32, step: i32, rng: &mut JavaRandom| {
        let value = rng.next_double();
        match step {
            -1 => f64::from(centre) + value * SIDE_DEPTH,
            1 => f64::from(centre + 1) - value * SIDE_DEPTH,
            _ => f64::from(centre) + value,
        }
    };
    DVec3::new(
        axis(pos.x, dx, rng),
        axis(pos.y, dy, rng),
        axis(pos.z, dz, rng),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::ambient::testing::{kinds, World};
    use umicraft_world::BlockState;

    #[test]
    fn a_fire_on_the_ground_makes_three_smokes_in_its_upper_half() {
        let mut world = World::default();
        world.set(0, 63, 0, BlockState::new(1, 0));
        let mut out = Vec::new();
        fire(
            &world,
            BlockPos::new(0, 64, 0),
            &mut JavaRandom::new(5),
            &mut out,
        );
        assert_eq!(kinds(&out), [ParticleKind::LargeSmoke; 3]);
        for request in &out {
            assert!((64.5..65.0).contains(&request.position.y));
            assert!((0.0..1.0).contains(&request.position.x));
        }
    }

    #[test]
    fn a_floating_fire_smokes_from_the_burning_sides_only() {
        let mut world = World::default();
        // Planks to the west and above.
        world.set(-1, 64, 0, BlockState::new(5, 0));
        world.set(0, 65, 0, BlockState::new(5, 0));
        let mut out = Vec::new();
        fire(
            &world,
            BlockPos::new(0, 64, 0),
            &mut JavaRandom::new(5),
            &mut out,
        );
        assert_eq!(out.len(), 4);
        assert!(out[..2].iter().all(|r| r.position.x < 0.1 + 1e-6));
        assert!(out[2..].iter().all(|r| r.position.y >= 64.9));
    }

    #[test]
    fn a_floating_fire_with_no_fuel_makes_nothing() {
        let mut out = Vec::new();
        fire(
            &World::default(),
            BlockPos::new(0, 64, 0),
            &mut JavaRandom::new(5),
            &mut out,
        );
        assert!(out.is_empty());
    }
}
