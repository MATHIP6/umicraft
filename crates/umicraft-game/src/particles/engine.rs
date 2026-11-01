// SPDX-License-Identifier: MIT OR Apache-2.0

//! The particle store and its tick (MCP `EffectRenderer`).

use umicraft_core::java_random::JavaRandom;

use super::particle::{Layer, Particle};
use crate::shapes::BlockSource;

/// The most particles one layer and pass keeps; adding one more drops the
/// oldest (MCP `EffectRenderer.addEffect`, line 140).
pub const MAX_PER_BUCKET: usize = 4000;

/// A layer and a blend pass: translucent particles do not write depth.
const BUCKETS: usize = Layer::ALL.len() * 2;

/// All the live particles, grouped like the reference's `fxLayers`.
#[derive(Debug)]
pub struct ParticleEngine {
    buckets: [Vec<Particle>; BUCKETS],
    /// Particles whose opacity changed during a tick and now belong to the
    /// other pass; kept to reuse its allocation.
    migrating: Vec<Particle>,
    rng: JavaRandom,
}

/// The bucket of a particle: `layer * 2 + (opaque ? 1 : 0)` like `fxLayers`.
fn bucket_of(particle: &Particle) -> usize {
    particle.layer.index() * 2 + usize::from(particle.alpha == 1.0)
}

impl ParticleEngine {
    /// An empty engine; `seed` makes the random draws repeatable.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            buckets: Default::default(),
            migrating: Vec::new(),
            rng: JavaRandom::new(seed),
        }
    }

    /// The random source particles are built from.
    pub fn rng(&mut self) -> &mut JavaRandom {
        &mut self.rng
    }

    /// Adds a particle; a full bucket drops its oldest first.
    pub fn add(&mut self, particle: Particle) {
        let bucket = &mut self.buckets[bucket_of(&particle)];
        if bucket.len() >= MAX_PER_BUCKET {
            bucket.remove(0);
        }
        bucket.push(particle);
    }

    /// Advances every particle by one tick, drops the dead ones and moves the
    /// ones whose opacity changed to the other pass.
    pub fn tick<S: BlockSource + ?Sized>(&mut self, world: &S) {
        for index in 0..BUCKETS {
            let bucket = &mut self.buckets[index];
            for particle in bucket.iter_mut() {
                let behaviour = particle.behaviour;
                behaviour.update(particle, world, &mut self.rng);
            }
            bucket.retain(|particle| !particle.dead);
            let mut kept = 0;
            for position in 0..bucket.len() {
                if bucket_of(&bucket[position]) == index {
                    bucket.swap(kept, position);
                    kept += 1;
                }
            }
            self.migrating.extend(bucket.drain(kept..));
        }
        while let Some(particle) = self.migrating.pop() {
            self.add(particle);
        }
    }

    /// The particles of one layer and pass, oldest first.
    #[must_use]
    pub fn particles(&self, layer: Layer, translucent: bool) -> &[Particle] {
        &self.buckets[layer.index() * 2 + usize::from(!translucent)]
    }

    /// The number of live particles.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buckets.iter().map(Vec::len).sum()
    }

    /// Whether there is no live particle.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drops every particle (a world change).
    pub fn clear(&mut self) {
        self.buckets.iter_mut().for_each(Vec::clear);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;
    use umicraft_world::World;

    fn particle(engine: &mut ParticleEngine) -> Particle {
        Particle::new(DVec3::new(0.5, 100.0, 0.5), engine.rng())
    }

    #[test]
    fn dead_particles_are_dropped_on_tick() {
        let world = World::new(0);
        let mut engine = ParticleEngine::new(1);
        let mut dying = particle(&mut engine);
        dying.max_age = 0;
        engine.add(dying);
        engine.tick(&world);
        assert!(engine.is_empty());
    }

    #[test]
    fn a_full_bucket_drops_its_oldest() {
        let mut engine = ParticleEngine::new(1);
        for age in 0..=MAX_PER_BUCKET {
            let mut next = particle(&mut engine);
            next.age = age as u32;
            engine.add(next);
        }
        let kept = engine.particles(Layer::Misc, false);
        assert_eq!(kept.len(), MAX_PER_BUCKET);
        assert_eq!(kept[0].age, 1);
    }

    #[test]
    fn translucent_particles_use_the_other_pass() {
        let mut engine = ParticleEngine::new(1);
        let mut faint = particle(&mut engine);
        faint.alpha = 0.5;
        engine.add(faint);
        assert_eq!(engine.particles(Layer::Misc, true).len(), 1);
        assert!(engine.particles(Layer::Misc, false).is_empty());
    }

    #[test]
    fn a_particle_changing_opacity_moves_pass_on_tick() {
        let world = World::new(0);
        let mut engine = ParticleEngine::new(1);
        let mut fading = particle(&mut engine);
        fading.max_age = 100;
        engine.add(fading);
        engine.buckets[Layer::Misc.index() * 2 + 1][0].alpha = 0.4;
        engine.tick(&world);
        assert_eq!(engine.particles(Layer::Misc, true).len(), 1);
        assert!(engine.particles(Layer::Misc, false).is_empty());
    }

    #[test]
    fn clear_empties_every_bucket() {
        let mut engine = ParticleEngine::new(1);
        let next = particle(&mut engine);
        engine.add(next);
        engine.clear();
        assert_eq!(engine.len(), 0);
    }
}
