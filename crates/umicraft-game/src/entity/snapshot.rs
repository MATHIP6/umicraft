// SPDX-License-Identifier: MIT OR Apache-2.0

//! Interpolation data shipped to the render thread, and the snapshots it yields.

use std::sync::Arc;

use glam::Vec3;
use umicraft_world::LightLevel;

use super::angle::lerp_degrees;
use super::anim::{Animation, BedPose};
use super::cape::{cape_angles, CapeAngles};
use super::flags::EntityFlags;
use super::item::EntityItem;
use super::kind::{EntityId, EntityKind};
use super::leash::{LeashAnchor, LeashFrame};
use super::limbs::LimbSwing;
use super::meta::EntityMeta;
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
    /// Position at the previous simulation tick.
    pub prev_position: Vec3,
    /// Position at the current simulation tick.
    pub position: Vec3,
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
    pub prev_cape_chase: Vec3,
    /// The cape position now.
    pub cape_chase: Vec3,
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
    /// Who holds the leash of the entity, if it is leashed.
    pub leash_holder: Option<EntityId>,
    /// Where the holder is, found by [`super::leash::leash_entities`].
    pub leash: Option<LeashFrame>,
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
    #[must_use]
    pub fn interpolate(&self, alpha: f32) -> EntityRenderSnapshot {
        let a = alpha.clamp(0.0, 1.0);
        let position = self.prev_position.lerp(self.position, a);
        let yaw = lerp_degrees(self.prev_yaw, self.yaw, a);
        let body_yaw = lerp_degrees(self.prev_body_yaw, self.body_yaw, a);
        let pitch = self.prev_pitch + (self.pitch - self.prev_pitch) * a;
        let head_yaw = lerp_degrees(self.prev_head_yaw, self.head_yaw, a);

        EntityRenderSnapshot {
            id: self.id,
            kind: self.kind,
            light: self.light,
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
                self.prev_cape_chase.lerp(self.cape_chase, a) - position,
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
            leash: self.leash.map(|holder| holder.at(a)),
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
    /// Interpolated world space position.
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
            age: 0.0,
            frame_direction: 0,
            frame_rotation: 0,
            flags: EntityFlags::default(),
        }
    }
}
