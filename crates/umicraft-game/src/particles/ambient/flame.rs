// SPDX-License-Identifier: MIT OR Apache-2.0

//! Blocks that burn quietly: the torch and the lit furnace (smoke and a flame
//! together).

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;
use umicraft_world::{BlockPos, BlockState};

use super::emit::{emit, Requests};
use super::facing::{front_offset, furnace_facing, torch_facing};
use crate::particles::ParticleKind;

/// How far a wall torch's flame sits from the block centre (`d4 = 0.27D`).
const TORCH_WALL_OFFSET: f64 = 0.27;
/// How far above the torch base the flame is (`pos.getY() + 0.7D`).
const TORCH_HEIGHT: f64 = 0.7;
/// The extra rise of a wall torch's flame (`d3 = 0.22D`).
const TORCH_WALL_RISE: f64 = 0.22;
/// How far a furnace's fire sits out of its front face (`d3 = 0.52D`).
const FURNACE_FRONT: f64 = 0.52;

/// The smoke and the flame at one spot, in the order the reference spawns them.
pub(super) fn smoke_and_flame(out: &mut Requests, position: DVec3) {
    emit(out, ParticleKind::Smoke, position);
    emit(out, ParticleKind::Flame, position);
}

/// `BlockTorch.randomDisplayTick`: the flame sits on the tip, leaning towards
/// the wall for a wall torch (MCP `BlockTorch.java:214`).
pub(super) fn torch(pos: BlockPos, state: BlockState, out: &mut Requests) {
    let (front_x, front_z) = front_offset(torch_facing(state.metadata()));
    let mut position = DVec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + TORCH_HEIGHT,
        f64::from(pos.z) + 0.5,
    );
    // The leaning is towards the opposite of the facing: the wall it hangs on.
    position.x -= TORCH_WALL_OFFSET * f64::from(front_x);
    position.z -= TORCH_WALL_OFFSET * f64::from(front_z);
    if (front_x, front_z) != (0, 0) {
        position.y += TORCH_WALL_RISE;
    }
    smoke_and_flame(out, position);
}

/// `BlockFurnace.randomDisplayTick` of a burning furnace: fire at the mouth,
/// anywhere along it and up to 6/16 high (MCP `BlockFurnace.java:82`).
pub(super) fn burning_furnace(
    pos: BlockPos,
    state: BlockState,
    rng: &mut JavaRandom,
    out: &mut Requests,
) {
    let (front_x, front_z) = front_offset(furnace_facing(state.metadata()));
    let height = f64::from(pos.y) + rng.next_double() * 6.0 / 16.0;
    let slide = rng.next_double() * 0.6 - 0.3;
    let across = |front: i32, centre: i32| {
        f64::from(centre)
            + 0.5
            + FURNACE_FRONT * f64::from(front)
            + if front == 0 { slide } else { 0.0 }
    };
    let position = DVec3::new(across(front_x, pos.x), height, across(front_z, pos.z));
    smoke_and_flame(out, position);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::ambient::testing::kinds;

    #[test]
    fn a_standing_torch_burns_on_its_tip() {
        let mut out = Vec::new();
        torch(BlockPos::new(3, 64, -2), BlockState::new(50, 5), &mut out);
        assert_eq!(kinds(&out), [ParticleKind::Smoke, ParticleKind::Flame]);
        assert_eq!(out[0].position, DVec3::new(3.5, 64.7, -1.5));
        assert_eq!(out[0].position, out[1].position);
    }

    #[test]
    fn a_wall_torch_leans_towards_its_wall() {
        let mut out = Vec::new();
        // Metadata 1 faces east, so it hangs on the block to the west.
        torch(BlockPos::new(0, 10, 0), BlockState::new(50, 1), &mut out);
        let position = out[0].position;
        assert!((position.x - (0.5 - 0.27)).abs() < 1e-12);
        assert!((position.y - 10.92).abs() < 1e-12);
        assert_eq!(position.z, 0.5);
    }

    #[test]
    fn a_burning_furnace_fires_out_of_its_front() {
        let mut rng = JavaRandom::new(7);
        let mut out = Vec::new();
        // Metadata 4 faces west.
        burning_furnace(
            BlockPos::new(0, 5, 0),
            BlockState::new(62, 4),
            &mut rng,
            &mut out,
        );
        assert_eq!(kinds(&out), [ParticleKind::Smoke, ParticleKind::Flame]);
        let position = out[1].position;
        assert!((position.x - (0.5 - 0.52)).abs() < 1e-12);
        assert!((5.0..5.375).contains(&position.y));
        assert!((0.2..0.8).contains(&position.z));
    }

    #[test]
    fn a_furnace_facing_south_slides_along_x() {
        let mut rng = JavaRandom::new(1);
        let mut out = Vec::new();
        burning_furnace(
            BlockPos::new(0, 0, 0),
            BlockState::new(62, 3),
            &mut rng,
            &mut out,
        );
        assert!((out[0].position.z - 1.02).abs() < 1e-12);
        assert!((0.2..0.8).contains(&out[0].position.x));
    }
}
