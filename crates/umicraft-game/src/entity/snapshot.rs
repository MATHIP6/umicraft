// SPDX-License-Identifier: MIT OR Apache-2.0

//! Interpolation data shipped to the render thread, and the snapshots it yields.

use glam::Vec3;

use super::flags::EntityFlags;
use super::item::EntityItem;
use super::kind::{EntityId, EntityKind};

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
    /// Previous body yaw in degrees.
    pub prev_yaw: f32,
    /// Current body yaw in degrees.
    pub yaw: f32,
    /// Previous pitch in degrees.
    pub prev_pitch: f32,
    /// Current pitch in degrees.
    pub pitch: f32,
    /// Previous head yaw in degrees.
    pub prev_head_yaw: f32,
    /// Current head yaw in degrees.
    pub head_yaw: f32,
    /// Accumulated limb swing phase.
    pub limb_swing: f32,
    /// Previous limb swing amplitude.
    pub prev_limb_swing_amount: f32,
    /// Current limb swing amplitude.
    pub limb_swing_amount: f32,
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
        let yaw = interpolate_angle_deg(self.prev_yaw, self.yaw, a);
        let pitch = self.prev_pitch + (self.pitch - self.prev_pitch) * a;
        let head_yaw = interpolate_angle_deg(self.prev_head_yaw, self.head_yaw, a);
        let limb_swing_amount = self.prev_limb_swing_amount
            + (self.limb_swing_amount - self.prev_limb_swing_amount) * a;
        let limb_swing = self.limb_swing - self.limb_swing_amount * (1.0 - a);

        EntityRenderSnapshot {
            id: self.id,
            kind: self.kind,
            position,
            yaw,
            pitch,
            head_yaw,
            limb_swing,
            limb_swing_amount,
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

/// Interpolates between two degree angles taking the shortest arc.
fn interpolate_angle_deg(prev: f32, current: f32, alpha: f32) -> f32 {
    let mut diff = (current - prev) % 360.0;
    if diff < -180.0 {
        diff += 360.0;
    } else if diff >= 180.0 {
        diff -= 360.0;
    }
    prev + diff * alpha
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
    /// Interpolated body yaw in degrees.
    pub yaw: f32,
    /// Interpolated pitch in degrees.
    pub pitch: f32,
    /// Interpolated head yaw in degrees.
    pub head_yaw: f32,
    /// Interpolated limb swing progress in radians.
    pub limb_swing: f32,
    /// Interpolated limb swing amplitude (0.0 to 1.0).
    pub limb_swing_amount: f32,
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
