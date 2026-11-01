// SPDX-License-Identifier: MIT OR Apache-2.0

//! The redstone dust of the lit torch, the wire, the lit ore and the powered
//! repeater (`reddust` particles).

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;
use umicraft_world::{BlockPos, BlockState};

use super::emit::{emit, emit_with, Requests};
use super::facing::{front_offset, repeater_facing, torch_facing};
use crate::particles::ParticleKind;
use crate::shapes::BlockSource;

/// How far a lit redstone ore's dust floats out of a face (`d0 = 0.0625D`).
const ORE_MARGIN: f64 = 0.0625;

/// `(rand.nextDouble() - 0.5D) * 0.2D`, the jitter of the torch dust.
fn torch_jitter(rng: &mut JavaRandom) -> f64 {
    (rng.next_double() - 0.5) * 0.2
}

/// `BlockRedstoneTorch.randomDisplayTick` of a lit torch (MCP
/// `BlockRedstoneTorch.java:185`).
pub(super) fn lit_torch(
    pos: BlockPos,
    state: BlockState,
    rng: &mut JavaRandom,
    out: &mut Requests,
) {
    let mut position = DVec3::new(
        f64::from(pos.x) + 0.5 + torch_jitter(rng),
        f64::from(pos.y) + 0.7 + torch_jitter(rng),
        f64::from(pos.z) + 0.5 + torch_jitter(rng),
    );
    let (front_x, front_z) = front_offset(torch_facing(state.metadata()));
    if (front_x, front_z) != (0, 0) {
        position.x -= 0.27 * f64::from(front_x);
        position.y += 0.22;
        position.z -= 0.27 * f64::from(front_z);
    }
    emit(out, ParticleKind::Redstone, position);
}

/// `BlockRedstoneWire.randomDisplayTick`: a puff whose colour (the velocity of
/// a `reddust`) goes from dark to bright red with the power (MCP
/// `BlockRedstoneWire.java:445`).
pub(super) fn wire(pos: BlockPos, state: BlockState, rng: &mut JavaRandom, out: &mut Requests) {
    let power = state.metadata();
    if power == 0 {
        return;
    }
    let position = DVec3::new(
        f64::from(pos.x) + 0.5 + (f64::from(rng.next_float()) - 0.5) * 0.2,
        f64::from(pos.y as f32 + 0.0625),
        f64::from(pos.z) + 0.5 + (f64::from(rng.next_float()) - 0.5) * 0.2,
    );
    let strength = f32::from(power) / 15.0;
    let colour = DVec3::new(
        f64::from(strength * 0.6 + 0.4),
        f64::from((strength * strength * 0.7 - 0.5).max(0.0)),
        f64::from((strength * strength * 0.6 - 0.7).max(0.0)),
    );
    emit_with(out, ParticleKind::Redstone, position, colour);
}

/// `BlockRedstoneRepeater.randomDisplayTick` of a powered repeater: dust on the
/// torch at the back (`-5/16`) or the one at the delay slider (MCP
/// `BlockRedstoneRepeater.java:105`).
pub(super) fn powered_repeater(
    pos: BlockPos,
    state: BlockState,
    rng: &mut JavaRandom,
    out: &mut Requests,
) {
    let metadata = state.metadata();
    let mut jittered = |centre: i32, base: f32| {
        f64::from(centre as f32 + base) + f64::from(rng.next_float() - 0.5) * 0.2
    };
    let x = jittered(pos.x, 0.5);
    let y = jittered(pos.y, 0.4);
    let z = jittered(pos.z, 0.5);
    let slots = if rng.next_int(2) == 1 {
        f32::from((metadata >> 2) + 1) * 2.0 - 1.0
    } else {
        -5.0
    };
    let reach = slots / 16.0;
    let (front_x, front_z) = front_offset(repeater_facing(metadata));
    let position = DVec3::new(
        x + f64::from(reach * front_x as f32),
        y,
        z + f64::from(reach * front_z as f32),
    );
    emit(out, ParticleKind::Redstone, position);
}

