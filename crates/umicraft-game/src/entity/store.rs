// SPDX-License-Identifier: MIT OR Apache-2.0

//! The game thread entity store: owns all active non-local entities and
//! produces interpolated render snapshots.

use std::collections::BTreeMap;

use glam::{DVec3, Vec3};

use crate::physics::EYE_HEIGHT;
use crate::view::ViewEffects;
use crate::RenderState;

use super::{
    EntityFlags, EntityFrameData, EntityId, EntityItem, EntityKind, EntityRecord,
    EntityRenderSnapshot,
};

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

    /// The kind of a tracked entity.
    #[must_use]
    pub fn kind(&self, id: EntityId) -> Option<EntityKind> {
        self.entities.get(&id).map(|entity| entity.kind)
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
        held: Option<EntityItem>,
        flags: EntityFlags,
    ) {
        let mut record = EntityRecord::new(id, EntityKind::Player, position, yaw, pitch);
        record.uuid = Some(uuid);
        record.held = held;
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
    ///
    /// `data` is the object-specific integer of `Spawn Object`; for an item
    /// frame it is the wall direction (MCP `EntityHanging.facingDirection`
    /// horizontal index), clamped to `0..=3`.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_object(
        &mut self,
        id: EntityId,
        object_type: u8,
        position: DVec3,
        pitch: f32,
        yaw: f32,
        velocity: Vec3,
        data: i32,
    ) {
        let kind = EntityKind::from_object_type(object_type);
        let mut record = EntityRecord::new(id, kind, position, yaw, pitch);
        record.velocity = velocity;
        if kind == EntityKind::ItemFrame {
            record.frame_direction = data.clamp(0, 3) as u8;
        }
        self.entities.insert(id, record);
    }

    /// Removes entities by network id.
    pub fn destroy(&mut self, entity_ids: &[i32]) {
        for &id in entity_ids {
            self.entities.remove(&EntityId(id));
        }
    }

    /// The view of an entity for the spectator camera (`Camera`, 0x43).
    ///
    /// The eye sits at the player eye height above the interpolated feet;
    /// the view turns with the head yaw and the pitch (MCP
    /// `EntityRenderer.orientCamera` follows the view entity). `None` when
    /// the entity is not tracked, so the caller keeps the local view.
    #[must_use]
    pub fn camera_view(&self, id: EntityId, alpha: f32) -> Option<RenderState> {
        let record = self.entities.get(&id)?;
        let alpha_f64 = f64::from(alpha.clamp(0.0, 1.0));
        let feet = record.previous_position.lerp(record.position, alpha_f64);
        let eye = feet + DVec3::Y * EYE_HEIGHT;
        let lerp = |from: f32, to: f32| from + (to - from) * alpha.clamp(0.0, 1.0);
        Some(RenderState {
            position: Vec3::new(eye.x as f32, eye.y as f32, eye.z as f32),
            yaw: lerp(record.previous_head_yaw, record.head_yaw).to_radians(),
            pitch: lerp(record.previous_pitch, record.pitch).to_radians(),
            on_ground: record.on_ground,
            effects: ViewEffects::default(),
        })
    }

    /// Applies a relative movement delta to an entity.
    pub fn move_relative(&mut self, id: EntityId, delta: DVec3, on_ground: bool) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.target_position += delta;
            entity.on_ground = on_ground;
            entity.interpolation_steps = 3;
        }
    }

    /// Updates entity orientation.
    pub fn look(&mut self, id: EntityId, yaw: f32, pitch: f32, on_ground: bool) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.target_yaw = yaw;
            entity.target_pitch = pitch;
            entity.on_ground = on_ground;
            entity.interpolation_steps = 3;
        }
    }

    /// Updates entity position delta and orientation.
    pub fn look_move(&mut self, id: EntityId, delta: DVec3, yaw: f32, pitch: f32, on_ground: bool) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.target_position += delta;
            entity.target_yaw = yaw;
            entity.target_pitch = pitch;
            entity.on_ground = on_ground;
            entity.interpolation_steps = 3;
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

    /// Updates the stack an entity holds in its hand.
    pub fn set_held(&mut self, id: EntityId, held: Option<EntityItem>) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.held = held;
        }
    }

    /// Sets the dropped stack of an item entity or the stack inside a frame.
    pub fn set_item(&mut self, id: EntityId, item: Option<EntityItem>) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.item = item;
        }
    }

    /// Sets the rotation step of the item inside a frame (masked to `0..=7`).
    pub fn set_frame_rotation(&mut self, id: EntityId, rotation: u8) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.frame_rotation = rotation & 7;
        }
    }

    /// Sets an entity's health (from data watcher index 6).
    pub fn set_health(&mut self, id: EntityId, health: f32) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.health = Some(health);
        }
    }

    /// Borrows the tracked entity records.
    #[must_use]
    pub fn entities(&self) -> &BTreeMap<EntityId, EntityRecord> {
        &self.entities
    }

    /// Advances the simulation by one tick (20 Hz).
    ///
    /// Position and rotation glide toward the server target over 3 ticks,
    /// matching vanilla `EntityLivingBase.onLivingUpdate`.
    pub fn tick(&mut self) {
        for entity in self.entities.values_mut() {
            entity.previous_position = entity.position;
            entity.previous_yaw = entity.yaw;
            entity.previous_pitch = entity.pitch;
            entity.previous_head_yaw = entity.head_yaw;

            if entity.interpolation_steps > 0 {
                let steps = f64::from(entity.interpolation_steps);
                entity.position += (entity.target_position - entity.position) / steps;

                let yaw_diff = wrap_degrees(entity.target_yaw - entity.yaw);
                entity.yaw += yaw_diff / steps as f32;

                entity.pitch += (entity.target_pitch - entity.pitch) / steps as f32;

                let head_diff = wrap_degrees(entity.target_head_yaw - entity.head_yaw);
                entity.head_yaw += head_diff / steps as f32;

                entity.interpolation_steps -= 1;
            }

            entity.update_limbs();
            entity.age_ticks = entity.age_ticks.saturating_add(1);
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

                // Vanilla: limbSwing - limbSwingAmount * (1.0 - partialTicks)
                let limb_swing = entity.limb_swing - entity.limb_swing_amount * (1.0 - alpha_f);

                EntityRenderSnapshot {
                    id: entity.id,
                    kind: entity.kind,
                    position,
                    yaw,
                    pitch,
                    head_yaw,
                    limb_swing,
                    limb_swing_amount,
                    uuid: entity.uuid,
                    held: entity.held.clone(),
                    item: entity.item.clone(),
                    age: entity.age_ticks as f32 + alpha_f,
                    frame_direction: entity.frame_direction,
                    frame_rotation: entity.frame_rotation,
                    flags: entity.flags,
                }
            })
            .collect()
    }

    /// Produces per-entity interpolation data for the render thread.
    ///
    /// Unlike [`snapshots`](Self::snapshots), this carries both previous and
    /// current tick states so the render thread can interpolate at any sub-tick
    /// fraction without pre-baking the alpha on the game thread.
    #[must_use]
    pub fn frame_data(&self) -> Vec<EntityFrameData> {
        self.entities
            .values()
            .map(EntityRecord::frame_data)
            .collect()
    }
}

