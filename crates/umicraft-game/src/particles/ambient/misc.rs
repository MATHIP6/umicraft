// SPDX-License-Identifier: MIT OR Apache-2.0

//! The single-particle blocks: mycelium, the brewing stand, dripping leaves and
//! the wet sponge.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;
use umicraft_world::BlockPos;

use super::emit::{emit, float_add, Requests};
use super::facing::front_offset;
use super::surface::has_solid_top;
use super::weather::Weather;
use crate::particles::ParticleKind;
use crate::shapes::BlockSource;

/// `BlockMycelium.randomDisplayTick`: a spore 1.1 above the top, one tick in
/// ten (MCP `BlockMycelium.java:68`).
pub(super) fn mycelium(pos: BlockPos, rng: &mut JavaRandom, out: &mut Requests) {
    if rng.next_int(10) != 0 {
        return;
    }
    let position = DVec3::new(
        float_add(pos.x, rng),
        f64::from(pos.y as f32 + 1.1),
        float_add(pos.z, rng),
    );
    emit(out, ParticleKind::TownAura, position);
}

/// `BlockBrewingStand.randomDisplayTick`: smoke over the bottles every tick (MCP
/// `BlockBrewingStand.java:131`).
pub(super) fn brewing_stand(pos: BlockPos, rng: &mut JavaRandom, out: &mut Requests) {
    let mut span = |base: i32, start: f32, length: f32| {
        f64::from(base as f32 + start + rng.next_float() * length)
    };
    let position = DVec3::new(
        span(pos.x, 0.4, 0.2),
        span(pos.y, 0.7, 0.3),
        span(pos.z, 0.4, 0.2),
    );
    emit(out, ParticleKind::Smoke, position);
}

/// `BlockLeaves.randomDisplayTick`: a drop falls from leaves that rain reaches
/// and that hang over a gap, one tick in fifteen (MCP `BlockLeaves.java:192`).
pub(super) fn leaves<S, W>(
    world: &S,
    weather: &W,
    pos: BlockPos,
    rng: &mut JavaRandom,
    out: &mut Requests,
) where
    S: BlockSource + ?Sized,
    W: Weather + ?Sized,
{
    if !weather.can_lightning_strike(BlockPos::new(pos.x, pos.y + 1, pos.z))
        || has_solid_top(world.block(pos.x, pos.y - 1, pos.z))
        || rng.next_int(15) != 1
    {
        return;
    }
    let position = DVec3::new(
        float_add(pos.x, rng),
        f64::from(pos.y) - 0.05,
        float_add(pos.z, rng),
    );
    emit(out, ParticleKind::DripWater, position);
}

