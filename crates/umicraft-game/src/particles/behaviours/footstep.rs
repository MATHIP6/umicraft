// SPDX-License-Identifier: MIT OR Apache-2.0

//! Footstep: `EntityFootStepFX`, a footprint stamped on the ground.
//!
//! Unlike every other particle it is **not a billboard**. The renderer draws
//! it as a flat 0.25 x 0.25 quad lying on the ground at `position`
//! (`position.y` is the bottom of the quad, corners at `+-0.125` on X and Z),
//! with the `textures/particle/footprint.png` picture, alpha blending, no
//! light map, and a grey tint equal to the block brightness at the particle
//! ([`Lighting::BlockBrightness`]). It is in [`Layer::Lit`], the
//! reference's layer 3, so it is drawn apart from the billboards. Its alpha
//! comes from [`alpha`]; `Particle::alpha` stays 1.
//!
//! It never moves and lives exactly 200 ticks, counted in `Particle::age`
//! (the reference keeps a private `footstepAge` that is the same number).

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::Behaviour;
use crate::particles::particle::{Layer, Lighting, Particle};

/// The life of a footprint in ticks, MCP `EntityFootStepFX.java:24`.
const LIFE: u32 = 200;
/// The peak opacity of a footprint, MCP `EntityFootStepFX.java:42`.
const PEAK_ALPHA: f32 = 0.2;

/// A footprint at `position` (MCP `EntityFootStepFX` constructor, lines
/// 19-25 and the factory): at rest, for [`LIFE`] ticks. The random draws of
/// the base constructor are still consumed, as in the reference.
#[must_use]
pub(super) fn create(position: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, DVec3::ZERO, rng);
    particle.motion = DVec3::ZERO;
    particle.max_age = LIFE;
    particle.layer = Layer::Lit;
    particle.lighting = Lighting::BlockBrightness;
    particle.behaviour = Behaviour::Footstep;
    particle
}

/// `EntityFootStepFX.onUpdate`: only ages; it dies exactly at [`LIFE`].
pub(super) fn update(particle: &mut Particle) {
    particle.age += 1;
    if particle.age == particle.max_age {
        particle.dead = true;
    }
}

/// The opacity to draw with: `0.2 * min(1, 2 - 2 f^2)` where `f` is the
/// fraction of the life (`renderParticle`, MCP `EntityFootStepFX.java:37-44`).
/// It stays at the peak for the first 70% of the life, then fades out.
#[must_use]
pub(super) fn alpha(particle: &Particle, partial_ticks: f32) -> f32 {
    let progress = (particle.age as f32 + partial_ticks) / particle.max_age as f32;
    (2.0 - progress * progress * 2.0).min(1.0) * PEAK_ALPHA
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make() -> Particle {
        create(DVec3::new(3.0, 64.0, 3.0), &mut JavaRandom::new(1))
    }

    fn tick(particle: &mut Particle) {
        update(particle);
    }

    #[test]
    fn footprint_is_flat_and_lasts_200_ticks() {
        let particle = make();
        assert_eq!(particle.layer, Layer::Lit);
        assert_eq!(particle.max_age, 200);
        assert_eq!(particle.motion, DVec3::ZERO);
        assert_eq!(particle.lighting, Lighting::BlockBrightness);
    }

    #[test]
    fn footprint_does_not_move_and_dies_at_200() {
        let mut particle = make();
        for _ in 0..199 {
            tick(&mut particle);
        }
        assert!(!particle.dead);
        assert_eq!(particle.position, DVec3::new(3.0, 64.0, 3.0));
        tick(&mut particle);
        assert!(particle.dead);
    }

    #[test]
    fn alpha_holds_the_peak_then_fades() {
        let mut particle = make();
        assert_eq!(alpha(&particle, 0.0), PEAK_ALPHA);
        particle.age = 100;
        assert_eq!(alpha(&particle, 0.0), PEAK_ALPHA);
        particle.age = 200;
        assert_eq!(alpha(&particle, 0.0), 0.0);
        particle.age = 180;
        assert!(alpha(&particle, 0.0) < PEAK_ALPHA);
    }
}
