// SPDX-License-Identifier: MIT OR Apache-2.0

//! Interpolation data shipped to the render thread, and the snapshots it yields.

use std::sync::Arc;

use glam::{DVec3, Vec3};
use umicraft_world::LightLevel;

use super::angle::lerp_degrees;
use super::anim::{Animation, BedPose, SquidPose};
use super::cape::{cape_angles, CapeAngles};
use super::flags::EntityFlags;
use super::flight::{DragonFlight, DragonPose};
use super::guardian_beam::BeamTarget;
use super::horse_state::{HorsePose, HorseState};
use super::item::EntityItem;
use super::kind::{EntityId, EntityKind};
use super::leash::{LeashAnchor, LeashFrame};
use super::limbs::LimbSwing;
use super::meta::EntityMeta;
use super::mob_state::{MobPose, MobState};
use super::profile::PlayerProfile;
use super::shadow::ShadowBlock;

/// Per-entity interpolation data sent to the render thread each tick.
///
/// Carries the previous and current tick states so the render thread can
/// produce smoothly interpolated snapshots at any sub-tick fraction.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityFrameData {
    /// Unique entity ID.
    pub id: EntityId,
    /// Entity category.
    pub kind: EntityKind,
    /// The world light at the entity (see [`super::light`]); full sky until
    /// the game thread samples it.
    pub light: LightLevel,
    /// Position at the previous simulation tick, in f64: the render thread
    /// subtracts its origin before any cast to f32 (F2-1b).
    pub prev_position: DVec3,
    /// Position at the current simulation tick.
    pub position: DVec3,
    /// Previous facing direction in degrees.
    pub prev_yaw: f32,
    /// Current facing direction in degrees.
    pub yaw: f32,
    /// Previous pitch in degrees.
    pub prev_pitch: f32,
    /// Current pitch in degrees.
    pub pitch: f32,
    /// Previous head yaw in degrees.
    pub prev_head_yaw: f32,
    /// Current head yaw in degrees.
    pub head_yaw: f32,
    /// Previous torso yaw in degrees.
    pub prev_body_yaw: f32,
    /// Current torso yaw in degrees.
    pub body_yaw: f32,
    /// The walking animation.
    pub limbs: LimbSwing,
    /// The hurt flash, death fall, arm swing and bed.
    pub animation: Animation,
    /// The typed metadata.
    pub meta: EntityMeta,
    /// The cape position one tick ago.
    pub prev_cape_chase: DVec3,
    /// The cape position now.
    pub cape_chase: DVec3,
    /// The text of the name tag above the entity.
    pub name_tag: Option<Arc<str>>,
    /// The profile of a player.
    pub profile: Option<Arc<PlayerProfile>>,
    /// Player UUID for skin lookup.
    pub uuid: Option<[u8; 16]>,
    /// The stack in the hand (players).
    pub held: Option<EntityItem>,
    /// The worn armour, boots first.
    pub armor: [Option<EntityItem>; 4],
    /// Whether the player is in spectator mode: only the head is drawn.
    pub spectator: bool,
    /// The `data` of the spawn packet of an object.
    pub object_data: i32,
    /// The blocks the shadow of the entity falls on (the game thread finds
    /// them from the world).
    pub shadow: Arc<[ShadowBlock]>,
    /// The dropped stack (items) or the stack inside the frame (item frames).
    pub item: Option<EntityItem>,
    /// Ticks the held item has been in use.
    pub use_ticks: u32,
    /// Whether the player has a bobber out.
    pub fishing: bool,
    /// Whether the entity sits on a vehicle.
    pub riding: bool,
    /// The flight history of an ender dragon.
    pub flight: Option<DragonFlight>,
    /// The squish factor of a slime or magma cube.
    pub squish: f32,
    /// The grazing, rearing and mouth of a horse.
    pub horse: Option<HorseState>,
    /// The timers of a hop, a swing, a shake or a grazing.
    pub mob: MobState,
    /// Whether another entity sits on this one.
    pub ridden: bool,
    /// The light in front of each block of a painting, column by column (the
    /// game thread samples it; empty for anything else).
    pub block_lights: Arc<[LightLevel]>,
    /// Where the line of this bobber ties to the rod when the angler is the
    /// local player in first person; set by the game thread.
    pub rod_tip: Option<DVec3>,
    /// Whether this is the body of the local player.
    pub local: bool,
    /// Whether the viewer sees the invisible body as a ghost by team.
    pub ghost_visible: bool,
    /// Whether the dragon is alive: a dead one shows its history frozen.
    pub flight_alive: bool,
    /// Who holds the leash of the entity, if it is leashed.
    pub leash_holder: Option<EntityId>,
    /// Where the holder is, found by [`super::leash::leash_entities`].
    pub leash: Option<LeashFrame>,
    /// What a guardian beams at, found by
    /// [`super::guardian_beam::aim_guardians`].
    pub beam: Option<BeamTarget>,
    /// Ticks lived at the current simulation tick.
    pub age_ticks: u32,
    /// Wall direction of an item frame: 0 south, 1 west, 2 north, 3 east.
    pub frame_direction: u8,
    /// Rotation step (0..=7) of the item inside an item frame.
    pub frame_rotation: u8,
    /// Entity visual flags.
    pub flags: EntityFlags,
}

