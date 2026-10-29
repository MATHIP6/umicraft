// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mutable per-entity state owned by the game thread.

use glam::{DVec3, Vec3};

use super::flags::EntityFlags;
use super::item::EntityItem;
use super::kind::{EntityId, EntityKind};
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
    /// Server-authoritative target position when interpolating movement packets.
    pub target_position: DVec3,
    /// Velocity in blocks per tick.
    pub velocity: Vec3,
    /// Body horizontal yaw in degrees.
    pub yaw: f32,
    /// Previous body yaw for interpolation.
    pub previous_yaw: f32,
    /// Target body yaw.
    pub target_yaw: f32,
    /// Vertical pitch in degrees.
    pub pitch: f32,
    /// Previous vertical pitch.
    pub previous_pitch: f32,
    /// Target vertical pitch.
    pub target_pitch: f32,
    /// Head yaw in degrees.
    pub head_yaw: f32,
    /// Previous head yaw.
    pub previous_head_yaw: f32,
    /// Target head yaw.
    pub target_head_yaw: f32,
    /// Whether the entity rests on the ground.
    pub on_ground: bool,
    /// Accumulated limb swing animation progress in radians.
    pub limb_swing: f32,
    /// Current limb swing amplitude (0.0 = still, 1.0 = full walk speed).
    pub limb_swing_amount: f32,
    /// Previous limb swing amplitude.
    pub previous_limb_swing_amount: f32,
    /// Player UUID if this is a player entity.
    pub uuid: Option<[u8; 16]>,
    /// The stack in the hand (players).
    pub held: Option<EntityItem>,
    /// The dropped stack (items) or the stack inside the frame (item frames).
    pub item: Option<EntityItem>,
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
    /// Advances the limb animation by the horizontal distance moved during
    /// the tick (`previous_position` to `position`).
    pub fn update_limbs(&mut self) {
        let dx = self.position.x - self.previous_position.x;
        let dz = self.position.z - self.previous_position.z;
        let speed = (dx * dx + dz * dz).sqrt() as f32;

        self.previous_limb_swing_amount = self.limb_swing_amount;
        self.limb_swing += speed * 4.0;
        let target_amount = (speed * 4.0).clamp(0.0, 1.0);
        self.limb_swing_amount += (target_amount - self.limb_swing_amount) * 0.4;
    }

    /// Moves the entity to where the simulation put it this tick, without
    /// interpolation, and animates its limbs. For the local player, whose
    /// position is already the predicted one.
    pub fn follow(&mut self, position: DVec3, yaw: f32, pitch: f32, flags: EntityFlags) {
        self.previous_position = self.position;
        self.position = position;
        self.previous_yaw = self.yaw;
        self.previous_head_yaw = self.head_yaw;
        self.previous_pitch = self.pitch;
        self.yaw = yaw;
        self.head_yaw = yaw;
        self.pitch = pitch;
        self.flags = flags;
        self.update_limbs();
    }

    /// The data the render thread interpolates between two ticks.
    #[must_use]
    pub fn frame_data(&self) -> EntityFrameData {
        EntityFrameData {
            id: self.id,
            kind: self.kind,
            prev_position: self.previous_position.as_vec3(),
            position: self.position.as_vec3(),
            prev_yaw: self.previous_yaw,
            yaw: self.yaw,
            prev_pitch: self.previous_pitch,
            pitch: self.pitch,
            prev_head_yaw: self.previous_head_yaw,
            head_yaw: self.head_yaw,
            limb_swing: self.limb_swing,
            prev_limb_swing_amount: self.previous_limb_swing_amount,
            limb_swing_amount: self.limb_swing_amount,
            uuid: self.uuid,
            held: self.held.clone(),
            item: self.item.clone(),
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
            target_head_yaw: yaw,
            on_ground: true,
            limb_swing: 0.0,
            limb_swing_amount: 0.0,
            previous_limb_swing_amount: 0.0,
            uuid: None,
            held: None,
            item: None,
            age_ticks: 0,
            frame_direction: 0,
            frame_rotation: 0,
            flags: EntityFlags::default(),
            health: None,
            interpolation_steps: 0,
        }
    }
}
