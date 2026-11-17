// SPDX-License-Identifier: MIT OR Apache-2.0

//! The packets about the local player: joining, respawning, health,
//! placement and game mode.

use std::sync::mpsc::SyncSender;

use glam::DVec3;
use umicraft_game::gamemode::{GameMode, GAMEMODE_REASON};
use umicraft_game::hud::boss_bar::BossBar;
use umicraft_game::interaction::{Interaction, Outgoing, PositionAck};
use umicraft_game::inventory::WindowManager;
use umicraft_game::item_use::ItemUse;
use umicraft_game::particles::{ParticleKind, SpawnRequest};
use umicraft_game::player_list::PlayerList;
use umicraft_game::scoreboard::Scoreboard;
use umicraft_game::sound::cue::{CueCategory, SoundCue};
use umicraft_game::title::{ActionBar, TitleState};
use umicraft_game::traffic::ServerView;
use umicraft_game::{GameState, ServerCorrection};
use umicraft_net::play::JoinGameInfo;
use umicraft_protocol::packets::play::clientbound::{
    ChangeGameState, EntityVelocity, Respawn, UpdateHealth,
};
use umicraft_world::World;

use super::wire::VELOCITY_UNITS;
use crate::game::abilities::{apply_mode_rules, set_gamemode};
use crate::game::commands::{command_of, CommandQueue};
use crate::game::messages::{RenderEvent, ViewAngle};
use crate::game::particles::particle_filter;
use crate::game::publish::{publish_inventory, publish_status};
use crate::game::state::Game;
use crate::network::ClientCommand;

/// The `Change Game State` reason of the elder guardian curse (MCP
/// `NetHandlerPlayClient.handleChangeGameState`: reason 10).
pub(in crate::game) const ELDER_GUARDIAN_CURSE_REASON: u8 = 10;

