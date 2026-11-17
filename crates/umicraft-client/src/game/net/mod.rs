// SPDX-License-Identifier: MIT OR Apache-2.0

//! The clientbound packets: how each decoded [`PlayEvent`] changes the
//! game state.

use std::sync::mpsc::SyncSender;

use umicraft_game::chat::Position;
use umicraft_game::entity::nametag::mob_tag;
use umicraft_game::entity::{EntityKind, EntityRecord};
use umicraft_game::hud::team_names::{player_label, sees_team_ghost};
use umicraft_game::inventory::payload;
use umicraft_game::scoreboard::Scoreboard;
use umicraft_game::server::ServerState;
use umicraft_net::play::PlayEvent;

use super::abilities::apply_mode_rules;
use super::chat::show_chat;
use super::commands::CommandQueue;
use super::container_events::apply_trade_list;
use super::messages::RenderEvent;
use super::particles::{apply_effect, spawn_particles};
use super::publish::{publish_server_pack, publish_stats, publish_status};
use super::state::Game;

pub(super) mod entities;
pub(super) mod inventory;
pub(super) mod player;
pub(super) mod social;
mod wire;
pub(super) mod world;

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
        let players = &game.social.player_list;
        game.entities.refresh_spectators(|uuid| {
            players
                .get(&u128::from_be_bytes(*uuid))
                .is_some_and(|entry| entry.game_mode == SPECTATOR_WIRE)
        });
        let (scoreboard, own_name) = (&game.social.scoreboard, game.own_name.as_str());
        game.entities
            .refresh_name_tags(|entity| entity_tag(scoreboard, own_name, entity));
        game.entities
            .refresh_ghosts(|entity| is_team_ghost(scoreboard, own_name, entity));
    }
}

