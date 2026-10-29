// SPDX-License-Identifier: MIT OR Apache-2.0

//! The game thread entity store: owns all active non-local entities and
//! produces interpolated render snapshots.

use std::collections::BTreeMap;

use std::sync::Arc;

use glam::{DVec3, Vec3};
use umicraft_protocol::packets::play::Metadata;

use crate::physics::EYE_HEIGHT;
use crate::view::ViewEffects;
use crate::RenderState;

use super::{
    BedPose, BodyYaw, EntityFlags, EntityFrameData, EntityId, EntityItem, EntityKind, EntityRecord,
    EntityRenderSnapshot, PlayerProfile, Variant,
};

/// Data watcher entry of the entity flags (MCP `Entity.entityInit`).
const ENTITY_FLAGS: u8 = 0;
/// Data watcher entry of the player skin parts (MCP `EntityPlayer.entityInit`).
const PLAYER_SKIN_PARTS: u8 = 10;
/// Data watcher entry of the health (MCP `EntityLivingBase.entityInit`).
const LIVING_HEALTH: u8 = 6;

/// The limb swing amplitude a hurt entity jolts to
/// (MCP `EntityLivingBase.handleStatusUpdate`, `limbSwingAmount = 1.5`).
const HURT_LIMB_AMOUNT: f32 = 1.5;

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
        // MCP `handleSpawnMob`: `renderYawOffset = rotationYawHead = head`.
        record.body_yaw = BodyYaw::new(head_yaw);
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
        if kind == EntityKind::FishHook {
            // MCP `NetHandlerPlayClient.handleSpawnObject`: the bobber
            // registers itself on its angler (`fishEntity`).
            if let Some(angler) = self.entities.get_mut(&EntityId(data)) {
                angler.bobber = Some(id);
            }
        }
        self.entities.insert(id, record);
    }

    /// Removes entities by network id.
    pub fn destroy(&mut self, entity_ids: &[i32]) {
        for &id in entity_ids {
            self.entities.remove(&EntityId(id));
        }
        for angler in self.entities.values_mut() {
            if angler
                .bobber
                .is_some_and(|bobber| entity_ids.contains(&bobber.0))
            {
                angler.bobber = None;
            }
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

    /// The feet position of a rider seated on the tracked entity `id`, `None`
    /// when the vehicle is not tracked.
    #[must_use]
    pub fn rider_feet(&self, id: EntityId) -> Option<DVec3> {
        self.entities.get(&id).map(crate::riding::rider_feet)
    }

    /// Applies a relative movement delta to an entity (`Entity Relative Move`).
    ///
    /// The rotation is retargeted to the current one, which stops a turn in
    /// progress (MCP `handleEntityMovement`).
    pub fn move_relative(&mut self, id: EntityId, delta: DVec3, on_ground: bool) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.server_position += delta;
            entity.retarget(entity.server_position, entity.yaw, entity.pitch, on_ground);
        }
    }

    /// Updates entity orientation (`Entity Look`).
    pub fn look(&mut self, id: EntityId, yaw: f32, pitch: f32, on_ground: bool) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.retarget(entity.server_position, yaw, pitch, on_ground);
        }
    }

    /// Updates entity position delta and orientation (`Entity Look And
    /// Relative Move`).
    pub fn look_move(&mut self, id: EntityId, delta: DVec3, yaw: f32, pitch: f32, on_ground: bool) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.server_position += delta;
            entity.retarget(entity.server_position, yaw, pitch, on_ground);
        }
    }

    /// Teleports an entity to an absolute location (`Entity Teleport`).
    ///
    /// Interpolated over 3 ticks like a relative move, as in vanilla.
    pub fn teleport(
        &mut self,
        id: EntityId,
        position: DVec3,
        yaw: f32,
        pitch: f32,
        on_ground: bool,
    ) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.teleport_to(position, yaw, pitch, on_ground);
        }
    }

    /// Sets an entity's head yaw (`Entity Head Look`); applied at once, not
    /// glided (MCP `handleEntityHeadLook`).
    pub fn set_head_yaw(&mut self, id: EntityId, head_yaw: f32) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.head_yaw = head_yaw;
        }
    }

    /// Sets an entity's velocity.
    pub fn set_velocity(&mut self, id: EntityId, velocity: Vec3) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.velocity = velocity;
        }
    }

    /// Merges a data watcher list into an entity: the visual flags (entry
    /// 0 and, for players, the skin parts in entry 10) and the typed
    /// metadata. A health of zero or less starts the death fall, as in
    /// vanilla (MCP `EntityLivingBase.onUpdate` on a dead entity).
    ///
    /// The list holds only the entries that changed; the others keep their
    /// value.
    pub fn apply_metadata(&mut self, id: EntityId, metadata: &Metadata) {
        let Some(entity) = self.entities.get_mut(&id) else {
            return;
        };
        if let Some(flags) = metadata.get_byte(ENTITY_FLAGS) {
            entity.flags.flags = flags;
        }
        if entity.kind == EntityKind::Player {
            if let Some(parts) = metadata.get_byte(PLAYER_SKIN_PARTS) {
                entity.flags.skin_parts = parts;
            }
        }
        entity.meta.apply(entity.kind, metadata);
        if entity.kind.is_living() && metadata.get_float(LIVING_HEALTH).is_some_and(|h| h <= 0.0) {
            entity.animation.die();
        }
    }

    /// Gives every tracked player with `uuid` a profile (name, skin, cape).
    ///
    /// Used when a player spawns before or after its tab list entry.
    pub fn set_profile(&mut self, uuid: [u8; 16], profile: PlayerProfile) {
        let profile = Arc::new(profile);
        for entity in self.entities.values_mut() {
            if entity.uuid == Some(uuid) {
                entity.profile = Some(Arc::clone(&profile));
            }
        }
    }

    /// Recomputes the name tag of every player and named mob with `rules`,
    /// which maps an entity to its tag text (see [`nametag`](super::nametag)).
    ///
    /// Called before each frame is published, because teams change at any
    /// time; the tag text is shared, so an unchanged tag costs nothing.
    pub fn refresh_name_tags(&mut self, rules: impl Fn(&EntityRecord) -> Option<String>) {
        for entity in self.entities.values_mut() {
            let tag = rules(entity);
            if entity.name_tag.as_deref() != tag.as_deref() {
                entity.name_tag = tag.map(Arc::from);
            }
        }
    }

    /// Updates an entity's visual flags.
    pub fn set_flags(&mut self, id: EntityId, flags: EntityFlags) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.flags = flags;
        }
    }

    /// Updates a piece of worn armour: `slot` is the `Entity Equipment` slot
    /// (1 boots to 4 helmet); other slots are ignored.
    pub fn set_armor(&mut self, id: EntityId, slot: i16, item: Option<EntityItem>) {
        let Some(index) = usize::try_from(slot - 1).ok().filter(|&i| i < 4) else {
            return;
        };
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.armor[index] = item;
        }
    }

    /// Updates the stack an entity holds in its hand.
    pub fn set_held(&mut self, id: EntityId, held: Option<EntityItem>) {
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.held = held;
            // MCP `EntityOtherPlayerMP.onUpdate`: a use of another item
            // starts over.
            entity.use_ticks = 0;
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

    /// Applies an `Entity Status` that animates: `2` hurts (and makes the
    /// legs jolt, MCP `handleStatusUpdate`), `3` kills. Other codes are not
    /// animations and are ignored.
    pub fn status(&mut self, id: EntityId, status: i8) {
        let Some(entity) = self.entities.get_mut(&id) else {
            return;
        };
        match status {
            2 => {
                entity.limbs.amount = HURT_LIMB_AMOUNT;
                entity.animation.hurt();
            }
            3 => entity.animation.die(),
            // MCP `EntityPlayer.handleStatusUpdate`: the item use finished.
            9 => entity.use_ticks = 0,
            _ => {}
        }
    }

    /// Applies an `Animation` packet: `0` swings the arm, `1` hurts, `2`
    /// wakes a sleeper. The critical-hit particles (`4`, `5`) belong to the
    /// particle system.
    pub fn play_animation(&mut self, id: EntityId, animation: u8) {
        let Some(entity) = self.entities.get_mut(&id) else {
            return;
        };
        match animation {
            0 => entity.animation.swing_item(),
            1 => entity.animation.hurt(),
            2 => entity.animation.bed = None,
            _ => {}
        }
    }

    /// Puts a player to bed (`Use Bed`): the player lies at the head end of
    /// the bed block at `bed`, whose head points towards `facing`
    /// (`0` south, `1` west, `2` north, `3` east).
    pub fn sleep(&mut self, id: EntityId, bed: DVec3, facing: u8) {
        if let Some(entity) = self.entities.get_mut(&id) {
            let pose = BedPose::from_facing(facing);
            let position = bed + pose.head_offset.as_dvec3();
            entity.position = position;
            entity.previous_position = position;
            entity.target_position = position;
            entity.animation.bed = Some(pose);
        }
    }

    /// Advances the simulation by one tick (20 Hz): glide to the server
    /// position, then animate limbs and body (MCP `EntityLivingBase.onUpdate`).
    pub fn tick(&mut self) {
        for entity in self.entities.values_mut() {
            entity.begin_tick();
            entity.glide_to_target();
            entity.animate();
            entity.animation.tick();
            if let Variant::Creeper(creeper) = &entity.meta.variant {
                // A lit creeper fuses whatever its state says.
                let state = if creeper.ignited { 1 } else { creeper.state };
                entity.animation.tick_fuse(state);
            }
            entity.tick_use();
            entity.age_ticks = entity.age_ticks.saturating_add(1);
        }
        self.entities
            .retain(|_, entity| !entity.animation.is_finished());
    }

    /// Produces interpolated render snapshots for all tracked entities.
    ///
    /// `alpha` is the partial tick fraction in `0.0..=1.0`.
    #[must_use]
    pub fn snapshots(&self, alpha: f32) -> Vec<EntityRenderSnapshot> {
        self.entities
            .values()
            .map(|entity| entity.frame_data().interpolate(alpha))
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
    fn equipment_slots_one_to_four_fill_the_armor_boots_first() {
        let mut store = EntityStore::new();
        let id = EntityId(10);
        store.spawn_player(
            id,
            [1u8; 16],
            DVec3::ZERO,
            0.0,
            0.0,
            None,
            EntityFlags::default(),
        );
        store.set_armor(id, 4, Some(sword()));
        store.set_armor(id, 1, Some(sword()));
        store.set_armor(id, 5, Some(sword()));
        store.set_armor(id, 0, Some(sword()));
        let armor = &store.snapshots(0.0)[0].armor;
        assert!(armor[0].is_some() && armor[3].is_some());
        assert!(
            armor[1].is_none() && armor[2].is_none(),
            "slots 0 and 5 are not armour"
        );
        store.set_armor(id, 4, None);
        assert!(store.snapshots(0.0)[0].armor[3].is_none());
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

    fn zombie(store: &mut EntityStore) -> EntityId {
        let id = EntityId(30);
        store.spawn_mob(
            id,
            54,
            DVec3::new(0.0, 64.0, 0.0),
            0.0,
            0.0,
            0.0,
            Vec3::ZERO,
            EntityFlags::default(),
        );
        id
    }

    #[test]
    fn a_teleport_is_interpolated_instead_of_snapping() {
        let mut store = EntityStore::new();
        let id = zombie(&mut store);
        store.teleport(id, DVec3::new(6.0, 64.0, 0.0), 0.0, 0.0, true);
        assert_eq!(store.snapshots(1.0)[0].position.x, 0.0);
        store.tick();
        assert!((store.snapshots(1.0)[0].position.x - 2.0).abs() < 1e-4);
    }

    #[test]
    fn a_head_look_applies_at_once() {
        let mut store = EntityStore::new();
        let id = zombie(&mut store);
        store.set_head_yaw(id, 50.0);
        assert_eq!(store.snapshots(1.0)[0].head_yaw, 50.0);
    }

    #[test]
    fn a_relative_move_stops_a_turn_in_progress() {
        let mut store = EntityStore::new();
        let id = zombie(&mut store);
        store.look(id, 90.0, 0.0, true);
        store.tick();
        let turned = store.snapshots(1.0)[0].yaw;
        store.move_relative(id, DVec3::ZERO, true);
        store.tick();
        store.tick();
        assert!((store.snapshots(1.0)[0].yaw - turned).abs() < 1e-4);
    }

    #[test]
    fn a_walking_mob_swings_its_limbs_in_the_snapshot() {
        let mut store = EntityStore::new();
        let id = zombie(&mut store);
        store.move_relative(id, DVec3::new(0.6, 0.0, 0.0), true);
        store.tick();
        let snapshot = &store.snapshots(1.0)[0];
        assert!(snapshot.limb_swing_amount > 0.0);
        assert!(snapshot.limb_swing > 0.0);
    }

    #[test]
    fn a_hurt_status_flashes_the_entity_and_jolts_its_legs() {
        let mut store = EntityStore::new();
        let id = zombie(&mut store);
        store.status(id, 2);
        let snapshot = &store.snapshots(1.0)[0];
        assert!(snapshot.hurt);
        assert_eq!(snapshot.limb_swing_amount, 1.5);
        for _ in 0..10 {
            store.tick();
        }
        assert!(!store.snapshots(1.0)[0].hurt);
    }

    #[test]
    fn a_death_status_removes_the_entity_after_the_fall() {
        let mut store = EntityStore::new();
        let id = zombie(&mut store);
        store.status(id, 3);
        store.tick();
        assert!(store.snapshots(0.0)[0].death_time > 0.0);
        for _ in 0..19 {
            store.tick();
        }
        assert!(store.is_empty());
    }

    #[test]
    fn other_statuses_do_not_animate() {
        let mut store = EntityStore::new();
        let id = zombie(&mut store);
        store.status(id, 9);
        let snapshot = &store.snapshots(1.0)[0];
        assert!(!snapshot.hurt);
        assert_eq!(snapshot.death_time, 0.0);
    }

    #[test]
    fn an_animation_packet_swings_the_arm() {
        let mut store = EntityStore::new();
        let id = zombie(&mut store);
        store.play_animation(id, 0);
        store.tick();
        store.tick();
        assert!(store.snapshots(1.0)[0].swing_progress > 0.0);
    }

    #[test]
    fn a_sleeping_player_lies_in_the_bed_until_woken() {
        let mut store = EntityStore::new();
        let id = EntityId(7);
        store.spawn_player(
            id,
            [3u8; 16],
            DVec3::ZERO,
            0.0,
            0.0,
            None,
            EntityFlags::default(),
        );
        store.sleep(id, DVec3::new(10.0, 64.0, 20.0), 0);
        let asleep = &store.snapshots(1.0)[0];
        assert_eq!(asleep.bed.map(|bed| bed.orientation), Some(90.0));
        assert!((asleep.position.y - 64.6875).abs() < 1e-4);
        assert!((asleep.position.z - 20.9).abs() < 1e-4);
        store.play_animation(id, 2);
        assert!(store.snapshots(1.0)[0].bed.is_none());
    }

    fn watched(
        index: u8,
        type_id: u8,
        value: Vec<u8>,
    ) -> umicraft_protocol::packets::play::metadata::WatchedObject {
        umicraft_protocol::packets::play::metadata::WatchedObject {
            type_id,
            index,
            value,
        }
    }

    #[test]
    fn a_partial_metadata_update_keeps_the_other_flags() {
        let mut store = EntityStore::new();
        let id = EntityId(8);
        store.spawn_player(
            id,
            [4u8; 16],
            DVec3::ZERO,
            0.0,
            0.0,
            None,
            EntityFlags::new(0x02, 0x7F),
        );
        // Only the health changes: the sneak flag and the skin parts stay.
        let update = Metadata {
            objects: vec![watched(6, 3, 10.0f32.to_be_bytes().to_vec())],
        };
        store.apply_metadata(id, &update);
        let snapshot = &store.snapshots(1.0)[0];
        assert!(snapshot.flags.is_sneaking());
        assert_eq!(snapshot.flags.skin_parts, 0x7F);
        assert_eq!(snapshot.meta.living.health, 10.0);
    }

    #[test]
    fn zero_health_in_the_metadata_kills_the_entity() {
        let mut store = EntityStore::new();
        let id = zombie(&mut store);
        let update = Metadata {
            objects: vec![watched(6, 3, 0.0f32.to_be_bytes().to_vec())],
        };
        store.apply_metadata(id, &update);
        store.tick();
        assert!(store.snapshots(0.0)[0].death_time > 0.0);
    }

    #[test]
    fn the_skin_parts_entry_of_a_non_player_is_not_a_skin() {
        let mut store = EntityStore::new();
        let id = zombie(&mut store);
        let update = Metadata {
            objects: vec![watched(10, 0, vec![0x7F])],
        };
        store.apply_metadata(id, &update);
        assert_eq!(store.snapshots(0.0)[0].flags.skin_parts, 0);
    }

    #[test]
    fn a_profile_reaches_the_players_with_its_uuid_only() {
        let mut store = EntityStore::new();
        for (id, uuid) in [(1, [1u8; 16]), (2, [2u8; 16])] {
            store.spawn_player(
                EntityId(id),
                uuid,
                DVec3::ZERO,
                0.0,
                0.0,
                None,
                EntityFlags::default(),
            );
        }
        store.set_profile([2u8; 16], PlayerProfile::named("Alex"));
        let snapshots = store.snapshots(1.0);
        assert!(snapshots[0].profile.is_none());
        assert_eq!(
            snapshots[1].profile.as_ref().map(|p| &*p.name),
            Some("Alex")
        );
    }

    #[test]
    fn the_camera_view_of_an_unknown_entity_is_none() {
        assert!(EntityStore::new().camera_view(EntityId(9), 1.0).is_none());
    }

    #[test]
    fn the_use_of_the_held_item_is_counted_while_the_flag_is_set() {
        let mut store = EntityStore::new();
        let id = EntityId(3);
        store.spawn_player(
            id,
            [3; 16],
            DVec3::ZERO,
            0.0,
            0.0,
            Some(sword()),
            EntityFlags::default(),
        );
        store.tick();
        assert_eq!(store.snapshots(1.0)[0].use_ticks, 0);
        store.set_flags(id, EntityFlags::new(0x10, 0));
        (0..5).for_each(|_| store.tick());
        assert_eq!(store.snapshots(1.0)[0].use_ticks, 5);
        store.status(id, 9);
        assert_eq!(store.snapshots(1.0)[0].use_ticks, 0);
        store.tick();
        store.set_flags(id, EntityFlags::default());
        store.tick();
        assert_eq!(store.snapshots(1.0)[0].use_ticks, 0);
    }

    #[test]
    fn a_bobber_marks_its_angler_until_it_is_destroyed() {
        let mut store = EntityStore::new();
        store.spawn_player(
            EntityId(3),
            [3; 16],
            DVec3::ZERO,
            0.0,
            0.0,
            None,
            EntityFlags::default(),
        );
        let fishing = |store: &EntityStore| store.snapshots(1.0)[0].fishing;
        assert!(!fishing(&store));
        spawn_object_at_origin(&mut store, 40, 90, 3);
        assert!(fishing(&store));
        store.destroy(&[40]);
        assert!(!fishing(&store));
    }
}
