// SPDX-License-Identifier: MIT OR Apache-2.0

//! The packets about entities: spawns, moves, metadata, statuses and
//! animations.

use glam::DVec3;
use umicraft_game::combat::HitStar;
use umicraft_game::entity::{EntityFlags, EntityId, EntityItem, EntityKind, EntityStore};
use umicraft_protocol::packets::play::clientbound::{
    Animation, AttachEntity, CollectItem, DestroyEntities, EntityEquipment, EntityHeadLook,
    EntityLook, EntityLookMove, EntityMetadata, EntityRelativeMove, EntityStatus, EntityTeleport,
    EntityVelocity, SpawnGlobalEntity, SpawnMob, SpawnObject, SpawnPainting, SpawnPlayer,
    SpawnXpOrb, UseBed,
};
use umicraft_protocol::packets::play::Metadata;
use umicraft_world::BlockPos;

use super::wire;
use crate::game::combat;
use crate::game::particles::spawn_status_particles;
use crate::game::state::Game;
use crate::profiles::profile_from_properties;

/// The `Entity Status` that ends the use of an item
/// (MCP `EntityPlayer.handleStatusUpdate`, id 9).
pub(in crate::game) const ITEM_USE_FINISHED: i8 = 9;

/// The data watcher index of the player score (MCP `EntityPlayer` line 160).
pub(in crate::game) const SCORE_WATCHER: u8 = 18;

pub(in crate::game) const ABSORPTION_WATCHER: u8 = 17;

/// Data watcher of the stack of a dropped item (MCP `EntityItem`, watcher 10).
pub(in crate::game) const DROPPED_ITEM_WATCHER: u8 = 10;

/// Data watcher of the stack inside an item frame (MCP `EntityItemFrame`, 8).
pub(in crate::game) const FRAME_ITEM_WATCHER: u8 = 8;

/// Data watcher of the rotation step inside an item frame (MCP
/// `EntityItemFrame`, 9); only the low three bits are meaningful.
pub(in crate::game) const FRAME_ROTATION_WATCHER: u8 = 9;

/// Applies the item-carrying data watchers of dropped items and item frames.
pub(in crate::game) fn apply_item_metadata(
    entities: &mut EntityStore,
    id: EntityId,
    metadata: &Metadata,
) {
    match entities.kind(id) {
        Some(EntityKind::Item) => {
            if let Some(stack) = metadata.get_item_stack(DROPPED_ITEM_WATCHER) {
                entities.set_item(id, EntityItem::from_stack(&stack));
            }
        }
        Some(EntityKind::ItemFrame) => {
            if let Some(stack) = metadata.get_item_stack(FRAME_ITEM_WATCHER) {
                entities.set_item(id, EntityItem::from_stack(&stack));
            }
            if let Some(rotation) = metadata.get_byte(FRAME_ROTATION_WATCHER) {
                entities.set_frame_rotation(id, rotation);
            }
        }
        _ => {}
    }
}

/// Block id of the bed (MCP `Blocks.bed`).
pub(in crate::game) const BED_BLOCK: u16 = 26;

/// Lays a player in the bed of a `Use Bed` packet, facing the way the bed
/// block's metadata says (MCP `EntityPlayer.trySleep`).
pub(in crate::game) fn put_to_bed(game: &mut Game, packet: &UseBed) {
    let facing = game
        .world
        .as_ref()
        .map(|world| world.block(BlockPos::new(packet.x, packet.y, packet.z)))
        .filter(|state| state.id() == BED_BLOCK)
        .map_or(0, |state| state.metadata() & 3);
    let bed = DVec3::new(
        f64::from(packet.x),
        f64::from(packet.y),
        f64::from(packet.z),
    );
    game.entities.sleep(EntityId(packet.entity_id), bed, facing);
}

/// Registers the bobber `id` on the local player when they cast it (MCP
/// `NetHandlerPlayClient.handleSpawnObject`: the angler of a new bobber gets
/// it as `fishEntity`, the local player included). The entity store does not
/// hold the local player, so it cannot do it: without this the rod of the
/// local player never shows its cast model.
pub(in crate::game) fn hook_local_angler(
    game: &mut Game,
    id: EntityId,
    object_type: u8,
    angler: i32,
) {
    if EntityKind::from_object_type(object_type) == EntityKind::FishHook && angler == game.entity_id
    {
        game.body.bobber = Some(id);
    }
}

/// `Entity Teleport`: an absolute position and look.
pub(super) fn teleport_entity(game: &mut Game, packet: &EntityTeleport) {
    let pos = wire::position(packet.x, packet.y, packet.z);
    let yaw = wire::angle(packet.yaw);
    let pitch = wire::angle(packet.pitch);
    game.entities.teleport(
        EntityId(packet.entity_id),
        pos,
        yaw,
        pitch,
        packet.on_ground,
    );
}

