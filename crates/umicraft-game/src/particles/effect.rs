// SPDX-License-Identifier: MIT OR Apache-2.0

//! The particles of the `Effect` packet (MCP `RenderGlobal.playAuxSFX`, line
//! 2121 on) and of `ItemDye.spawnBonemealParticles`.
//!
//! The cases 1000 to 1022 (dispenser, door, music disc, ghast, wither, ...)
//! only play sounds and make no particle, so they are ignored here. Case 2001
//! (block break) needs the block state and its tint and is
//! [`ParticleEngine::add_block_destroy_effects`].

use std::f64::consts::PI;

use glam::DVec3;
use umicraft_world::BlockPos;

use super::engine::ParticleEngine;
use super::kind::ParticleKind;
use super::spawn::{SpawnFilter, SpawnRequest};

/// Step of the ender eye ring, MCP `RenderGlobal.java:2331`.
const ENDER_RING_STEP: f64 = 0.157_079_632_679_489_66;
/// Radius of the ender eye ring, MCP line 2333.
const ENDER_RING_RADIUS: f64 = 5.0;
/// Particles of a thrown potion, MCP line 2297.
const SPLASH_SPARKS: u32 = 100;
/// Flakes of the item thrown by a potion or an ender eye breaking.
const ITEM_FLAKES: u32 = 8;
/// The ids of `Items.potionitem` and `Items.ender_eye`.
const POTION_ITEM: u16 = 373;
const ENDER_EYE_ITEM: u16 = 381;
/// Smoke puffs per side of a smoke effect (case 2000), MCP line 2257.
const SMOKE_PUFFS: u32 = 10;
/// Particles of a spawner, MCP line 2341.
const SPAWNER_PUFFS: u32 = 20;
/// Particles of bone meal when the packet carries no count, MCP
/// `ItemDye.java:134`.
const DEFAULT_BONEMEAL: i32 = 15;

/// What the effect needs from outside the engine.
///
/// The reference reads the potion table and the block at the position; the
/// game thread owns them, so the caller passes the results.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EffectContext {
    /// `Items.potionitem.getColorFromDamage(data)` as `0xRRGGBB` (case 2002).
    pub potion_color: u32,
    /// `Items.potionitem.isEffectInstant(data)` (case 2002).
    pub potion_instant: bool,
    /// `getBlockBoundsMaxY` of the block at the position, or `None` for air
    /// (case 2005: the bone meal sparkles only over a real block).
    pub block_max_y: Option<f64>,
}

impl EffectContext {
    /// The context of an `Effect` packet whose `data` is `data`: for the potion
    /// splash (2002) that is the potion's damage value, from which the colour
    /// and the instant flag follow. Without it the sparkles come out black.
    #[must_use]
    pub fn new(data: i32, block_max_y: Option<f64>) -> Self {
        Self {
            potion_color: crate::items::potion_liquid_color(data),
            potion_instant: crate::items::is_instant_potion(data),
            block_max_y,
        }
    }
}

impl ParticleEngine {
    /// Spawns the particles of the `Effect` packet `effect_id` at `pos` with
    /// its `data` (MCP `RenderGlobal.playAuxSFX`): 2000 smoke jets, 2002
    /// potion splash, 2003 ender eye, 2004 spawner flames, 2005 bone meal.
    /// Other ids make nothing.
    ///
    /// `filter` applies like for any particle.
    pub fn spawn_effect(
        &mut self,
        effect_id: i32,
        pos: BlockPos,
        data: i32,
        context: &EffectContext,
        filter: &SpawnFilter,
    ) {
        match effect_id {
            2000 => self.smoke_jets(pos, data, filter),
            2002 => {
                let at = DVec3::new(pos.x.into(), pos.y.into(), pos.z.into());
                self.item_flakes(at, POTION_ITEM, data, filter);
                self.potion_splash(pos, context, filter);
            }
            2003 => {
                let at = DVec3::new(f64::from(pos.x) + 0.5, pos.y.into(), f64::from(pos.z) + 0.5);
                self.item_flakes(at, ENDER_EYE_ITEM, 0, filter);
                self.ender_eye_ring(pos, filter);
            }
            2004 => self.spawner_flames(pos, filter),
            2005 => self.bonemeal(pos, data, context.block_max_y, filter),
            _ => {}
        }
    }

    /// Spawns `kind` and says whether it was created.
    pub(super) fn emit(
        &mut self,
        kind: ParticleKind,
        position: DVec3,
        velocity: DVec3,
        filter: &SpawnFilter,
    ) -> bool {
        let request = SpawnRequest {
            kind,
            arguments: &[],
            ignore_range: kind.always_shown(),
            position,
            velocity,
        };
        self.spawn(&request, filter)
    }

