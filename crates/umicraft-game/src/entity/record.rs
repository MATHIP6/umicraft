// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mutable per-entity state owned by the game thread.

use std::sync::Arc;

use glam::{DVec3, Vec3};
use umicraft_world::LightLevel;

use super::anim::Animation;
use super::body_yaw::BodyYaw;
use super::cape::CapeChase;
use super::flags::EntityFlags;
use super::item::EntityItem;
use super::kind::{EntityId, EntityKind};
use super::limbs::LimbSwing;
use super::meta::EntityMeta;
use super::profile::PlayerProfile;
use super::snapshot::EntityFrameData;

/// Full mutable record of an entity owned by the game thread.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityRecord {
    /// Unique entity ID.
    pub id: EntityId,
    /// Entity category.
    pub kind: EntityKind,
    /// Current world position of the entity's feet.
    pub position: DVec3,
    /// Position at the previous simulation tick.
    pub previous_position: DVec3,
    /// The position the server last reported (MCP `serverPosX / 32`);
    /// relative moves accumulate on it.
    pub server_position: DVec3,
    /// The position the entity glides to over the interpolation steps.
    pub target_position: DVec3,
    /// Velocity in blocks per tick.
    pub velocity: Vec3,
    /// Facing direction (MCP `rotationYaw`) in degrees.
    pub yaw: f32,
    /// Previous facing direction for interpolation.
    pub previous_yaw: f32,
    /// Target facing direction.
    pub target_yaw: f32,
    /// Vertical pitch in degrees.
    pub pitch: f32,
    /// Previous vertical pitch.
    pub previous_pitch: f32,
    /// Target vertical pitch.
    pub target_pitch: f32,
    /// Head yaw in degrees; set directly by `Entity Head Look`, not glided.
    pub head_yaw: f32,
    /// Previous head yaw.
    pub previous_head_yaw: f32,
    /// Whether the entity rests on the ground.
    pub on_ground: bool,
    /// The walking animation.
    pub limbs: LimbSwing,
    /// Where the torso points, which lags behind `yaw` and the head.
    pub body_yaw: BodyYaw,
    /// The hurt flash, death fall, arm swing and bed.
    pub animation: Animation,
    /// The typed metadata (variants, age, name, health).
    pub meta: EntityMeta,
    /// The text of the name tag above the entity, when it has one.
    pub name_tag: Option<Arc<str>>,
    /// The position the cape trails behind.
    pub cape: CapeChase,
    /// The profile of a player: name, skin and cape.
    pub profile: Option<Arc<PlayerProfile>>,
    /// Player UUID if this is a player entity.
    pub uuid: Option<[u8; 16]>,
    /// The stack in the hand (players).
    pub held: Option<EntityItem>,
    /// The worn armour, boots first (`Entity Equipment` slots 1 to 4).
    pub armor: [Option<EntityItem>; 4],
    /// Whether the tab list says this player is in spectator mode (shown as
    /// a head only).
    pub spectator: bool,
    /// The dropped stack (items) or the stack inside the frame (item frames).
    pub item: Option<EntityItem>,
    /// Ticks the held item has been in use (MCP `getItemInUseDuration` of
    /// another player), `0` while it is not.
    pub use_ticks: u32,
    /// The bobber this player has out, if any (MCP `EntityPlayer.fishEntity`).
    pub bobber: Option<EntityId>,
    /// Ticks the entity has lived since it was spawned.
    pub age_ticks: u32,
    /// Wall direction of an item frame: 0 south, 1 west, 2 north, 3 east.
    pub frame_direction: u8,
    /// Rotation step (0..=7) of the item inside an item frame.
    pub frame_rotation: u8,
    /// Visual flags.
    pub flags: EntityFlags,
    /// Entity health from metadata index 6 (if present).
    pub health: Option<f32>,
    /// Remaining interpolation ticks for position/rotation smoothing (vanilla: 3).
    pub interpolation_steps: u8,
}

impl EntityRecord {
    /// Moves the entity to where the simulation put it this tick, without
    /// interpolation, and animates its limbs and body. For the local player, whose
    /// position is already the predicted one.
    pub fn follow(&mut self, position: DVec3, yaw: f32, pitch: f32, flags: EntityFlags) {
        self.begin_tick();
        self.position = position;
        self.yaw = yaw;
        self.head_yaw = yaw;
        self.pitch = pitch;
        self.flags = flags;
        self.animate();
    }

    /// Counts one tick of item use: the held item is in use while the use
    /// flag is set (MCP `EntityOtherPlayerMP.onUpdate` starts and ends the
    /// use from `isEating`), and the count starts over when it stops.
    pub(super) fn tick_use(&mut self) {
        if self.held.is_some() && self.flags.is_using_item() {
            self.use_ticks = self.use_ticks.saturating_add(1);
        } else {
            self.use_ticks = 0;
        }
    }

    /// The data the render thread interpolates between two ticks.
    #[must_use]
    pub fn frame_data(&self) -> EntityFrameData {
        EntityFrameData {
            id: self.id,
            kind: self.kind,
            light: LightLevel::FULL_SKY,
            prev_position: self.previous_position.as_vec3(),
            position: self.position.as_vec3(),
            prev_yaw: self.previous_yaw,
            yaw: self.yaw,
            prev_pitch: self.previous_pitch,
            pitch: self.pitch,
            prev_head_yaw: self.previous_head_yaw,
            head_yaw: self.head_yaw,
            prev_body_yaw: self.body_yaw.previous,
            body_yaw: self.body_yaw.offset,
            limbs: self.limbs,
            animation: self.animation,
            meta: self.meta.clone(),
            prev_cape_chase: self.cape.previous.as_vec3(),
            cape_chase: self.cape.current.as_vec3(),
            name_tag: self.name_tag.clone(),
            profile: self.profile.clone(),
            uuid: self.uuid,
            held: self.held.clone(),
            armor: self.armor.clone(),
            spectator: self.spectator,
            item: self.item.clone(),
            use_ticks: self.use_ticks,
            fishing: self.bobber.is_some(),
            age_ticks: self.age_ticks,
            frame_direction: self.frame_direction,
            frame_rotation: self.frame_rotation,
            flags: self.flags,
        }
    }

    /// Creates a new entity record at the given initial position.
    #[must_use]
    pub fn new(id: EntityId, kind: EntityKind, position: DVec3, yaw: f32, pitch: f32) -> Self {
        Self {
            id,
            kind,
            position,
            previous_position: position,
            server_position: position,
            target_position: position,
            velocity: Vec3::ZERO,
            yaw,
            previous_yaw: yaw,
            target_yaw: yaw,
            pitch,
            previous_pitch: pitch,
            target_pitch: pitch,
            head_yaw: yaw,
            previous_head_yaw: yaw,
            on_ground: true,
            limbs: LimbSwing::default(),
            body_yaw: BodyYaw::new(yaw),
            animation: Animation::default(),
            meta: EntityMeta::default(),
            name_tag: None,
            cape: CapeChase::at(position),
            profile: None,
            uuid: None,
            held: None,
            item: None,
            armor: [None, None, None, None],
            spectator: false,
            use_ticks: 0,
            bobber: None,
            age_ticks: 0,
            frame_direction: 0,
            frame_rotation: 0,
            flags: EntityFlags::default(),
            health: None,
            interpolation_steps: 0,
        }
    }
}
