// SPDX-License-Identifier: MIT OR Apache-2.0

//! The rain droplet and the splash that extends it (MCP `EntityRainFX`,
//! `EntitySplashFX`).
//!
//! The splash only changes the constructor, so both share [`Behaviour::Rain`]
//! and its update.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::shared::{count_down, is_under_surface, random_life};
use super::{Behaviour, TickContext};
use crate::particles::particle::{Particle, Texture};
use crate::shapes::{selection_shape, BlockSource};

/// The sideways share kept by the droplet constructor, MCP
/// `EntityRainFX.java:15` (`0.3F` widened).
const SCATTER_KEPT: f64 = 0.300_000_011_920_928_96;
/// The damping per tick, MCP `EntityRainFX.java:43` (`0.98F` widened).
const DRAG: f64 = 0.980_000_019_073_486_3;
/// The ground friction per tick, MCP `EntityRainFX.java:54` (`0.7F` widened).
const GROUND_FRICTION: f64 = 0.699_999_988_079_071;
/// The first of the four droplet cells of `particles.png`, MCP
/// `EntityRainFX.java:21`.
const FIRST_CELL: u32 = 19;
/// The upward motion a splash takes from a sideways packet, MCP
/// `EntitySplashFX.java:19`.
const SPLASH_LIFT: f64 = 0.1;

/// `EntityRainFX.Factory`: a droplet thrown up from `position` (MCP
/// `EntityRainFX.java:11-26`).
#[must_use]
pub(super) fn droplet(position: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, DVec3::ZERO, rng);
    particle.motion.x *= SCATTER_KEPT;
    particle.motion.y = rng.next_double() * 0.2 + 0.1;
    particle.motion.z *= SCATTER_KEPT;
    particle.color = [1.0; 3];
    particle.texture = Texture::Cell {
        index: (FIRST_CELL + rng.next_int(4)) as u8,
    };
    particle.size = 0.01;
    particle.gravity = 0.06;
    particle.max_age = random_life(rng, 8.0);
    particle.behaviour = Behaviour::Rain;
    particle
}

/// `EntitySplashFX.Factory`: a droplet with less gravity, the next picture of
/// the strip and, for a sideways packet with no vertical speed, the packet's
/// motion lifted by 0.1 (MCP `EntitySplashFX.java:12-23`).
#[must_use]
pub(super) fn splash(position: DVec3, speed: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = droplet(position, rng);
    particle.gravity = 0.04;
    if let Texture::Cell { index } = &mut particle.texture {
        *index += 1;
    }
    if speed.y == 0.0 && (speed.x != 0.0 || speed.z != 0.0) {
        particle.motion = DVec3::new(speed.x, speed.y + SPLASH_LIFT, speed.z);
    }
    particle
}

/// `EntityRainFX.onUpdate`: falls with the raw gravity (not scaled by 0.04),
/// half the droplets die when they touch the ground, and one dies as soon as
/// it is under the surface of a liquid or solid block.
pub(super) fn update<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    context: &mut TickContext<'_, S>,
) {
    particle.previous_position = particle.position;
    particle.motion.y -= f64::from(particle.gravity);
    particle.move_with_collision(context.world);
    particle.motion *= DRAG;
    count_down(particle);
    if particle.on_ground {
        if context.rng.next_double() < 0.5 {
            particle.dead = true;
        }
        particle.motion.x *= GROUND_FRICTION;
        particle.motion.z *= GROUND_FRICTION;
    }
    let world = context.world;
    // `setBlockBoundsBasedOnState` then `getBlockBoundsMaxY`: the bounds, not
    // the collision boxes.
    let bounds_top = |state, cell| {
        selection_shape(state, world, cell)
            .boxes()
            .first()
            .map_or(1.0, |bounds| bounds.max.y)
    };
    if is_under_surface(world, particle.position, bounds_top) {
        particle.dead = true;
    }
}

#[cfg(test)]
mod tests {
    use super::super::shared::Layered;
    use super::*;
    use umicraft_world::BlockState;

    fn tick<S: BlockSource>(particle: &mut Particle, world: &S, rng: &mut JavaRandom) {
        let mut spawned = Vec::new();
        let mut context = TickContext {
            world,
            rng,
            spawned: &mut spawned,
        };
        let behaviour = particle.behaviour;
        behaviour.update(particle, &mut context);
    }

    fn air() -> Layered {
        Layered {
            top: 0,
            state: BlockState::AIR,
        }
    }

