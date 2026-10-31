// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-tick motion of a tracked entity: gliding to the position the server
//! reported, then animating the limbs and the body.

use glam::DVec3;

use super::angle::wrap_degrees;
use super::kind::EntityKind;
use super::meta::Variant;
use super::record::EntityRecord;

/// Ticks over which a movement or teleport packet is applied
/// (MCP `NetHandlerPlayClient`: `setPositionAndRotation2(…, 3, …)`).
pub const INTERPOLATION_TICKS: u8 = 3;

/// A teleport this close to the current position keeps it, per axis
/// (MCP `handleEntityTeleport`: 1/32 block horizontally, 1/64 vertically).
const TELEPORT_SNAP: DVec3 = DVec3::new(0.031_25, 0.015_625, 0.031_25);

impl EntityRecord {
    /// Records the current state as the previous one; first step of a tick.
    pub(super) fn begin_tick(&mut self) {
        self.previous_position = self.position;
        self.previous_yaw = self.yaw;
        self.previous_pitch = self.pitch;
        self.previous_head_yaw = self.head_yaw;
        self.body_yaw.begin_tick();
    }

    /// Starts gliding to `target` and the given rotation over
    /// [`INTERPOLATION_TICKS`] (MCP `Entity.setPositionAndRotation2`).
    pub(super) fn retarget(&mut self, target: DVec3, yaw: f32, pitch: f32, on_ground: bool) {
        self.target_position = target;
        self.target_yaw = yaw;
        self.target_pitch = pitch;
        self.on_ground = on_ground;
        self.interpolation_steps = INTERPOLATION_TICKS;
    }

    /// Applies an absolute position from `Entity Teleport`.
    ///
    /// Interpolated like a relative move; a teleport within [`TELEPORT_SNAP`]
    /// of the current position keeps the current one.
    pub(super) fn teleport_to(&mut self, position: DVec3, yaw: f32, pitch: f32, on_ground: bool) {
        self.server_position = position;
        let target = if (self.position - position).abs().cmplt(TELEPORT_SNAP).all() {
            self.position
        } else {
            position
        };
        self.retarget(target, yaw, pitch, on_ground);
    }

    /// Moves one step towards the target: each tick covers `1 / steps` of
    /// what remains (MCP `EntityLivingBase.onLivingUpdate`).
    pub(super) fn glide_to_target(&mut self) {
        if self.interpolation_steps == 0 {
            return;
        }
        let steps = f32::from(self.interpolation_steps);
        self.position += (self.target_position - self.position) / f64::from(steps);
        self.yaw += wrap_degrees(self.target_yaw - self.yaw) / steps;
        self.pitch += (self.target_pitch - self.pitch) / steps;
        self.interpolation_steps -= 1;
    }

    /// Advances the limb swing and the body yaw from this tick's horizontal
    /// displacement; last step of a tick, after the position has moved.
    pub(super) fn animate(&mut self) {
        let moved = (
            self.position.x - self.previous_position.x,
            self.position.z - self.previous_position.z,
        );
        self.limbs.update(moved.0.hypot(moved.1) as f32);
        self.cape.update(self.position);
        if matches!(self.kind, EntityKind::Slime | EntityKind::MagmaCube) {
            self.squish.tick(self.on_ground);
        }
        if let (EntityKind::Horse, Variant::Horse(horse)) = (self.kind, &self.meta.variant) {
            self.horse
                .tick(horse.eating(), horse.rearing(), horse.mouth_open());
        }
        if self.kind == EntityKind::Dragon {
            let alive = self.health.is_none_or(|health| health > 0.0);
            self.flight.get_or_insert_with(Default::default).tick(
                self.yaw,
                self.position.y,
                self.velocity,
                alive,
            );
        }
        match self.kind {
            EntityKind::Player => self.body_yaw.update_player(moved, self.yaw, false),
            kind if kind.is_living() => {
                self.body_yaw
                    .update_mob(moved, self.yaw, &mut self.head_yaw);
            }
            _ => {}
        }
    }
}

