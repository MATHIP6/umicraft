// SPDX-License-Identifier: MIT OR Apache-2.0

//! The particles mobs give off by themselves on the client: every tick
//! (`onLivingUpdate`), on an `Entity Status` (`handleStatusUpdate`) and when
//! the death fall ends (`EntityLivingBase.onDeathUpdate`).
//!
//! The server never sends these: in 1.8 `World.spawnParticle` is empty on the
//! server, so each client spawns them from the entity state it tracks. Every
//! position and velocity below is the reference's; the draws come from the
//! engine's random source instead of the entity's own `rand`.

use glam::DVec3;
use umicraft_core::java_random::JavaRandom;
use umicraft_world::BlockPos;

use super::engine::ParticleEngine;
use super::kind::ParticleKind;
use super::spawn::{SpawnFilter, SpawnRequest};
use crate::combat::hitbox::body_size;
use crate::entity::meta::Variant;
use crate::entity::mob_state::wither_head_positions;
use crate::entity::{EntityKind, EntityRecord, MobState};
use crate::item_use::{use_action, UseAction};
use crate::particles::ambient::{running_dust, RunningBody};
use crate::particles::{is_bite_tick, Eater};
use crate::physics::EYE_HEIGHT;
use crate::raycast::look_direction;
use crate::shapes::BlockSource;
use umicraft_protocol::packets::play::ItemStack;

/// Health at or below which a wither is armoured: half of its 300
/// (`EntityWither.isArmored`).
const WITHER_ARMORED_HEALTH: f32 = 150.0;
/// The squared horizontal speed above which an iron golem kicks up the
/// ground (`EntityIronGolem.onLivingUpdate`, `2.500000277905201E-7`).
const GOLEM_STEP_SPEED_SQUARED: f64 = 2.500_000_277_905_201e-7;
/// How far under the feet the block a golem treads on is read
/// (`posY - 0.20000000298023224D`).
const GOLEM_PROBE_DEPTH: f64 = 0.200_000_002_980_232_24;

/// Where a mob is and how big it is (MCP `posX/Y/Z`, `width`, `height`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MobBody {
    /// The feet.
    pub feet: DVec3,
    /// The width of the bounding box.
    pub width: f64,
    /// The height of the bounding box.
    pub height: f64,
}

impl MobBody {
    /// The body of `entity`, or `None` for an entity without one (an item, an
    /// arrow, the dragon, whose parts are its boxes).
    #[must_use]
    pub fn of(entity: &EntityRecord) -> Option<Self> {
        let (width, height) = body_size(entity)?;
        Some(Self {
            feet: entity.position,
            width,
            height,
        })
    }

    /// A point in the box: `(r - 0.5) * width` across, `r * height` up (the
    /// spread of `EntityBlaze`, `EntityEnderman` and the potion swirls).
    fn inside(&self, rng: &mut JavaRandom) -> DVec3 {
        let x = (rng.next_double() - 0.5) * self.width;
        let y = rng.next_double() * self.height;
        let z = (rng.next_double() - 0.5) * self.width;
        self.feet + DVec3::new(x, y, z)
    }

    /// A point over twice the width: `r * width * 2 - width` across, with
    /// float draws (the hearts, smoke and explosions of the statuses).
    fn around(&self, rng: &mut JavaRandom, lift: f64) -> DVec3 {
        let width = self.width as f32;
        let x = f64::from(rng.next_float() * width * 2.0) - self.width;
        let y = lift + f64::from(rng.next_float() * self.height as f32);
        let z = f64::from(rng.next_float() * width * 2.0) - self.width;
        self.feet + DVec3::new(x, y, z)
    }
}

/// A slow drift with a gaussian of `0.02` on each axis.
fn drift(rng: &mut JavaRandom) -> DVec3 {
    DVec3::new(
        rng.next_gaussian() * 0.02,
        rng.next_gaussian() * 0.02,
        rng.next_gaussian() * 0.02,
    )
}

/// A blaze's smoke: anywhere in its box, still (`EntityBlaze`, line 115).
fn blaze_smoke(rng: &mut JavaRandom, body: &MobBody) -> (DVec3, DVec3) {
    (body.inside(rng), DVec3::ZERO)
}

/// A portal particle of an enderman (`sink` 0.25) or an endermite (`sink` 0)
/// drawn back towards the body (`EntityEnderman`, line 157).
fn portal(rng: &mut JavaRandom, body: &MobBody, sink: f64) -> (DVec3, DVec3) {
    let position = body.inside(rng) - DVec3::new(0.0, sink, 0.0);
    let velocity = DVec3::new(
        (rng.next_double() - 0.5) * 2.0,
        -rng.next_double(),
        (rng.next_double() - 0.5) * 2.0,
    );
    (position, velocity)
}