/// `BlockRedstoneOre.spawnParticles` of a lit ore: six puffs, each stuck to
/// the face that is not an opaque cube (MCP `BlockRedstoneOre.java:126`).
///
/// The reference clamps the height against `d2 < 0.0D` instead of the block's
/// own y, so the puffs under an ore never show; that is kept.
pub(super) fn lit_ore<S: BlockSource + ?Sized>(
    world: &S,
    pos: BlockPos,
    rng: &mut JavaRandom,
    out: &mut Requests,
) {
    // (axis, side) of up, down, south, north, east, west, as the reference
    // numbers them.
    const FACES: [(usize, i32); 6] = [(1, 1), (1, -1), (2, 1), (2, -1), (0, 1), (0, -1)];
    let origin = [pos.x, pos.y, pos.z];
    for (axis, side) in FACES {
        let mut cell = origin.map(|base| f64::from(base as f32 + rng.next_float()));
        let mut neighbour = origin;
        neighbour[axis] += side;
        if !world
            .block(neighbour[0], neighbour[1], neighbour[2])
            .is_opaque()
        {
            let edge = if side > 0 {
                1.0 + ORE_MARGIN
            } else {
                -ORE_MARGIN
            };
            cell[axis] = f64::from(origin[axis]) + edge;
        }
        let [x, y, z] = cell;
        let inside = (f64::from(pos.x)..=f64::from(pos.x + 1)).contains(&x)
            && (0.0..=f64::from(pos.y + 1)).contains(&y)
            && (f64::from(pos.z)..=f64::from(pos.z + 1)).contains(&z);
        if !inside {
            emit(out, ParticleKind::Redstone, DVec3::new(x, y, z));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::ambient::testing::{kinds, World};

    #[test]
    fn a_lit_standing_torch_sprinkles_dust_around_its_tip() {
        let mut out = Vec::new();
        lit_torch(
            BlockPos::new(0, 10, 0),
            BlockState::new(76, 5),
            &mut JavaRandom::new(3),
            &mut out,
        );
        assert_eq!(kinds(&out), [ParticleKind::Redstone]);
        let p = out[0].position;
        assert!(
            (0.4..0.6).contains(&p.x) && (10.6..10.8).contains(&p.y) && (0.4..0.6).contains(&p.z)
        );
    }

    #[test]
    fn a_lit_wall_torch_shifts_towards_the_wall_and_up() {
        let mut out = Vec::new();
        // Facing north (metadata 4): the wall is to the south.
        lit_torch(
            BlockPos::new(0, 10, 0),
            BlockState::new(76, 4),
            &mut JavaRandom::new(3),
            &mut out,
        );
        let p = out[0].position;
        assert!((10.82..11.02).contains(&p.y) && (0.67..0.87).contains(&(p.z)));
    }

    #[test]
    fn dead_wire_is_silent_and_live_wire_encodes_power_as_colour() {
        let mut out = Vec::new();
        wire(
            BlockPos::new(0, 4, 0),
            BlockState::new(55, 0),
            &mut JavaRandom::new(1),
            &mut out,
        );
        assert!(out.is_empty());
        wire(
            BlockPos::new(0, 4, 0),
            BlockState::new(55, 15),
            &mut JavaRandom::new(1),
            &mut out,
        );
        assert!((out[0].velocity.x - 1.0).abs() < 1e-6);
        assert!((out[0].velocity.y - 0.2).abs() < 1e-6);
        assert_eq!(out[0].velocity.z, 0.0);
        assert_eq!(out[0].position.y, f64::from(4.0625f32));
        wire(
            BlockPos::new(0, 4, 0),
            BlockState::new(55, 1),
            &mut JavaRandom::new(1),
            &mut out,
        );
        assert_eq!((out[1].velocity.y, out[1].velocity.z), (0.0, 0.0));
        assert!(out[1].velocity.x > 0.4 && out[1].velocity.x < 0.5);
    }

    #[test]
    fn a_powered_repeater_puts_dust_on_one_of_its_two_torches() {
        for seed in 0..8 {
            let mut out = Vec::new();
            // Facing north (metadata 2), delay 4 (3 << 2).
            let state = BlockState::new(94, 2 | (3 << 2));
            powered_repeater(
                BlockPos::new(0, 4, 0),
                state,
                &mut JavaRandom::new(seed),
                &mut out,
            );
            let offset = out[0].position.z - 0.5;
            // Back torch: +5/16 (north has dz = -1); slider: -7/16 (+-0.1 jitter).
            assert!(
                (offset - 0.3125).abs() < 0.11 || (offset + 0.4375).abs() < 0.11,
                "{offset}"
            );
        }
    }

    #[test]
    fn a_buried_lit_ore_shows_no_dust_and_an_exposed_one_shows_dust_above() {
        let mut world = World::default();
        let pos = BlockPos::new(0, 40, 0);
        for (dx, dy, dz) in [
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
            (1, 0, 0),
            (-1, 0, 0),
        ] {
            world.set(dx, 40 + dy, dz, BlockState::new(1, 0));
        }
        let mut out = Vec::new();
        lit_ore(&world, pos, &mut JavaRandom::new(2), &mut out);
        assert!(out.is_empty());
        world.set(0, 41, 0, BlockState::AIR);
        lit_ore(&world, pos, &mut JavaRandom::new(2), &mut out);
        assert_eq!(out.len(), 1);
        assert!((out[0].position.y - 41.0625).abs() < 1e-12);
    }
}
