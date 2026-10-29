// SPDX-License-Identifier: MIT OR Apache-2.0

//! Interpolation data shipped to the render thread, and the snapshots it yields.

use glam::Vec3;

use super::angle::lerp_degrees;
use super::anim::{Animation, BedPose};
use super::flags::EntityFlags;
use super::item::EntityItem;
use super::kind::{EntityId, EntityKind};
use super::limbs::LimbSwing;

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
    /// Player UUID for skin lookup.
    pub uuid: Option<[u8; 16]>,
    /// The stack in the hand (players).
    pub held: Option<EntityItem>,
    /// The dropped stack (items) or the stack inside the frame (item frames).
    pub item: Option<EntityItem>,
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
            position,
            yaw,
            body_yaw,
            pitch,
            head_yaw,
            limb_swing: self.limbs.swing_at(a),
            limb_swing_amount: self.limbs.amount_at(a),
            swing_progress: self.animation.swing_progress_at(a),
            hurt: self.animation.is_flashing(),
            death_time: self.animation.death_time_at(a),
            bed: self.animation.bed,
            uuid: self.uuid,
            held: self.held.clone(),
            item: self.item.clone(),
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
    /// True while the red hurt or death flash shows.
    pub hurt: bool,
    /// Ticks into the death fall plus the partial tick, `0` while alive.
    pub death_time: f32,
    /// The bed the entity sleeps in.
    pub bed: Option<BedPose>,
    /// Player UUID for skin lookup.
    pub uuid: Option<[u8; 16]>,
    /// The stack in the hand (players).
    pub held: Option<EntityItem>,
    /// The dropped stack (items) or the stack inside the frame (item frames).
    pub item: Option<EntityItem>,
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
            position,
            yaw: 0.0,
            body_yaw: 0.0,
            pitch: 0.0,
            head_yaw: 0.0,
            limb_swing: 0.0,
            limb_swing_amount: 0.0,
            swing_progress: 0.0,
            hurt: false,
            death_time: 0.0,
            bed: None,
            uuid: None,
            held: None,
            item: None,
            age: 0.0,
            frame_direction: 0,
            frame_rotation: 0,
            flags: EntityFlags::default(),
        }
    }
}
