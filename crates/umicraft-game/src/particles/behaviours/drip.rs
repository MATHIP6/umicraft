// SPDX-License-Identifier: MIT OR Apache-2.0

//! The drops that fall from the ceiling: `dripWater` and `dripLava` (MCP
//! `EntityDropParticleFX`).
//!
//! A drop first hangs for 40 ticks (`bobTimer`), nearly motionless, then
//! falls. A water drop splashes where it lands; a lava drop stays on the
//! ground and fades.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;
use umicraft_world::fluid::Liquid;

use super::rain::splash;
use super::shared::{count_down, is_under_surface, random_life};
use super::{Behaviour, TickContext};
use crate::particles::particle::{Lighting, Particle, Texture};
use crate::shapes::BlockSource;

/// The ticks a drop hangs before it falls, MCP `EntityDropParticleFX.java:37`.
const HANG_TICKS: i32 = 40;
/// The factor that holds a hanging drop, MCP `EntityDropParticleFX.java:82`.
const HANGING_DAMPING: f64 = 0.02;
/// The damping per tick, MCP `EntityDropParticleFX.java:96` (`0.98F` widened).
const DRAG: f64 = 0.980_000_019_073_486_3;
/// The ground friction per tick, MCP `EntityDropParticleFX.java:114`
/// (`0.7F` widened).
const GROUND_FRICTION: f64 = 0.699_999_988_079_071;
/// The cell of a hanging drop, of one that falls, and of one that landed
/// (lava), MCP `EntityDropParticleFX.java:34,86,92,110`.
const HANGING_CELL: u8 = 113;
/// See [`HANGING_CELL`].
const FALLING_CELL: u8 = 112;
/// See [`HANGING_CELL`].
const LANDED_CELL: u8 = 114;

/// `EntityDropParticleFX.WaterFactory` and `LavaFactory`: a drop of `liquid`
/// hanging at `position` (MCP `EntityDropParticleFX.java:19-41`).
///
/// A lava drop is always fully lit (`getBrightnessForRender` is 257, past
/// the end of the lightmap).
#[must_use]
pub(super) fn drop(position: DVec3, liquid: Liquid, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, DVec3::ZERO, rng);
    particle.motion = DVec3::ZERO;
    particle.color = match liquid {
        Liquid::Water => [0.0, 0.0, 1.0],
        Liquid::Lava => [1.0, 0.0, 0.0],
    };
    particle.texture = Texture::Cell {
        index: HANGING_CELL,
    };
    particle.size = 0.01;
    particle.gravity = 0.06;
    particle.max_age = random_life(rng, 64.0);
    if liquid == Liquid::Lava {
        particle.lighting = Lighting::FullBright;
    }
    particle.behaviour = Behaviour::Drip {
        liquid,
        bob_timer: HANG_TICKS,
    };
    particle
}

/// The colour of a drop at a bob timer: a constant blue for water, a lava
/// that cools from yellow-white to red as it hangs (MCP
/// `EntityDropParticleFX.java:58-71`).
fn colour(liquid: Liquid, bob_timer: i32) -> [f32; 3] {
    match liquid {
        Liquid::Water => [0.2, 0.3, 1.0],
        Liquid::Lava => {
            let elapsed = (HANG_TICKS - bob_timer) as f32;
            [1.0, 16.0 / (elapsed + 16.0), 4.0 / (elapsed + 8.0)]
        }
    }
}

/// `EntityDropParticleFX.onUpdate`.
///
/// The hanging damping applies to every axis, gravity included, so the drop
/// creeps down until the timer is spent. Landing kills a water drop and
/// queues a splash at its position; it only changes the picture of a lava
/// drop. Entering a liquid or solid block kills either.
pub(super) fn update<S: BlockSource + ?Sized>(
    liquid: Liquid,
    bob_timer: i32,
    particle: &mut Particle,
    context: &mut TickContext<'_, S>,
) {
    particle.previous_position = particle.position;
    particle.color = colour(liquid, bob_timer);
    particle.motion.y -= f64::from(particle.gravity);
    if bob_timer > 0 {
        particle.motion *= HANGING_DAMPING;
        particle.texture = Texture::Cell {
            index: HANGING_CELL,
        };
    } else {
        particle.texture = Texture::Cell {
            index: FALLING_CELL,
        };
    }
    particle.behaviour = Behaviour::Drip {
        liquid,
        bob_timer: bob_timer - 1,
    };
    particle.move_with_collision(context.world);
    particle.motion *= DRAG;
    count_down(particle);
    if particle.on_ground {
        land(liquid, particle, context);
    }
    // The solid case reads 0 from `d0`, so the surface is the cell's top.
    if is_under_surface(context.world, particle.position, |_, _| 1.0) {
        particle.dead = true;
    }
}