/// A point within a gaussian of 0.3 of a wither head.
fn near_head(rng: &mut JavaRandom, head: DVec3) -> DVec3 {
    let jitter = 0.300_000_011_920_928_96;
    head + DVec3::new(
        rng.next_gaussian() * jitter,
        rng.next_gaussian() * jitter,
        rng.next_gaussian() * jitter,
    )
}

/// The `(r, g, b)` of a potion colour `0xRRGGBB`, `0..=1`, which the spell
/// particles take as their velocity.
fn potion_rgb(color: i32) -> DVec3 {
    let channel = |shift: i32| f64::from((color >> shift) & 0xFF) / 255.0;
    DVec3::new(channel(16), channel(8), channel(0))
}

/// The kind and lift of the burst a status asks of `kind`, and how many
/// particles it has (`handleStatusUpdate` of `EntityAnimal` (18),
/// `EntityVillager` (12, 13, 14), `EntityTameable` and `EntityHorse` (6, 7)).
fn status_burst(kind: EntityKind, status: i8) -> Option<(ParticleKind, f64, u32)> {
    let tameable = matches!(
        kind,
        EntityKind::Wolf | EntityKind::Ocelot | EntityKind::Horse
    );
    match status {
        6 if tameable => Some((ParticleKind::Smoke, 0.5, 7)),
        7 if tameable => Some((ParticleKind::Heart, 0.5, 7)),
        12 if kind == EntityKind::Villager => Some((ParticleKind::Heart, 1.0, 5)),
        13 if kind == EntityKind::Villager => Some((ParticleKind::AngryVillager, 1.0, 5)),
        14 if kind == EntityKind::Villager => Some((ParticleKind::HappyVillager, 1.0, 5)),
        18 if is_animal(kind) => Some((ParticleKind::Heart, 0.5, 7)),
        _ => None,
    }
}

/// The subclasses of `EntityAnimal`, which show hearts when in love.
const fn is_animal(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Cow
            | EntityKind::Mooshroom
            | EntityKind::Pig
            | EntityKind::Sheep
            | EntityKind::Chicken
            | EntityKind::Wolf
            | EntityKind::Ocelot
            | EntityKind::Horse
            | EntityKind::Rabbit
    )
}

impl ParticleEngine {
    /// Creates the particles `entity` gives off this tick: the smoke of a
    /// blaze, the portal sparks of an enderman or endermite, the smoke and
    /// spells of a wither, the dust under a walking iron golem, the flames of
    /// a landing magma cube, the swirls of its potion effects and the
    /// explosions of a dying dragon. Call it once per tick after the entities
    /// moved; `world` gives the blocks a golem treads on.
    pub fn spawn_mob_tick<S: BlockSource + ?Sized>(
        &mut self,
        entity: &EntityRecord,
        world: &S,
        filter: &SpawnFilter,
    ) {
        if entity.kind == EntityKind::Dragon {
            self.dragon_explosions(entity, filter);
            return;
        }
        let Some(body) = MobBody::of(entity) else {
            return;
        };
        match entity.kind {
            EntityKind::Blaze => {
                for _ in 0..2 {
                    let (position, velocity) = blaze_smoke(self.rng(), &body);
                    self.emit(ParticleKind::LargeSmoke, position, velocity, filter);
                }
            }
            EntityKind::Enderman | EntityKind::Endermite => {
                let sink = if entity.kind == EntityKind::Enderman {
                    0.25
                } else {
                    0.0
                };
                for _ in 0..2 {
                    let (position, velocity) = portal(self.rng(), &body, sink);
                    self.emit(ParticleKind::Portal, position, velocity, filter);
                }
            }
            EntityKind::Wither => self.wither_breath(entity, &body, filter),
            EntityKind::IronGolem => self.golem_dust(entity, &body, world, filter),
            EntityKind::Guardian => self.guardian_wake(entity, &body, filter),
            EntityKind::Wolf => self.wolf_spray(entity, &body, filter),
            EntityKind::Slime | EntityKind::MagmaCube if entity.squish.just_landed() => {
                self.slime_landing(entity, &body, filter);
            }
            _ => {}
        }
        if entity.kind.is_living() {
            self.potion_swirl(entity, &body, filter);
        }
        if entity.kind == EntityKind::Player {
            self.eating_crumbs(entity, filter);
        }
    }

