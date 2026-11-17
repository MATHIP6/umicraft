// SPDX-License-Identifier: MIT OR Apache-2.0

//! The clientbound packets: how each decoded [`PlayEvent`] changes the
//! game state.

use std::sync::mpsc::SyncSender;

use glam::{DVec3, Vec3};
use umicraft_game::chat::Position;
use umicraft_game::combat::HitStar;
use umicraft_game::entity::nametag::mob_tag;
use umicraft_game::entity::{
    EntityFlags, EntityId, EntityItem, EntityKind, EntityRecord, EntityStore,
};
use umicraft_game::gamemode::{GameMode, GAMEMODE_REASON};
use umicraft_game::hud::boss_bar::BossBar;
use umicraft_game::hud::team_names::{player_label, sees_team_ghost};
use umicraft_game::interaction::{BlockAck, Interaction, Outgoing, PositionAck};
use umicraft_game::inventory::payload;
use umicraft_game::inventory::{WindowKind, WindowManager};
use umicraft_game::item_use::ItemUse;
use umicraft_game::particles::{ParticleKind, SpawnRequest};
use umicraft_game::piston::motion;
use umicraft_game::player_list::PlayerList;
use umicraft_game::scoreboard::Scoreboard;
use umicraft_game::server::ServerState;
use umicraft_game::sound::cue::{CueCategory, SoundCue};
use umicraft_game::title::{ActionBar, TitleState};
use umicraft_game::traffic::ServerView;
use umicraft_game::{GameState, ServerCorrection};
use umicraft_net::play::PlayEvent;
use umicraft_protocol::packets::play::clientbound::player_list_item::PlayerListItem;
use umicraft_protocol::packets::play::clientbound::{ChunkData, Explosion, MapChunkBulk, UseBed};
use umicraft_protocol::packets::play::Metadata;
use umicraft_world::{BlockPos, BlockState, ChunkPayload, ChunkPos, World};

use super::abilities::{apply_mode_rules, set_gamemode};
use super::chat::show_chat;
use super::combat;
use super::commands::{command_of, CommandQueue};
use super::container_events::apply_trade_list;
use super::messages::RenderEvent;
use super::particles::{apply_effect, particle_filter, spawn_particles, spawn_status_particles};
use super::publish::{publish_inventory, publish_server_pack, publish_stats, publish_status};
use super::state::Game;
use crate::network::ClientCommand;
use crate::profiles::profile_from_properties;

/// The `Entity Status` that ends the use of an item
/// (MCP `EntityPlayer.handleStatusUpdate`, id 9).
const ITEM_USE_FINISHED: i8 = 9;

/// The data watcher index of the player score (MCP `EntityPlayer` line 160).
const SCORE_WATCHER: u8 = 18;
const ABSORPTION_WATCHER: u8 = 17;

/// Data watcher of the stack of a dropped item (MCP `EntityItem`, watcher 10).
const DROPPED_ITEM_WATCHER: u8 = 10;

/// Data watcher of the stack inside an item frame (MCP `EntityItemFrame`, 8).
const FRAME_ITEM_WATCHER: u8 = 8;

/// Data watcher of the rotation step inside an item frame (MCP
/// `EntityItemFrame`, 9); only the low three bits are meaningful.
const FRAME_ROTATION_WATCHER: u8 = 9;

/// The wire units of `Entity Velocity` per block per tick
/// (MCP `handleEntityVelocity`: `/ 8000`).
const VELOCITY_UNITS: f64 = 8000.0;