/// The ground contact of a drop: a water drop becomes a splash, a lava drop
/// flattens.
fn land<S: BlockSource + ?Sized>(
    liquid: Liquid,
    particle: &mut Particle,
    context: &mut TickContext<'_, S>,
) {
    match liquid {
        Liquid::Water => {
            particle.dead = true;
            let drop_splash = splash(particle.position, DVec3::ZERO, context.rng);
            context.spawned.push(drop_splash);
        }
        Liquid::Lava => particle.texture = Texture::Cell { index: LANDED_CELL },
    }
    particle.motion.x *= GROUND_FRICTION;
    particle.motion.z *= GROUND_FRICTION;
}

#[cfg(test)]
mod tests {
    use super::super::shared::Layered;
    use super::*;
    use umicraft_world::BlockState;

    fn floor_at_4() -> Layered {
        Layered {
            top: 4,
            state: BlockState::new(1, 0),
        }
    }

    fn tick<S: BlockSource>(
        particle: &mut Particle,
        world: &S,
        rng: &mut JavaRandom,
        spawned: &mut Vec<Particle>,
    ) {
        let mut context = TickContext {
            world,
            rng,
            spawned,
        };
        let behaviour = particle.behaviour;
        behaviour.update(particle, &mut context);
    }

    #[test]
    fn a_new_drop_hangs_with_its_liquid_colour() {
        let mut rng = JavaRandom::new(1);
        let water = drop(DVec3::ZERO, Liquid::Water, &mut rng);
        assert_eq!(water.color, [0.0, 0.0, 1.0]);
        assert_eq!(water.lighting, Lighting::World);
        let lava = drop(DVec3::ZERO, Liquid::Lava, &mut rng);
        assert_eq!(lava.color, [1.0, 0.0, 0.0]);
        assert_eq!(lava.lighting, Lighting::FullBright);
        assert_eq!(lava.motion, DVec3::ZERO);
        assert!((64..=320).contains(&lava.max_age));
    }

    #[test]
    fn a_hanging_drop_barely_moves_then_falls() {
        let mut rng = JavaRandom::new(1);
        let mut spawned = Vec::new();
        let mut drop = drop(DVec3::new(0.5, 100.0, 0.5), Liquid::Water, &mut rng);
        let air = Layered {
            top: 0,
            state: BlockState::AIR,
        };
        tick(&mut drop, &air, &mut rng, &mut spawned);
        assert!(drop.motion.y.abs() < 0.002);
        for _ in 0..HANG_TICKS {
            tick(&mut drop, &air, &mut rng, &mut spawned);
        }
        assert_eq!(
            drop.texture,
            Texture::Cell {
                index: FALLING_CELL
            }
        );
        assert!(drop.motion.y < -0.05);
    }

    #[test]
    fn a_water_drop_splashes_where_it_lands() {
        let mut rng = JavaRandom::new(1);
        let mut spawned = Vec::new();
        let mut drop = drop(DVec3::new(0.5, 4.0, 0.5), Liquid::Water, &mut rng);
        tick(&mut drop, &floor_at_4(), &mut rng, &mut spawned);
        assert!(drop.dead);
        assert_eq!(spawned.len(), 1);
        assert_eq!(spawned[0].behaviour, Behaviour::Rain);
        assert_eq!(spawned[0].gravity, 0.04);
        assert!((spawned[0].position - drop.position).length() < 1e-9);
    }

    #[test]
    fn a_lava_drop_stays_and_flattens_on_the_ground() {
        let mut rng = JavaRandom::new(1);
        let mut spawned = Vec::new();
        let mut drop = drop(DVec3::new(0.5, 4.0, 0.5), Liquid::Lava, &mut rng);
        tick(&mut drop, &floor_at_4(), &mut rng, &mut spawned);
        assert!(!drop.dead);
        assert!(spawned.is_empty());
        assert_eq!(drop.texture, Texture::Cell { index: LANDED_CELL });
    }

    #[test]
    fn a_lava_drop_cools_while_it_hangs() {
        assert_eq!(colour(Liquid::Lava, HANG_TICKS), [1.0, 1.0, 0.5]);
        let late = colour(Liquid::Lava, 0);
        assert!(late[1] < 0.5 && late[2] < 0.1);
    }

    #[test]
    fn a_drop_dies_inside_a_solid_block() {
        let mut rng = JavaRandom::new(1);
        let mut spawned = Vec::new();
        let stone = Layered {
            top: i32::MAX,
            state: BlockState::new(1, 0),
        };
        let mut drop = drop(DVec3::new(0.5, 4.5, 0.5), Liquid::Lava, &mut rng);
        tick(&mut drop, &stone, &mut rng, &mut spawned);
        assert!(drop.dead);
    }

    #[test]
    fn a_drop_dies_when_its_life_runs_out() {
        let mut rng = JavaRandom::new(1);
        let mut spawned = Vec::new();
        let air = Layered {
            top: 0,
            state: BlockState::AIR,
        };
        let mut drop = drop(DVec3::new(0.5, 200.0, 0.5), Liquid::Lava, &mut rng);
        drop.max_age = 1;
        tick(&mut drop, &air, &mut rng, &mut spawned);
        assert!(!drop.dead);
        tick(&mut drop, &air, &mut rng, &mut spawned);
        assert!(drop.dead);
    }
}