/// The curse of an elder guardian: its apparition swings in front of the
/// player and the curse sounds (MCP `handleChangeGameState`, reason 10).
pub(in crate::game) fn haunt(game: &mut Game) {
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
pub(in crate::game) fn is_flat(world_type: &str) -> bool {
    world_type.eq_ignore_ascii_case("flat")
}

/// Applies a server placement.
pub(in crate::game) fn apply_placement(
    game: &mut Game,
    placement: &umicraft_net::play::PlayerPlacement,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
) {
    // Relative flags apply to where the player is now, not to the last
    // placement the session saw, and to the look the player has now (the
    // mouse turned it since the last tick, and the physics does not run on
    // a vehicle or over a missing chunk): MCP `handlePlayerPosLook` adds to
    // the live `rotationYaw`.
    let (look_yaw, look_pitch) = (game.yaw, game.pitch);
    let resolved = game.state.as_ref().map(|state| {
        let position = state.position();
        placement.resolved_against([position.x, position.y, position.z], look_yaw, look_pitch)
    });
    let placement = resolved.as_ref().unwrap_or(placement);
    let yaw = placement.yaw;
    let pitch = placement.pitch;
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
    game.body.teleport_to_exact(position, game.yaw, game.pitch);

    // Immediately notify render thread to snap camera view and prevent gliding
    let eye = position + glam::DVec3::Y * umicraft_game::physics::EYE_HEIGHT;
    let flags = placement.raw.as_deref().map_or(0, |raw| raw.flags);
    let angle = |bit: u8, now: f32, turn: Option<f32>| match turn {
        Some(turn) if flags & bit != 0 => ViewAngle::By(turn.to_radians()),
        _ => ViewAngle::To(now.to_radians()),
    };
    let raw = placement.raw.as_deref();
    let _ = frames.try_send(RenderEvent::Teleport {
        position: eye,
        yaw: angle(0x08, game.yaw, raw.map(|raw| raw.yaw)),
        pitch: angle(0x10, game.pitch, raw.map(|raw| raw.pitch)),
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
pub(in crate::game) fn apply_correction(
    game: &mut Game,
    correction: &ServerCorrection,
    queue: &mut CommandQueue,
) {
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
        yaw: correction.yaw,
        pitch: correction.pitch,
        on_ground: correction.on_ground,
    });
    game.server_view.forget_position();
}

/// `Join Game`: a new world and a new player; nothing of the previous
/// server's session survives.
pub(super) fn join_game(game: &mut Game, info: &JoinGameInfo, frames: &SyncSender<RenderEvent>) {
    game.world = Some(World::new(info.dimension));
    game.dirty.clear();
    game.entities.clear();
    game.body.bobber = None;
    game.entity_id = info.entity_id;
    game.vitals.hardcore = info.gamemode & 8 != 0;
    game.vitals.absorption = 0.0;
    game.server.on_join(info.entity_id);
    game.server_view = ServerView::default();
    game.vehicle = None;
    // `handleJoinGame` makes a new `PlayerControllerMP` and a new
    // player, and closes the screen: nothing of the old session's dig,
    // item use, pending click or open window survives a server switch.
    game.interaction = Interaction::new();
    game.item_use = ItemUse::new();
    game.controls.attack_pressed = false;
    game.controls.use_pressed = false;
    game.pending_book = None;
    game.windows = WindowManager::new();
    // `handleJoinGame` makes a new `PlayerControllerMP`
    // (`currentPlayerItem` 0) and the new player starts on slot 0.
    game.hotbar.selected = 0;
    game.hotbar.applied = 0;
    game.hotbar.synced = 0;
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
    game.social.player_list = PlayerList::new();
    game.social.scoreboard = Scoreboard::new();
    game.social.title = TitleState::new();
    game.social.action_bar = ActionBar::new();
    game.social.boss_bar = BossBar::new();
    game.chat.completions.clear();
    game.hud_dirty = true;
    let _ = frames.try_send(RenderEvent::Reset);
    publish_status(game, frames);
}

/// `Update Health`: the health and the food of the local player.
pub(super) fn update_health(game: &mut Game, packet: &UpdateHealth) {
    // MCP `EntityPlayerSP.setPlayerSPHealth`: a drop of health hurts, so
    // the camera shakes even where the server sent no `Entity Status 2`.
    if game.vitals.health_known && packet.health < game.vitals.health {
        game.body.animation.hurt();
    }
    game.vitals.health_known = true;
    game.vitals.health = packet.health;
    game.vitals.food = packet.food as f32;
    game.hud_dirty = true;
}

/// `Held Item Change`: the server selects a hotbar slot.
pub(super) fn change_held_item(game: &mut Game, slot: i8) {
    if (0..9).contains(&slot) {
        game.hotbar.selected = slot as u8;
        // `handleHeldItemChange` only sets `currentItem`; the
        // controller keeps `currentPlayerItem`, so the next sync
        // tells the server this slot back, like the vanilla client.
        game.hotbar.applied = slot as u8;
        game.windows.set_selected_slot(slot as u8);
        game.hud_dirty = true;
    }
}

/// `Respawn`: a new player entity, and a new world when the dimension
/// changes.
pub(super) fn respawn(game: &mut Game, packet: &Respawn, frames: &SyncSender<RenderEvent>) {
    // MCP `handleRespawn` sets the gamemode of the new player.
    set_gamemode(game, GameMode::from_wire(packet.game_mode));
    // A respawned player stands on foot and has told the server
    // nothing yet (MCP `handleRespawn` creates a new player entity,
    // whose sprint and sneak states start off).
    game.vehicle = None;
    // A new player entity: its first health update only sets the health.
    game.vitals.health_known = false;
    game.overlays.reset();
    game.server_view = ServerView::default();
    // The new player has a new inventory on slot 0, but the
    // controller is kept: `currentPlayerItem` stays, and the next
    // sync tells the server the slot if it was not 0.
    game.hotbar.selected = 0;
    game.hotbar.applied = 0;
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
        game.social.scoreboard = Scoreboard::new();
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
    game.vitals.health = 20.0;
    game.vitals.food = 20.0;
    game.hud_dirty = true;
    game.status_data.status = game.status.clone();
    publish_status(game, frames);
}

/// `Entity Velocity` of the local player.
pub(super) fn push_local_player(game: &mut Game, packet: &EntityVelocity) {
    // MCP `handleEntityVelocity` sets the motion of the local player:
    // the knockback of a hit, a fishing rod, a piston.
    if let Some(state) = game.state.as_mut() {
        state.set_velocity(DVec3::new(
            f64::from(packet.velocity_x) / VELOCITY_UNITS,
            f64::from(packet.velocity_y) / VELOCITY_UNITS,
            f64::from(packet.velocity_z) / VELOCITY_UNITS,
        ));
    }
}

/// `Change Game State`: the game mode and the elder guardian curse,
/// besides the state the server keeps.
pub(super) fn change_game_state(game: &mut Game, packet: &ChangeGameState) {
    game.server.apply_game_state(packet);
    if packet.reason == GAMEMODE_REASON {
        set_gamemode(game, GameMode::from_state_value(packet.value));
    } else if packet.reason == ELDER_GUARDIAN_CURSE_REASON {
        haunt(game);
    }
}