/// `Entity Equipment`: the held item or a piece of armour of another
/// entity.
pub(super) fn equip_entity(game: &mut Game, packet: &EntityEquipment) {
    if packet.entity_id == game.entity_id {
        // The local player's items come from the inventory windows
        // (ADR-0025), never from `Entity Equipment`.
    } else if packet.slot == 0 {
        game.entities.set_held(
            EntityId(packet.entity_id),
            EntityItem::from_stack(&packet.item),
        );
    } else {
        game.entities.set_armor(
            EntityId(packet.entity_id),
            packet.slot,
            EntityItem::from_stack(&packet.item),
        );
    }
}

/// `Spawn Player`: another player, with the profile of its tab list entry.
pub(super) fn spawn_player(game: &mut Game, packet: &SpawnPlayer) {
    let pos = wire::position(packet.x, packet.y, packet.z);
    let yaw = wire::angle(packet.yaw);
    let pitch = wire::angle(packet.pitch);
    // `Spawn Player` carries an item id only: no damage, count or NBT.
    let held = (packet.held_item > 0).then_some(EntityItem {
        item_id: packet.held_item,
        damage: 0,
        count: 1,
        nbt: None,
    });
    let id = EntityId(packet.entity_id);
    game.entities.spawn_player(
        id,
        packet.uuid,
        pos,
        yaw,
        pitch,
        held,
        EntityFlags::default(),
    );
    game.entities.apply_metadata(id, &packet.metadata);
    let listed = game
        .server
        .player_list()
        .get(u128::from_be_bytes(packet.uuid))
        .map(|info| profile_from_properties(&info.name, &info.properties));
    if let Some(profile) = listed {
        game.entities.set_profile(packet.uuid, profile);
    }
}

/// `Spawn Mob`.
pub(super) fn spawn_mob(game: &mut Game, packet: &SpawnMob) {
    let pos = wire::position(packet.x, packet.y, packet.z);
    let yaw = wire::angle(packet.yaw);
    let pitch = wire::angle(packet.pitch);
    let head_yaw = wire::angle(packet.head_yaw);
    let vel = wire::velocity(packet.velocity_x, packet.velocity_y, packet.velocity_z);
    let id = EntityId(packet.entity_id);
    game.entities.spawn_mob(
        id,
        packet.mob_type,
        pos,
        yaw,
        pitch,
        head_yaw,
        vel,
        EntityFlags::default(),
    );
    game.entities.apply_metadata(id, &packet.metadata);
    if let Some(health) = packet.metadata.get_float(6) {
        game.entities.set_health(id, health);
    }
}

/// `Spawn Object`: vehicles, projectiles, items and the like.
pub(super) fn spawn_object(game: &mut Game, packet: &SpawnObject) {
    let pos = wire::position(packet.x, packet.y, packet.z);
    let pitch = wire::angle(packet.pitch);
    let yaw = wire::angle(packet.yaw);
    let vel = wire::velocity(packet.velocity_x, packet.velocity_y, packet.velocity_z);
    game.entities.spawn_object(
        EntityId(packet.entity_id),
        packet.object_type,
        pos,
        pitch,
        yaw,
        vel,
        packet.data,
    );
    hook_local_angler(
        game,
        EntityId(packet.entity_id),
        packet.object_type,
        packet.data,
    );
}

/// MCP `handleCollectItem`: the collected entity flies to the collector
/// in three ticks and leaves the world. The pop is not played.
pub(super) fn collect_item(game: &mut Game, packet: &CollectItem) {
    let local = game
        .state
        .as_ref()
        .map_or(DVec3::ZERO, |state| state.position());
    game.entities.collect(
        EntityId(packet.collected_entity_id),
        EntityId(packet.collector_entity_id),
        local,
    );
}

/// MCP `handleSpawnGlobalEntity`: type 1 is a lightning bolt, the only
/// global entity of 1.8.
pub(super) fn spawn_lightning(game: &mut Game, packet: &SpawnGlobalEntity) {
    let pos = wire::position(packet.x, packet.y, packet.z);
    game.entities
        .spawn_lightning(EntityId(packet.entity_id), pos);
}

/// `Spawn Experience Orb`.
pub(super) fn spawn_xp_orb(game: &mut Game, packet: &SpawnXpOrb) {
    let pos = wire::position(packet.x, packet.y, packet.z);
    game.entities
        .spawn_xp_orb(EntityId(packet.entity_id), pos, packet.count);
}

/// `Spawn Painting`.
pub(super) fn spawn_painting(game: &mut Game, packet: &SpawnPainting) {
    game.entities.spawn_painting(
        EntityId(packet.entity_id),
        &packet.title,
        [packet.x, packet.y, packet.z],
        packet.facing,
    );
}

/// `Destroy Entities`, which may take the local player's bobber.
pub(super) fn destroy_entities(game: &mut Game, packet: &DestroyEntities) {
    game.entities.destroy(&packet.entity_ids);
    if game
        .body
        .bobber
        .is_some_and(|bobber| packet.entity_ids.contains(&bobber.0))
    {
        game.body.bobber = None;
    }
}

/// `Entity Relative Move`.
pub(super) fn move_entity(game: &mut Game, packet: &EntityRelativeMove) {
    let delta = wire::position(packet.dx, packet.dy, packet.dz);
    game.entities
        .move_relative(EntityId(packet.entity_id), delta, packet.on_ground);
}