/// `BlockSponge.randomDisplayTick` of a wet sponge: a random side (never the
/// top) that is not resting on a solid top drips (MCP `BlockSponge.java:149`).
pub(super) fn wet_sponge<S: BlockSource + ?Sized>(
    world: &S,
    pos: BlockPos,
    rng: &mut JavaRandom,
    out: &mut Requests,
) {
    // `EnumFacing.random`: down, up, north, south, west, east.
    let facing = rng.next_int(6) as u8;
    let (dx, dy, dz) = match facing {
        0 => (0, -1, 0),
        1 => return,
        other => {
            let (x, z) = front_offset(other);
            (x, 0, z)
        }
    };
    if has_solid_top(world.block(pos.x + dx, pos.y + dy, pos.z + dz)) {
        return;
    }
    let mut position = DVec3::new(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
    if facing == 0 {
        position.y -= 0.05;
        position.x += rng.next_double();
        position.z += rng.next_double();
    } else {
        position.y += rng.next_double() * 0.8;
        // The face's own axis sits against the wall; the other one slides.
        let (wall, slide) = if dx != 0 { (0, 2) } else { (2, 0) };
        let toward = f64::from(dx + dz);
        let mut axes = [position.x, position.y, position.z];
        axes[slide] += rng.next_double();
        axes[wall] += if toward > 0.0 { 1.0 } else { 0.05 };
        position = DVec3::from(axes);
    }
    emit(out, ParticleKind::DripWater, position);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::ambient::testing::{kinds, World};
    use crate::particles::ambient::weather::{Dry, Weather};
    use umicraft_world::BlockState;

    struct Wet;

    impl Weather for Wet {
        fn can_lightning_strike(&self, _pos: BlockPos) -> bool {
            true
        }

        fn rains_on_biome(&self, _pos: BlockPos) -> bool {
            true
        }
    }

    fn many(ticks: u64, mut tick: impl FnMut(&mut JavaRandom, &mut Requests)) -> Requests {
        let mut rng = JavaRandom::new(17);
        let mut out = Vec::new();
        for _ in 0..ticks {
            tick(&mut rng, &mut out);
        }
        out
    }

    #[test]
    fn mycelium_releases_a_spore_one_tick_in_ten() {
        let out = many(2000, |rng, out| mycelium(BlockPos::new(0, 64, 0), rng, out));
        assert!((150..250).contains(&out.len()), "{}", out.len());
        assert!(out.iter().all(|r| r.kind == ParticleKind::TownAura));
        assert!(out.iter().all(|r| (r.position.y - 65.1).abs() < 1e-5));
    }

    #[test]
    fn a_brewing_stand_smokes_inside_its_bottle_box() {
        let out = many(50, |rng, out| {
            brewing_stand(BlockPos::new(0, 10, 0), rng, out)
        });
        assert_eq!(out.len(), 50);
        for request in &out {
            assert_eq!(request.kind, ParticleKind::Smoke);
            let p = request.position;
            assert!(
                (0.4..0.6).contains(&p.x)
                    && (10.7..11.0).contains(&p.y)
                    && (0.4..0.6).contains(&p.z)
            );
        }
    }

    #[test]
    fn leaves_drip_only_in_the_rain_over_a_gap() {
        let world = World::default();
        let pos = BlockPos::new(0, 64, 0);
        let wet = many(3000, |rng, out| leaves(&world, &Wet, pos, rng, out));
        assert!((140..260).contains(&wet.len()), "{}", wet.len());
        assert!(wet
            .iter()
            .all(|r| r.kind == ParticleKind::DripWater && (r.position.y - 63.95).abs() < 1e-12));
        assert!(many(3000, |rng, out| leaves(&world, &Dry, pos, rng, out)).is_empty());
        let mut roofed = World::default();
        roofed.set(0, 63, 0, BlockState::new(1, 0));
        assert!(many(3000, |rng, out| leaves(&roofed, &Wet, pos, rng, out)).is_empty());
    }

    #[test]
    fn a_wet_sponge_drips_from_its_open_sides_and_underside() {
        let world = World::default();
        let pos = BlockPos::new(0, 64, 0);
        let out = many(600, |rng, out| wet_sponge(&world, pos, rng, out));
        // Five of six facings drip over open air.
        assert!((420..580).contains(&out.len()), "{}", out.len());
        assert_eq!(kinds(&out[..1]), [ParticleKind::DripWater]);
        for request in &out {
            let p = request.position;
            let on_wall = [0.05, 1.0].iter().any(|w| p.x == *w || p.z == *w);
            assert!((p.y - 63.95).abs() < 1e-12 || on_wall, "{p}");
            assert!((63.95..=64.8).contains(&p.y));
        }
    }

    #[test]
    fn a_wet_sponge_resting_on_everything_makes_no_drip() {
        let mut world = World::default();
        let pos = BlockPos::new(0, 64, 0);
        for (dx, dy, dz) in [(0, -1, 0), (0, 0, -1), (0, 0, 1), (-1, 0, 0), (1, 0, 0)] {
            world.set(dx, 64 + dy, dz, BlockState::new(1, 0));
        }
        assert!(many(600, |rng, out| wet_sponge(&world, pos, rng, out)).is_empty());
    }
}