    /// Creates the particles of an `Entity Status`: the dust of a rabbit's hop
    /// (1), the spawner puff (20), the witch's magic (15), the hearts of an
    /// animal in love (18), of a villager (12, 13, 14) and of a taming (6, 7).
    pub fn spawn_mob_status<S: BlockSource + ?Sized>(
        &mut self,
        entity: &EntityRecord,
        status: i8,
        world: &S,
        filter: &SpawnFilter,
    ) {
        let Some(body) = MobBody::of(entity) else {
            return;
        };
        if status == 1 && entity.kind == EntityKind::Rabbit {
            self.rabbit_hop_dust(entity, &body, world, filter);
        } else if status == 20 && entity.kind.is_living() {
            self.spawner_puff(&body, filter);
        } else if status == 15 && entity.kind == EntityKind::Witch {
            self.witch_magic(&body, filter);
        } else if let Some((kind, lift, count)) = status_burst(entity.kind, status) {
            for _ in 0..count {
                let velocity = drift(self.rng());
                let position = body.around(self.rng(), lift);
                self.emit(kind, position, velocity, filter);
            }
        }
    }

    /// The puff of a body whose death fall ended: 20 explosions over its box
    /// (`EntityLivingBase.onDeathUpdate`, at `deathTime == 20`).
    pub fn spawn_death_puff(&mut self, body: &MobBody, filter: &SpawnFilter) {
        for _ in 0..20 {
            let velocity = drift(self.rng());
            let position = body.around(self.rng(), 0.0);
            self.emit(ParticleKind::Explode, position, velocity, filter);
        }
    }

    /// `EntityRabbit.handleStatusUpdate(1)`: a hop kicks up the block under
    /// the rabbit, as a sprinting entity does (`Entity.createRunningParticles`).
    fn rabbit_hop_dust<S: BlockSource + ?Sized>(
        &mut self,
        entity: &EntityRecord,
        body: &MobBody,
        world: &S,
        filter: &SpawnFilter,
    ) {
        let running = RunningBody {
            position: body.feet,
            width: body.width,
            motion: entity.velocity.as_dvec3(),
        };
        let mut argument = [0];
        if let Some(request) = running_dust(world, &running, &mut argument, self.rng()) {
            self.spawn(&request, filter);
        }
    }

    /// `EntityWolf.onUpdate`: a shaking wolf sprays water drops from 0.4 into
    /// the shake, up to 7 a tick, 0.8 above its feet and with its own motion.
    fn wolf_spray(&mut self, entity: &EntityRecord, body: &MobBody, filter: &SpawnFilter) {
        let Some(time) = entity.mob.shake_time().filter(|&time| time > 0.4) else {
            return;
        };
        let drops = ((time - 0.4) * std::f32::consts::PI).sin() * 7.0;
        for _ in 0..drops as u32 {
            let rng = self.rng();
            let across = |draw: f32| f64::from((draw * 2.0 - 1.0) * body.width as f32 * 0.5);
            let x = across(rng.next_float());
            let z = across(rng.next_float());
            let position = body.feet + DVec3::new(x, 0.8, z);
            self.emit(
                ParticleKind::Splash,
                position,
                entity.velocity.as_dvec3(),
                filter,
            );
        }
    }

    /// `EntityLiving.spawnExplosionParticle`: 20 explosions, each started back
    /// along its drift by ten ticks.
    fn spawner_puff(&mut self, body: &MobBody, filter: &SpawnFilter) {
        for _ in 0..20 {
            let velocity = drift(self.rng());
            let position = body.around(self.rng(), 0.0) - velocity * 10.0;
            self.emit(ParticleKind::Explode, position, velocity, filter);
        }
    }

    /// `EntityWitch.handleStatusUpdate(15)`: 10 to 44 sparks half a block
    /// above the hat.
    fn witch_magic(&mut self, body: &MobBody, filter: &SpawnFilter) {
        let spread = 0.129_999_995_231_628_42;
        let top = body.feet.y + body.height + 0.5;
        // The bound is drawn again at every turn of the reference's loop.
        let mut drawn = 0;
        while drawn < self.rng().next_int(35) + 10 {
            let rng = self.rng();
            let position = DVec3::new(
                body.feet.x + rng.next_gaussian() * spread,
                top + rng.next_gaussian() * spread,
                body.feet.z + rng.next_gaussian() * spread,
            );
            self.emit(ParticleKind::WitchMagic, position, DVec3::ZERO, filter);
            drawn += 1;
        }
    }

