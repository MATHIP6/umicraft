// SPDX-License-Identifier: MIT OR Apache-2.0

//! The emitter of the huge explosion (MCP `EntityHugeExplodeFX`): it draws
//! nothing and throws six large explosion flashes per tick for eight ticks.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::{explode, Behaviour, TickContext};
use crate::particles::particle::Particle;
use crate::shapes::BlockSource;

/// Flashes thrown per tick, MCP `EntityHugeExplodeFX.java:31`.
const FLASHES_PER_TICK: u32 = 6;
/// Ticks of emission, MCP `EntityHugeExplodeFX.java:13` (`maximumTime`).
const EMITTING_TICKS: u32 = 8;
/// Half side of the cube the flashes are scattered in, MCP line 33.
const SPREAD: f64 = 4.0;

/// `EntityHugeExplodeFX.Factory`: the constructor ignores the packet speed
/// but still runs the velocity constructor of `EntityFX`, so it draws as it.
#[must_use]
pub(super) fn create(position: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, DVec3::ZERO, rng);
    particle.behaviour = Behaviour::HugeExplosion { ticks: 0 };
    particle
}

/// `EntityHugeExplodeFX.onUpdate`: six flashes at random points within 4
/// blocks (a difference of two draws per axis), whose `speed.x` is the share
/// of the emission elapsed so the later ones are smaller. The particle never
/// ages; it dies once it has emitted for eight ticks.
///
/// The reference spawns them through `World.spawnParticle`, and the large
/// explosion ignores the range filter, so they are built directly.
pub(super) fn update<S: BlockSource + ?Sized>(
    particle: &mut Particle,
    ticks: u32,
    context: &mut TickContext<'_, S>,
) {
    let progress = f64::from(ticks as f32 / EMITTING_TICKS as f32);
    for _ in 0..FLASHES_PER_TICK {
        let at = DVec3::new(
            scattered(particle.position.x, context.rng),
            scattered(particle.position.y, context.rng),
            scattered(particle.position.z, context.rng),
        );
        let speed = DVec3::new(progress, 0.0, 0.0);
        let flash = explode::large_flash(at, speed, context.rng);
        context.spawned.push(flash);
    }
    let ticks = ticks + 1;
    particle.behaviour = Behaviour::HugeExplosion { ticks };
    particle.dead = ticks == EMITTING_TICKS;
}

/// `base + (r1 - r2) * 4`, the draw order of the reference.
fn scattered(base: f64, rng: &mut JavaRandom) -> f64 {
    base + (rng.next_double() - rng.next_double()) * SPREAD
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::{Layer, ParticleEngine};
    use umicraft_world::World;

    #[test]
    fn it_emits_six_flashes_a_tick_for_eight_ticks_then_dies() {
        let mut engine = ParticleEngine::new(1);
        let mut rng = JavaRandom::new(2);
        engine.add(create(DVec3::new(0.0, 64.0, 0.0), &mut rng));
        let world = World::new(0);
        for _ in 0..8 {
            engine.tick(&world);
        }
        // The emitter is gone, only its 48 flashes remain (none expired yet:
        // they live at least 6 ticks and the first ones are 8 ticks old).
        let flashes = engine.particles(Layer::Lit, false).len();
        assert_eq!(engine.len(), flashes);
        assert!(flashes <= 48 && flashes > 6);
    }

    #[test]
    fn the_first_tick_makes_six_full_size_flashes_in_the_cube() {
        let mut engine = ParticleEngine::new(1);
        let mut rng = JavaRandom::new(2);
        let centre = DVec3::new(10.0, 64.0, 10.0);
        engine.add(create(centre, &mut rng));
        engine.tick(&World::new(0));
        let flashes = engine.particles(Layer::Lit, false);
        assert_eq!(flashes.len(), 6);
        for flash in flashes {
            assert!((flash.position - centre).abs().max_element() <= 4.0);
            assert_eq!(flash.behaviour, Behaviour::LargeExplode { size: 1.0 });
        }
    }

    #[test]
    fn the_emitter_itself_is_invisible() {
        let mut rng = JavaRandom::new(2);
        let emitter = create(DVec3::ZERO, &mut rng);
        assert_eq!(emitter.behaviour.render_scale(&emitter, 0.5), 0.0);
        assert_eq!(emitter.behaviour.render_alpha(&emitter, 0.5), 0.0);
    }
}