    /// Eight flakes of item `id` thrown out of `at` (cases 2002 and 2003): a
    /// Gaussian of 0.15 sideways and up to 0.2 up.
    fn item_flakes(&mut self, at: DVec3, id: u16, damage: i32, filter: &SpawnFilter) {
        let arguments = [i32::from(id), damage];
        for _ in 0..ITEM_FLAKES {
            let rng = self.rng();
            let velocity = DVec3::new(
                rng.next_gaussian() * 0.15,
                rng.next_double() * 0.2,
                rng.next_gaussian() * 0.15,
            );
            let request = SpawnRequest {
                kind: ParticleKind::ItemCrack,
                arguments: &arguments,
                ignore_range: false,
                position: at,
                velocity,
            };
            self.spawn(&request, filter);
        }
    }

    /// Case 2000: ten puffs from a face of a dispenser; `data` packs the
    /// direction as `x + 1 + (z + 1) * 3` (line 2250).
    fn smoke_jets(&mut self, pos: BlockPos, data: i32, filter: &SpawnFilter) {
        let dx = f64::from(data % 3 - 1);
        let dz = f64::from(data / 3 % 3 - 1);
        let base = DVec3::new(
            f64::from(pos.x) + dx * 0.6 + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + dz * 0.6 + 0.5,
        );
        for _ in 0..SMOKE_PUFFS {
            let rng = self.rng();
            let speed = rng.next_double() * 0.2 + 0.01;
            let x = base.x + dx * 0.01 + (rng.next_double() - 0.5) * dz * 0.5;
            let y = base.y + (rng.next_double() - 0.5) * 0.5;
            let z = base.z + dz * 0.01 + (rng.next_double() - 0.5) * dx * 0.5;
            let velocity = DVec3::new(
                dx * speed + rng.next_gaussian() * 0.01,
                -0.03 + rng.next_gaussian() * 0.01,
                dz * speed + rng.next_gaussian() * 0.01,
            );
            self.emit(ParticleKind::Smoke, DVec3::new(x, y, z), velocity, filter);
        }
    }

    /// Case 2002: 100 sparkles thrown out in a disc, tinted with the potion
    /// colour (each at 75% to 100%) and slowed by their distance.
    fn potion_splash(&mut self, pos: BlockPos, context: &EffectContext, filter: &SpawnFilter) {
        let origin = DVec3::new(pos.x.into(), pos.y.into(), pos.z.into());
        let kind = if context.potion_instant {
            ParticleKind::InstantSpell
        } else {
            ParticleKind::Spell
        };
        let channel = |shift: u32| ((context.potion_color >> shift) & 255) as f32 / 255.0;
        let tint = [channel(16), channel(8), channel(0)];
        for _ in 0..SPLASH_SPARKS {
            let rng = self.rng();
            let radius = rng.next_double() * 4.0;
            let angle = rng.next_double() * PI * 2.0;
            let (x, z) = (angle.cos() * radius, angle.sin() * radius);
            let lift = 0.01 + rng.next_double() * 0.5;
            let request = SpawnRequest {
                kind,
                arguments: &[],
                ignore_range: kind.always_shown(),
                position: origin + DVec3::new(x * 0.1, 0.3, z * 0.1),
                velocity: DVec3::new(x, lift, z),
            };
            self.spawn_with(&request, filter, |particle, rng| {
                let shade = 0.75 + rng.next_float() * 0.25;
                let mut particle = particle.multiply_velocity(radius as f32);
                particle.color = tint.map(|channel| channel * shade);
                particle
            });
        }
    }