/// `Entity Look`.
pub(super) fn turn_entity(game: &mut Game, packet: &EntityLook) {
    let yaw = wire::angle(packet.yaw);
    let pitch = wire::angle(packet.pitch);
    game.entities
        .look(EntityId(packet.entity_id), yaw, pitch, packet.on_ground);
}

/// `Entity Look And Relative Move`.
pub(super) fn move_and_turn_entity(game: &mut Game, packet: &EntityLookMove) {
    let delta = wire::position(packet.dx, packet.dy, packet.dz);
    let yaw = wire::angle(packet.yaw);
    let pitch = wire::angle(packet.pitch);
    game.entities.look_move(
        EntityId(packet.entity_id),
        delta,
        yaw,
        pitch,
        packet.on_ground,
    );
}

/// `Entity Head Look`.
pub(super) fn turn_entity_head(game: &mut Game, packet: &EntityHeadLook) {
    let head_yaw = wire::angle(packet.head_yaw);
    game.entities
        .set_head_yaw(EntityId(packet.entity_id), head_yaw);
}

/// `Entity Velocity` of another entity.
pub(super) fn set_entity_velocity(game: &mut Game, packet: &EntityVelocity) {
    let vel = wire::velocity(packet.velocity_x, packet.velocity_y, packet.velocity_z);
    game.entities.set_velocity(EntityId(packet.entity_id), vel);
}

/// `Entity Metadata`, with the score and the absorption of the local
/// player.
pub(super) fn apply_entity_metadata(game: &mut Game, packet: &EntityMetadata) {
    if packet.entity_id == game.entity_id {
        // MCP EntityPlayer.getScore: data watcher 18.
        if let Some(score) = packet.metadata.get_int(SCORE_WATCHER) {
            game.score = score;
            game.hud_dirty = true;
        }
        // MCP `EntityPlayer.entityInit`: the absorption hearts.
        if let Some(absorption) = packet.metadata.get_float(ABSORPTION_WATCHER) {
            game.absorption = absorption;
            game.hud_dirty = true;
        }
        game.overlays.apply_metadata(&packet.metadata);
    }
    let id = EntityId(packet.entity_id);
    game.entities.apply_metadata(id, &packet.metadata);
    apply_item_metadata(&mut game.entities, id, &packet.metadata);
    if let Some(health) = packet.metadata.get_float(6) {
        game.entities.set_health(id, health);
    }
}

/// `Attach Entity`: riding (leash 0) and leashes (leash 1).
pub(super) fn attach_entity(game: &mut Game, packet: &AttachEntity) {
    // MCP `NetHandlerPlayClient.handleEntityAttach`: leash 0 mounts
    // (vehicle -1 dismounts); only the local player's riding changes
    // the serverbound traffic. Passengers of other entities are P5.
    if packet.leash == 0 && packet.entity_id == game.entity_id {
        game.vehicle = (packet.vehicle_id != -1).then_some(packet.vehicle_id);
    }
    // Leash 1 ties the entity to the holder (-1 frees it).
    if packet.leash == 1 {
        let holder = (packet.vehicle_id != -1).then_some(EntityId(packet.vehicle_id));
        game.entities.set_leash(EntityId(packet.entity_id), holder);
    }
    if packet.leash == 0 {
        let vehicle = (packet.vehicle_id != -1).then_some(EntityId(packet.vehicle_id));
        game.entities.attach(EntityId(packet.entity_id), vehicle);
    }
}

/// `Entity Status`: the end of an item use, hurts, deaths and their
/// particles.
pub(super) fn apply_entity_status(game: &mut Game, packet: &EntityStatus) {
    // MCP `EntityPlayer.handleStatusUpdate`: 9 finishes the use of
    // the item; 2 and 3 hurt and kill an entity.
    if packet.status == ITEM_USE_FINISHED && packet.entity_id == game.entity_id {
        game.item_use.finish();
    }
    // `handleStatusUpdate(2)` of the local player: the hurt shake of the
    // camera and the resistance that flashes the hearts.
    if packet.entity_id == game.entity_id && packet.status == 2 {
        game.body.animation.hurt_by_status();
    }
    // Only the local player hears its own packet: other entities'
    // sounds arrive as Sound Effect packets (`sound::entity_sounds`).
    if packet.entity_id == game.entity_id {
        if let Some(state) = game.state.as_mut() {
            state.on_local_player_status(packet.status);
        }
    }
    spawn_status_particles(game, EntityId(packet.entity_id), packet.status);
    game.entities
        .status(EntityId(packet.entity_id), packet.status);
}

/// `Animation`: swings, hurts and the stars of a critical hit.
pub(super) fn play_animation(game: &mut Game, packet: &Animation) {
    // `performHurtAnimation` of the local player.
    if packet.entity_id == game.entity_id && packet.animation == 1 {
        game.body.animation.hurt();
    }
    // `handleAnimation` 4 and 5: the stars of a critical hit on that
    // entity.
    if let Some(star) = HitStar::from_animation(packet.animation) {
        combat::emit_stars(game, EntityId(packet.entity_id), star);
    }
    game.entities
        .play_animation(EntityId(packet.entity_id), packet.animation);
}
