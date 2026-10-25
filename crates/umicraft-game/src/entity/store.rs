// SPDX-License-Identifier: MIT OR Apache-2.0

//! The game thread entity store: owns all active non-local entities and
//! produces interpolated render snapshots.

use std::collections::BTreeMap;

use glam::{DVec3, Vec3};

use super::types::{EntityFlags, EntityId, EntityKind, EntityRecord, EntityRenderSnapshot};

/// The storage and simulation of non-local entities on the game thread.
#[derive(Debug, Clone, Default)]
pub struct EntityStore {
    entities: BTreeMap<EntityId, EntityRecord>,
}

impl EntityStore {
    /// Creates an empty entity store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entities: BTreeMap::new(),
        }
    }

    /// Number of tracked entities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Returns true if no entities are currently tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Clears all tracked entities.
    pub fn clear(&mut self) {
        self.entities.clear();
    }

    /// Spawns a remote player.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_player(
        &mut self,
        id: EntityId,
        uuid: [u8; 16],
        position: DVec3,
        yaw: f32,
        pitch: f32,
        held_item: Option<u16>,
        flags: EntityFlags,
    ) {
        let mut record = EntityRecord::new(id, EntityKind::Player, position, yaw, pitch);
        record.uuid = Some(uuid);
        record.held_item = held_item;
        record.flags = flags;
        self.entities.insert(id, record);
    }

    /// Spawns a mob.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_mob(
        &mut self,
        id: EntityId,
        mob_type: u8,
        position: DVec3,
        yaw: f32,
        pitch: f32,
        head_yaw: f32,
        velocity: Vec3,
        flags: EntityFlags,
    ) {
        let kind = EntityKind::from_mob_type(mob_type);
        let mut record = EntityRecord::new(id, kind, position, yaw, pitch);
        record.head_yaw = head_yaw;
        record.previous_head_yaw = head_yaw;
        record.target_head_yaw = head_yaw;
        record.velocity = velocity;
        record.flags = flags;
        self.entities.insert(id, record);
    }

    /// Spawns an object or item.
    pub fn spawn_object(
        &mut self,
        id: EntityId,
        object_type: u8,
        position: DVec3,
        pitch: f32,
        yaw: f32,
        velocity: Vec3,
    ) {
        let kind = EntityKind::from_object_type(object_type);
        let mut record = EntityRecord::new(id, kind, position, yaw, pitch);
        record.velocity = velocity;
        self.entities.insert(id, record);
    }

    /// Removes entities by network id.
    pub fn destroy(&mut self, entity_ids: &[i32]) {
        for &id in entity_ids {
            self.entities.remove(&EntityId(id));
        }
    }

    /// Applies a relative movement delta to an entity.
    pub fn move_relative(&mut self, id: EntityId, delta: DVec3, on_ground: bool) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.target_position += delta;
            entity.on_ground = on_ground;
        }
    }

    /// Updates entity orientation.
    pub fn look(&mut self, id: EntityId, yaw: f32, pitch: f32, on_ground: bool) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.target_yaw = yaw;
            entity.target_pitch = pitch;
            entity.on_ground = on_ground;
        }
    }

    /// Updates entity position delta and orientation.
    pub fn look_move(&mut self, id: EntityId, delta: DVec3, yaw: f32, pitch: f32, on_ground: bool) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.target_position += delta;
            entity.target_yaw = yaw;
            entity.target_pitch = pitch;
            entity.on_ground = on_ground;
        }
    }

    /// Teleports an entity to an absolute location.
    pub fn teleport(
        &mut self,
        id: EntityId,
        position: DVec3,
        yaw: f32,
        pitch: f32,
        on_ground: bool,
    ) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.position = position;
            entity.previous_position = position;
            entity.target_position = position;
            entity.yaw = yaw;
            entity.previous_yaw = yaw;
            entity.target_yaw = yaw;
            entity.pitch = pitch;
            entity.previous_pitch = pitch;
            entity.target_pitch = pitch;
            entity.on_ground = on_ground;
        }
    }

    /// Sets an entity's head yaw.
    pub fn set_head_yaw(&mut self, id: EntityId, head_yaw: f32) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.target_head_yaw = head_yaw;
        }
    }

    /// Sets an entity's velocity.
    pub fn set_velocity(&mut self, id: EntityId, velocity: Vec3) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.velocity = velocity;
        }
    }

    /// Updates an entity's visual flags.
    pub fn set_flags(&mut self, id: EntityId, flags: EntityFlags) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.flags = flags;
        }
    }

    /// Updates an entity's held item.
    pub fn set_held_item(&mut self, id: EntityId, held_item: Option<u16>) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.held_item = held_item;
        }
    }

    /// Advances the simulation by one tick (20 Hz).
    pub fn tick(&mut self) {
        for entity in self.entities.values_mut() {
            entity.previous_position = entity.position;
            entity.position = entity.target_position;

            entity.previous_yaw = entity.yaw;
            entity.yaw = entity.target_yaw;

            entity.previous_pitch = entity.pitch;
            entity.pitch = entity.target_pitch;

            entity.previous_head_yaw = entity.head_yaw;
            entity.head_yaw = entity.target_head_yaw;

            // Walking limb swing animation
            let dx = entity.position.x - entity.previous_position.x;
            let dz = entity.position.z - entity.previous_position.z;
            let speed = (dx * dx + dz * dz).sqrt() as f32;

            entity.previous_limb_swing_amount = entity.limb_swing_amount;
            entity.limb_swing += speed * 4.0;
            let target_amount = (speed * 4.0).clamp(0.0, 1.0);
            entity.limb_swing_amount += (target_amount - entity.limb_swing_amount) * 0.4;
        }
    }

    /// Produces interpolated render snapshots for all tracked entities.
    ///
    /// `alpha` is the partial tick fraction in `0.0..=1.0`.
    #[must_use]
    pub fn snapshots(&self, alpha: f32) -> Vec<EntityRenderSnapshot> {
        let alpha_d = f64::from(alpha.clamp(0.0, 1.0));
        let alpha_f = alpha.clamp(0.0, 1.0);

        self.entities
            .values()
            .map(|entity| {
                let interpolated_pos = entity.previous_position.lerp(entity.position, alpha_d);
                let position = Vec3::new(
                    interpolated_pos.x as f32,
                    interpolated_pos.y as f32,
                    interpolated_pos.z as f32,
                );

                let yaw = interpolate_angle(entity.previous_yaw, entity.yaw, alpha_f);
                let pitch =
                    entity.previous_pitch + (entity.pitch - entity.previous_pitch) * alpha_f;
                let head_yaw =
                    interpolate_angle(entity.previous_head_yaw, entity.head_yaw, alpha_f);

                let limb_swing_amount = entity.previous_limb_swing_amount
                    + (entity.limb_swing_amount - entity.previous_limb_swing_amount) * alpha_f;

                EntityRenderSnapshot {
                    id: entity.id,
                    kind: entity.kind,
                    position,
                    yaw,
                    pitch,
                    head_yaw,
                    limb_swing: entity.limb_swing,
                    limb_swing_amount,
                    uuid: entity.uuid,
                    held_item: entity.held_item,
                    flags: entity.flags,
                }
            })
            .collect()
    }
}

