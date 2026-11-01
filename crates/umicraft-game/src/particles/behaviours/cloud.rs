// SPDX-License-Identifier: MIT OR Apache-2.0

//! The white puffs: `EntityCloudFX` (MCP `client/particle/EntityCloudFX.java`).

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::common::{begin_tick, drag_and_rest, light_grey, step_cell};
use super::{Behaviour, TickContext};
use crate::particles::Particle;
use crate::shapes::BlockSource;

/// The scale and life factor of the factory, MCP `EntityCloudFX.java:15`.
const PUFF: f32 = 2.5;
/// Damping of the random start motion, MCP `EntityCloudFX.java:16`.
const START_DAMPING: f64 = 0.100_000_001_490_116_12;
/// Per-tick drag, MCP `EntityCloudFX.java:57-59`.
const DRAG: f64 = 0.959_999_978_542_327_9;

/// `IParticleFactory.getEntityFX` of `Cloud`: the packet speed is added to a
/// weak random motion.
///
/// Approximation: the reference also pulls a puff within two blocks of a
/// player down to the player's feet (`getClosestPlayerToEntity`); the tick
/// context has no player, so that pull is left out.
pub(super) fn create(position: DVec3, speed: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, DVec3::ZERO, rng);
    particle.motion = particle.motion * START_DAMPING + speed;
    particle.color = light_grey(rng);
    particle.scale *= 0.75 * PUFF;
    let life = (8.0 / (rng.next_double() * 0.8 + 0.3)) as u32;
    particle.max_age = (life as f32 * PUFF) as u32;
    particle.behaviour = Behaviour::Cloud;
    particle
}

/// `EntityCloudFX.onUpdate`: ages, steps through eight pictures, moves with
/// a drag and no gravity.
pub(super) fn update<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    context: &mut TickContext<'_, S>,
) {
    begin_tick(particle);
    particle.texture = step_cell(0, particle);
    particle.move_with_collision(context.world);
    drag_and_rest(particle, DRAG);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::Texture;
    use umicraft_world::World;

    fn make(seed: u64, speed: DVec3) -> Particle {
        let mut rng = JavaRandom::new(seed);
        create(DVec3::new(0.5, 100.0, 0.5), speed, &mut rng)
    }

    fn tick(particle: &mut Particle) {
        let world = World::new(0);
        let mut rng = JavaRandom::new(1);
        let mut spawned = Vec::new();
        let mut context = TickContext {
            world: &world,
            rng: &mut rng,
            spawned: &mut spawned,
        };
        let behaviour = particle.behaviour;
        behaviour.update(particle, &mut context);
    }

    #[test]
    fn constructor_values_stay_in_the_reference_ranges() {
        for seed in 0..100 {
            let puff = make(seed, DVec3::ZERO);
            let grey = puff.color[0];
            assert!((0.7..=1.0).contains(&grey));
            assert_eq!(puff.color, [grey; 3]);
            // (1 .. 2) * 0.75 * 2.5.
            assert!((1.875..=3.75).contains(&puff.scale));
            // int(8 / (0.3 .. 1.1)) is 7 to 26, times 2.5.
            assert!((17..=65).contains(&puff.max_age));
        }
    }

    #[test]
    fn the_packet_speed_is_added_in_full() {
        let still = make(3, DVec3::ZERO);
        let thrown = make(3, DVec3::new(0.5, 0.0, -0.25));
        assert!((thrown.motion.x - still.motion.x - 0.5).abs() < 1e-12);
        assert!((thrown.motion.z - still.motion.z + 0.25).abs() < 1e-12);
    }

    #[test]
    fn a_tick_steps_the_picture_and_drags_without_gravity() {
        let mut puff = make(3, DVec3::ZERO);
        puff.max_age = 40;
        puff.motion = DVec3::new(0.0, 0.5, 0.0);
        tick(&mut puff);
        // Age 1 of 40: 7 - 1 * 8 / 40 = 7.
        assert_eq!(puff.texture, Texture::Cell { index: 7 });
        assert!((puff.motion.y - 0.5 * DRAG).abs() < 1e-12);
    }

    #[test]
    fn the_picture_reaches_cell_zero_late_in_the_life_and_the_puff_dies() {
        let mut puff = make(3, DVec3::ZERO);
        puff.max_age = 40;
        for _ in 0..40 {
            tick(&mut puff);
            assert!(!puff.dead);
        }
        // Age 40 of 40: 7 - 40 * 8 / 40 = -1, which the cell index saturates.
        assert_eq!(puff.texture, Texture::Cell { index: 0 });
        tick(&mut puff);
        assert!(puff.dead);
    }
}
