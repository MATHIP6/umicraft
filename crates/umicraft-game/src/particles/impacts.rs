// SPDX-License-Identifier: MIT OR Apache-2.0

//! The particles of a thrown object that lands on a block.
//!
//! `EntityThrowable.onImpact` runs on the client as well as on the server and
//! spawns its particles before the `!isRemote` branch: eight snowball puffs
//! for a snowball (`EntitySnowball`), eight pieces of egg for an egg
//! (`EntityEgg`) and 32 portal sparks for an ender pearl (`EntityEnderPearl`).
//! Potions and experience bottles make theirs from the `Effect` packet.

use glam::DVec3;

use super::engine::ParticleEngine;
use super::kind::ParticleKind;
use super::spawn::{SpawnFilter, SpawnRequest};
use crate::entity::{EntityKind, ObjectImpact};

/// Puffs of a snowball, `EntitySnowball.java:44`.
const SNOWBALL_PUFFS: u32 = 8;
/// Pieces of an egg, `EntityEgg.java:59`.
const EGG_PIECES: u32 = 8;
/// Sparks of a pearl, `EntityEnderPearl.java:51`.
const PEARL_SPARKS: u32 = 32;
/// The item id of the egg, `Items.egg`.
const EGG_ITEM: i32 = 344;
/// The scatter of an egg piece, `EntityEgg.java:59` (`0.08D`).
const EGG_SCATTER: f64 = 0.08;

impl ParticleEngine {
    /// Creates the particles of `impact`; other objects make none.
    pub fn spawn_impact(&mut self, impact: &ObjectImpact, filter: &SpawnFilter) {
        let at = impact.position;
        match impact.kind {
            EntityKind::Snowball => {
                for _ in 0..SNOWBALL_PUFFS {
                    self.emit(ParticleKind::Snowball, at, DVec3::ZERO, filter);
                }
            }
            EntityKind::Egg => {
                for _ in 0..EGG_PIECES {
                    let rng = self.rng();
                    let mut piece = || (f64::from(rng.next_float()) - 0.5) * EGG_SCATTER;
                    let velocity = DVec3::new(piece(), piece(), piece());
                    self.spawn(
                        &SpawnRequest {
                            kind: ParticleKind::ItemCrack,
                            arguments: &[EGG_ITEM],
                            ignore_range: false,
                            position: at,
                            velocity,
                        },
                        filter,
                    );
                }
            }
            EntityKind::EnderPearl => {
                for _ in 0..PEARL_SPARKS {
                    let rng = self.rng();
                    let height = rng.next_double() * 2.0;
                    let velocity = DVec3::new(rng.next_gaussian(), 0.0, rng.next_gaussian());
                    self.emit(
                        ParticleKind::Portal,
                        at + DVec3::new(0.0, height, 0.0),
                        velocity,
                        filter,
                    );
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::ParticleSetting;

    fn filter() -> SpawnFilter {
        SpawnFilter {
            viewer: DVec3::new(0.0, 64.0, 0.0),
            setting: ParticleSetting::All,
        }
    }

    fn count(kind: EntityKind) -> usize {
        let mut engine = ParticleEngine::new(5);
        let impact = ObjectImpact {
            kind,
            position: DVec3::new(1.0, 64.0, 1.0),
        };
        engine.spawn_impact(&impact, &filter());
        engine.len()
    }

    #[test]
    fn each_thrown_object_makes_its_own_number_of_particles() {
        assert_eq!(count(EntityKind::Snowball), 8);
        assert_eq!(count(EntityKind::Egg), 8);
        assert_eq!(count(EntityKind::EnderPearl), 32);
    }

    #[test]
    fn a_potion_or_a_bottle_makes_none_here() {
        assert_eq!(count(EntityKind::Potion), 0);
        assert_eq!(count(EntityKind::ExpBottle), 0);
    }
}