/// Wraps a degree difference into `(-180, 180]`.
fn wrap_degrees(d: f32) -> f32 {
    let mut r = d % 360.0;
    if r < -180.0 {
        r += 360.0;
    } else if r >= 180.0 {
        r -= 360.0;
    }
    r
}

/// Interpolates between two angles in degrees taking the shortest arc.
fn interpolate_angle(prev: f32, current: f32, alpha: f32) -> f32 {
    prev + wrap_degrees(current - prev) * alpha
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sword() -> EntityItem {
        EntityItem {
            item_id: 276,
            damage: 0,
            count: 1,
            nbt: None,
        }
    }

    fn spawn_object_at_origin(store: &mut EntityStore, id: i32, object_type: u8, data: i32) {
        store.spawn_object(
            EntityId(id),
            object_type,
            DVec3::ZERO,
            0.0,
            0.0,
            Vec3::ZERO,
            data,
        );
    }

    #[test]
    fn an_item_frame_keeps_its_wall_direction() {
        let mut store = EntityStore::new();
        spawn_object_at_origin(&mut store, 1, 71, 3);
        spawn_object_at_origin(&mut store, 2, 71, 9);
        spawn_object_at_origin(&mut store, 3, 2, 3);
        let snaps = store.snapshots(0.0);
        assert_eq!(snaps[0].kind, EntityKind::ItemFrame);
        assert_eq!(snaps[0].frame_direction, 3);
        assert_eq!(
            snaps[1].frame_direction, 3,
            "clamped to the horizontal range"
        );
        assert_eq!(snaps[2].frame_direction, 0, "only frames use the data");
    }

    #[test]
    fn items_and_frame_rotations_reach_the_snapshot() {
        let mut store = EntityStore::new();
        spawn_object_at_origin(&mut store, 1, 71, 0);
        store.set_item(EntityId(1), Some(sword()));
        store.set_frame_rotation(EntityId(1), 11);
        let snap = &store.snapshots(0.0)[0];
        assert_eq!(snap.item, Some(sword()));
        assert_eq!(snap.frame_rotation, 3, "masked to 0..=7");

        store.set_item(EntityId(1), None);
        assert_eq!(store.snapshots(0.0)[0].item, None);
    }

    #[test]
    fn the_age_grows_with_ticks_and_the_alpha() {
        let mut store = EntityStore::new();
        spawn_object_at_origin(&mut store, 1, 2, 0);
        assert!(store.snapshots(0.25)[0].age - 0.25 < 1e-6);
        store.tick();
        store.tick();
        assert!((store.snapshots(0.5)[0].age - 2.5).abs() < 1e-6);
        let from_frame = store.frame_data()[0].interpolate(0.5);
        assert!((from_frame.age - 2.5).abs() < 1e-6);
    }

    #[test]
    fn the_age_saturates() {
        let mut store = EntityStore::new();
        spawn_object_at_origin(&mut store, 1, 2, 0);
        store
            .entities
            .get_mut(&EntityId(1))
            .expect("tracked")
            .age_ticks = u32::MAX;
        store.tick();
        assert_eq!(store.entities[&EntityId(1)].age_ticks, u32::MAX);
    }

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
            Some(sword()), // diamond sword
            EntityFlags::default(),
        );

        assert_eq!(store.len(), 1);
        let snapshots = store.snapshots(0.5);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].id, player_id);
        assert_eq!(snapshots[0].held, Some(sword()));

        store.destroy(&[10]);
        assert!(store.is_empty());
    }

    #[test]
    fn movement_smoothed_over_three_ticks() {
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

        store.move_relative(mob_id, DVec3::new(3.0, 0.0, 0.0), true);

        // Tick 1: moves 1/3 of the remaining distance → position ≈ 1.0
        store.tick();
        let s1 = store.snapshots(1.0);
        assert!(
            (s1[0].position.x - 1.0).abs() < 1e-4,
            "tick 1: {}",
            s1[0].position.x
        );

        // Tick 2: moves 1/2 of the remaining 2.0 → position ≈ 2.0
        store.tick();
        let s2 = store.snapshots(1.0);
        assert!(
            (s2[0].position.x - 2.0).abs() < 1e-4,
            "tick 2: {}",
            s2[0].position.x
        );

        // Tick 3: moves the last 1.0 → position ≈ 3.0
        store.tick();
        let s3 = store.snapshots(1.0);
        assert!(
            (s3[0].position.x - 3.0).abs() < 1e-4,
            "tick 3: {}",
            s3[0].position.x
        );
    }

    #[test]
    fn sub_tick_interpolation() {
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

        store.move_relative(mob_id, DVec3::new(3.0, 0.0, 0.0), true);
        store.tick();

        // After tick 1: prev=0.0, current=1.0 → alpha 0.5 gives 0.5
        let mid = store.snapshots(0.5);
        assert!((mid[0].position.x - 0.5).abs() < 1e-4);
    }

    #[test]
    fn the_camera_view_sits_at_the_eyes_of_the_entity() {
        let mut store = EntityStore::new();
        let id = EntityId(5);
        store.spawn_player(
            id,
            [2u8; 16],
            DVec3::new(1.0, 64.0, 2.0),
            90.0,
            45.0,
            None,
            EntityFlags::default(),
        );
        let view = store.camera_view(id, 1.0).expect("tracked entity");
        assert!((view.position.y - 65.62).abs() < 1e-4);
        assert!((view.position.x - 1.0).abs() < 1e-4);
        assert!((view.pitch - 45f32.to_radians()).abs() < 1e-4);
    }

    #[test]
    fn the_camera_view_of_an_unknown_entity_is_none() {
        assert!(EntityStore::new().camera_view(EntityId(9), 1.0).is_none());
    }
}