    /// The end of `EntityWither.onLivingUpdate`: smoke around each head, a
    /// spell now and then once armoured, and spells around the body while it
    /// is being spawned.
    fn wither_breath(&mut self, entity: &EntityRecord, body: &MobBody, filter: &SpawnFilter) {
        let armored = entity.meta.living.health <= WITHER_ARMORED_HEALTH;
        let armour_tint = DVec3::new(0.699_999_988_079_071, 0.699_999_988_079_071, 0.5);
        for head in wither_head_positions(body.feet, entity.body_yaw.offset) {
            let position = near_head(self.rng(), head);
            self.emit(ParticleKind::Smoke, position, DVec3::ZERO, filter);
            if armored && self.rng().next_int(4) == 0 {
                let position = near_head(self.rng(), head);
                self.emit(ParticleKind::MobSpell, position, armour_tint, filter);
            }
        }
        let spawning =
            matches!(entity.meta.variant, Variant::Wither { invulnerable, .. } if invulnerable > 0);
        if spawning {
            let tint = DVec3::new(
                0.699_999_988_079_071,
                0.699_999_988_079_071,
                0.899_999_976_158_142_1,
            );
            for _ in 0..3 {
                let rng = self.rng();
                let position = body.feet
                    + DVec3::new(
                        rng.next_gaussian(),
                        f64::from(rng.next_float() * 3.3),
                        rng.next_gaussian(),
                    );
                self.emit(ParticleKind::MobSpell, position, tint, filter);
            }
        }
    }

    /// `EntityIronGolem.onLivingUpdate`: a moving golem breaks a fragment off
    /// the block under it one tick in five.
    fn golem_dust<S: BlockSource + ?Sized>(
        &mut self,
        entity: &EntityRecord,
        body: &MobBody,
        world: &S,
        filter: &SpawnFilter,
    ) {
        let motion = entity.velocity.as_dvec3();
        if motion.x * motion.x + motion.z * motion.z <= GOLEM_STEP_SPEED_SQUARED
            || self.rng().next_int(5) != 0
        {
            return;
        }
        let ground = BlockPos::new(
            body.feet.x.floor() as i32,
            (body.feet.y - GOLEM_PROBE_DEPTH).floor() as i32,
            body.feet.z.floor() as i32,
        );
        let state = world.block(ground.x, ground.y, ground.z);
        if state.is_air() {
            return;
        }
        // `Block.getStateId`: the id in the low 12 bits, the metadata above.
        let argument = [i32::from(state.id()) | (i32::from(state.metadata()) << 12)];
        let rng = self.rng();
        let position = DVec3::new(
            body.feet.x + (f64::from(rng.next_float()) - 0.5) * body.width,
            body.feet.y + 0.1,
            body.feet.z + (f64::from(rng.next_float()) - 0.5) * body.width,
        );
        let velocity = DVec3::new(
            4.0 * (f64::from(rng.next_float()) - 0.5),
            0.5,
            (f64::from(rng.next_float()) - 0.5) * 4.0,
        );
        self.spawn(
            &SpawnRequest {
                kind: ParticleKind::BlockCrack,
                arguments: &argument,
                ignore_range: false,
                position,
                velocity,
            },
            filter,
        );
    }

    /// `EntityGuardian.onLivingUpdate`: a guardian swimming along leaves two
    /// bubbles a tick, a block and a half behind where it looks.
    fn guardian_wake(&mut self, entity: &EntityRecord, body: &MobBody, filter: &SpawnFilter) {
        let moving = matches!(entity.meta.variant, Variant::Guardian { moving: true, .. });
        let in_water = matches!(entity.mob, MobState::Guardian(swim) if swim.in_water);
        if !moving || !in_water {
            return;
        }
        let behind = look_direction(entity.head_yaw.to_radians(), entity.pitch.to_radians()) * 1.5;
        for _ in 0..2 {
            let position = body.inside(self.rng()) - behind;
            self.emit(ParticleKind::Bubble, position, DVec3::ZERO, filter);
        }
    }

    /// `EntityGuardian.onLivingUpdate`: bubbles strung along the beam of a
    /// guardian to `end`, the middle of its target, closer together as the
    /// beam charges. Call it once per tick for each guardian with a target
    /// ([`crate::entity::EntityStore::beam_end`]).
    pub fn spawn_beam_bubbles(
        &mut self,
        guardian: &EntityRecord,
        end: DVec3,
        filter: &SpawnFilter,
    ) {
        let (MobState::Guardian(swim), Some(body)) = (guardian.mob, MobBody::of(guardian)) else {
            return;
        };
        // `getEyeHeight`: half its height.
        let eyes = body.feet + DVec3::Y * (body.height * 0.5);
        let charge = f64::from(swim.charge(0.0));
        let to_end = end - eyes;
        let distance = to_end.length();
        let direction = to_end.normalize_or_zero();
        let mut along = self.rng().next_double();
        // The step is taken before each bubble, so the last one can fall a
        // little past the target, as in the reference.
        while along < distance {
            along += 1.8 - charge + self.rng().next_double() * (1.7 - charge);
            self.emit(
                ParticleKind::Bubble,
                eyes + direction * along,
                DVec3::ZERO,
                filter,
            );
        }
    }