impl EntityFrameData {
    /// Produces an interpolated render snapshot at the given sub-tick fraction.
    ///
    /// `alpha` is in `0.0..=1.0` where 0 is the previous tick and 1 the current.
    /// Every position of the snapshot is relative to `origin` (the render
    /// origin of the camera; pass `DVec3::ZERO` for world coordinates): the
    /// interpolation and the subtraction happen in f64, so an entity far from
    /// the world origin keeps its smooth motion (F2-1b).
    #[must_use]
    pub fn interpolate(&self, alpha: f32, origin: DVec3) -> EntityRenderSnapshot {
        let a = alpha.clamp(0.0, 1.0);
        let wide = f64::from(a);
        let world_position = self.prev_position.lerp(self.position, wide);
        let position = (world_position - origin).as_vec3();
        let yaw = lerp_degrees(self.prev_yaw, self.yaw, a);
        let body_yaw = lerp_degrees(self.prev_body_yaw, self.body_yaw, a);
        let pitch = self.prev_pitch + (self.pitch - self.prev_pitch) * a;
        let head_yaw = lerp_degrees(self.prev_head_yaw, self.head_yaw, a);

        EntityRenderSnapshot {
            id: self.id,
            kind: self.kind,
            light: self.light,
            origin,
            position,
            yaw,
            body_yaw,
            pitch,
            head_yaw,
            limb_swing: self.limbs.swing_at(a),
            limb_swing_amount: self.limbs.amount_at(a),
            swing_progress: self.animation.swing_progress_at(a),
            fuse_flash: self.animation.fuse_flash_at(a),
            hurt: self.animation.is_flashing(),
            death_time: self.animation.death_time_at(a),
            bed: self.animation.bed,
            meta: self.meta.clone(),
            cape: cape_angles(
                (self.prev_cape_chase.lerp(self.cape_chase, wide) - world_position).as_vec3(),
                body_yaw,
                self.flags.is_sneaking(),
            ),
            name_tag: self.name_tag.clone(),
            profile: self.profile.clone(),
            uuid: self.uuid,
            held: self.held.clone(),
            armor: self.armor.clone(),
            spectator: self.spectator,
            object_data: self.object_data,
            shadow: self.shadow.clone(),
            item: self.item.clone(),
            use_ticks: self.use_ticks,
            fishing: self.fishing,
            riding: self.riding,
            leash: self.leash.map(|holder| holder.at(a, origin)),
            beam: self.beam.map(|target| target.at(a, origin)),
            local: self.local,
            ghost_visible: self.ghost_visible,
            squish: self.squish,
            squid: self.animation.squid_at(a),
            horse: self.horse.map(|horse| horse.pose(a)),
            mob: self.mob.pose(a),
            ridden: self.ridden,
            block_lights: self.block_lights.clone(),
            rod_tip: self.rod_tip.map(|tip| (tip - origin).as_vec3()),
            flight: self
                .flight
                .as_ref()
                .map(|flight| flight.pose(a, self.flight_alive)),
            age: self.age_ticks as f32 + a,
            frame_direction: self.frame_direction,
            frame_rotation: self.frame_rotation,
            flags: self.flags,
        }
    }
}

