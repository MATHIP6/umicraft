// SPDX-License-Identifier: MIT OR Apache-2.0

//! The stars of a critical hit and of a sharp weapon.
//!
//! The server tells the client with `Animation` 4 (critical) and 5 (magic
//! critical, from a Sharpness, Smite or Bane of Arthropods weapon); the
//! client then throws a short burst of particles at the entity that was hit
//! (MCP `NetHandlerPlayClient.handleAnimation`,
//! `EffectRenderer.emitParticleAtEntity`, `EntityParticleEmitter`). Against
//! another player the client also predicts the hit by itself
//! (`EntityPlayer.attackTargetEntityWithCurrentItem`), since a remote player
//! answers true to `attackEntityFrom` on the client world.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;

use crate::entity::{EntityId, EntityKind};

/// The Sharpness enchantment id (MCP `Enchantment.sharpness`).
pub const SHARPNESS: i16 = 16;

/// Ticks an emitter lives (`EntityParticleEmitter.lifetime`).
const EMITTER_LIFETIME: u8 = 3;
/// Particles tried per burst.
const TRIES: usize = 16;

/// The kind of star an emitter throws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitStar {
    /// A critical hit (`EnumParticleTypes.CRIT`).
    Critical,
    /// The enchanted hit (`EnumParticleTypes.CRIT_MAGIC`).
    Magic,
}

impl HitStar {
    /// The star of an `Animation` code, `None` for the other animations.
    #[must_use]
    pub fn from_animation(animation: u8) -> Option<Self> {
        match animation {
            4 => Some(Self::Critical),
            5 => Some(Self::Magic),
            _ => None,
        }
    }
}

/// One particle of a burst.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StarSpawn {
    /// Where it appears.
    pub position: DVec3,
    /// Its starting velocity.
    pub velocity: DVec3,
}

/// A burst of stars attached to an entity (`EntityParticleEmitter`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitEmitter {
    /// The entity that was hit.
    pub target: EntityId,
    /// The star it throws.
    pub star: HitStar,
    age: u8,
}

impl HitEmitter {
    /// A new emitter. The reference runs the first burst in the constructor:
    /// the caller takes it with [`HitEmitter::tick`] right away.
    #[must_use]
    pub const fn new(target: EntityId, star: HitStar) -> Self {
        Self {
            target,
            star,
            age: 0,
        }
    }

    /// One burst; `false` once the emitter has burst three times and is dead.
    pub fn tick(&mut self) -> bool {
        self.age += 1;
        self.age <= EMITTER_LIFETIME
    }
}

/// The particles of one burst around a body of `width` by `height` standing
/// at `feet` (`EntityParticleEmitter.onUpdate`): sixteen random points of the
/// unit ball, kept when they fall in it, scaled to a quarter of the body.
pub fn burst(rng: &mut JavaRandom, feet: DVec3, width: f64, height: f64) -> Vec<StarSpawn> {
    let mut spawns = Vec::new();
    for _ in 0..TRIES {
        let x = f64::from(rng.next_float() * 2.0 - 1.0);
        let y = f64::from(rng.next_float() * 2.0 - 1.0);
        let z = f64::from(rng.next_float() * 2.0 - 1.0);
        if x * x + y * y + z * z <= 1.0 {
            spawns.push(StarSpawn {
                position: DVec3::new(
                    feet.x + x * width / 4.0,
                    feet.y + height / 2.0 + y * height / 4.0,
                    feet.z + z * width / 4.0,
                ),
                velocity: DVec3::new(x, y + 0.2, z),
            });
        }
    }
    spawns
}

/// What the local attacker sees of its own hit on `target`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HitStars {
    /// The critical stars.
    pub critical: bool,
    /// The enchanted stars.
    pub magic: bool,
}

/// The conditions of a critical hit (MCP `EntityPlayer`: falling, off the
/// ground, off a ladder, out of water, not blind, not riding, and a living
/// target).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CriticalState {
    /// `fallDistance > 0`.
    pub falling: bool,
    /// `onGround`.
    pub on_ground: bool,
    /// `isOnLadder()`.
    pub on_ladder: bool,
    /// `isInWater()`.
    pub in_water: bool,
    /// `isPotionActive(blindness)`.
    pub blind: bool,
    /// `ridingEntity != null`.
    pub riding: bool,
}

