// SPDX-License-Identifier: MIT OR Apache-2.0

//! The crumbs of an eating player (MCP `EntityPlayer.updateItemUse`): pieces
//! of the food that fly out of the mouth, every fourth tick near the end of
//! the meal.
//!
//! The server never sends them: `World.spawnParticle` does nothing there, and
//! each client makes them for every player it sees eating, the local one
//! included.
//!
//! The bite sound (`random.eat`) is played by the same method. The server
//! relays it to the *other* players only (`playSoundToNearExcept`), so the
//! local player must make their own, see [`ParticleEngine::bite_cue`].

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use super::engine::ParticleEngine;
use super::kind::ParticleKind;
use super::spawn::{SpawnFilter, SpawnRequest};
use crate::sound::cue::{CueCategory, SoundCue};

/// Crumbs of a bite (`updateItemUse(item, 5)`).
const CRUMBS: u32 = 5;
/// The use count from which a bite shows, and its rhythm
/// (`itemInUseCount <= 25 && itemInUseCount % 4 == 0`).
const FIRST_BITE: u32 = 25;
const BITE_EVERY: u32 = 4;

/// Whether a meal with `count` ticks left (MCP `itemInUseCount`, `32` at the
/// start for food) gives a bite this tick. A finished meal (`0`) gives none:
/// that moment belongs to the server.
#[must_use]
pub fn is_bite_tick(count: u32) -> bool {
    count > 0 && count <= FIRST_BITE && count.is_multiple_of(BITE_EVERY)
}

/// Where an eater is and where it looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Eater {
    /// The eyes (`posY + getEyeHeight()`).
    pub eyes: DVec3,
    /// The yaw in degrees (MCP `rotationYaw`).
    pub yaw: f32,
    /// The pitch in degrees.
    pub pitch: f32,
}

/// `Vec3.rotatePitch` then `Vec3.rotateYaw`, both by the eater's look, in the
/// order `updateItemUse` applies them.
fn turned(vector: DVec3, eater: &Eater) -> DVec3 {
    let pitch = f64::from(-eater.pitch.to_radians());
    let (sin, cos) = pitch.sin_cos();
    let tilted = DVec3::new(
        vector.x,
        vector.y * cos + vector.z * sin,
        vector.z * cos - vector.y * sin,
    );
    let yaw = f64::from(-eater.yaw.to_radians());
    let (sin, cos) = yaw.sin_cos();
    DVec3::new(
        tilted.x * cos + tilted.z * sin,
        tilted.y,
        tilted.z * cos - tilted.x * sin,
    )
}

/// One crumb: where it starts (in front of the face, below the eyes) and how
/// it flies (up and a little sideways).
fn crumb(eater: &Eater, rng: &mut JavaRandom) -> (DVec3, DVec3) {
    let fly = DVec3::new(
        (f64::from(rng.next_float()) - 0.5) * 0.1,
        rng.next_double() * 0.1 + 0.1,
        0.0,
    );
    let velocity = turned(fly, eater);
    let drop = f64::from(-rng.next_float()) * 0.6 - 0.3;
    let start = DVec3::new((f64::from(rng.next_float()) - 0.5) * 0.3, drop, 0.6);
    (
        eater.eyes + turned(start, eater),
        velocity + DVec3::new(0.0, 0.05, 0.0),
    )
}

impl ParticleEngine {
    /// Spawns the crumbs of one bite of item `id` with `damage` (MCP
    /// `EntityPlayer.updateItemUse`).
    pub fn spawn_crumbs(&mut self, eater: &Eater, id: u16, damage: u16, filter: &SpawnFilter) {
        let arguments = [i32::from(id), i32::from(damage)];
        for _ in 0..CRUMBS {
            let (position, velocity) = crumb(eater, self.rng());
            self.spawn(
                &SpawnRequest {
                    kind: ParticleKind::ItemCrack,
                    arguments: &arguments,
                    ignore_range: false,
                    position,
                    velocity,
                },
                filter,
            );
        }
    }

    /// The `random.eat` cue of one bite at `at` (MCP `updateItemUse`): the
    /// volume is `0.5` or `1.0` and the pitch wobbles around `1.0` by `0.2`.
    pub fn bite_cue(&mut self, at: DVec3) -> SoundCue {
        let rng = self.rng();
        let volume = 0.5 + 0.5 * rng.next_int(2) as f32;
        let pitch = (rng.next_float() - rng.next_float()) * 0.2 + 1.0;
        SoundCue {
            name: "random.eat".into(),
            pos: at.to_array(),
            volume,
            pitch,
            category: CueCategory::Players,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::{Layer, ParticleSetting, Texture};

    fn eater(yaw: f32, pitch: f32) -> Eater {
        Eater {
            eyes: DVec3::new(0.0, 65.62, 0.0),
            yaw,
            pitch,
        }
    }

    #[test]
    fn a_bite_comes_every_fourth_tick_in_the_last_quarter_of_the_meal() {
        let bites: Vec<u32> = (0..=32).filter(|&count| is_bite_tick(count)).collect();
        assert_eq!(bites, [4, 8, 12, 16, 20, 24]);
    }

    #[test]
    fn crumbs_fly_up_from_in_front_of_the_face() {
        let mut rng = JavaRandom::new(3);
        // Yaw 0 looks towards +z: the crumbs start 0.6 ahead of the eyes.
        for _ in 0..50 {
            let (position, velocity) = crumb(&eater(0.0, 0.0), &mut rng);
            let offset = position - DVec3::new(0.0, 65.62, 0.0);
            assert!((offset.z - 0.6).abs() < 1e-9, "{offset}");
            assert!((-0.9..=-0.3).contains(&offset.y), "{offset}");
            assert!(velocity.y > 0.14 && velocity.y < 0.26, "{velocity}");
        }
    }

    #[test]
    fn a_bite_sounds_like_the_reference() {
        // Regression: the local player's own meal made no sound.
        let mut engine = ParticleEngine::new(3);
        for _ in 0..50 {
            let cue = engine.bite_cue(DVec3::new(1.0, 2.0, 3.0));
            assert_eq!(cue.name, "random.eat");
            assert_eq!(cue.pos, [1.0, 2.0, 3.0]);
            assert!(cue.volume == 0.5 || cue.volume == 1.0, "{}", cue.volume);
            assert!((0.8..=1.2).contains(&cue.pitch), "{}", cue.pitch);
        }
    }

    #[test]
    fn the_crumbs_follow_the_yaw() {
        let mut rng = JavaRandom::new(3);
        // Yaw 90 looks towards -x in this game's convention: the face is there.
        let (position, _) = crumb(&eater(90.0, 0.0), &mut rng);
        assert!(position.x < -0.5, "{position}");
        assert!(position.z.abs() < 0.3, "{position}");
    }

    #[test]
    fn a_bite_makes_five_pieces_of_the_food() {
        let mut engine = ParticleEngine::new(1);
        let filter = SpawnFilter {
            viewer: DVec3::new(0.0, 65.0, 0.0),
            setting: ParticleSetting::All,
        };
        engine.spawn_crumbs(&eater(0.0, 0.0), 297, 0, &filter);
        let crumbs = engine.particles(Layer::Terrain, false);
        assert_eq!(crumbs.len(), 5);
        assert!(crumbs
            .iter()
            .all(|piece| piece.texture == Texture::Item { id: 297, damage: 0 }));
    }
}