    /// `EntitySlime.onUpdate` for a slime that just landed: `8 * size`
    /// particles on a ring around its feet, pieces of slime ball for a slime
    /// (`EntitySlime.getParticleType`) and flames for a magma cube
    /// (`EntityMagmaCube.getParticleType`).
    fn slime_landing(&mut self, entity: &EntityRecord, body: &MobBody, filter: &SpawnFilter) {
        let size = match entity.meta.variant {
            Variant::Slime(size) => f32::from(size),
            _ => 1.0,
        };
        let landing = if entity.kind == EntityKind::Slime {
            ParticleKind::Slime
        } else {
            ParticleKind::Flame
        };
        for _ in 0..(8.0 * size) as u32 {
            let rng = self.rng();
            let angle = rng.next_float() * std::f32::consts::PI * 2.0;
            let reach = rng.next_float() * 0.5 + 0.5;
            let position = DVec3::new(
                body.feet.x + f64::from(angle.sin() * size * 0.5 * reach),
                body.feet.y,
                body.feet.z + f64::from(angle.cos() * size * 0.5 * reach),
            );
            self.emit(landing, position, DVec3::ZERO, filter);
        }
    }

    /// `EntityPlayer.updateItemUse` for a player another client sees eating:
    /// a bite of crumbs every fourth tick in the last quarter of the meal. The
    /// count of the reference (`32` at the start) is rebuilt from the ticks
    /// the use flag has been set (the flag shows one tick after the start).
    fn eating_crumbs(&mut self, entity: &EntityRecord, filter: &SpawnFilter) {
        let (Some(held), true) = (entity.held.as_ref(), entity.use_ticks > 0) else {
            return;
        };
        let stack = ItemStack {
            item_id: held.item_id,
            stack_size: 1,
            damage: held.damage,
            nbt: None,
        };
        if use_action(&stack) != Some(UseAction::Eat) {
            return;
        }
        let count = (UseAction::Eat.max_duration() + 1).saturating_sub(entity.use_ticks);
        if !is_bite_tick(count) {
            return;
        }
        // `EntityPlayer.getEyeHeight`: 1.62, lowered by 0.08 when sneaking.
        let eye = EYE_HEIGHT
            - if entity.flags.is_sneaking() {
                0.08
            } else {
                0.0
            };
        let eater = Eater {
            eyes: entity.position + DVec3::new(0.0, eye, 0.0),
            yaw: entity.yaw,
            pitch: entity.pitch,
        };
        let (id, damage) = (held.item_id.unsigned_abs(), held.damage.unsigned_abs());
        self.spawn_crumbs(&eater, id, damage, filter);
    }

    /// `EntityLivingBase.updatePotionEffects` on the client: a swirl in the
    /// colour of the effects, every other tick on average (one in 15 when
    /// invisible), five times rarer for beacon effects.
    fn potion_swirl(&mut self, entity: &EntityRecord, body: &MobBody, filter: &SpawnFilter) {
        let living = entity.meta.living;
        if living.potion_color <= 0 {
            return;
        }
        let rng = self.rng();
        let mut shown = if entity.flags.is_invisible() {
            rng.next_int(15) == 0
        } else {
            rng.next_int(2) == 0
        };
        if living.potion_ambient {
            shown &= rng.next_int(5) == 0;
        }
        if !shown {
            return;
        }
        let kind = if living.potion_ambient {
            ParticleKind::MobSpellAmbient
        } else {
            ParticleKind::MobSpell
        };
        let position = body.inside(self.rng());
        self.emit(kind, position, potion_rgb(living.potion_color), filter);
    }

    /// `EntityDragon`: a large explosion every tick once its health is gone
    /// (`onLivingUpdate`), and a huge one as well from tick 180 to 200 of its
    /// death (`onDeathUpdate`).
    fn dragon_explosions(&mut self, entity: &EntityRecord, filter: &SpawnFilter) {
        let Some(death_ticks) = entity.flight.as_ref().map(|flight| flight.death_ticks) else {
            return;
        };
        if death_ticks == 0 {
            return;
        }
        self.dragon_blast(ParticleKind::LargeExplode, entity.position, filter);
        if (180..=200).contains(&death_ticks) {
            self.dragon_blast(ParticleKind::HugeExplosion, entity.position, filter);
        }
    }

