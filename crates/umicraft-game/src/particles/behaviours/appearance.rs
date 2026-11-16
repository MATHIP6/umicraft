// SPDX-License-Identifier: MIT OR Apache-2.0

//! `MobAppearance`: the elder guardian that swings in front of the camera
//! when a server curses the player (game state change 10).
//!
//! The particle itself is only a clock of 30 ticks; the client draws an elder
//! guardian model from its age (see `entity_render::appearance`).

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use crate::particles::particle::{Layer, Lighting, Particle, Texture};

/// The ticks the apparition lasts (`MobAppearance.java:21`).
const LIFE: u32 = 30;

/// The picture the apparition shows: the elder guardian texture. The client
/// recognises it by its behaviour, not by this name; naming a sprite keeps the
/// particle quad from being drawn.
pub const MOB_APPEARANCE_TEXTURE: &str = "entity/guardian_elder";

/// `MobAppearance.Factory`: a particle that does not move and lives 30 ticks
/// (MCP `MobAppearance.java:18-23`).
#[must_use]
pub fn create(position: DVec3, rng: &mut JavaRandom) -> Particle {
    let mut particle = Particle::with_velocity(position, DVec3::ZERO, rng);
    particle.motion = DVec3::ZERO;
    particle.gravity = 0.0;
    particle.max_age = LIFE;
    particle.layer = Layer::Lit;
    particle.lighting = Lighting::FullBright;
    particle.texture = Texture::Sprite {
        name: MOB_APPEARANCE_TEXTURE,
    };
    particle.behaviour = super::Behaviour::MobAppearance;
    particle
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::behaviours::Behaviour;

    #[test]
    fn an_apparition_stays_put_for_thirty_ticks() {
        let mut rng = JavaRandom::new(1);
        let at = DVec3::new(3.0, 70.0, -2.0);
        let particle = create(at, &mut rng);
        assert_eq!(particle.position, at);
        assert_eq!(particle.motion, DVec3::ZERO);
        assert_eq!(particle.max_age, 30);
        assert_eq!(particle.layer, Layer::Lit);
        assert_eq!(particle.behaviour, Behaviour::MobAppearance);
    }
}
