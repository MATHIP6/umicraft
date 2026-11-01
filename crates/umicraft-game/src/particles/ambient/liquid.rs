// SPDX-License-Identifier: MIT OR Apache-2.0

//! `BlockLiquid.randomDisplayTick`: plankton in still water, popping lava, and
//! the drips under a liquid that hangs over a gap.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;
use umicraft_world::fluid::Liquid;
use umicraft_world::{BlockPos, BlockState};

use super::emit::{double_add, emit, Requests};
use super::surface::{has_solid_top, is_open_air};
use crate::particles::ParticleKind;
use crate::shapes::BlockSource;

/// How far under the liquid block a drip appears (`d1 - 1.05D`).
const DRIP_DEPTH: f64 = 1.05;

/// The display tick of a water or lava block (MCP `BlockLiquid.java:226`).
///
/// The sounds (`liquid.water`, `liquid.lavapop`, `liquid.lava`) are not played,
/// and their random draws are not made.
pub(super) fn tick<S: BlockSource + ?Sized>(
    world: &S,
    pos: BlockPos,
    state: BlockState,
    liquid: Liquid,
    rng: &mut JavaRandom,
    out: &mut Requests,
) {
    match liquid {
        Liquid::Water => plankton(pos, state, rng, out),
        Liquid::Lava => pop(world, pos, rng, out),
    }
    drip(world, pos, liquid, rng, out);
}

/// Specks in still water: the level 0 source and the falling levels, one tick
/// in ten. The flowing levels 1 to 7 only make a sound.
fn plankton(pos: BlockPos, state: BlockState, rng: &mut JavaRandom, out: &mut Requests) {
    if (1..8).contains(&state.metadata()) || rng.next_int(10) != 0 {
        return;
    }
    let position = DVec3::new(
        double_add(pos.x, rng),
        double_add(pos.y, rng),
        double_add(pos.z, rng),
    );
    emit(out, ParticleKind::Suspended, position);
}

/// A lava spark off the surface, one tick in a hundred, when nothing covers it.
fn pop<S: BlockSource + ?Sized>(
    world: &S,
    pos: BlockPos,
    rng: &mut JavaRandom,
    out: &mut Requests,
) {
    if !world.block(pos.x, pos.y + 1, pos.z).is_air() || rng.next_int(100) != 0 {
        return;
    }
    // A liquid block's bounds are the unit cube, so `maxY` is 1.
    let position = DVec3::new(
        double_add(pos.x, rng),
        f64::from(pos.y) + 1.0,
        double_add(pos.z, rng),
    );
    emit(out, ParticleKind::Lava, position);
}

/// A drop under the block, one tick in ten, when it rests on a solid top and
/// leaves open air two blocks below.
fn drip<S: BlockSource + ?Sized>(
    world: &S,
    pos: BlockPos,
    liquid: Liquid,
    rng: &mut JavaRandom,
    out: &mut Requests,
) {
    if rng.next_int(10) != 0
        || !has_solid_top(world.block(pos.x, pos.y - 1, pos.z))
        || !is_open_air(world.block(pos.x, pos.y - 2, pos.z))
    {
        return;
    }
    let position = DVec3::new(
        double_add(pos.x, rng),
        f64::from(pos.y) - DRIP_DEPTH,
        double_add(pos.z, rng),
    );
    let kind = match liquid {
        Liquid::Water => ParticleKind::DripWater,
        Liquid::Lava => ParticleKind::DripLava,
    };
    emit(out, kind, position);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::ambient::testing::{count_kind, World};
    use crate::particles::SpawnRequest;

    const WATER_SOURCE: BlockState = BlockState::new(9, 0);

    fn run(world: &World, state: BlockState, liquid: Liquid, ticks: u64) -> Vec<ParticleKind> {
        let mut rng = JavaRandom::new(11);
        let mut out = Vec::new();
        for _ in 0..ticks {
            super::tick(
                world,
                BlockPos::new(0, 64, 0),
                state,
                liquid,
                &mut rng,
                &mut out,
            );
        }
        out.iter().map(|r| r.kind).collect()
    }

    #[test]
    fn still_water_sheds_plankton_one_tick_in_ten() {
        let kinds = run(&World::default(), WATER_SOURCE, Liquid::Water, 2000);
        let suspended = count_kind(&kinds, ParticleKind::Suspended);
        assert!((150..250).contains(&suspended), "{suspended}");
    }

    #[test]
    fn flowing_water_levels_one_to_seven_make_no_plankton() {
        let kinds = run(&World::default(), BlockState::new(8, 3), Liquid::Water, 500);
        assert!(kinds.is_empty());
    }

    #[test]
    fn open_lava_pops_one_tick_in_a_hundred_and_covered_lava_does_not() {
        let kinds = run(
            &World::default(),
            BlockState::new(11, 0),
            Liquid::Lava,
            20_000,
        );
        let pops = count_kind(&kinds, ParticleKind::Lava);
        assert!((140..270).contains(&pops), "{pops}");
        let mut covered = World::default();
        covered.set(0, 65, 0, BlockState::new(1, 0));
        assert!(run(&covered, BlockState::new(11, 0), Liquid::Lava, 5000).is_empty());
    }

    #[test]
    fn a_liquid_over_a_ceiling_drips_into_the_gap() {
        let mut world = World::default();
        world.set(0, 63, 0, BlockState::new(1, 0));
        let kinds = run(&world, WATER_SOURCE, Liquid::Water, 1000);
        assert!(count_kind(&kinds, ParticleKind::DripWater) > 50);
        let lava = run(&world, BlockState::new(10, 0), Liquid::Lava, 1000);
        assert!(count_kind(&lava, ParticleKind::DripLava) > 50);
        world.set(0, 62, 0, BlockState::new(1, 0));
        let blocked = run(&world, WATER_SOURCE, Liquid::Water, 1000);
        assert_eq!(count_kind(&blocked, ParticleKind::DripWater), 0);
    }

    #[test]
    fn a_drip_hangs_just_under_the_ceiling() {
        let mut world = World::default();
        world.set(0, 63, 0, BlockState::new(1, 0));
        let mut rng = JavaRandom::new(4);
        let mut out: Vec<SpawnRequest<'static>> = Vec::new();
        while out.iter().all(|r| r.kind != ParticleKind::DripWater) {
            drip(
                &world,
                BlockPos::new(0, 64, 0),
                Liquid::Water,
                &mut rng,
                &mut out,
            );
        }
        assert!((out[0].position.y - 62.95).abs() < 1e-12);
    }
}