    /// One explosion within 4 blocks across and 2 up or down of a point 2
    /// blocks above the dragon.
    fn dragon_blast(&mut self, kind: ParticleKind, feet: DVec3, filter: &SpawnFilter) {
        let rng = self.rng();
        let offset = DVec3::new(
            f64::from((rng.next_float() - 0.5) * 8.0),
            2.0 + f64::from((rng.next_float() - 0.5) * 4.0),
            f64::from((rng.next_float() - 0.5) * 8.0),
        );
        self.emit(kind, feet + offset, DVec3::ZERO, filter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityId;
    use crate::particles::{Layer, ParticleSetting, Texture};
    use umicraft_world::BlockState;

    /// A world of stone below y = 64.
    struct Ground;

    impl BlockSource for Ground {
        fn block(&self, _x: i32, y: i32, _z: i32) -> BlockState {
            if y < 64 {
                BlockState::new(1, 0)
            } else {
                BlockState::AIR
            }
        }
    }

    fn mob(kind: EntityKind) -> EntityRecord {
        EntityRecord::new(EntityId(1), kind, DVec3::new(0.5, 64.0, 0.5), 0.0, 0.0)
    }

    fn filter() -> SpawnFilter {
        SpawnFilter {
            viewer: DVec3::new(0.5, 64.0, 0.5),
            setting: ParticleSetting::All,
        }
    }

    fn after_tick(entity: &EntityRecord) -> usize {
        let mut engine = ParticleEngine::new(7);
        engine.spawn_mob_tick(entity, &Ground, &filter());
        engine.len()
    }

    fn after_status(entity: &EntityRecord, status: i8) -> usize {
        let mut engine = ParticleEngine::new(7);
        engine.spawn_mob_status(entity, status, &Ground, &filter());
        engine.len()
    }

    #[test]
    fn a_blaze_smokes_and_an_enderman_sparks_every_tick() {
        assert_eq!(after_tick(&mob(EntityKind::Blaze)), 2);
        assert_eq!(after_tick(&mob(EntityKind::Enderman)), 2);
        assert_eq!(after_tick(&mob(EntityKind::Endermite)), 2);
        assert_eq!(after_tick(&mob(EntityKind::Zombie)), 0);
    }

    #[test]
    fn blaze_smoke_stays_inside_the_box() {
        let body = MobBody {
            feet: DVec3::new(10.0, 64.0, -3.0),
            width: 0.6,
            height: 1.8,
        };
        let mut rng = JavaRandom::new(3);
        for _ in 0..100 {
            let (position, velocity) = blaze_smoke(&mut rng, &body);
            let offset = position - body.feet;
            assert!(offset.x.abs() <= 0.3 && offset.z.abs() <= 0.3, "{offset}");
            assert!((0.0..=1.8).contains(&offset.y), "{offset}");
            assert_eq!(velocity, DVec3::ZERO);
        }
    }

    #[test]
    fn enderman_sparks_start_lower_and_fall_back() {
        let body = MobBody {
            feet: DVec3::ZERO,
            width: 0.6,
            height: 2.9,
        };
        let mut rng = JavaRandom::new(5);
        for _ in 0..100 {
            let (position, velocity) = portal(&mut rng, &body, 0.25);
            assert!(position.y >= -0.25 && position.y <= 2.65, "{position}");
            assert!(velocity.y <= 0.0 && velocity.x.abs() <= 1.0, "{velocity}");
        }
    }

    #[test]
    fn a_wither_breathes_smoke_from_its_three_heads() {
        let mut wither = mob(EntityKind::Wither);
        wither.meta.living.health = 300.0;
        assert_eq!(after_tick(&wither), 3);
        // The side heads sit 1.3 to either side of the centre one.
        let [centre, left, right] = wither_head_positions(DVec3::ZERO, 0.0);
        assert_eq!(centre, DVec3::new(0.0, 3.0, 0.0));
        assert!((left - DVec3::new(1.3, 2.2, 0.0)).length() < 1e-6, "{left}");
        assert!(
            (right - DVec3::new(-1.3, 2.2, 0.0)).length() < 1e-5,
            "{right}"
        );
    }

    #[test]
    fn a_spawning_wither_glows_with_spells() {
        let mut wither = mob(EntityKind::Wither);
        wither.meta.living.health = 300.0;
        wither.meta.variant = Variant::Wither {
            invulnerable: 200,
            targets: [0; 2],
        };
        assert_eq!(after_tick(&wither), 6);
    }

    #[test]
    fn a_walking_iron_golem_kicks_up_the_ground_now_and_then() {
        let mut golem = mob(EntityKind::IronGolem);
        golem.velocity = glam::Vec3::new(0.1, 0.0, 0.0);
        let mut engine = ParticleEngine::new(1);
        for _ in 0..200 {
            engine.spawn_mob_tick(&golem, &Ground, &filter());
        }
        // One tick in five on average.
        assert!((20..=70).contains(&engine.len()), "{}", engine.len());
        golem.velocity = glam::Vec3::ZERO;
        assert_eq!(after_tick(&golem), 0);
    }

    #[test]
    fn a_guardian_swimming_in_water_leaves_bubbles_behind() {
        let mut guardian = mob(EntityKind::Guardian);
        guardian.meta.variant = Variant::Guardian {
            moving: true,
            elder: false,
            target: 0,
        };
        assert_eq!(after_tick(&guardian), 0, "not in water yet");
        if let MobState::Guardian(swim) = &mut guardian.mob {
            swim.tick(true, true);
        }
        assert_eq!(after_tick(&guardian), 2);
    }

    #[test]
    fn a_charging_beam_is_strung_with_bubbles_up_to_its_target() {
        let mut guardian = mob(EntityKind::Guardian);
        guardian.meta.variant = Variant::Guardian {
            moving: false,
            elder: false,
            target: 2,
        };
        let eyes = guardian.position + DVec3::Y * (f64::from(0.85_f32) * 0.5);
        let end = eyes + DVec3::new(10.0, 0.0, 0.0);
        let mut engine = ParticleEngine::new(7);
        engine.spawn_beam_bubbles(&guardian, end, &filter());
        // A step of 1.8 to 3.5 blocks at the start of the charge: 3 to 6.
        let start = engine.len();
        assert!((3..=6).contains(&start), "{start}");
        let bubbles = engine.particles(Layer::Misc, false);
        assert_eq!(bubbles.len(), start);
        for bubble in bubbles {
            let offset = bubble.position - eyes;
            assert!(
                offset.y.abs() < 1e-9 && offset.z.abs() < 1e-9,
                "on the beam"
            );
            assert!(offset.x > 0.0 && offset.x < 13.5, "{offset}");
        }
        // Fully charged the step is 0.8 to 1.5 blocks: more bubbles.
        if let MobState::Guardian(swim) = &mut guardian.mob {
            for _ in 0..80 {
                swim.aim(2, false);
            }
        }
        let mut engine = ParticleEngine::new(7);
        engine.spawn_beam_bubbles(&guardian, end, &filter());
        assert!(engine.len() > start + 2, "{} vs {start}", engine.len());
    }

    #[test]
    fn a_slime_throws_pieces_of_slime_ball_when_it_lands() {
        // Regression: the slime had no landing particles (no `Slime` factory).
        let mut slime = mob(EntityKind::Slime);
        slime.meta.variant = Variant::Slime(1);
        slime.squish.tick(false);
        slime.squish.tick(true);
        let mut engine = ParticleEngine::new(7);
        engine.spawn_mob_tick(&slime, &Ground, &filter());
        assert_eq!(engine.len(), 8);
        let pieces = engine.particles(Layer::Terrain, false);
        assert!(pieces
            .iter()
            .all(|p| matches!(p.texture, Texture::Item { .. })));
    }

    #[test]
    fn a_magma_cube_throws_flames_when_it_lands() {
        let mut cube = mob(EntityKind::MagmaCube);
        cube.meta.variant = Variant::Slime(2);
        cube.squish.tick(false);
        assert_eq!(after_tick(&cube), 0);
        cube.squish.tick(true);
        assert_eq!(after_tick(&cube), 16);
        cube.squish.tick(true);
        assert_eq!(after_tick(&cube), 0, "only on the landing tick");
    }

    #[test]
    fn potion_effects_swirl_in_their_colour() {
        let mut zombie = mob(EntityKind::Zombie);
        zombie.meta.living.potion_color = 0x0033_66FF;
        let mut engine = ParticleEngine::new(2);
        for _ in 0..100 {
            engine.spawn_mob_tick(&zombie, &Ground, &filter());
        }
        assert!((30..=70).contains(&engine.len()), "{}", engine.len());
        let rgb = potion_rgb(0x0033_66FF);
        assert!((rgb - DVec3::new(0.2, 0.4, 1.0)).length() < 1e-9, "{rgb}");
    }

    #[test]
    fn the_statuses_show_their_bursts_on_the_right_mobs() {
        assert_eq!(after_status(&mob(EntityKind::Cow), 18), 7);
        assert_eq!(after_status(&mob(EntityKind::Zombie), 18), 0);
        assert_eq!(after_status(&mob(EntityKind::Villager), 13), 5);
        assert_eq!(after_status(&mob(EntityKind::Wolf), 7), 7);
        assert_eq!(after_status(&mob(EntityKind::Horse), 6), 7);
        assert_eq!(after_status(&mob(EntityKind::Pig), 7), 0);
        assert_eq!(after_status(&mob(EntityKind::Skeleton), 20), 20);
        let witch = after_status(&mob(EntityKind::Witch), 15);
        assert!((10..=44).contains(&witch), "{witch}");
    }

    #[test]
    fn a_hopping_rabbit_kicks_up_the_ground_under_it() {
        // Regression: status 1 only started the hop, with no dust.
        let rabbit = mob(EntityKind::Rabbit);
        assert_eq!(after_status(&rabbit, 1), 1);
        let mut engine = ParticleEngine::new(7);
        let floating = mob(EntityKind::Rabbit);
        struct Air;
        impl BlockSource for Air {
            fn block(&self, _: i32, _: i32, _: i32) -> BlockState {
                BlockState::AIR
            }
        }
        engine.spawn_mob_status(&floating, 1, &Air, &filter());
        assert!(engine.is_empty(), "nothing to kick up over air");
    }

    #[test]
    fn a_shaking_wolf_sprays_water_after_the_first_fifth_of_its_shake() {
        let mut wolf = mob(EntityKind::Wolf);
        wolf.mob.status(8);
        let mut sprayed = Vec::new();
        for _ in 0..40 {
            wolf.mob.tick(crate::entity::MobTick::default());
            sprayed.push(after_tick(&wolf));
        }
        // `timeWolfIsShaking` passes 0.4 on the ninth tick; the spray swells
        // to 7 drops about 0.9 units in and fades by 1.4.
        assert!(sprayed[..8].iter().all(|&n| n == 0), "{sprayed:?}");
        assert!(sprayed[8..].iter().any(|&n| n >= 6), "{sprayed:?}");
        assert_eq!(*sprayed.last().unwrap(), 0, "over by 1.4 + 0.4");
    }

    #[test]
    fn a_player_eating_throws_crumbs_every_fourth_tick_near_the_end() {
        // Regression: no player showed crumbs while eating.
        let mut player = mob(EntityKind::Player);
        let bread = |id| crate::entity::EntityItem {
            item_id: id,
            damage: 0,
            count: 1,
            nbt: None,
        };
        player.held = Some(bread(297));
        let crumbs: Vec<usize> = (0..=32)
            .map(|ticks| {
                player.use_ticks = ticks;
                after_tick(&player)
            })
            .collect();
        // Bites at 9, 13, 17, 21, 25 and 29 ticks into the meal (24 to 4 left).
        let bites: Vec<usize> = crumbs
            .iter()
            .enumerate()
            .filter(|&(_, &count)| count == 5)
            .map(|(ticks, _)| ticks)
            .collect();
        assert_eq!(bites, [9, 13, 17, 21, 25, 29]);
        assert_eq!(crumbs.iter().sum::<usize>(), 30);
        // A sword is held up, not eaten.
        player.held = Some(bread(267));
        player.use_ticks = 9;
        assert_eq!(after_tick(&player), 0);
    }

    #[test]
    fn a_death_puff_is_twenty_explosions_seen_from_afar() {
        let mut engine = ParticleEngine::new(4);
        let far = SpawnFilter {
            viewer: DVec3::new(500.0, 64.0, 0.0),
            setting: ParticleSetting::All,
        };
        let body = MobBody::of(&mob(EntityKind::Cow)).expect("a cow has a body");
        engine.spawn_death_puff(&body, &far);
        assert_eq!(engine.len(), 20);
    }

    #[test]
    fn a_dying_dragon_explodes_and_bursts_at_the_end() {
        let mut dragon = mob(EntityKind::Dragon);
        dragon.flight = Some(crate::entity::flight::DragonFlight::default());
        assert_eq!(after_tick(&dragon), 0, "alive");
        if let Some(flight) = dragon.flight.as_mut() {
            flight.death_ticks = 10;
        }
        assert_eq!(after_tick(&dragon), 1);
        if let Some(flight) = dragon.flight.as_mut() {
            flight.death_ticks = 190;
        }
        assert_eq!(after_tick(&dragon), 2);
    }
}