/// Immutable snapshot of one entity prepared for the render thread.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityRenderSnapshot {
    /// Unique entity ID.
    pub id: EntityId,
    /// Entity category.
    pub kind: EntityKind,
    /// The world light the entity is drawn with.
    pub light: LightLevel,
    /// The world position that `(0, 0, 0)` of this snapshot stands for: every
    /// position below is relative to it. Zero outside the renderer.
    pub origin: DVec3,
    /// Interpolated position, relative to [`Self::origin`].
    pub position: Vec3,
    /// Interpolated facing direction in degrees.
    pub yaw: f32,
    /// Interpolated torso yaw in degrees (MCP `renderYawOffset`); the model
    /// is drawn rotated by it, and the head turns by `head_yaw - body_yaw`.
    pub body_yaw: f32,
    /// Interpolated pitch in degrees.
    pub pitch: f32,
    /// Interpolated head yaw in degrees.
    pub head_yaw: f32,
    /// Interpolated limb swing progress in radians.
    pub limb_swing: f32,
    /// Interpolated limb swing amplitude (0.0 to 1.0).
    pub limb_swing_amount: f32,
    /// Arm swing progress `0..=1` (MCP `getSwingProgress`).
    pub swing_progress: f32,
    /// The creeper fuse flash, `0..=1` (above 1 at the end of the fuse).
    pub fuse_flash: f32,
    /// True while the red hurt or death flash shows.
    pub hurt: bool,
    /// Ticks into the death fall plus the partial tick, `0` while alive.
    pub death_time: f32,
    /// The bed the entity sleeps in.
    pub bed: Option<BedPose>,
    /// The typed metadata: variant, age, name, health.
    pub meta: EntityMeta,
    /// The swing of the cape.
    pub cape: CapeAngles,
    /// The text of the name tag above the entity, with formatting codes.
    pub name_tag: Option<Arc<str>>,
    /// The profile of a player: name, skin and cape.
    pub profile: Option<Arc<PlayerProfile>>,
    /// Player UUID for skin lookup.
    pub uuid: Option<[u8; 16]>,
    /// The stack in the hand (players).
    pub held: Option<EntityItem>,
    /// The worn armour, boots first.
    pub armor: [Option<EntityItem>; 4],
    /// Whether the player is in spectator mode: only the head is drawn.
    pub spectator: bool,
    /// The `data` of the spawn packet of an object.
    pub object_data: i32,
    /// The blocks the shadow of the entity falls on (the game thread finds
    /// them from the world).
    pub shadow: Arc<[ShadowBlock]>,
    /// The dropped stack (items) or the stack inside the frame (item frames).
    pub item: Option<EntityItem>,
    /// Ticks the held item has been in use: picks the bow drawing model.
    pub use_ticks: u32,
    /// Whether the player has a bobber out: the held rod shows its cast model.
    pub fishing: bool,
    /// Whether the entity sits on a vehicle: a humanoid sits with its legs forward.
    pub riding: bool,
    /// The holder of the leash of the entity, if it is leashed.
    pub leash: Option<LeashAnchor>,
    /// The middle of what a guardian beams at, relative to [`Self::origin`].
    pub beam: Option<Vec3>,
    /// The flight pose of an ender dragon: its wing phase and the path it flew.
    pub flight: Option<DragonPose>,
    /// How much a slime or magma cube is squashed (`0` at rest, positive when
    /// stretched, negative when it lands).
    pub squish: f32,
    /// The tentacle angle and the body turn of a squid.
    pub squid: SquidPose,
    /// How far a horse grazes, rears and opens its mouth.
    pub horse: Option<HorsePose>,
    /// What a hop, a swing, a shake or a grazing does to the model.
    pub mob: MobPose,
    /// Whether another entity sits on this one: a ridden horse shows its reins.
    pub ridden: bool,
    /// The light in front of each block of a painting, column by column; empty
    /// when the whole entity takes `light`.
    pub block_lights: Arc<[LightLevel]>,
    /// Where the line of this bobber ties to the rod (first person only),
    /// relative to [`Self::origin`].
    pub rod_tip: Option<Vec3>,
    /// Whether this is the body of the local player: it is not lowered further
    /// when crouching (`RenderPlayer.doRender`).
    pub local: bool,
    /// Whether the viewer sees this invisible body as a ghost: a team mate of
    /// the local player in a team that sees invisible team mates (a
    /// spectator sees every one, see `EntityTextures::set_viewer_spectator`).
    pub ghost_visible: bool,
    /// Ticks lived plus the interpolation alpha, for the bob and spin of items.
    pub age: f32,
    /// Wall direction of an item frame: 0 south, 1 west, 2 north, 3 east.
    pub frame_direction: u8,
    /// Rotation step (0..=7) of the item inside an item frame.
    pub frame_rotation: u8,
    /// Entity visual flags.
    pub flags: EntityFlags,
}