    /// Case 2003: two rings of portal particles closing in on the eye, at a
    /// radius of 5 blocks and speeds 5 and 7.
    fn ender_eye_ring(&mut self, pos: BlockPos, filter: &SpawnFilter) {
        let centre = DVec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) - 0.4,
            f64::from(pos.z) + 0.5,
        );
        let mut angle = 0.0;
        while angle < PI * 2.0 {
            let (cos, sin) = (angle.cos(), angle.sin());
            let at = centre + DVec3::new(cos, 0.0, sin) * ENDER_RING_RADIUS;
            for speed in [-5.0, -7.0] {
                let velocity = DVec3::new(cos * speed, 0.0, sin * speed);
                self.emit(ParticleKind::Portal, at, velocity, filter);
            }
            angle += ENDER_RING_STEP;
        }
    }

    /// Case 2004: twenty smoke and flame pairs in a 2-block cube, at rest.
    fn spawner_flames(&mut self, pos: BlockPos, filter: &SpawnFilter) {
        let centre = DVec3::new(pos.x.into(), pos.y.into(), pos.z.into()) + DVec3::splat(0.5);
        for _ in 0..SPAWNER_PUFFS {
            let rng = self.rng();
            let mut offset = || (f64::from(rng.next_float()) - 0.5) * 2.0;
            let at = centre + DVec3::new(offset(), offset(), offset());
            self.emit(ParticleKind::Smoke, at, DVec3::ZERO, filter);
            self.emit(ParticleKind::Flame, at, DVec3::ZERO, filter);
        }
    }

    /// Case 2005 (`ItemDye.spawnBonemealParticles`): `data` green stars
    /// (15 when 0) over the block, as high as its bounds, drifting by a small
    /// Gaussian. Nothing over air.
    fn bonemeal(
        &mut self,
        pos: BlockPos,
        data: i32,
        block_max_y: Option<f64>,
        filter: &SpawnFilter,
    ) {
        let Some(max_y) = block_max_y else {
            return;
        };
        let amount = if data == 0 { DEFAULT_BONEMEAL } else { data };
        for _ in 0..amount {
            let rng = self.rng();
            let drift = DVec3::new(
                rng.next_gaussian() * 0.02,
                rng.next_gaussian() * 0.02,
                rng.next_gaussian() * 0.02,
            );
            // `(float)pos + nextFloat()` is a float sum in the reference.
            let x = f64::from(pos.x as f32 + rng.next_float());
            let y = f64::from(pos.y) + f64::from(rng.next_float()) * max_y;
            let z = f64::from(pos.z as f32 + rng.next_float());
            self.emit(
                ParticleKind::HappyVillager,
                DVec3::new(x, y, z),
                drift,
                filter,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::{Layer, ParticleSetting, Texture};

    fn near() -> SpawnFilter {
        SpawnFilter {
            viewer: DVec3::new(0.0, 64.0, 0.0),
            setting: ParticleSetting::All,
        }
    }

    fn effect(id: i32, data: i32, context: EffectContext) -> ParticleEngine {
        let mut engine = ParticleEngine::new(7);
        engine.spawn_effect(id, BlockPos::new(0, 64, 0), data, &context, &near());
        engine
    }

    #[test]
    fn the_context_of_a_potion_takes_its_colour_and_kind_from_the_damage() {
        // 16421: splash Healing; 16386: splash Swiftness.
        let healing = EffectContext::new(16421, None);
        assert_eq!(healing.potion_color, 16_262_179);
        assert!(healing.potion_instant);
        let swiftness = EffectContext::new(16386, Some(1.0));
        assert_eq!(swiftness.potion_color, 8_171_462);
        assert!(!swiftness.potion_instant);
        assert_eq!(swiftness.block_max_y, Some(1.0));
    }

    #[test]
    fn sound_only_ids_make_no_particle() {
        for id in [1000, 1005, 1022, 2001, 3000] {
            assert!(effect(id, 0, EffectContext::default()).is_empty(), "{id}");
        }
    }

    #[test]
    fn smoke_jets_leave_the_side_of_the_dispenser() {
        // data 5 = x + 1 = 2, z + 1 = 1: east, no z.
        let engine = effect(2000, 5, EffectContext::default());
        let smoke = engine.particles(Layer::Misc, false);
        assert_eq!(smoke.len(), 10);
        assert!(smoke.iter().all(|puff| puff.position.x > 1.0));
    }

    #[test]
    fn a_splash_throws_a_hundred_tinted_sparkles() {
        let context = EffectContext {
            potion_color: 0xFF_00_00,
            ..EffectContext::default()
        };
        let engine = effect(2002, 0, context);
        let sparkles = engine.particles(Layer::Misc, false);
        assert_eq!(sparkles.len(), 100);
        assert!(sparkles
            .iter()
            .all(|spark| spark.color[0] >= 0.75 && spark.color[1] == 0.0));
    }

    #[test]
    fn the_ender_eye_makes_two_rings_of_portals_and_eight_flakes() {
        // 40 steps of pi / 20 around the circle, two speeds each, and the
        // flakes of the eye.
        let engine = effect(2003, 0, EffectContext::default());
        assert_eq!(engine.len(), 88);
        let flakes = engine.particles(Layer::Terrain, false);
        assert_eq!(flakes.len(), 8);
        assert!(flakes
            .iter()
            .all(|flake| flake.texture == Texture::Item { id: 381, damage: 0 }));
    }

    #[test]
    fn a_splash_also_throws_eight_flakes_of_the_potion() {
        // Regression: the flakes of the bottle were left out.
        let engine = effect(2002, 16_421, EffectContext::default());
        let flakes = engine.particles(Layer::Terrain, false);
        assert_eq!(flakes.len(), 8);
        assert!(flakes.iter().all(|flake| flake.texture
            == Texture::Item {
                id: 373,
                damage: 16_421
            }));
    }

    #[test]
    fn a_spawner_makes_twenty_smoke_and_flame_pairs() {
        assert_eq!(effect(2004, 0, EffectContext::default()).len(), 40);
    }

    #[test]
    fn bonemeal_defaults_to_fifteen_and_skips_air() {
        let over_block = EffectContext {
            block_max_y: Some(1.0),
            ..EffectContext::default()
        };
        assert_eq!(effect(2005, 0, over_block).len(), 15);
        assert_eq!(effect(2005, 4, over_block).len(), 4);
        assert!(effect(2005, 0, EffectContext::default()).is_empty());
    }
}
