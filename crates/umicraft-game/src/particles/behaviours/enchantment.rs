// SPDX-License-Identifier: MIT OR Apache-2.0

//! The glyphs that fly from the bookshelves into an enchanting table:
//! `EntityEnchantmentTableParticleFX` (MCP
//! `client/particle/EntityEnchantmentTableParticleFX.java`).

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::common::cell;
use super::{Behaviour, TickContext};
use crate::particles::{Lighting, Particle};
use crate::shapes::BlockSource;

/// How far the glyph sinks by the end of its life, MCP
/// `EntityEnchantmentTableParticleFX.java:73` (`1.2F`).
const SINK: f32 = 1.2;

/// `IParticleFactory.getEntityFX` of `EnchantmentTable`. The packet position
/// is the table and the packet speed the offset of the shelf the glyph starts
/// from; the glyph begins there and flies to the table.
pub(super) fn create(position: DVec3, speed: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, speed, rng);
    // The offset is kept in `motion`; the tick turns it into a position.
    particle.motion = speed;
    particle.position = position + speed;
    particle.previous_position = particle.position;
    let shade = rng.next_float() * 0.6 + 0.4;
    particle.scale = rng.next_float() * 0.5 + 0.2;
    particle.color = [shade * 0.9, shade * 0.9, shade];
    particle.max_age = (rng.next_double() * 10.0) as u32 + 30;
    particle.texture = cell((rng.next_double() * 26.0 + 1.0 + 224.0) as i32);
    particle.lighting = Lighting::AgeGlow;
    particle.behaviour = Behaviour::Enchantment { table: position };
    particle
}

/// `EntityEnchantmentTableParticleFX.onUpdate`: places the glyph on the line
/// from the shelf to the table by the fraction of life left, and sinks it by
/// the fourth power of the fraction elapsed. There is no collision.
///
/// Unlike the other families the age is read before it is incremented, so the
/// last glyph is drawn exactly at the table.
pub(super) fn update<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    table: DVec3,
    _context: &mut TickContext<'_, S>,
) {
    particle.previous_position = particle.position;
    let left = 1.0 - particle.age as f32 / particle.max_age as f32;
    let mut elapsed = 1.0 - left;
    elapsed *= elapsed;
    elapsed *= elapsed;
    let sink = elapsed * SINK;
    particle.position = table + particle.motion * f64::from(left) - DVec3::Y * f64::from(sink);
    if particle.age >= particle.max_age {
        particle.dead = true;
    }
    particle.age += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::Texture;
    use umicraft_world::World;

    const TABLE: DVec3 = DVec3::new(10.5, 64.0, -3.5);
    const SHELF: DVec3 = DVec3::new(2.0, 1.0, -1.0);

    fn make(seed: u64) -> Particle {
        let mut rng = JavaRandom::new(seed);
        create(TABLE, SHELF, &mut rng)
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
            let glyph = make(seed);
            assert!((0.2..0.7).contains(&glyph.scale));
            assert!((30..40).contains(&glyph.max_age));
            let Texture::Cell { index } = glyph.texture else {
                panic!("a glyph uses a cell");
            };
            assert!((225..=250).contains(&index));
            let [red, green, blue] = glyph.color;
            assert!((0.4..1.0).contains(&blue));
            assert!((red - blue * 0.9).abs() < 1e-6);
            assert_eq!(red, green);
        }
    }

    #[test]
    fn the_glyph_starts_at_the_shelf() {
        let glyph = make(1);
        assert_eq!(glyph.position, TABLE + SHELF);
        assert_eq!(glyph.previous_position, glyph.position);
        assert_eq!(glyph.lighting, Lighting::AgeGlow);
    }

    #[test]
    fn the_first_tick_stays_at_the_shelf_and_the_path_ends_at_the_table() {
        let mut glyph = make(1);
        glyph.max_age = 30;
        tick(&mut glyph);
        assert!((glyph.position - (TABLE + SHELF)).length() < 1e-9);
        for _ in 1..30 {
            tick(&mut glyph);
        }
        // Age 29 of 30: a thirtieth of the way left, sunk by (29/30)^4 * 1.2.
        let left = 1.0 / 30.0;
        let sink = (29.0_f64 / 30.0).powi(4) * 1.2;
        let expected = TABLE + SHELF * left - DVec3::new(0.0, sink, 0.0);
        assert!((glyph.position - expected).length() < 1e-5);
    }

    #[test]
    fn the_glyph_dies_when_the_age_reaches_the_life() {
        let mut glyph = make(1);
        glyph.max_age = 30;
        for _ in 0..30 {
            tick(&mut glyph);
            assert!(!glyph.dead);
        }
        tick(&mut glyph);
        assert!(glyph.dead);
    }
}