impl CriticalState {
    fn is_critical(self) -> bool {
        self.falling
            && !self.on_ground
            && !self.on_ladder
            && !self.in_water
            && !self.blind
            && !self.riding
    }
}

/// The stars the client throws for its own hit on `target`.
///
/// Only another player answers true to `attackEntityFrom` on the client
/// world; against a mob the server's `Animation` 4 and 5 do it. `sharpness`
/// is the level of the held item: players have no creature attribute, so the
/// only enchantment that counts against them is Sharpness.
#[must_use]
pub fn local_stars(target: EntityKind, state: CriticalState, sharpness: u32) -> HitStars {
    if target != EntityKind::Player {
        return HitStars::default();
    }
    HitStars {
        critical: state.is_critical(),
        magic: sharpness > 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FALLING: CriticalState = CriticalState {
        falling: true,
        on_ground: false,
        on_ladder: false,
        in_water: false,
        blind: false,
        riding: false,
    };

    #[test]
    fn animations_four_and_five_are_the_stars() {
        assert_eq!(HitStar::from_animation(4), Some(HitStar::Critical));
        assert_eq!(HitStar::from_animation(5), Some(HitStar::Magic));
        assert_eq!(HitStar::from_animation(0), None);
    }

    #[test]
    fn an_emitter_bursts_three_times() {
        let mut emitter = HitEmitter::new(EntityId(1), HitStar::Critical);
        assert!(emitter.tick());
        assert!(emitter.tick());
        assert!(emitter.tick());
        assert!(!emitter.tick());
    }

    #[test]
    fn a_burst_stays_inside_a_quarter_of_the_body() {
        let mut rng = JavaRandom::new(7);
        let feet = DVec3::new(10.0, 64.0, -3.0);
        let mut seen = 0;
        for _ in 0..50 {
            for star in burst(&mut rng, feet, 0.6, 1.8) {
                seen += 1;
                assert!((star.position.x - 10.0).abs() <= 0.15 + 1e-9);
                assert!((star.position.z + 3.0).abs() <= 0.15 + 1e-9);
                assert!((star.position.y - 64.9).abs() <= 0.45 + 1e-9);
                // The velocity is the point of the ball, lifted by 0.2.
                assert!(star.velocity.length() <= 1.4);
            }
        }
        // About half the points fall in the ball (4/3 pi / 8 = 52 %).
        assert!((300..500).contains(&seen), "{seen}");
    }

    #[test]
    fn a_falling_hit_on_a_player_is_a_critical() {
        let stars = local_stars(EntityKind::Player, FALLING, 0);
        assert_eq!(
            stars,
            HitStars {
                critical: true,
                magic: false
            }
        );
    }

    #[test]
    fn standing_on_the_ground_water_a_ladder_or_a_horse_is_no_critical() {
        for state in [
            CriticalState {
                on_ground: true,
                ..FALLING
            },
            CriticalState {
                in_water: true,
                ..FALLING
            },
            CriticalState {
                on_ladder: true,
                ..FALLING
            },
            CriticalState {
                riding: true,
                ..FALLING
            },
            CriticalState {
                blind: true,
                ..FALLING
            },
            CriticalState {
                falling: false,
                ..FALLING
            },
        ] {
            assert!(!local_stars(EntityKind::Player, state, 0).critical);
        }
    }

    #[test]
    fn sharpness_alone_gives_the_magic_stars() {
        let stars = local_stars(
            EntityKind::Player,
            CriticalState {
                falling: false,
                ..FALLING
            },
            2,
        );
        assert_eq!(
            stars,
            HitStars {
                critical: false,
                magic: true
            }
        );
    }

    #[test]
    fn a_mob_leaves_the_stars_to_the_server() {
        assert_eq!(
            local_stars(EntityKind::Zombie, FALLING, 5),
            HitStars::default()
        );
    }
}