/// Interpolates between two angles in degrees taking the shortest arc.
fn interpolate_angle(prev: f32, current: f32, alpha: f32) -> f32 {
    let mut diff = (current - prev) % 360.0;
    if diff < -180.0 {
        diff += 360.0;
    } else if diff >= 180.0 {
        diff -= 360.0;
    }
    prev + diff * alpha
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_and_destroy_flow() {
        let mut store = EntityStore::new();
        assert!(store.is_empty());

        let player_id = EntityId(10);
        store.spawn_player(
            player_id,
            [1u8; 16],
            DVec3::new(10.0, 64.0, 20.0),
            90.0,
            0.0,
            Some(276), // diamond sword
            EntityFlags::default(),
        );

        assert_eq!(store.len(), 1);
        let snapshots = store.snapshots(0.5);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].id, player_id);
        assert_eq!(snapshots[0].held_item, Some(276));

        store.destroy(&[10]);
        assert!(store.is_empty());
    }

    #[test]
    fn movement_interpolation() {
        let mut store = EntityStore::new();
        let mob_id = EntityId(20);
        store.spawn_mob(
            mob_id,
            54, // Zombie
            DVec3::new(0.0, 64.0, 0.0),
            0.0,
            0.0,
            0.0,
            Vec3::ZERO,
            EntityFlags::default(),
        );

        store.move_relative(mob_id, DVec3::new(2.0, 0.0, 0.0), true);
        store.tick();

        let mid = store.snapshots(0.5);
        assert_eq!(mid.len(), 1);
        // previous is 0.0, current is 2.0 -> lerp 0.5 is 1.0
        assert!((mid[0].position.x - 1.0).abs() < 1e-4);
    }
}
