// SPDX-License-Identifier: MIT OR Apache-2.0

//! `WorldClient.doVoidFogParticles`: the random sampling that wakes the display
//! tick of the blocks around the player.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;
use umicraft_world::BlockPos;

use super::display_tick;
use super::emit::{emit, Requests};
use super::weather::Weather;
use crate::particles::ParticleKind;
use crate::shapes::BlockSource;

/// How many blocks are sampled per call (`for j < 1000`).
pub const SAMPLES: u32 = 1000;
/// The radius of the sampled cube (`i = 16`).
const RADIUS: u32 = 16;
/// `nextInt(16) - nextInt(16)`: a triangular offset, mostly near 0.
fn offset(rng: &mut JavaRandom) -> i32 {
    rng.next_int(RADIUS) as i32 - rng.next_int(RADIUS) as i32
}

/// The block id of the barrier.
const BARRIER: u16 = 166;

/// Samples [`SAMPLES`] blocks around `center`, each offset by
/// `nextInt(16) - nextInt(16)` on every axis, and runs the display tick of the
/// block drawn (MCP `WorldClient.doVoidFogParticles`, line 294). With
/// `show_barriers` (creative mode holding a barrier, `flag` in the reference) a
/// barrier particle also marks each barrier drawn.
///
/// Call it once per client tick with the player's block position.
pub fn sample_around<S, W>(
    world: &S,
    weather: &W,
    center: BlockPos,
    show_barriers: bool,
    rng: &mut JavaRandom,
    out: &mut Requests,
) where
    S: BlockSource + ?Sized,
    W: Weather + ?Sized,
{
    for _ in 0..SAMPLES {
        let pos = BlockPos::new(
            center.x + offset(rng),
            center.y + offset(rng),
            center.z + offset(rng),
        );
        display_tick(world, weather, pos, rng, out);
        if show_barriers && world.block(pos.x, pos.y, pos.z).id() == BARRIER {
            let middle = |coordinate: i32| f64::from(coordinate as f32 + 0.5);
            emit(
                out,
                ParticleKind::Barrier,
                DVec3::new(middle(pos.x), middle(pos.y), middle(pos.z)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::ambient::testing::{count_kind, World};
    use crate::particles::ambient::weather::Dry;
    use umicraft_world::BlockState;

    fn kinds_of(out: &Requests) -> Vec<ParticleKind> {
        out.iter().map(|r| r.kind).collect()
    }

    #[test]
    fn an_empty_world_gives_no_particles() {
        let mut out = Vec::new();
        let center = BlockPos::new(0, 64, 0);
        sample_around(
            &World::default(),
            &Dry,
            center,
            true,
            &mut JavaRandom::new(1),
            &mut out,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn a_torch_at_the_center_is_hit_as_often_as_the_offsets_predict() {
        let mut world = World::default();
        world.set(0, 64, 0, BlockState::new(50, 5));
        let mut rng = JavaRandom::new(3);
        let mut out = Vec::new();
        let center = BlockPos::new(0, 64, 0);
        for _ in 0..300 {
            sample_around(&world, &Dry, center, false, &mut rng, &mut out);
        }
        // Each axis draws offset 0 with probability 1/16, so the cell is hit
        // 300 x 1000 x (1/16)^3 = 73 times.
        let flames = count_kind(&kinds_of(&out), ParticleKind::Flame);
        assert!((40..130).contains(&flames), "{flames}");
    }

    #[test]
    fn barriers_only_show_when_asked() {
        let mut world = World::default();
        world.set(0, 64, 0, BlockState::new(BARRIER, 0));
        let center = BlockPos::new(0, 64, 0);
        let run = |show| {
            let mut rng = JavaRandom::new(5);
            let mut out = Vec::new();
            for _ in 0..300 {
                sample_around(&world, &Dry, center, show, &mut rng, &mut out);
            }
            out
        };
        assert!(run(false).is_empty());
        let shown = run(true);
        assert!(!shown.is_empty());
        assert!(shown
            .iter()
            .all(|r| r.kind == ParticleKind::Barrier && r.position == DVec3::new(0.5, 64.5, 0.5)));
    }
}