/// The squash and stretch of a slime as it jumps and lands (MCP
/// `EntitySlime.onUpdate`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Squish {
    /// Where the factor is heading: `1` on leaving the ground, `-0.5` on
    /// landing, then decaying by `0.6` each tick.
    amount: f32,
    /// What is drawn.
    pub factor: f32,
    was_on_ground: bool,
}

impl Squish {
    /// One tick, after the entity has moved.
    pub fn tick(&mut self, on_ground: bool) {
        self.factor += (self.amount - self.factor) * 0.5;
        if on_ground && !self.was_on_ground {
            self.amount = -0.5;
        } else if !on_ground && self.was_on_ground {
            self.amount = 1.0;
        }
        self.was_on_ground = on_ground;
        self.amount *= 0.6;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::kind::EntityId;

    fn record(kind: EntityKind) -> EntityRecord {
        EntityRecord::new(EntityId(1), kind, DVec3::new(0.0, 64.0, 0.0), 0.0, 0.0)
    }

    #[test]
    fn a_slime_stretches_leaving_the_ground_and_squashes_landing() {
        let mut squish = Squish::default();
        squish.tick(true);
        squish.tick(false);
        squish.tick(false);
        assert!(squish.factor > 0.2, "{}", squish.factor);
        for _ in 0..6 {
            squish.tick(false);
        }
        squish.tick(true);
        squish.tick(true);
        assert!(squish.factor < -0.05, "{}", squish.factor);
    }

    #[test]
    fn a_teleport_glides_over_three_ticks() {
        let mut entity = record(EntityKind::Zombie);
        entity.teleport_to(DVec3::new(9.0, 64.0, 0.0), 0.0, 0.0, true);
        let mut xs = Vec::new();
        for _ in 0..4 {
            entity.begin_tick();
            entity.glide_to_target();
            xs.push(entity.position.x);
        }
        assert_eq!(xs, vec![3.0, 6.0, 9.0, 9.0]);
    }

    #[test]
    fn a_teleport_within_a_sixteenth_of_a_block_stays_put() {
        let mut entity = record(EntityKind::Zombie);
        entity.teleport_to(DVec3::new(0.02, 64.01, -0.02), 0.0, 0.0, true);
        entity.glide_to_target();
        assert_eq!(entity.position, DVec3::new(0.0, 64.0, 0.0));
        assert_eq!(entity.server_position, DVec3::new(0.02, 64.01, -0.02));
    }

    #[test]
    fn a_walking_entity_swings_its_limbs_from_the_glided_distance() {
        let mut entity = record(EntityKind::Zombie);
        entity.retarget(DVec3::new(0.3, 64.0, 0.0), 0.0, 0.0, true);
        entity.begin_tick();
        entity.glide_to_target();
        entity.animate();
        // 0.1 blocks this tick: target amplitude 0.4, smoothed by 0.4.
        assert!((entity.limbs.amount - 0.16).abs() < 1e-5);
        assert!((entity.limbs.swing - 0.16).abs() < 1e-5);
    }

    #[test]
    fn a_walking_player_body_leaves_the_facing_direction() {
        let mut entity = record(EntityKind::Player);
        entity.retarget(DVec3::new(3.0, 64.0, 0.0), 0.0, 0.0, true);
        entity.begin_tick();
        entity.glide_to_target();
        entity.animate();
        assert!(entity.body_yaw.offset < -20.0, "{}", entity.body_yaw.offset);
    }

    #[test]
    fn objects_have_no_body_yaw() {
        let mut entity = record(EntityKind::Arrow);
        entity.retarget(DVec3::new(3.0, 64.0, 0.0), 30.0, 0.0, true);
        entity.begin_tick();
        entity.glide_to_target();
        entity.animate();
        assert_eq!(entity.body_yaw.offset, 0.0);
    }
}