    #[test]
    fn a_droplet_starts_in_the_reference_ranges() {
        let mut rng = JavaRandom::new(4);
        for _ in 0..200 {
            let droplet = droplet(DVec3::ZERO, &mut rng);
            assert!((0.1..=0.3).contains(&droplet.motion.y));
            assert!((19..=22).contains(&match droplet.texture {
                Texture::Cell { index } => index,
                Texture::Sprite { .. } | Texture::Block { .. } | Texture::Item { .. } => 0,
            }));
            assert_eq!(droplet.size, 0.01);
            assert_eq!(droplet.gravity, 0.06);
            assert!((8..=40).contains(&droplet.max_age));
        }
    }

    #[test]
    fn a_droplet_falls_by_its_raw_gravity() {
        let mut rng = JavaRandom::new(4);
        let mut droplet = droplet(DVec3::new(0.5, 100.0, 0.5), &mut rng);
        droplet.motion = DVec3::ZERO;
        tick(&mut droplet, &air(), &mut rng);
        assert!((droplet.motion.y - (-f64::from(0.06_f32) * DRAG)).abs() < 1e-12);
    }

    #[test]
    fn a_droplet_dies_when_its_life_runs_out() {
        let mut rng = JavaRandom::new(4);
        let mut droplet = droplet(DVec3::new(0.5, 200.0, 0.5), &mut rng);
        droplet.max_age = 2;
        droplet.gravity = 0.0;
        droplet.motion = DVec3::ZERO;
        for _ in 0..2 {
            tick(&mut droplet, &air(), &mut rng);
            assert!(!droplet.dead);
        }
        tick(&mut droplet, &air(), &mut rng);
        assert!(droplet.dead);
    }

    #[test]
    fn a_droplet_inside_stone_dies() {
        let mut rng = JavaRandom::new(4);
        let stone = Layered {
            top: i32::MAX,
            state: BlockState::new(1, 0),
        };
        let mut droplet = droplet(DVec3::new(0.5, 4.5, 0.5), &mut rng);
        tick(&mut droplet, &stone, &mut rng);
        assert!(droplet.dead);
    }

    #[test]
    fn a_droplet_over_the_water_surface_survives_below_it_dies() {
        let mut rng = JavaRandom::new(4);
        let water = Layered {
            top: 5,
            state: BlockState::new(9, 0),
        };
        let mut above = droplet(DVec3::new(0.5, 5.2, 0.5), &mut rng);
        above.motion = DVec3::ZERO;
        above.gravity = 0.0;
        tick(&mut above, &water, &mut rng);
        assert!(!above.dead);
        let mut inside = droplet(DVec3::new(0.5, 4.5, 0.5), &mut rng);
        inside.motion = DVec3::ZERO;
        inside.gravity = 0.0;
        tick(&mut inside, &water, &mut rng);
        assert!(inside.dead);
    }

    #[test]
    fn some_droplets_survive_and_some_die_on_the_ground() {
        let stone = Layered {
            top: 4,
            state: BlockState::new(1, 0),
        };
        let mut rng = JavaRandom::new(11);
        let mut dead = 0;
        for _ in 0..100 {
            let mut droplet = droplet(DVec3::new(0.5, 4.0, 0.5), &mut rng);
            droplet.motion = DVec3::new(0.0, -0.1, 0.0);
            tick(&mut droplet, &stone, &mut rng);
            assert!(droplet.on_ground);
            dead += usize::from(droplet.dead);
        }
        assert!((20..=80).contains(&dead), "{dead} of 100 died");
    }

    #[test]
    fn a_splash_has_less_gravity_and_the_next_cell() {
        let mut rng = JavaRandom::new(4);
        let splash = splash(DVec3::ZERO, DVec3::ZERO, &mut rng);
        assert_eq!(splash.gravity, 0.04);
        let Texture::Cell { index } = splash.texture else {
            unreachable!("a splash samples particles.png");
        };
        assert!((20..=23).contains(&index));
    }

    #[test]
    fn a_sideways_splash_takes_the_packet_motion() {
        let mut rng = JavaRandom::new(4);
        let splash = splash(DVec3::ZERO, DVec3::new(0.2, 0.0, -0.1), &mut rng);
        assert_eq!(splash.motion, DVec3::new(0.2, SPLASH_LIFT, -0.1));
    }

    #[test]
    fn a_splash_with_vertical_speed_keeps_the_droplet_motion() {
        let mut rng = JavaRandom::new(4);
        let splash = splash(DVec3::ZERO, DVec3::new(0.2, 0.5, 0.0), &mut rng);
        assert!((0.1..=0.3).contains(&splash.motion.y));
    }
}