/// Applies the item-carrying data watchers of dropped items and item frames.
fn apply_item_metadata(entities: &mut EntityStore, id: EntityId, metadata: &Metadata) {
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

/// The name tag text of an entity under the team rules, if it has one.
///
/// A player's label may hold a second line, the below-name score; the two
/// lines are one tag, split by `place_tags`.
fn entity_tag(scoreboard: &Scoreboard, local: &str, entity: &EntityRecord) -> Option<String> {
    if entity.kind == EntityKind::Player {
        let name = &entity.profile.as_ref()?.name;
        return player_label(scoreboard, local, name, entity.flags.is_invisible());
    }
    mob_tag(
        entity.meta.custom_name.as_deref(),
        entity.meta.custom_name_visible,
    )
}

/// Whether the local player sees an invisible player as a ghost (MCP
/// `Entity.isInvisibleToPlayer`).
fn is_team_ghost(scoreboard: &Scoreboard, local: &str, entity: &EntityRecord) -> bool {
    entity
        .profile
        .as_ref()
        .filter(|_| entity.kind == EntityKind::Player)
        .is_some_and(|profile| sees_team_ghost(scoreboard, local, &profile.name))
}

/// Block id of the bed (MCP `Blocks.bed`).
const BED_BLOCK: u16 = 26;

/// Lays a player in the bed of a `Use Bed` packet, facing the way the bed
/// block's metadata says (MCP `EntityPlayer.trySleep`).
fn put_to_bed(game: &mut Game, packet: &UseBed) {
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

/// The game mode a tab list entry has for a player in spectator mode.
const SPECTATOR_WIRE: i32 = 3;

/// Handles one decoded clientbound packet.
pub(super) fn handle_net(
    game: &mut Game,
    event: PlayEvent,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
) {
    // What a name tag shows depends on the teams, the tab list and the
    // metadata of the entity.
    let changes_name_tags = matches!(
        event,
        PlayEvent::Teams(_)
            | PlayEvent::ScoreboardObjective(_)
            | PlayEvent::UpdateScore(_)
            | PlayEvent::DisplayScoreboard(_)
            | PlayEvent::PlayerListItem(_)
            | PlayEvent::SpawnPlayer(_)
            | PlayEvent::SpawnMob(_)
            | PlayEvent::EntityMetadata(_)
    );
    let answers_stats = matches!(event, PlayEvent::Statistics(_));
    let offers_pack = matches!(event, PlayEvent::ResourcePackSend(_));
    apply_net_event(game, event, queue, frames);
    if offers_pack {
        publish_server_pack(game, frames);
    }
    if answers_stats {
        publish_stats(game, frames);
    }
    if changes_name_tags {
        let players = &game.player_list;
        game.entities.refresh_spectators(|uuid| {
            players
                .get(&u128::from_be_bytes(*uuid))
                .is_some_and(|entry| entry.game_mode == SPECTATOR_WIRE)
        });
        let (scoreboard, own_name) = (&game.scoreboard, game.own_name.as_str());
        game.entities
            .refresh_name_tags(|entity| entity_tag(scoreboard, own_name, entity));
        game.entities
            .refresh_ghosts(|entity| is_team_ghost(scoreboard, own_name, entity));
    }
}

fn apply_net_event(
    game: &mut Game,
    event: PlayEvent,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
) {
    game.hud_view.observe(&event, game.entity_id);
    match event {
        PlayEvent::JoinGame(info) => {
            game.world = Some(World::new(info.dimension));
            game.dirty.clear();
            game.entities.clear();
            game.body.bobber = None;
            game.entity_id = info.entity_id;
            game.hardcore = info.gamemode & 8 != 0;
            game.absorption = 0.0;
            game.server.on_join(info.entity_id);
            game.server_view = ServerView::default();
            game.vehicle = None;
            // `handleJoinGame` makes a new `PlayerControllerMP` and a new
            // player, and closes the screen: nothing of the old session's dig,
            // item use, pending click or open window survives a server switch.
            game.interaction = Interaction::new();
            game.item_use = ItemUse::new();
            game.attack_pressed = false;
            game.use_pressed = false;
            game.pending_book = None;
            game.windows = WindowManager::new();
            // `handleJoinGame` makes a new `PlayerControllerMP`
            // (`currentPlayerItem` 0) and the new player starts on slot 0.
            game.selected_slot = 0;
            game.applied_slot = 0;
            game.synced_slot = 0;
            publish_inventory(game, frames);
            set_gamemode(game, GameMode::from_wire(info.gamemode));
            game.dimension = info.dimension;
            game.flat_world = is_flat(&info.level_type);
            // The world clock of the new session is unknown until the first
            // `Time Update`; start at the beginning of the cycle.
            game.world_time = 0;
            game.playing = true;
            game.status = "PLAYING".to_string();
            game.status_data.status = game.status.clone();
            game.player_list = PlayerList::new();
            game.scoreboard = Scoreboard::new();
            game.title = TitleState::new();
            game.action_bar = ActionBar::new();
            game.boss_bar = BossBar::new();
            game.completions.clear();
            game.hud_dirty = true;
            let _ = frames.try_send(RenderEvent::Reset);
            publish_status(game, frames);
        }
        PlayEvent::Placement(placement) => {
            let first = !game.status_data.placed;
            apply_placement(game, &placement, queue, frames);
            if first {
                // The window leaves the connecting screen on this flag, so it
                // must not wait for the periodic status.
                publish_status(game, frames);
            }
        }
        PlayEvent::Chat(json) => show_chat(game, &json, Position::Chat),
        PlayEvent::SystemChat(json) => show_chat(game, &json, Position::System),
        PlayEvent::ActionBar(json) => {
            game.action_bar.show(&json);
            game.hud_dirty = true;
        }
        PlayEvent::PlayerListItem(packet) => {
            game.player_list.apply(&packet);
            game.server.apply_player_list(&packet);
            if let PlayerListItem::AddPlayer(entries) = &packet {
                for entry in entries {
                    let profile = profile_from_properties(&entry.name, &entry.properties);
                    // A skin plugin re-sends the own entry once the skin is
                    // applied, so the local body must follow the update.
                    if entry.name == game.own_name {
                        game.body.uuid = Some(entry.uuid.to_be_bytes());
                        game.body.profile = Some(std::sync::Arc::new(profile.clone()));
                    }
                    game.entities.set_profile(entry.uuid.to_be_bytes(), profile);
                }
            }
            game.hud_dirty = true;
        }
        PlayEvent::PlayerListHeaderFooter(packet) => {
            game.player_list.set_header_footer(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::TabComplete(packet) => {
            game.completions = packet.matches;
            game.completions_serial += 1;
            game.hud_dirty = true;
        }
        PlayEvent::ScoreboardObjective(packet) => {
            game.scoreboard.apply_objective(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::UpdateScore(packet) => {
            game.scoreboard.apply_score(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::DisplayScoreboard(packet) => {
            game.scoreboard.apply_display(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::Teams(packet) => {
            game.scoreboard.apply_teams(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::Title(packet) => {
            game.title.apply(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::ChunkData(packet) => {
            game.status_data.chunk_packets += 1;
            if let Some(world) = game.world.as_mut() {
                let (pos, payload) = chunk_payload(&packet);
                match world.apply_chunk(pos, payload) {
                    Ok(dirty) => game.dirty.extend(dirty),
                    Err(error) => {
                        tracing::warn!(%error, "chunk data rejected");
                        game.status = format!("WORLD ERROR {error}");
                    }
                }
            }
        }
        PlayEvent::MapChunkBulk(packet) => {
            game.status_data.chunk_packets += 1;
            if let Some(world) = game.world.as_mut() {
                match world.apply_chunk_bulk(packet.sky_light, &bulk_payloads(&packet)) {
                    Ok(dirty) => game.dirty.extend(dirty),
                    Err(error) => {
                        tracing::warn!(%error, "bulk chunk rejected");
                        game.status = format!("WORLD ERROR {error}");
                    }
                }
            }
        }
        PlayEvent::BlockChange(packet) => {
            game.status_data.block_updates += 1;
            let (x, y, z) = packet.position();
            let position = BlockPos::new(x, y, z);
            if let Some(world) = game.world.as_mut() {
                let state = BlockState::from_wire(packet.block_state);
                game.dirty.extend(world.apply_block_change(position, state));
            }
            acknowledge_block(game, position, packet.block_state);
        }
        PlayEvent::MultiBlockChange(packet) => {
            game.status_data.block_updates += packet.records.len() as u64;
            if let Some(world) = game.world.as_mut() {
                let chunk = ChunkPos::new(packet.chunk_x, packet.chunk_z);
                let changes = packet.records.iter().map(|record| {
                    (
                        record.local_position(),
                        BlockState::from_wire(record.block_state),
                    )
                });
                game.dirty.extend(world.apply_block_changes(chunk, changes));
            }
            // The server confirms a dig or a placement with a single block
            // change, but a batch carries the same information.
            for record in &packet.records {
                let (x, y, z) = record.local_position();
                let position = BlockPos::new(
                    packet.chunk_x * 16 + i32::from(x),
                    i32::from(y),
                    packet.chunk_z * 16 + i32::from(z),
                );
                acknowledge_block(game, position, record.block_state);
            }
        }
        PlayEvent::UpdateHealth(packet) => {
            // MCP `EntityPlayerSP.setPlayerSPHealth`: a drop of health hurts, so
            // the camera shakes even where the server sent no `Entity Status 2`.
            if game.health_known && packet.health < game.health {
                game.body.animation.hurt();
            }
            game.health_known = true;
            game.health = packet.health;
            game.food = packet.food as f32;
            game.hud_dirty = true;
        }
        PlayEvent::HeldItemChange(slot) => {
            if (0..9).contains(&slot) {
                game.selected_slot = slot as u8;
                // `handleHeldItemChange` only sets `currentItem`; the
                // controller keeps `currentPlayerItem`, so the next sync
                // tells the server this slot back, like the vanilla client.
                game.applied_slot = slot as u8;
                game.windows.set_selected_slot(slot as u8);
                game.hud_dirty = true;
            }
        }
        PlayEvent::Respawn(packet) => {
            // MCP `handleRespawn` sets the gamemode of the new player.
            set_gamemode(game, GameMode::from_wire(packet.game_mode));
            // A respawned player stands on foot and has told the server
            // nothing yet (MCP `handleRespawn` creates a new player entity,
            // whose sprint and sneak states start off).
            game.vehicle = None;
            // A new player entity: its first health update only sets the health.
            game.health_known = false;
            game.overlays.reset();
            game.server_view = ServerView::default();
            // The new player has a new inventory on slot 0, but the
            // controller is kept: `currentPlayerItem` stays, and the next
            // sync tells the server the slot if it was not 0.
            game.selected_slot = 0;
            game.applied_slot = 0;
            game.windows.set_selected_slot(0);
            // The server keeps the chunks of the dimension, so only a change
            // of the dimension replaces the world.
            let dimension = packet.dimension as i8;
            let changed = game
                .world
                .as_ref()
                .is_none_or(|world| world.dimension() != dimension);
            game.server.on_respawn(changed);
            // `handleRespawn` makes a new player, without a bobber.
            game.body.bobber = None;
            if changed {
                // `handleRespawn` builds the new world with the type of the
                // packet; a same-dimension respawn keeps the old one.
                game.flat_world = is_flat(&packet.world_type);
                game.world = Some(World::new(dimension));
                game.dirty.clear();
                game.entities.clear();
                game.particles.clear();
                // A new world has its own scoreboard; the tab list belongs to
                // the connection and stays.
                game.scoreboard = Scoreboard::new();
                game.hud_dirty = true;
                // The new dimension has its own clock; the first `Time
                // Update` of the new world arrives right after the respawn.
                game.world_time = 0;
                let _ = frames.try_send(RenderEvent::Reset);
            }
            // A same-dimension respawn keeps the current world clock.
            game.dimension = dimension;
            game.state = None;
            game.status = "RESPAWN".to_string();
            // The player comes back with a full health (MCP
            // `NetHandlerPlayClient.handleRespawn`).
            game.health = 20.0;
            game.food = 20.0;
            game.hud_dirty = true;
            game.status_data.status = game.status.clone();
            publish_status(game, frames);
        }
        PlayEvent::EntityTeleport(packet) => {
            let pos = DVec3::new(
                f64::from(packet.x) / 32.0,
                f64::from(packet.y) / 32.0,
                f64::from(packet.z) / 32.0,
            );
            let yaw = f32::from(packet.yaw) * 360.0 / 256.0;
            let pitch = f32::from(packet.pitch) * 360.0 / 256.0;
            game.entities.teleport(
                EntityId(packet.entity_id),
                pos,
                yaw,
                pitch,
                packet.on_ground,
            );
        }
        PlayEvent::EntityEquipment(packet) => {
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
        PlayEvent::SpawnPlayer(p) => {
            let pos = DVec3::new(
                f64::from(p.x) / 32.0,
                f64::from(p.y) / 32.0,
                f64::from(p.z) / 32.0,
            );
            let yaw = f32::from(p.yaw) * 360.0 / 256.0;
            let pitch = f32::from(p.pitch) * 360.0 / 256.0;
            // `Spawn Player` carries an item id only: no damage, count or NBT.
            let held = (p.held_item > 0).then_some(EntityItem {
                item_id: p.held_item,
                damage: 0,
                count: 1,
                nbt: None,
            });
            let id = EntityId(p.entity_id);
            game.entities
                .spawn_player(id, p.uuid, pos, yaw, pitch, held, EntityFlags::default());
            game.entities.apply_metadata(id, &p.metadata);
            let listed = game
                .server
                .player_list()
                .get(u128::from_be_bytes(p.uuid))
                .map(|info| profile_from_properties(&info.name, &info.properties));
            if let Some(profile) = listed {
                game.entities.set_profile(p.uuid, profile);
            }
        }
        PlayEvent::SpawnMob(m) => {
            let pos = DVec3::new(
                f64::from(m.x) / 32.0,
                f64::from(m.y) / 32.0,
                f64::from(m.z) / 32.0,
            );
            let yaw = f32::from(m.yaw) * 360.0 / 256.0;
            let pitch = f32::from(m.pitch) * 360.0 / 256.0;
            let head_yaw = f32::from(m.head_yaw) * 360.0 / 256.0;
            let vel = Vec3::new(
                f32::from(m.velocity_x) / 8000.0,
                f32::from(m.velocity_y) / 8000.0,
                f32::from(m.velocity_z) / 8000.0,
            );
            let id = EntityId(m.entity_id);
            game.entities.spawn_mob(
                id,
                m.mob_type,
                pos,
                yaw,
                pitch,
                head_yaw,
                vel,
                EntityFlags::default(),
            );
            game.entities.apply_metadata(id, &m.metadata);
            if let Some(health) = m.metadata.get_float(6) {
                game.entities.set_health(id, health);
            }
        }
        PlayEvent::SpawnObject(o) => {
            let pos = DVec3::new(
                f64::from(o.x) / 32.0,
                f64::from(o.y) / 32.0,
                f64::from(o.z) / 32.0,
            );
            let pitch = f32::from(o.pitch) * 360.0 / 256.0;
            let yaw = f32::from(o.yaw) * 360.0 / 256.0;
            let vel = Vec3::new(
                f32::from(o.velocity_x) / 8000.0,
                f32::from(o.velocity_y) / 8000.0,
                f32::from(o.velocity_z) / 8000.0,
            );
            game.entities.spawn_object(
                EntityId(o.entity_id),
                o.object_type,
                pos,
                pitch,
                yaw,
                vel,
                o.data,
            );
            hook_local_angler(game, EntityId(o.entity_id), o.object_type, o.data);
        }
        // MCP `handleCollectItem`: the collected entity flies to the collector
        // in three ticks and leaves the world. The pop is not played.
        PlayEvent::CollectItem(c) => {
            let local = game
                .state
                .as_ref()
                .map_or(DVec3::ZERO, |state| state.position());
            game.entities.collect(
                EntityId(c.collected_entity_id),
                EntityId(c.collector_entity_id),
                local,
            );
        }
        // MCP `handleSpawnGlobalEntity`: type 1 is a lightning bolt, the only
        // global entity of 1.8.
        PlayEvent::SpawnGlobalEntity(g) if g.entity_type == 1 => {
            let pos = DVec3::new(
                f64::from(g.x) / 32.0,
                f64::from(g.y) / 32.0,
                f64::from(g.z) / 32.0,
            );
            game.entities.spawn_lightning(EntityId(g.entity_id), pos);
        }
        PlayEvent::SpawnXpOrb(o) => {
            let pos = DVec3::new(
                f64::from(o.x) / 32.0,
                f64::from(o.y) / 32.0,
                f64::from(o.z) / 32.0,
            );
            game.entities
                .spawn_xp_orb(EntityId(o.entity_id), pos, o.count);
        }
        PlayEvent::SpawnPainting(p) => {
            game.entities.spawn_painting(
                EntityId(p.entity_id),
                &p.title,
                [p.x, p.y, p.z],
                p.facing,
            );
        }
        PlayEvent::DestroyEntities(d) => {
            game.entities.destroy(&d.entity_ids);
            if game
                .body
                .bobber
                .is_some_and(|bobber| d.entity_ids.contains(&bobber.0))
            {
                game.body.bobber = None;
            }
        }
        PlayEvent::EntityRelativeMove(m) => {
            let delta = DVec3::new(
                f64::from(m.dx) / 32.0,
                f64::from(m.dy) / 32.0,
                f64::from(m.dz) / 32.0,
            );
            game.entities
                .move_relative(EntityId(m.entity_id), delta, m.on_ground);
        }
        PlayEvent::EntityLook(l) => {
            let yaw = f32::from(l.yaw) * 360.0 / 256.0;
            let pitch = f32::from(l.pitch) * 360.0 / 256.0;
            game.entities
                .look(EntityId(l.entity_id), yaw, pitch, l.on_ground);
        }
        PlayEvent::EntityLookMove(lm) => {
            let delta = DVec3::new(
                f64::from(lm.dx) / 32.0,
                f64::from(lm.dy) / 32.0,
                f64::from(lm.dz) / 32.0,
            );
            let yaw = f32::from(lm.yaw) * 360.0 / 256.0;
            let pitch = f32::from(lm.pitch) * 360.0 / 256.0;
            game.entities
                .look_move(EntityId(lm.entity_id), delta, yaw, pitch, lm.on_ground);
        }
        PlayEvent::EntityHeadLook(h) => {
            let head_yaw = f32::from(h.head_yaw) * 360.0 / 256.0;
            game.entities.set_head_yaw(EntityId(h.entity_id), head_yaw);
        }
        PlayEvent::EntityVelocity(v) if v.entity_id == game.entity_id => {
            // MCP `handleEntityVelocity` sets the motion of the local player:
            // the knockback of a hit, a fishing rod, a piston.
            if let Some(state) = game.state.as_mut() {
                state.set_velocity(DVec3::new(
                    f64::from(v.velocity_x) / VELOCITY_UNITS,
                    f64::from(v.velocity_y) / VELOCITY_UNITS,
                    f64::from(v.velocity_z) / VELOCITY_UNITS,
                ));
            }
        }
        PlayEvent::EntityVelocity(v) => {
            let vel = Vec3::new(
                f32::from(v.velocity_x) / 8000.0,
                f32::from(v.velocity_y) / 8000.0,
                f32::from(v.velocity_z) / 8000.0,
            );
            game.entities.set_velocity(EntityId(v.entity_id), vel);
        }
        PlayEvent::EntityMetadata(m) => {
            if m.entity_id == game.entity_id {
                // MCP EntityPlayer.getScore: data watcher 18.
                if let Some(score) = m.metadata.get_int(SCORE_WATCHER) {
                    game.score = score;
                    game.hud_dirty = true;
                }
                // MCP `EntityPlayer.entityInit`: the absorption hearts.
                if let Some(absorption) = m.metadata.get_float(ABSORPTION_WATCHER) {
                    game.absorption = absorption;
                    game.hud_dirty = true;
                }
                game.overlays.apply_metadata(&m.metadata);
            }
            let id = EntityId(m.entity_id);
            game.entities.apply_metadata(id, &m.metadata);
            apply_item_metadata(&mut game.entities, id, &m.metadata);
            if let Some(health) = m.metadata.get_float(6) {
                game.entities.set_health(id, health);
            }
        }
        PlayEvent::AttachEntity(packet) => {
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
        PlayEvent::EntityStatus(status) => {
            // MCP `EntityPlayer.handleStatusUpdate`: 9 finishes the use of
            // the item; 2 and 3 hurt and kill an entity.
            if status.status == ITEM_USE_FINISHED && status.entity_id == game.entity_id {
                game.item_use.finish();
            }
            // `handleStatusUpdate(2)` of the local player: the hurt shake of the
            // camera and the resistance that flashes the hearts.
            if status.entity_id == game.entity_id && status.status == 2 {
                game.body.animation.hurt_by_status();
            }
            // Only the local player hears its own status: other entities'
            // sounds arrive as Sound Effect packets (`sound::entity_sounds`).
            if status.entity_id == game.entity_id {
                if let Some(state) = game.state.as_mut() {
                    state.on_local_player_status(status.status);
                }
            }
            spawn_status_particles(game, EntityId(status.entity_id), status.status);
            game.entities
                .status(EntityId(status.entity_id), status.status);
        }
        PlayEvent::Animation(packet) => {
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
        PlayEvent::UseBed(bed) => put_to_bed(game, &bed),
        PlayEvent::SpawnPosition(packet) => {
            game.spawn = Some((packet.x, packet.y, packet.z));
        }
        PlayEvent::SetExperience(packet) => {
            game.experience = packet.experience;
            game.level = packet.level;
            game.hud_dirty = true;
        }
        PlayEvent::TimeUpdate(packet) => {
            // The signed value is kept as-is: a negative time means the
            // server froze the cycle. The client never advances the clock
            // between packets (ADR-0008 §4).
            game.world_time = packet.time_of_day;
        }
        PlayEvent::OpenWindow(packet) => {
            if let Some(kind) = WindowKind::from_type_string(&packet.window_type) {
                game.windows
                    .open_window(packet.window_id, kind, packet.title, packet.slot_count);
                publish_inventory(game, frames);
            } else {
                tracing::warn!(
                    window_type = %packet.window_type,
                    "unknown container type, ignoring"
                );
            }
        }
        PlayEvent::CloseWindow { window_id } => {
            game.windows.close_window(window_id);
            publish_inventory(game, frames);
        }
        PlayEvent::SetSlot(packet) => {
            game.windows
                .apply_set_slot(packet.window_id, packet.slot, packet.item);
            sync_armor(game);
            publish_inventory(game, frames);
        }
        PlayEvent::WindowItems(packet) => {
            game.windows
                .apply_window_items(packet.window_id, &packet.items);
            sync_armor(game);
            publish_inventory(game, frames);
        }
        PlayEvent::WindowProperty(packet) => {
            game.windows
                .apply_property(packet.window_id, packet.property, packet.value);
            publish_inventory(game, frames);
        }
        PlayEvent::ConfirmTransaction(packet) => {
            answer_transaction(game, &packet, queue);
            // The prediction is confirmed or rolled back.
            game.windows.confirm_transaction(
                packet.window_id,
                packet.action_number,
                packet.accepted,
            );
            sync_armor(game);
            publish_inventory(game, frames);
        }
        PlayEvent::Explosion(packet) => apply_explosion(game, &packet),
        PlayEvent::Particle(packet) => spawn_particles(game, &packet),
        PlayEvent::Effect(packet) => {
            if let Some(state) = game.state.as_mut() {
                state.on_effect(&packet);
            }
            apply_effect(game, &packet);
        }
        PlayEvent::SoundEffect(packet) => {
            if let Some(state) = game.state.as_mut() {
                state.on_sound_effect(&packet);
            }
        }
        PlayEvent::PlayerAbilities(packet) => {
            game.server.apply_abilities(&packet);
            apply_mode_rules(game);
        }
        PlayEvent::ChangeGameState(packet) => {
            game.server.apply_game_state(&packet);
            if packet.reason == GAMEMODE_REASON {
                set_gamemode(game, GameMode::from_state_value(packet.value));
            } else if packet.reason == ELDER_GUARDIAN_CURSE_REASON {
                haunt(game);
            }
        }
        PlayEvent::BlockAction(packet) => {
            if let Some(world) = game.world.as_mut() {
                let dirty = motion::start(world, game.server.moving_blocks_mut(), &packet);
                game.dirty.extend(dirty);
            }
            game.server.apply_block_action(&packet);
        }
        PlayEvent::PluginMessage(packet) if packet.channel == payload::TRADE_LIST => {
            apply_trade_list(game, &packet.data, frames);
        }
        other => apply_server_state(&mut game.server, &other),
    }
}

/// The `Change Game State` reason of the elder guardian curse (MCP
/// `NetHandlerPlayClient.handleChangeGameState`: reason 10).
const ELDER_GUARDIAN_CURSE_REASON: u8 = 10;

/// The curse of an elder guardian: its apparition swings in front of the
/// player and the curse sounds (MCP `handleChangeGameState`, reason 10).
fn haunt(game: &mut Game) {
    let Some(feet) = game.state.as_ref().map(GameState::position) else {
        return;
    };
    if let Some(filter) = particle_filter(game) {
        game.particles.spawn(
            &SpawnRequest {
                kind: ParticleKind::MobAppearance,
                arguments: &[],
                ignore_range: true,
                position: feet,
                velocity: DVec3::ZERO,
            },
            &filter,
        );
    }
    if let Some(state) = game.state.as_mut() {
        state.queue_sound_cues([SoundCue {
            name: std::borrow::Cow::Borrowed("mob.guardian.curse"),
            pos: [feet.x, feet.y, feet.z],
            volume: 1.0,
            pitch: 1.0,
            category: CueCategory::Hostile,
        }]);
    }
}

/// Whether the world type the server names is `flat` (`WorldType.FLAT`;
/// `WorldType.parseWorldType` compares the names without regard to case).
fn is_flat(world_type: &str) -> bool {
    world_type.eq_ignore_ascii_case("flat")
}

/// Removes the blocks of an explosion from the world.
///
/// MCP `NetHandlerPlayClient.handleExplosion` runs `Explosion.doExplosionB`
/// on the client, which sets every affected block to air before the server's
/// block changes arrive. The player's knockback from the packet is P2.
pub(super) fn apply_explosion(game: &mut Game, packet: &Explosion) {
    // MCP `handleExplosion`: the push is added to the motion of the player.
    if let Some(state) = game.state.as_mut() {
        state.add_velocity(DVec3::new(
            f64::from(packet.player_motion_x),
            f64::from(packet.player_motion_y),
            f64::from(packet.player_motion_z),
        ));
    }
    let Some(world) = game.world.as_mut() else {
        return;
    };
    let affected: Vec<(i32, i32, i32)> = packet.affected_blocks().collect();
    for &(x, y, z) in &affected {
        game.dirty
            .extend(world.apply_block_change(BlockPos::new(x, y, z), BlockState::AIR));
    }
    if let Some(filter) = particle_filter(game) {
        game.particles.explode(
            DVec3::new(packet.x.into(), packet.y.into(), packet.z.into()),
            packet.strength,
            &affected,
            &filter,
        );
    }
}

/// Keeps the state of the packets whose consumers arrive in later
/// milestones (ADR-0020 §3); purely visual or audible events are dropped
/// here until their milestone (P5 entities, P8 particles, P11 audio).
fn apply_server_state(server: &mut ServerState, event: &PlayEvent) {
    match event {
        PlayEvent::EntityEffect(packet) => server.apply_entity_effect(packet),
        PlayEvent::RemoveEntityEffect(packet) => server.apply_remove_entity_effect(packet),
        PlayEvent::EntityProperties(packet) => server.apply_entity_properties(packet),
        PlayEvent::ServerDifficulty(packet) => server.apply_difficulty(packet),
        PlayEvent::WorldBorder(packet) => server.apply_world_border(packet),
        PlayEvent::PlayerListItem(packet) => server.apply_player_list(packet),
        PlayEvent::PlayerListHeaderFooter(packet) => server.apply_header_footer(packet),
        PlayEvent::ScoreboardObjective(packet) => server.apply_objective(packet),
        PlayEvent::UpdateScore(packet) => server.apply_score(packet),
        PlayEvent::DisplayScoreboard(packet) => server.apply_display(packet),
        PlayEvent::Teams(packet) => server.apply_team(packet),
        PlayEvent::Title(packet) => server.apply_title(packet),
        PlayEvent::UpdateSign(packet) => server.apply_sign(packet),
        PlayEvent::BlockAction(packet) => server.apply_block_action(packet),
        PlayEvent::UpdateBlockEntity(packet) => server.apply_block_entity(packet),
        PlayEvent::BlockBreakAnim(packet) => server.apply_block_break_anim(packet),
        PlayEvent::Maps(packet) => server.apply_map(packet),
        PlayEvent::Statistics(packet) => server.apply_statistics(packet),
        PlayEvent::Camera(packet) => server.apply_camera(packet),
        PlayEvent::PluginMessage(packet) => server.apply_plugin_message(packet),
        PlayEvent::CombatEvent(packet) => server.apply_combat_event(packet),
        PlayEvent::SignEditorOpen(packet) => server.apply_sign_editor_open(packet),
        PlayEvent::ResourcePackSend(packet) => server.apply_resource_pack_offer(packet),
        // Use Bed, Collect Item, the painting/orb/lightning spawns, Effect,
        // Sound Effect, Particle, Block Action, Tab-Complete and Update
        // Entity NBT are events, not state: their consumers render or play
        // them when they arrive.
        _ => {}
    }
}

/// The column and payload of a `Chunk Data` packet.
fn chunk_payload(packet: &ChunkData) -> (ChunkPos, ChunkPayload<'_>) {
    (
        ChunkPos::new(packet.chunk_x, packet.chunk_z),
        ChunkPayload {
            primary_bit_mask: packet.primary_bit_mask,
            ground_up: packet.ground_up,
            data: &packet.data,
        },
    )
}

/// The columns and payloads of a `Map Chunk Bulk` packet; every bulk entry
/// is a ground-up column.
fn bulk_payloads(packet: &MapChunkBulk) -> Vec<(ChunkPos, ChunkPayload<'_>)> {
    packet
        .chunks
        .iter()
        .map(|chunk| {
            (
                ChunkPos::new(chunk.chunk_x, chunk.chunk_z),
                ChunkPayload {
                    primary_bit_mask: chunk.primary_bit_mask,
                    ground_up: true,
                    data: &chunk.data,
                },
            )
        })
        .collect()
}

/// Recomputes the armour bar from the armour slots of the inventory.
pub(super) fn sync_armor(game: &mut Game) {
    game.armor = f32::from(game.windows.armor_value());
    game.hud_dirty = true;
}

/// Counts a block change the server confirmed.
fn acknowledge_block(game: &mut Game, position: BlockPos, block_state: u16) {
    tracing::debug!(?position, block_state, "block change applied");
    match game.interaction.acknowledge_block(position) {
        Some(BlockAck::Break) => {
            game.status_data.breaks_acknowledged += 1;
            tracing::info!(
                ?position,
                block_state,
                "the server confirmed the block break"
            );
        }
        Some(BlockAck::Placement) => {
            game.status_data.placements_acknowledged += 1;
            tracing::info!(
                ?position,
                block_state,
                "the server confirmed the block placement"
            );
        }
        None => {}
    }
}

/// Answers a `Confirm Transaction` the server did not accept.
///
/// Vanilla answers on the game thread, in the order of the packets
/// (`NetHandlerPlayClient.handleConfirmTransaction`). GrimAC sends a
/// transaction before and after a teleport and needs the replies in that
/// order: the position reply must leave between the two. The session thread
/// used to answer at once, so the second transaction overtook the position
/// reply and Grim flagged `BadPacketsN` (observed 2026-11-02).
fn answer_transaction(
    game: &Game,
    packet: &umicraft_protocol::packets::play::clientbound::ConfirmTransaction,
    queue: &mut CommandQueue,
) {
    if !packet.accepted && game.windows.answers_transaction(packet.window_id) {
        queue.push(ClientCommand::ConfirmTransaction {
            window_id: packet.window_id as i8,
            action_number: packet.action_number,
        });
    }
}

/// Applies a server placement.
fn apply_placement(
    game: &mut Game,
    placement: &umicraft_net::play::PlayerPlacement,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
) {
    // Relative flags apply to where the player is now, not to the last
    // placement the session saw.
    let resolved = game.state.as_ref().map(|state| {
        let position = state.position();
        placement.resolved_against(
            [position.x, position.y, position.z],
            state.player().yaw.to_degrees(),
            state.player().pitch.to_degrees(),
        )
    });
    let placement = resolved.as_ref().unwrap_or(placement);
    let yaw = placement.yaw.to_radians();
    let pitch = placement.pitch.to_radians();
    let position = DVec3::new(placement.x, placement.y, placement.z);

    match game.state.as_mut() {
        None => {
            game.state = Some(GameState::new(position));
            game.status_data.placed = true;
            game.yaw = yaw;
            game.pitch = pitch;
            apply_mode_rules(game);
        }
        Some(state) => {
            let correction = ServerCorrection::new(
                placement.x,
                placement.y,
                placement.z,
                yaw,
                pitch,
                state.player().on_ground,
            );
            let predicted = state.position();
            // A `Player Position and Look` is a teleport, not a correction to
            // reconcile: nothing is replayed (see `GameState::teleport`).
            let flags = placement.raw.as_deref().map_or(0, |raw| raw.flags);
            state.teleport(&correction, [0x01, 0x02, 0x04].map(|bit| flags & bit != 0));
            game.yaw = state.player().yaw;
            game.pitch = state.player().pitch;
            game.status_data.resyncs += 1;
            tracing::debug!(
                ?predicted,
                authoritative = ?position,
                delta = ?(predicted - position),
                "the server corrected the position",
            );
        }
    }

    // Immediately snap the local player body entity without triggering limb acceleration
    game.body
        .teleport_to_exact(position, game.yaw.to_degrees(), game.pitch.to_degrees());

    // Immediately notify render thread to snap camera view and prevent gliding
    let eye = position + glam::DVec3::Y * umicraft_game::physics::EYE_HEIGHT;
    let _ = frames.try_send(RenderEvent::Teleport {
        position: eye,
        yaw: game.yaw,
        pitch: game.pitch,
    });

    // The server expects the acknowledgement of the placement.
    let ack = PositionAck {
        x: placement.x,
        y: placement.y,
        z: placement.z,
        yaw: placement.yaw,
        pitch: placement.pitch,
        on_ground: false,
    };
    // Vanilla answers every `Player Position and Look`
    // (`NetHandlerPlayClient.handlePlayerPosLook`). Anti-cheats such as
    // GrimAC require it: a teleport that is not answered is flagged
    // (`BadPacketsN`), so a repeated position is acknowledged again.
    queue.push(command_of(Outgoing::AckPosition(ack)));
    // Phase E.9: the position the view reported is NOT forgotten. Vanilla's
    // `handlePlayerPosLook` leaves `lastReportedPos*` alone (bytecode `bcy.a(fi)`:
    // it sets the position, answers, and nothing else), so a teleport to the
    // position already reported does not make the next tick report it again. The
    // forced report was one position-bearing packet per tick during a teleport
    // loop, each of which made the server send one more teleport.
}

/// Applies an authoritative correction from the server.
#[allow(dead_code)]
fn apply_correction(game: &mut Game, correction: &ServerCorrection, queue: &mut CommandQueue) {
    if let Some(state) = game.state.as_mut() {
        if let Some(world) = game.world.as_ref() {
            state.apply_correction(correction, world);
        }
        game.yaw = state.player().yaw;
        game.pitch = state.player().pitch;
    }
    queue.push(ClientCommand::AckPosition {
        x: correction.x,
        y: correction.y,
        z: correction.z,
        yaw: correction.yaw.to_degrees(),
        pitch: correction.pitch.to_degrees(),
        on_ground: correction.on_ground,
    });
    game.server_view.forget_position();
}

/// Registers the bobber `id` on the local player when they cast it (MCP
/// `NetHandlerPlayClient.handleSpawnObject`: the angler of a new bobber gets
/// it as `fishEntity`, the local player included). The entity store does not
/// hold the local player, so it cannot do it: without this the rod of the
/// local player never shows its cast model.
fn hook_local_angler(game: &mut Game, id: EntityId, object_type: u8, angler: i32) {
    if EntityKind::from_object_type(object_type) == EntityKind::FishHook && angler == game.entity_id
    {
        game.body.bobber = Some(id);
    }
}