impl EntityRenderSnapshot {
    /// A still, upright snapshot of `kind` at `position`, facing south
    /// (yaw 0), with no item and no flags.
    #[must_use]
    pub fn new(id: EntityId, kind: EntityKind, position: Vec3) -> Self {
        Self {
            id,
            kind,
            light: LightLevel::FULL_SKY,
            origin: DVec3::ZERO,
            position,
            yaw: 0.0,
            body_yaw: 0.0,
            pitch: 0.0,
            head_yaw: 0.0,
            limb_swing: 0.0,
            limb_swing_amount: 0.0,
            swing_progress: 0.0,
            fuse_flash: 0.0,
            hurt: false,
            death_time: 0.0,
            bed: None,
            meta: EntityMeta::default(),
            cape: CapeAngles::default(),
            name_tag: None,
            profile: None,
            uuid: None,
            held: None,
            item: None,
            armor: [None, None, None, None],
            spectator: false,
            object_data: 0,
            shadow: Arc::default(),
            use_ticks: 0,
            fishing: false,
            riding: false,
            leash: None,
            beam: None,
            flight: None,
            squish: 0.0,
            squid: Default::default(),
            horse: None,
            mob: MobPose::None,
            ridden: false,
            block_lights: Arc::default(),
            rod_tip: None,
            local: false,
            ghost_visible: false,
            age: 0.0,
            frame_direction: 0,
            frame_rotation: 0,
            flags: EntityFlags::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityRecord;

    fn walker(from: f64, to: f64) -> EntityFrameData {
        let mut record = EntityRecord::new(
            EntityId(1),
            EntityKind::Pig,
            DVec3::new(to, 64.0, 1_000_000.25),
            0.0,
            0.0,
        );
        record.previous_position = DVec3::new(from, 64.0, 1_000_000.25);
        record.frame_data()
    }

    #[test]
    fn an_entity_far_from_the_origin_moves_smoothly() {
        // F2-1b regression: a pig walks 5 cm per tick at x = 1 000 000. An f32
        // position has a 6 cm grain there, so the steps used to vanish and
        // the pig advanced in jumps; relative to the origin they stay exact.
        let origin = DVec3::new(1_000_000.0, 64.0, 1_000_000.0);
        let frame = walker(1_000_000.40, 1_000_000.45);
        let x_at = |alpha: f32| frame.interpolate(alpha, origin).position.x;
        let steps: Vec<f32> = (0..=5).map(|n| x_at(n as f32 / 5.0)).collect();
        for pair in steps.windows(2) {
            assert!((pair[1] - pair[0] - 0.01).abs() < 1e-5, "{steps:?}");
        }
        assert!((steps[0] - 0.40).abs() < 1e-5);
    }

    #[test]
    fn the_origin_is_carried_and_subtracted_from_every_position() {
        let origin = DVec3::new(1_000_000.0, 60.0, 1_000_000.0);
        let mut frame = walker(1_000_001.0, 1_000_002.0);
        frame.rod_tip = Some(DVec3::new(1_000_003.5, 65.0, 1_000_000.5));
        let snapshot = frame.interpolate(1.0, origin);
        assert_eq!(snapshot.origin, origin);
        assert_eq!(snapshot.position, Vec3::new(2.0, 4.0, 0.25));
        assert_eq!(snapshot.rod_tip, Some(Vec3::new(3.5, 5.0, 0.5)));
    }
}