/// Applies one clientbound packet to the game state; the larger handlers
/// live in the submodules, by what the packet is about.
fn apply_net_event(
    game: &mut Game,
    event: PlayEvent,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
) {
    game.hud_view.observe(&event, game.entity_id);
    match event {
        PlayEvent::JoinGame(info) => player::join_game(game, &info, frames),
        PlayEvent::Placement(placement) => {
            let first = !game.status_data.placed;
            player::apply_placement(game, &placement, queue, frames);
            if first {
                // The window leaves the connecting screen on this flag, so it
                // must not wait for the periodic status.
                publish_status(game, frames);
            }
        }
        PlayEvent::Chat(json) => show_chat(game, &json, Position::Chat),
        PlayEvent::SystemChat(json) => show_chat(game, &json, Position::System),
        PlayEvent::ActionBar(json) => {
            game.social.action_bar.show(&json);
            game.hud_dirty = true;
        }
        PlayEvent::PlayerListItem(packet) => social::update_player_list(game, &packet),
        PlayEvent::PlayerListHeaderFooter(packet) => {
            game.social.player_list.set_header_footer(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::TabComplete(packet) => {
            game.chat.completions = packet.matches;
            game.chat.completions_serial += 1;
            game.hud_dirty = true;
        }
        PlayEvent::ScoreboardObjective(packet) => {
            game.social.scoreboard.apply_objective(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::UpdateScore(packet) => {
            game.social.scoreboard.apply_score(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::DisplayScoreboard(packet) => {
            game.social.scoreboard.apply_display(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::Teams(packet) => {
            game.social.scoreboard.apply_teams(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::Title(packet) => {
            game.social.title.apply(&packet);
            game.hud_dirty = true;
        }
        PlayEvent::ChunkData(packet) => world::load_chunk(game, &packet),
        PlayEvent::MapChunkBulk(packet) => world::load_chunk_bulk(game, &packet),
        PlayEvent::BlockChange(packet) => world::change_block(game, &packet),
        PlayEvent::MultiBlockChange(packet) => world::change_blocks(game, &packet),
        PlayEvent::UpdateHealth(packet) => player::update_health(game, &packet),
        PlayEvent::HeldItemChange(slot) => player::change_held_item(game, slot),
        PlayEvent::Respawn(packet) => player::respawn(game, &packet, frames),
        PlayEvent::EntityTeleport(packet) => entities::teleport_entity(game, &packet),
        PlayEvent::EntityEquipment(packet) => entities::equip_entity(game, &packet),
        PlayEvent::SpawnPlayer(packet) => entities::spawn_player(game, &packet),
        PlayEvent::SpawnMob(packet) => entities::spawn_mob(game, &packet),
        PlayEvent::SpawnObject(packet) => entities::spawn_object(game, &packet),
        PlayEvent::CollectItem(packet) => entities::collect_item(game, &packet),
        PlayEvent::SpawnGlobalEntity(packet) if packet.entity_type == 1 => {
            entities::spawn_lightning(game, &packet)
        }
        PlayEvent::SpawnXpOrb(packet) => entities::spawn_xp_orb(game, &packet),
        PlayEvent::SpawnPainting(packet) => entities::spawn_painting(game, &packet),
        PlayEvent::DestroyEntities(packet) => entities::destroy_entities(game, &packet),
        PlayEvent::EntityRelativeMove(packet) => entities::move_entity(game, &packet),
        PlayEvent::EntityLook(packet) => entities::turn_entity(game, &packet),
        PlayEvent::EntityLookMove(packet) => entities::move_and_turn_entity(game, &packet),
        PlayEvent::EntityHeadLook(packet) => entities::turn_entity_head(game, &packet),
        PlayEvent::EntityVelocity(packet) if packet.entity_id == game.entity_id => {
            player::push_local_player(game, &packet)
        }
        PlayEvent::EntityVelocity(packet) => entities::set_entity_velocity(game, &packet),
        PlayEvent::EntityMetadata(packet) => entities::apply_entity_metadata(game, &packet),
        PlayEvent::AttachEntity(packet) => entities::attach_entity(game, &packet),
        PlayEvent::EntityStatus(packet) => entities::apply_entity_status(game, &packet),
        PlayEvent::Animation(packet) => entities::play_animation(game, &packet),
        PlayEvent::UseBed(bed) => entities::put_to_bed(game, &bed),
        PlayEvent::SpawnPosition(packet) => {
            game.spawn = Some((packet.x, packet.y, packet.z));
        }
        PlayEvent::SetExperience(packet) => {
            game.vitals.experience = packet.experience;
            game.vitals.level = packet.level;
            game.hud_dirty = true;
        }
        PlayEvent::TimeUpdate(packet) => {
            // The signed value is kept as-is: a negative time means the
            // server froze the cycle. The client never advances the clock
            // between packets (ADR-0008 §4).
            game.world_time = packet.time_of_day;
        }
        PlayEvent::OpenWindow(packet) => inventory::open_window(game, packet, frames),
        PlayEvent::CloseWindow { window_id } => inventory::close_window(game, window_id, frames),
        PlayEvent::SetSlot(packet) => inventory::set_slot(game, packet, frames),
        PlayEvent::WindowItems(packet) => inventory::fill_window(game, &packet, frames),
        PlayEvent::WindowProperty(packet) => inventory::set_window_property(game, &packet, frames),
        PlayEvent::ConfirmTransaction(packet) => {
            inventory::confirm_transaction(game, &packet, queue, frames)
        }
        PlayEvent::Explosion(packet) => world::apply_explosion(game, &packet),
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
        PlayEvent::ChangeGameState(packet) => player::change_game_state(game, &packet),
        PlayEvent::BlockAction(packet) => world::start_block_action(game, &packet),
        PlayEvent::PluginMessage(packet) if packet.channel == payload::TRADE_LIST => {
            apply_trade_list(game, &packet.data, frames);
        }
        other => apply_server_state(&mut game.server, &other),
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
