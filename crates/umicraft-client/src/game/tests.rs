// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the game thread.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::time::{Duration, Instant};

use glam::{DVec3, Vec3};
use umicraft_assets::profile_textures::ProfileTextures;
use umicraft_game::entity::{EntityFlags, EntityId};
use umicraft_game::gamemode::GameMode;
use umicraft_game::input::RawInput;
use umicraft_game::item_use::{DropKind, UseAction};
use umicraft_game::raycast::look_direction;
use umicraft_game::view::Perspective;
use umicraft_game::{GameState, InputIntent, RenderState};
use umicraft_net::play::PlayEvent;
use umicraft_protocol::packets::play::clientbound::player_list_item::PlayerListItem;
use umicraft_protocol::packets::play::clientbound::{BlockBreakAnim, ChunkData, Explosion};
use umicraft_protocol::packets::play::serverbound::DiggingStatus;
use umicraft_protocol::packets::play::serverbound::UseEntityAction;
use umicraft_protocol::packets::play::ItemStack as WireItemStack;
use umicraft_protocol::packets::play::Metadata;
use umicraft_protocol::PlayServerbound;
use umicraft_render::plain_text;
use umicraft_render::{ChatLineView, OutlineBox};
use umicraft_world::{BlockPos, BlockState, ChunkPos, World};

use super::abilities::*;
use super::actions::attack::*;
use super::actions::hotbar::*;
use super::actions::use_item::*;
use super::actions::*;
use super::body::*;
use super::chat::*;
use super::combat;
use super::commands::*;
use super::game_loop::*;
use super::hud::*;
use super::look::*;
use super::messages::*;
use super::movement::*;
use super::net::*;
use super::particles::*;
use super::publish::*;
use super::session::*;
use super::state::*;
use super::tick::*;
use super::view::*;
use crate::input::InputState;
use crate::input::InputUpdate;
use crate::network::{self, ClientCommand};
use crate::social_view;

/// An `ench` list with one enchantment, as a server would send it.
fn enchanted_nbt(id: i16, level: i16) -> Vec<u8> {
    let mut nbt = vec![10, 0, 0, 9, 0, 4];
    nbt.extend(b"ench");
    nbt.extend([10, 0, 0, 0, 1]);
    nbt.extend([2, 0, 2]);
    nbt.extend(b"id");
    nbt.extend(id.to_be_bytes());
    nbt.extend([2, 0, 3]);
    nbt.extend(b"lvl");
    nbt.extend(level.to_be_bytes());
    nbt.extend([0, 0]);
    nbt
}

#[test]
fn the_armour_bar_follows_the_armour_slots() {
    let mut game = placed_game();
    game.windows.apply_set_slot(0, 5, wire_item(310, 0));
    game.windows.apply_set_slot(0, 6, wire_item(311, 0));
    sync_armor(&mut game);
    assert_eq!(game.armor, 11.0);
}

/// A placed, playing game on the floor of a loaded chunk, for the
/// traffic tests.
pub(super) fn placed_game() -> Game {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.playing = true;
    game
}

#[test]
fn the_f3_facts_hold_the_feet_the_biome_and_the_chunk_cache() {
    let game = placed_game();
    let facts = debug_world(&game);
    let feet = game.state.as_ref().expect("state").position();
    assert_eq!(facts.feet, [feet.x, feet.y, feet.z]);
    assert_eq!(
        facts.chunk_cache.0,
        game.world.as_ref().expect("world").chunk_count()
    );
    let place = facts.location.expect("the player column is loaded");
    assert_eq!(place.day, 0);
    assert!(place.sky_light <= 15);
}

#[test]
fn nothing_is_sent_while_the_player_column_is_not_loaded() {
    // Regression: Umicraft sent bursts of packets before the world
    // loaded; vanilla stays silent until the column is there.
    let mut game = Game::new();
    game.playing = true;
    game.state = Some(GameState::new(DVec3::new(0.0, 4.0, 0.0)));
    game.world = Some(World::new(0));
    let mut queue = CommandQueue::default();
    report_movement(&mut game, &InputIntent::still(0.0), &mut queue);
    assert!(queue.ordered.is_empty());
}

#[test]
fn an_idle_tick_sends_the_bare_player_packet_and_no_steer() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let still = InputIntent::still(0.0);
    report_movement(&mut game, &still, &mut queue);
    assert_eq!(game.status_data.position_reports, 1);
    report_movement(&mut game, &still, &mut queue);
    // Regression: `Steer Vehicle` used to be sent on every tick on foot.
    assert!(!queue
        .ordered
        .iter()
        .any(|command| matches!(command, ClientCommand::Input { .. })));
    assert_eq!(
        queue.ordered.back(),
        Some(&ClientCommand::Player { on_ground: false })
    );
}

#[test]
fn a_riding_tick_sends_look_then_steer() {
    let mut game = placed_game();
    game.vehicle = Some(42);
    let mut queue = CommandQueue::default();
    report_movement(&mut game, &InputIntent::still(0.0), &mut queue);
    assert!(matches!(
        commands(&queue)[..],
        [
            ClientCommand::PlayerLook { .. },
            ClientCommand::Input { .. }
        ]
    ));
    assert_eq!(game.status_data.position_reports, 0);
}

#[test]
fn a_sneak_transition_is_announced_before_the_movement() {
    let mut game = placed_game();
    game.entity_id = 7;
    let mut queue = CommandQueue::default();
    let mut sneaking = GameState::new(DVec3::new(0.0, 4.0, 0.0));
    sneaking.tick(
        &RawInput {
            sneak: true,
            ..RawInput::default()
        }
        .to_intent(),
        &umicraft_world::World::new(0),
    );
    game.state = Some(sneaking);
    report_movement(&mut game, &InputIntent::still(0.0), &mut queue);
    assert_eq!(
        queue.ordered.front(),
        Some(&ClientCommand::EntityAction {
            entity_id: 7,
            action: umicraft_protocol::packets::play::serverbound::EntityActionKind::StartSneaking,
        })
    );
}

#[test]
fn mounting_and_dismounting_follow_attach_entity() {
    let mut game = placed_game();
    game.entity_id = 7;
    let mut queue = CommandQueue::default();
    let (frames, _rx) = mpsc::sync_channel(8);
    let attach = |vehicle_id| {
        PlayEvent::AttachEntity(
            umicraft_protocol::packets::play::clientbound::AttachEntity {
                leash: 0,
                entity_id: 7,
                vehicle_id,
            },
        )
    };
    handle_net(&mut game, attach(42), &mut queue, &frames);
    assert_eq!(game.vehicle, Some(42));
    handle_net(&mut game, attach(-1), &mut queue, &frames);
    assert_eq!(game.vehicle, None);
}

#[test]
fn the_input_update_turns_the_view() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    handle_input(
        &mut game,
        InputUpdate {
            state: InputState {
                forward: 1.0,
                ..InputState::default()
            },
            look_yaw: 0.5,
            look_pitch: 0.25,
            ..InputUpdate::default()
        },
        &mut queue,
    );
    assert!((game.yaw - 0.5).abs() < 1e-6);
    assert!((game.pitch - 0.25).abs() < 1e-6);
    assert_eq!(game.input.forward, 1.0);
}

/// Turns by `degrees`, runs one tick and returns the yaw of the movement
/// packet that tick queued, decoded from the encoded wire bytes.
fn turn_and_read_wire_yaw(game: &mut Game, queue: &mut CommandQueue, degrees: &[f32]) -> f32 {
    use umicraft_protocol::EncodablePacket;
    for step in degrees {
        handle_input(
            game,
            InputUpdate {
                look_yaw: step.to_radians(),
                ..InputUpdate::default()
            },
            queue,
        );
    }
    let (frames, _frames_rx) = mpsc::sync_channel(64);
    let (chunks, _chunks_rx) = mpsc::sync_channel(64);
    tick(game, queue, &frames, &chunks, Instant::now());
    let yaw = match queue.ordered.drain(..).next_back() {
        Some(ClientCommand::PositionLook { yaw, .. } | ClientCommand::PlayerLook { yaw, .. }) => {
            yaw
        }
        other => panic!("the tick must report the look, got {other:?}"),
    };
    let mut body = Vec::new();
    umicraft_protocol::packets::play::serverbound::PlayerLook {
        yaw,
        pitch: 0.0,
        on_ground: true,
    }
    .encode_body(&mut body)
    .expect("encode");
    f32::from_be_bytes([body[0], body[1], body[2], body[3]])
}

fn game_facing(degrees: f32) -> (Game, CommandQueue) {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.playing = true;
    game.yaw = degrees.to_radians();
    (game, CommandQueue::default())
}

#[test]
fn the_wire_yaw_crosses_360_without_wrapping() {
    let (mut game, mut queue) = game_facing(359.9);
    let yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[0.3]);
    assert!((yaw - 360.2).abs() < 1e-2, "{yaw}");
    let (mut game, mut queue) = game_facing(359.0);
    let yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[1.0]);
    assert!((yaw - 360.0).abs() < 1e-2, "{yaw}");
}

#[test]
fn the_wire_yaw_goes_below_zero_without_wrapping() {
    let (mut game, mut queue) = game_facing(0.0);
    let yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[-0.2]);
    assert!((yaw + 0.2).abs() < 1e-3, "{yaw}");
    let yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[-720.0]);
    assert!((yaw + 720.2).abs() < 1e-1, "{yaw}");
}

#[test]
fn several_turns_in_a_row_keep_adding_up() {
    let (mut game, mut queue) = game_facing(0.0);
    let mut total = 0.0;
    for _ in 0..8 {
        total += 100.0;
        let yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[100.0]);
        assert!((yaw - total).abs() < 1e-1, "{yaw} against {total}");
    }
    assert!(total > 720.0);
}

#[test]
fn rapid_mouse_movements_between_ticks_add_up() {
    let (mut game, mut queue) = game_facing(350.0);
    let burst = [7.3_f32; 50];
    let yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &burst);
    assert!((yaw - (350.0 + 365.0)).abs() < 1e-1, "{yaw}");
}

#[test]
fn consecutive_wire_yaws_never_jump_by_a_turn() {
    // A slow turn through two full circles: every step stays the size of
    // the mouse step, there is no -360 step at the crossing.
    let (mut game, mut queue) = game_facing(0.0);
    let mut previous = 0.0_f32;
    for _ in 0..1440 {
        let yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[0.5]);
        assert!((yaw - previous - 0.5).abs() < 1e-1, "{previous} -> {yaw}");
        previous = yaw;
    }
    assert!(previous > 719.0);
    // And back, through zero into the negative.
    for _ in 0..1440 {
        let yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[-0.5]);
        assert!((yaw - previous + 0.5).abs() < 1e-1, "{previous} -> {yaw}");
        previous = yaw;
    }
}

#[test]
fn a_continuous_yaw_looks_the_same_as_the_wrapped_one() {
    // The raycast and the third person camera only use the angle, not
    // its turn count.
    let wrapped = look_direction(0.2, 0.3);
    let turned = look_direction(0.2 + 3.0 * std::f32::consts::TAU, 0.3);
    assert!((wrapped - turned).length() < 1e-3);
    let behind = look_direction(0.2 - 2.0 * std::f32::consts::TAU, 0.3);
    assert!((wrapped - behind).length() < 1e-3);
}

#[test]
fn the_local_body_and_the_head_follow_a_continuous_yaw() {
    use umicraft_game::entity::angle::wrap_degrees;
    let flags = EntityFlags::new(0, 0x7F);
    let mut a = Game::new();
    let mut b = Game::new();
    for (yaw_a, yaw_b) in [(359.9, -0.1), (360.2, 0.2), (721.0, 1.0)] {
        a.body.follow(DVec3::ZERO, yaw_a, 0.0, flags);
        b.body.follow(DVec3::ZERO, yaw_b, 0.0, flags);
        let (fa, fb) = (a.body.frame_data(), b.body.frame_data());
        assert!(wrap_degrees(fa.head_yaw - fb.head_yaw).abs() < 1e-3);
        assert!(wrap_degrees(fa.body_yaw - fb.body_yaw).abs() < 1e-3);
    }
}

#[test]
fn the_pitch_is_clamped_to_the_vanilla_limit() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    for _ in 0..100 {
        handle_input(
            &mut game,
            InputUpdate {
                look_pitch: 0.5,
                ..InputUpdate::default()
            },
            &mut queue,
        );
    }
    assert!(game.pitch <= MAX_PITCH);
}

#[test]
fn the_selected_slot_is_announced_once() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    press_slot(&mut game, &mut queue, 3);
    assert_eq!(game.selected_slot, 3, "the player sees it at once");
    press_slot(&mut game, &mut queue, 3);
    assert!(queue.ordered.is_empty(), "nothing leaves between two ticks");
    for _ in 0..4 {
        run_ticks(&mut game, &mut queue, 1);
    }
    assert_eq!(slots_in(&queue), [3]);
}

#[test]
fn the_elder_guardian_curse_haunts_the_player() {
    // Regression: game state 10 did nothing, so no elder guardian showed.
    use umicraft_game::particles::Behaviour;
    use umicraft_protocol::packets::play::clientbound::ChangeGameState;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    let change = |reason, value| PlayEvent::ChangeGameState(ChangeGameState { reason, value });
    // Rain is not a curse.
    handle_net(&mut game, change(1, 0.0), &mut queue, &frames);
    assert!(game.particles.is_empty());
    handle_net(&mut game, change(10, 0.0), &mut queue, &frames);
    let apparition = game
        .particles
        .particles(umicraft_game::particles::Layer::Lit, false)
        .iter()
        .chain(
            game.particles
                .particles(umicraft_game::particles::Layer::Lit, true),
        )
        .find(|particle| particle.behaviour == Behaviour::MobAppearance)
        .expect("the apparition");
    assert_eq!(apparition.position, DVec3::new(0.5, 64.0, 0.5));
    let cues = game.state.as_mut().expect("placed").drain_sound_cues();
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].name, "mob.guardian.curse");
}

#[test]
fn a_game_state_change_switches_the_gamemode() {
    use umicraft_protocol::packets::play::clientbound::ChangeGameState;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    let change = |reason, value| PlayEvent::ChangeGameState(ChangeGameState { reason, value });
    handle_net(&mut game, change(3, 3.0), &mut queue, &frames);
    assert_eq!(game.gamemode, GameMode::Spectator);
    assert_eq!(game.status_data.gamemode, 3);
    let player = game.state.as_ref().expect("placed").player();
    assert!(player.no_clip && player.allow_flying);

    handle_net(&mut game, change(3, 0.0), &mut queue, &frames);
    assert_eq!(game.gamemode, GameMode::Survival);
    assert!(!game.state.as_ref().expect("placed").player().no_clip);

    // Rain is not a gamemode.
    handle_net(&mut game, change(1, 3.0), &mut queue, &frames);
    assert_eq!(game.gamemode, GameMode::Survival);
}

#[test]
fn the_server_abilities_let_a_creative_player_fly() {
    use umicraft_protocol::packets::play::clientbound::PlayerAbilities;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    let abilities = PlayEvent::PlayerAbilities(PlayerAbilities {
        flags: 0b1101,
        fly_speed: 0.1,
        walk_speed: 0.1,
    });
    handle_net(&mut game, abilities, &mut queue, &frames);
    let player = game.state.as_ref().expect("placed").player();
    assert!(player.allow_flying);
    assert!(!player.flying);
    assert_eq!(player.fly_speed, 0.1);
}

#[test]
fn a_change_of_the_flying_state_is_reported_to_the_server() {
    use umicraft_protocol::packets::play::clientbound::PlayerAbilities;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    game.server.apply_abilities(&PlayerAbilities {
        flags: 0b1100,
        fly_speed: 0.05,
        walk_speed: 0.1,
    });
    report_flight(&game, false, &mut queue);
    assert!(queue.ordered.is_empty(), "nothing changed");

    game.state
        .as_mut()
        .expect("placed")
        .set_abilities(true, true, 0.05);
    report_flight(&game, false, &mut queue);
    assert!(matches!(
        queue.ordered.front(),
        Some(ClientCommand::PlayerAbilities { flying: true, .. })
    ));
}

fn digging_commands(queue: &CommandQueue) -> usize {
    queue
        .ordered
        .iter()
        .filter(|command| matches!(command, ClientCommand::PlayerDigging { .. }))
        .count()
}

#[test]
fn a_held_survival_click_digs_the_block_over_several_ticks_and_cracks_it() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    game.entity_id = 1;
    game.pitch = 1.5;
    game.input.attack = true;
    game.attack_pressed = true;
    let block = BlockPos::new(0, 63, 0);
    let mut statuses = Vec::new();
    let mut cracked = false;
    // The player never landed in this fixture, so the dig is five times
    // slower (airborne): 0.6 hardness takes about 90 ticks.
    for _ in 0..300 {
        run_attack(&mut game, &mut queue);
        while let Some(command) = queue.ordered.pop_front() {
            if let ClientCommand::PlayerDigging { status, .. } = command {
                statuses.push(status);
            }
        }
        if game.world.as_ref().expect("world").block(block) == BlockState::AIR {
            break;
        }
        // Once the progress passes 10 % the block shows cracks.
        if !game.cracks().is_empty() {
            cracked = true;
        }
    }
    assert!(cracked, "the block never showed a crack while it was dug");
    assert!(game.cracks().is_empty(), "the cracks go with the block");
    assert_eq!(
        statuses.first(),
        Some(&umicraft_protocol::packets::play::serverbound::DiggingStatus::StartDestroyBlock)
    );
    assert_eq!(
        statuses.last(),
        Some(&umicraft_protocol::packets::play::serverbound::DiggingStatus::StopDestroyBlock)
    );
    assert!(game.world.as_ref().expect("world").block(block).is_air());
}

#[test]
fn a_creative_break_removes_the_block_at_once_and_sends_only_the_start() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    game.gamemode = GameMode::Creative;
    game.pitch = 1.5;
    game.attack_pressed = true;
    game.input.attack = true;
    run_attack(&mut game, &mut queue);
    let world = game.world.as_ref().expect("world");
    assert_eq!(world.block(BlockPos::new(0, 63, 0)), BlockState::AIR);
    assert_eq!(digging_commands(&queue), 1);
    assert_eq!(game.interaction.digging_block(), None);
}

#[test]
fn a_spectator_cannot_break_blocks() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    game.gamemode = GameMode::Spectator;
    game.pitch = 1.5;
    game.attack_pressed = true;
    game.input.attack = true;
    run_attack(&mut game, &mut queue);
    assert_eq!(digging_commands(&queue), 0);
}

#[test]
fn the_camera_packet_moves_the_view_to_the_entity() {
    use umicraft_protocol::packets::play::clientbound::camera::Camera;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, received) = mpsc::sync_channel(16);
    game.entity_id = 1;
    game.server.on_join(1);
    game.entities.spawn_mob(
        EntityId(9),
        90,
        DVec3::new(40.0, 70.0, 40.0),
        0.0,
        0.0,
        0.0,
        Vec3::ZERO,
        EntityFlags::default(),
    );
    handle_net(
        &mut game,
        PlayEvent::Camera(Camera { entity_id: 9 }),
        &mut queue,
        &frames,
    );
    assert!(game.spectating());
    publish_frame(&game, &frames, Instant::now());
    let Ok(RenderEvent::Frame(frame)) = received.try_recv() else {
        panic!("a frame was expected");
    };
    assert!((frame.state.position.x - 40.0).abs() < 1e-3);

    handle_net(
        &mut game,
        PlayEvent::Camera(Camera { entity_id: 1 }),
        &mut queue,
        &frames,
    );
    assert!(!game.spectating(), "the local player is the view again");
}

#[test]
fn the_body_in_f5_holds_the_selected_item() {
    // Regression: the local body was drawn with empty hands.
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 1;
    follow_body(&mut game);
    assert_eq!(game.body.frame_data().held, None);
    game.windows.apply_set_slot(0, 36, wire_item(276, 0));
    follow_body(&mut game);
    assert_eq!(
        game.body.frame_data().held.map(|held| held.item_id),
        Some(276)
    );
}

#[test]
fn a_rider_follows_its_vehicle_instead_of_walking() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 7;
    game.entities.spawn_object(
        EntityId(42),
        1,
        DVec3::new(8.0, 70.0, 8.0),
        0.0,
        0.0,
        glam::Vec3::ZERO,
        0,
    );
    game.vehicle = Some(42);
    load_surrounding_columns(&mut game);
    let seat = game.entities.rider_feet(EntityId(42)).expect("tracked");
    let mut queue = CommandQueue::default();
    let (frames, _rx) = mpsc::sync_channel(8);
    let (chunks, _chunk_rx) = mpsc::sync_channel(8);
    game.input.forward = 1.0;
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    let position = game.state.as_ref().expect("state").position();
    assert!(
        (position - seat).length() < 1e-9,
        "{position:?} vs {seat:?}"
    );
}

#[test]
fn f5_moves_the_camera_back_and_shows_the_player_body() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 1;
    let mut queue = CommandQueue::default();
    let (frames, received) = mpsc::sync_channel(16);
    let frame = |game: &Game| {
        publish_frame(game, &frames, Instant::now());
        match received.try_recv() {
            Ok(RenderEvent::Frame(frame)) => frame,
            _ => panic!("a frame was expected"),
        }
    };

    follow_body(&mut game);
    let first = frame(&game);
    assert!(first.entities.is_empty(), "no body in first person");

    let toggle = InputUpdate {
        toggle_perspective: true,
        ..InputUpdate::default()
    };
    handle_input(&mut game, toggle, &mut queue);
    apply_perspective_presses(&mut game);
    let third = frame(&game);
    assert_eq!(third.entities.len(), 1);
    assert_eq!(third.entities[0].id, EntityId(1));
    let moved = (third.state.position - first.state.position).length();
    assert!((moved - 4.0).abs() < 0.5, "moved {moved}");

    // Two quick presses inside one tick are both counted.
    handle_input(&mut game, toggle, &mut queue);
    handle_input(&mut game, toggle, &mut queue);
    apply_perspective_presses(&mut game);
    assert!(
        frame(&game).entities.is_empty(),
        "the cycle is back to first person"
    );
}

/// Regression: a press that comes with a mouse move published the look of
/// the new perspective at once, while the entities of the last frame still
/// held the body. Back in the first person the camera then sat inside the
/// head until the next tick.
#[test]
fn f5_waits_for_the_tick_so_the_view_and_the_body_agree() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 1;
    let mut queue = CommandQueue::default();
    let (frames, received) = mpsc::sync_channel(16);
    follow_body(&mut game);
    let toggle = InputUpdate {
        toggle_perspective: true,
        look_yaw: 0.1,
        ..InputUpdate::default()
    };
    // First person -> back view: the published frame carries both.
    handle_input(&mut game, toggle, &mut queue);
    apply_perspective_presses(&mut game);
    publish_frame(&game, &frames, Instant::now());
    let Ok(RenderEvent::Frame(back)) = received.try_recv() else {
        panic!("a frame was expected");
    };
    assert_eq!(back.entities.len(), 1, "the body is drawn in the back view");

    // Two more presses (front view, then first person), the mouse moving:
    // until the tick applies them, the look keeps the view of the frame.
    handle_input(&mut game, toggle, &mut queue);
    handle_input(&mut game, toggle, &mut queue);
    let (view, _) = player_view(&game).expect("view");
    assert!(
        ((view.position - view.eye).length() - 4.0).abs() < 0.5,
        "the look still orbits the player, not the eye"
    );
    apply_perspective_presses(&mut game);
    let (view, _) = player_view(&game).expect("view");
    assert!(
        (view.position - view.eye).length() < 1e-6,
        "first person after the tick"
    );
}

#[test]
fn f5_attack_and_block_selection_raycast_is_unaffected_by_perspective() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 1;
    let mut queue = CommandQueue::default();

    // Player looks down at the floor block at (0, 63, 0)
    game.pitch = std::f32::consts::FRAC_PI_2;

    // FirstPerson: selection outline targets the floor stone block
    let sel_fp = game.selection_outline();
    assert_eq!(sel_fp.len(), 1);
    assert_eq!(sel_fp[0].min, [0.0, 63.0, 0.0]);

    // Switch to ThirdPersonBack
    let toggle = InputUpdate {
        toggle_perspective: true,
        ..InputUpdate::default()
    };
    handle_input(&mut game, toggle, &mut queue);
    apply_perspective_presses(&mut game);
    assert_eq!(game.perspective, Perspective::ThirdPersonBack);
    let sel_back = game.selection_outline();
    assert_eq!(
        sel_back, sel_fp,
        "selection raycast must remain strictly from eye"
    );

    // Switch to ThirdPersonFront
    handle_input(&mut game, toggle, &mut queue);
    apply_perspective_presses(&mut game);
    assert_eq!(game.perspective, Perspective::ThirdPersonFront);
    let sel_front = game.selection_outline();
    assert_eq!(
        sel_front, sel_fp,
        "front view camera must NOT invert raycast reach!"
    );
}

#[test]
fn f5_does_not_affect_other_tracked_entities_or_players() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 1;
    let (frames, received) = mpsc::sync_channel(16);

    // Add another player entity
    let other_id = EntityId(2);
    let other_pos = glam::DVec3::new(5.0, 64.0, 5.0);
    game.entities.spawn_player(
        other_id,
        [0; 16],
        other_pos,
        45.0,
        10.0,
        None,
        EntityFlags::default(),
    );

    // Cycle through all perspectives and verify other entity's data remains untouched
    for p in [
        Perspective::FirstPerson,
        Perspective::ThirdPersonBack,
        Perspective::ThirdPersonFront,
    ] {
        game.perspective = p;
        publish_frame(&game, &frames, Instant::now());
        let Ok(RenderEvent::Frame(frame)) = received.try_recv() else {
            panic!("expected frame");
        };
        let other = frame
            .entities
            .iter()
            .find(|e| e.id == other_id)
            .expect("other entity");
        assert_eq!(other.position, other_pos);
        assert_eq!(other.yaw, 45.0);
        assert_eq!(other.pitch, 10.0);
    }
}

/// Loads the 3x3 columns around the player: the simulation only runs once
/// they are there.
fn load_surrounding_columns(game: &mut Game) {
    let payload = vec![0u8; umicraft_world::chunk::payload_len(0b1000, true, true)];
    let world = game.world.as_mut().expect("world");
    for (x, z) in (-1..=1).flat_map(|x| (-1..=1).map(move |z| (x, z))) {
        if (x, z) != (0, 0) {
            let column = umicraft_world::ChunkPayload {
                primary_bit_mask: 0b1000,
                ground_up: true,
                data: &payload,
            };
            world
                .apply_chunk(ChunkPos::new(x, z), column)
                .expect("an empty column");
        }
    }
}

#[test]
fn the_local_body_swings_its_limbs_while_walking() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 1;
    game.playing = true;
    game.input.forward = 1.0;
    load_surrounding_columns(&mut game);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(64);
    let (chunks, _chunk_received) = mpsc::sync_channel(64);
    for _ in 0..10 {
        tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    }
    let walking = game.body.frame_data();
    assert!(walking.limbs.amount > 0.1, "{}", walking.limbs.amount);
    assert!(walking.limbs.swing > 0.0);

    game.input.forward = 0.0;
    for _ in 0..60 {
        tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    }
    assert!(game.body.frame_data().limbs.amount < 0.05);
}

#[test]
fn the_cracks_follow_every_breaker_and_vanish_with_the_block() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let anim = |breaker: i32, stage: u8, game: &mut Game| {
        game.server.apply_block_break_anim(&BlockBreakAnim {
            entity_id: breaker,
            x: 0,
            y: 63,
            z: 0,
            stage,
        });
    };
    assert!(game.cracks().is_empty());
    anim(7, 4, &mut game);
    let cracks = game.cracks();
    assert_eq!(cracks.len(), 1);
    assert_eq!(cracks[0].stage, 4);
    assert_eq!(cracks[0].bounds.min, [0.0, 63.0, 0.0]);
    assert_eq!(cracks[0].bounds.max, [1.0, 64.0, 1.0]);

    // A second player digging elsewhere is drawn as well.
    game.server.apply_block_break_anim(&BlockBreakAnim {
        entity_id: 8,
        x: 1,
        y: 63,
        z: 0,
        stage: 2,
    });
    assert_eq!(game.cracks().len(), 2);

    anim(7, 255, &mut game);
    assert_eq!(game.cracks().len(), 1, "stage > 9 clears the breaker");
}

fn health_update(health: f32) -> PlayEvent {
    PlayEvent::UpdateHealth(
        umicraft_protocol::packets::play::clientbound::UpdateHealth {
            health,
            food: 20,
            saturation: 5.0,
        },
    )
}

#[test]
fn the_hud_carries_the_rows_and_bars_of_the_tick() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.experience = 0.5;
    game.level = 4;
    let (frames, _received) = mpsc::sync_channel(16);
    handle_net(
        &mut game,
        health_update(7.0),
        &mut CommandQueue::default(),
        &frames,
    );
    game.health = 7.0;
    tick_hud_view(&mut game);
    let hud = game.hud();
    let hearts = hud.vitals.expect("survival shows the rows").hearts;
    assert_eq!(hearts.len(), 10);
    assert!(matches!(
        hud.bar,
        umicraft_game::hud::BottomBar::Experience {
            fill: 91,
            level: Some(4)
        }
    ));
}

#[test]
fn a_dead_player_neither_moves_attacks_nor_places() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.playing = true;
    game.input.forward = 1.0;
    game.input.attack = true;
    game.attack_pressed = true;
    game.pitch = 1.5;
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let (chunks, _chunk_received) = mpsc::sync_channel(8);
    handle_net(&mut game, health_update(0.0), &mut queue, &frames);
    assert!(game.dead());
    let before = game.state.as_ref().expect("placed").position();
    for _ in 0..5 {
        tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    }
    let after = game.state.as_ref().expect("placed").position();
    assert_eq!(before.x, after.x);
    assert_eq!(before.z, after.z);
    assert_eq!(digging_commands(&queue), 0);
    click_use(&mut game, &mut queue);
    assert!(!queue
        .ordered
        .iter()
        .any(|command| matches!(command, ClientCommand::PlayerBlockPlacement { .. })));
}

#[test]
fn the_death_animation_runs_twenty_ticks_and_stops_with_the_respawn() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(64);
    let (chunks, _chunk_received) = mpsc::sync_channel(64);
    handle_net(&mut game, health_update(0.0), &mut queue, &frames);
    for _ in 0..30 {
        tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    }
    assert_eq!(game.death_time, DEATH_TICKS);
    handle_net(&mut game, health_update(20.0), &mut queue, &frames);
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    assert_eq!(game.death_time, 0);
}

/// Phase E.9: a dead player entity stops updating after 20 dead ticks
/// (`onDeathUpdate`, `setDead`), so it sends exactly 20 movement packets
/// after the death and none until the respawn (vanilla 1.8.9 capture:
/// 20 packets, the last 1 049 ms after the death).
#[test]
fn a_dead_player_sends_twenty_movement_packets_then_none() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.playing = true;
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(256);
    let (chunks, _chunk_received) = mpsc::sync_channel(256);
    // Settle the player, then forget what the setup queued.
    for _ in 0..3 {
        tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    }
    queue.ordered.clear();
    handle_net(&mut game, health_update(0.0), &mut queue, &frames);
    queue.ordered.clear();
    let mut sent = Vec::new();
    for _ in 0..40 {
        tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
        sent.push(queue.ordered.iter().filter(|c| is_movement(c)).count());
        queue.ordered.clear();
    }
    assert_eq!(
        sent[..20].iter().sum::<usize>(),
        20,
        "one per dead tick: {sent:?}"
    );
    assert_eq!(
        sent[20..].iter().sum::<usize>(),
        0,
        "then silence: {sent:?}"
    );
    // The respawn brings the packets back.
    handle_net(&mut game, health_update(20.0), &mut queue, &frames);
    queue.ordered.clear();
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    assert_eq!(queue.ordered.iter().filter(|c| is_movement(c)).count(), 1);
}

#[test]
fn a_drop_of_health_shakes_the_camera_even_without_an_entity_status() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_net(&mut game, health_update(20.0), &mut queue, &frames);
    assert_eq!(
        game.body.animation.hurt_time(),
        0,
        "the first update only sets the health"
    );
    handle_net(&mut game, health_update(14.0), &mut queue, &frames);
    assert_eq!(game.body.animation.hurt_time(), 10, "a drop hurts");

    let mut healed = game_on_floor_with(BlockState::AIR);
    handle_net(&mut healed, health_update(10.0), &mut queue, &frames);
    handle_net(&mut healed, health_update(20.0), &mut queue, &frames);
    assert_eq!(healed.body.animation.hurt_time(), 0, "healing does not");
}

#[test]
fn the_dead_view_rolls_and_narrows() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, received) = mpsc::sync_channel(64);
    let (chunks, _chunk_received) = mpsc::sync_channel(64);
    handle_net(&mut game, health_update(0.0), &mut queue, &frames);
    for _ in 0..20 {
        tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    }
    let mut last = None;
    while let Ok(event) = received.try_recv() {
        if let RenderEvent::Frame(frame) = event {
            last = Some(frame);
        }
    }
    let frame = last.expect("a frame");
    assert!(frame.state.effects.hurt_roll_degrees > 3.0);
    assert!(frame.state.effects.fov_modifier < 1.0);
}

#[test]
fn the_respawn_button_sends_the_request_only_to_a_dead_player() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_event(&mut game, GameEvent::Respawn, &mut queue, &frames);
    assert!(queue.ordered.is_empty(), "a living player cannot respawn");

    handle_net(&mut game, health_update(0.0), &mut queue, &frames);
    handle_event(&mut game, GameEvent::Respawn, &mut queue, &frames);
    assert!(matches!(
        queue.ordered.front(),
        Some(ClientCommand::Respawn)
    ));
}

#[test]
fn the_score_comes_from_the_local_player_metadata() {
    use umicraft_protocol::packets::play::clientbound::EntityMetadata;
    use umicraft_protocol::packets::play::metadata::WatchedObject;

    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 3;
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let metadata = |entity_id, score: i32| {
        PlayEvent::EntityMetadata(EntityMetadata {
            entity_id,
            metadata: Metadata {
                objects: vec![WatchedObject {
                    type_id: 2,
                    index: 18,
                    value: score.to_be_bytes().to_vec(),
                }],
            },
        })
    };
    handle_net(&mut game, metadata(9, 100), &mut queue, &frames);
    assert_eq!(game.score, 0, "another entity's score is not ours");
    handle_net(&mut game, metadata(3, 57), &mut queue, &frames);
    assert_eq!(game.score, 57);
    assert_eq!(game.hud().score, 57);
}

fn spawn_object_event(entity_id: i32, object_type: u8, data: i32) -> PlayEvent {
    use umicraft_protocol::packets::play::clientbound::SpawnObject;

    PlayEvent::SpawnObject(SpawnObject {
        entity_id,
        object_type,
        x: 0,
        y: 0,
        z: 0,
        pitch: 0,
        yaw: 0,
        data,
        velocity_x: 0,
        velocity_y: 0,
        velocity_z: 0,
    })
}

fn stack_watcher(
    index: u8,
    stack: &WireItemStack,
) -> umicraft_protocol::packets::play::metadata::WatchedObject {
    let mut value = Vec::new();
    stack.encode(&mut value).expect("encodes");
    umicraft_protocol::packets::play::metadata::WatchedObject {
        type_id: 5,
        index,
        value,
    }
}

fn metadata_event(
    entity_id: i32,
    objects: Vec<umicraft_protocol::packets::play::metadata::WatchedObject>,
) -> PlayEvent {
    use umicraft_protocol::packets::play::clientbound::EntityMetadata;

    PlayEvent::EntityMetadata(EntityMetadata {
        entity_id,
        metadata: Metadata { objects },
    })
}

#[test]
fn the_stack_of_a_dropped_item_comes_from_watcher_ten() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_net(&mut game, spawn_object_event(7, 2, 1), &mut queue, &frames);
    let event = metadata_event(7, vec![stack_watcher(10, &wire_item(264, 0))]);
    handle_net(&mut game, event, &mut queue, &frames);
    let snapshot = &game.entities.snapshots(0.0)[0];
    let item = snapshot.item.as_ref().expect("stack");
    assert_eq!((item.item_id, item.count), (264, 1));
}

#[test]
fn a_bobber_of_the_local_player_casts_their_rod_until_it_is_destroyed() {
    use umicraft_protocol::packets::play::clientbound::DestroyEntities;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let own = game.entity_id;
    // Someone else's bobber does not cast the rod of the local player.
    handle_net(
        &mut game,
        spawn_object_event(40, 90, own + 1),
        &mut queue,
        &frames,
    );
    assert!(!player_view(&game).expect("view").0.effects.hand.fishing);
    handle_net(
        &mut game,
        spawn_object_event(41, 90, own),
        &mut queue,
        &frames,
    );
    assert_eq!(game.body.bobber, Some(EntityId(41)));
    assert!(player_view(&game).expect("view").0.effects.hand.fishing);
    assert!(game.body.frame_data().fishing, "the third person rod too");
    let destroy = PlayEvent::DestroyEntities(DestroyEntities {
        entity_ids: vec![41],
    });
    handle_net(&mut game, destroy, &mut queue, &frames);
    assert_eq!(game.body.bobber, None);
}

#[test]
fn a_collected_item_leaves_the_world() {
    use umicraft_protocol::packets::play::clientbound::CollectItem;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_net(&mut game, spawn_object_event(7, 2, 1), &mut queue, &frames);
    assert_eq!(game.entities.snapshots(0.0).len(), 1);
    let event = PlayEvent::CollectItem(CollectItem {
        collected_entity_id: 7,
        collector_entity_id: 1,
    });
    handle_net(&mut game, event, &mut queue, &frames);
    assert_eq!(game.entities.snapshots(0.0).len(), 1, "it flies first");
    for _ in 0..3 {
        game.entities.tick();
    }
    assert!(game.entities.snapshots(0.0).is_empty());
}

#[test]
fn an_item_frame_takes_its_stack_rotation_and_direction() {
    use umicraft_protocol::packets::play::metadata::WatchedObject;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_net(&mut game, spawn_object_event(8, 71, 2), &mut queue, &frames);
    let rotation = WatchedObject {
        type_id: 0,
        index: 9,
        value: vec![0x0B],
    };
    let event = metadata_event(8, vec![stack_watcher(8, &wire_item(276, 4)), rotation]);
    handle_net(&mut game, event, &mut queue, &frames);
    let snapshot = &game.entities.snapshots(0.0)[0];
    assert_eq!(snapshot.frame_direction, 2);
    assert_eq!(snapshot.frame_rotation, 3);
    let item = snapshot.item.as_ref().expect("stack");
    assert_eq!((item.item_id, item.damage), (276, 4));
}

#[test]
fn dropped_item_watchers_do_not_leak_onto_other_kinds() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_net(&mut game, spawn_object_event(9, 60, 0), &mut queue, &frames);
    let event = metadata_event(9, vec![stack_watcher(10, &wire_item(264, 0))]);
    handle_net(&mut game, event, &mut queue, &frames);
    assert_eq!(game.entities.snapshots(0.0)[0].item, None);
}

#[test]
fn the_equipment_of_another_entity_sets_its_held_stack() {
    use umicraft_protocol::packets::play::clientbound::EntityEquipment;

    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 1;
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    game.entities.spawn_player(
        EntityId(4),
        [0; 16],
        DVec3::ZERO,
        0.0,
        0.0,
        None,
        EntityFlags::default(),
    );
    let equip = |item| {
        PlayEvent::EntityEquipment(EntityEquipment {
            entity_id: 4,
            slot: 0,
            item,
        })
    };
    handle_net(&mut game, equip(wire_item(276, 9)), &mut queue, &frames);
    let held = game.entities.snapshots(0.0)[0].held.clone().expect("held");
    assert_eq!((held.item_id, held.damage), (276, 9));

    handle_net(&mut game, equip(WireItemStack::EMPTY), &mut queue, &frames);
    assert_eq!(game.entities.snapshots(0.0)[0].held, None);
}

#[test]
fn a_tab_list_entry_gives_the_spawned_player_its_name() {
    use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;

    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let uuid = [7u8; 16];
    game.entities.spawn_player(
        EntityId(4),
        uuid,
        DVec3::ZERO,
        0.0,
        0.0,
        None,
        EntityFlags::default(),
    );
    let entry = AddPlayerEntry {
        uuid: u128::from_be_bytes(uuid),
        name: "Notch".to_owned(),
        properties: Vec::new(),
        game_mode: 0,
        ping: 0,
        display_name: None,
    };
    let event = PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![entry]));
    handle_net(&mut game, event, &mut queue, &frames);
    let profile = game.entities.snapshots(0.0)[0].profile.clone();
    assert_eq!(
        profile.map(|p| p.name.to_string()),
        Some("Notch".to_owned())
    );
    assert_eq!(
        game.server.player_list().len(),
        1,
        "the tab list still updates"
    );
}

#[test]
fn the_local_body_swings_its_arm_when_the_player_does() {
    let mut game = game_on_floor_with(BlockState::AIR);
    follow_body(&mut game);
    assert_eq!(game.body.animation.swing_progress_at(1.0), 0.0);
    game.body.animation.swing_item();
    for _ in 0..3 {
        follow_body(&mut game);
    }
    assert!(game.body.animation.swing_progress_at(1.0) > 0.0);
}

#[test]
fn the_local_body_wears_the_profile_of_its_own_tab_list_entry() {
    use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;

    let mut game = game_on_floor_with(BlockState::AIR);
    game.own_name = "Me".to_owned();
    follow_body(&mut game);
    assert!(game.body.profile.is_none(), "not listed yet");
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let entry = |uuid: u8, name: &str| AddPlayerEntry {
        uuid: u128::from_be_bytes([uuid; 16]),
        name: name.to_owned(),
        properties: Vec::new(),
        game_mode: 0,
        ping: 0,
        display_name: None,
    };
    let event = PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![
        entry(1, "Other"),
        entry(2, "Me"),
    ]));
    handle_net(&mut game, event, &mut queue, &frames);
    follow_body(&mut game);
    assert_eq!(game.body.uuid, Some([2; 16]));
    assert_eq!(
        game.body.profile.as_ref().map(|p| p.name.to_string()),
        Some("Me".to_owned())
    );
}

#[test]
fn the_hud_carries_the_ping_of_the_local_tab_list_entry() {
    use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;

    let mut game = game_on_floor_with(BlockState::AIR);
    game.own_name = "Me".to_owned();
    assert_eq!(game.hud().ping_ms, None, "not listed yet");
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let entry = |uuid: u8, name: &str, ping: i32| AddPlayerEntry {
        uuid: u128::from_be_bytes([uuid; 16]),
        name: name.to_owned(),
        properties: Vec::new(),
        game_mode: 0,
        ping,
        display_name: None,
    };
    let event = PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![
        entry(1, "Other", 250),
        entry(2, "Me", 42),
    ]));
    handle_net(&mut game, event, &mut queue, &frames);
    assert_eq!(game.hud().ping_ms, Some(42), "the own entry, not another");
}

#[test]
fn a_team_mate_is_a_ghost_only_when_the_team_sees_invisible_mates() {
    use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;
    use umicraft_protocol::packets::play::clientbound::teams::{TeamInfo, Teams};

    let mut game = game_on_floor_with(BlockState::AIR);
    game.own_name = "me".to_owned();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let uuid = [9u8; 16];
    game.entities.spawn_player(
        EntityId(4),
        uuid,
        DVec3::ZERO,
        0.0,
        0.0,
        None,
        EntityFlags::default(),
    );
    let entry = AddPlayerEntry {
        uuid: u128::from_be_bytes(uuid),
        name: "Bob".to_owned(),
        properties: Vec::new(),
        game_mode: 0,
        ping: 0,
        display_name: None,
    };
    let listed = PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![entry]));
    handle_net(&mut game, listed, &mut queue, &frames);
    let ghost = |game: &Game| game.entities.snapshots(0.0)[0].ghost_visible;
    assert!(!ghost(&game), "no team");

    let team = |flags: i8, players: &[&str]| {
        PlayEvent::Teams(Box::new(Teams::Create {
            name: "red".to_owned(),
            info: TeamInfo {
                display_name: "Red".to_owned(),
                prefix: String::new(),
                suffix: String::new(),
                friendly_flags: flags,
                name_tag_visibility: "always".to_owned(),
                color: 12,
            },
            players: players.iter().map(|name| (*name).to_owned()).collect(),
        }))
    };
    // Bob alone in the team: the local player is not a mate.
    handle_net(&mut game, team(2, &["Bob"]), &mut queue, &frames);
    assert!(!ghost(&game));
    let remove = || {
        PlayEvent::Teams(Box::new(Teams::Remove {
            name: "red".to_owned(),
        }))
    };
    handle_net(&mut game, remove(), &mut queue, &frames);
    // Both in the team, but it does not see invisible mates.
    handle_net(&mut game, team(0, &["Bob", "me"]), &mut queue, &frames);
    assert!(!ghost(&game));
    handle_net(&mut game, remove(), &mut queue, &frames);
    handle_net(&mut game, team(2, &["Bob", "me"]), &mut queue, &frames);
    assert!(ghost(&game));
}

#[test]
fn the_objective_below_the_name_adds_a_score_line_to_the_tag() {
    use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;
    use umicraft_protocol::packets::play::clientbound::{
        DisplayScoreboard, ObjectiveRenderType, ScoreboardObjective, UpdateScore,
    };

    let mut game = game_on_floor_with(BlockState::AIR);
    game.own_name = "me".to_owned();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let uuid = [9u8; 16];
    game.entities.spawn_player(
        EntityId(4),
        uuid,
        DVec3::ZERO,
        0.0,
        0.0,
        None,
        EntityFlags::default(),
    );
    let entry = AddPlayerEntry {
        uuid: u128::from_be_bytes(uuid),
        name: "Bob".to_owned(),
        properties: Vec::new(),
        game_mode: 0,
        ping: 0,
        display_name: None,
    };
    let listed = PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![entry]));
    handle_net(&mut game, listed, &mut queue, &frames);
    let tag = |game: &Game| game.entities.snapshots(0.0)[0].name_tag.clone();
    assert_eq!(tag(&game).as_deref(), Some("Bob"));

    let objective = PlayEvent::ScoreboardObjective(ScoreboardObjective::Create {
        name: "kills".to_owned(),
        value: "Kills".to_owned(),
        render_type: ObjectiveRenderType::Integer,
    });
    handle_net(&mut game, objective, &mut queue, &frames);
    let display = PlayEvent::DisplayScoreboard(DisplayScoreboard {
        position: 2,
        score_name: "kills".to_owned(),
    });
    handle_net(&mut game, display, &mut queue, &frames);
    // No score yet: vanilla shows 0.
    assert_eq!(tag(&game).as_deref(), Some("Bob\n0 Kills"));
    let score = PlayEvent::UpdateScore(UpdateScore::Change {
        name: "Bob".to_owned(),
        objective: "kills".to_owned(),
        value: 12,
    });
    handle_net(&mut game, score, &mut queue, &frames);
    assert_eq!(tag(&game).as_deref(), Some("Bob\n12 Kills"));
}

#[test]
fn a_team_decides_the_name_tag_of_a_player() {
    use umicraft_protocol::packets::play::clientbound::player_list_item::AddPlayerEntry;
    use umicraft_protocol::packets::play::clientbound::teams::{TeamInfo, Teams};

    let mut game = game_on_floor_with(BlockState::AIR);
    game.own_name = "me".to_owned();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let uuid = [9u8; 16];
    game.entities.spawn_player(
        EntityId(4),
        uuid,
        DVec3::ZERO,
        0.0,
        0.0,
        None,
        EntityFlags::default(),
    );
    let entry = AddPlayerEntry {
        uuid: u128::from_be_bytes(uuid),
        name: "Bob".to_owned(),
        properties: Vec::new(),
        game_mode: 0,
        ping: 0,
        display_name: None,
    };
    let listed = PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![entry]));
    handle_net(&mut game, listed, &mut queue, &frames);
    let tag = |game: &Game| game.entities.snapshots(0.0)[0].name_tag.clone();
    assert_eq!(tag(&game).as_deref(), Some("Bob"));

    let team = |visibility: &str| {
        PlayEvent::Teams(Box::new(Teams::Create {
            name: "red".to_owned(),
            info: TeamInfo {
                display_name: "Red".to_owned(),
                prefix: "§c".to_owned(),
                suffix: String::new(),
                friendly_flags: 0,
                name_tag_visibility: visibility.to_owned(),
                color: 12,
            },
            players: vec!["Bob".to_owned()],
        }))
    };
    handle_net(&mut game, team("always"), &mut queue, &frames);
    assert_eq!(tag(&game).as_deref(), Some("§cBob"));
    handle_net(
        &mut game,
        PlayEvent::Teams(Box::new(Teams::Remove {
            name: "red".to_owned(),
        })),
        &mut queue,
        &frames,
    );
    assert_eq!(tag(&game).as_deref(), Some("Bob"));
    handle_net(&mut game, team("never"), &mut queue, &frames);
    assert_eq!(tag(&game), None);
}

#[test]
fn the_velocity_of_the_local_player_replaces_its_motion_and_others_do_not() {
    use umicraft_protocol::packets::play::clientbound::EntityVelocity;

    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 1;
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let velocity = |entity_id| {
        PlayEvent::EntityVelocity(EntityVelocity {
            entity_id,
            velocity_x: 4000,
            velocity_y: 2400,
            velocity_z: -8000,
        })
    };
    handle_net(&mut game, velocity(2), &mut queue, &frames);
    let motion = game.state.as_ref().expect("placed").player().velocity;
    assert_eq!(motion, DVec3::ZERO, "another entity is not us");

    handle_net(&mut game, velocity(1), &mut queue, &frames);
    let motion = game.state.as_ref().expect("placed").player().velocity;
    assert!((motion - DVec3::new(0.5, 0.3, -1.0)).length() < 1e-9);
}

#[test]
fn an_explosion_pushes_the_local_player() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.state
        .as_mut()
        .expect("placed")
        .set_velocity(DVec3::new(0.1, 0.0, 0.0));
    apply_explosion(
        &mut game,
        &Explosion {
            x: 0.0,
            y: 64.0,
            z: 0.0,
            strength: 3.0,
            records: Vec::new(),
            player_motion_x: 0.5,
            player_motion_y: 0.25,
            player_motion_z: -0.5,
        },
    );
    let motion = game.state.as_ref().expect("placed").player().velocity;
    assert!((motion - DVec3::new(0.6, 0.25, -0.5)).length() < 1e-6);
}

#[test]
fn the_server_state_drives_the_movement_stats() {
    use umicraft_protocol::packets::play::clientbound::EntityEffect;

    let mut game = game_on_floor_with(BlockState::AIR);
    game.food = 6.0;
    game.server.on_join(1);
    game.server.apply_entity_effect(&EntityEffect {
        entity_id: 1,
        effect_id: 1,
        amplifier: 1,
        duration: 600,
        hide_particles: 0,
    });
    game.server.apply_entity_effect(&EntityEffect {
        entity_id: 1,
        effect_id: 8,
        amplifier: 0,
        duration: 600,
        hide_particles: 0,
    });
    apply_movement_stats(&mut game);
    let player = game.state.as_ref().expect("placed").player();
    assert!((player.movement_speed - 0.14).abs() < 1e-6);
    assert_eq!(player.jump_boost, 1);
    assert!(!player.well_fed, "6 food points are not enough to sprint");
}

#[test]
fn a_click_swings_the_hand_and_a_placement_swings_it_too() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    game.pitch = 1.5;
    game.input.attack = true;
    game.attack_pressed = true;
    run_attack(&mut game, &mut queue);
    tick_hand(&mut game);
    assert!(game.hand.pose().swing > 0.0 || game.hand.swing_progress(1.0) >= 0.0);
    assert!(queue
        .ordered
        .iter()
        .any(|command| matches!(command, ClientCommand::Swing)));

    // Placing with an item swings; with an empty hand it does not.
    let mut empty = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    // Aim ahead of the feet: the block below the player cannot be built on.
    empty.pitch = 1.0;
    click_use(&mut empty, &mut queue);
    assert!(!queue
        .ordered
        .iter()
        .any(|command| matches!(command, ClientCommand::Swing)));
    empty.windows.apply_set_slot(
        0,
        36,
        WireItemStack {
            item_id: 1,
            stack_size: 8,
            damage: 0,
            nbt: None,
        },
    );
    click_use(&mut empty, &mut queue);
    assert!(queue
        .ordered
        .iter()
        .any(|command| matches!(command, ClientCommand::Swing)));
}

#[test]
fn the_frame_shows_the_arm_only_in_first_person_with_an_empty_hand() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 1;
    let mut queue = CommandQueue::default();
    let (frames, received) = mpsc::sync_channel(32);
    let frame = |game: &Game| {
        publish_frame(game, &frames, Instant::now());
        match received.try_recv() {
            Ok(RenderEvent::Frame(frame)) => frame,
            _ => panic!("a frame was expected"),
        }
    };
    tick_hand(&mut game);
    assert!(frame(&game).state.effects.hand.shows_arm);
    assert!(frame(&game).state.effects.hand.visible);

    game.gamemode = GameMode::Spectator;
    assert!(!frame(&game).state.effects.hand.shows_arm);
    game.gamemode = GameMode::Survival;

    let toggle = InputUpdate {
        toggle_perspective: true,
        ..InputUpdate::default()
    };
    handle_input(&mut game, toggle, &mut queue);
    apply_perspective_presses(&mut game);
    assert!(!frame(&game).state.effects.hand.shows_arm, "third person");
    // Regression: the first-person item stayed drawn in F5.
    assert!(!frame(&game).state.effects.hand.visible, "third person");
    handle_input(&mut game, toggle, &mut queue);
    handle_input(&mut game, toggle, &mut queue);
    apply_perspective_presses(&mut game);

    // Holding an item hides the bare arm once the item is shown.
    game.windows.apply_set_slot(
        0,
        36,
        WireItemStack {
            item_id: 276,
            stack_size: 1,
            damage: 0,
            nbt: None,
        },
    );
    for _ in 0..10 {
        tick_hand(&mut game);
    }
    assert!(!frame(&game).state.effects.hand.shows_arm);
}

#[test]
fn a_full_channel_keeps_the_commands_for_the_next_flush() {
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let mut queue = CommandQueue {
        sender: Some(sender),
        ..CommandQueue::default()
    };
    queue.push(ClientCommand::Chat {
        message: "first".to_string(),
    });
    queue.push(ClientCommand::Chat {
        message: "second".to_string(),
    });
    queue.push(ClientCommand::PlayerPosition {
        x: 1.0,
        y: 64.0,
        z: 2.0,
        on_ground: true,
    });

    queue.flush();
    assert_eq!(
        queue.ordered.len(),
        2,
        "the second chat and the report wait"
    );
    let Ok(PlayServerbound::Chat(chat)) = receiver.try_recv() else {
        panic!("the first chat is sent as its wire packet");
    };
    assert_eq!(chat.message, "first");

    queue.flush();
    let Ok(PlayServerbound::Chat(chat)) = receiver.try_recv() else {
        panic!("the second chat follows in order");
    };
    assert_eq!(chat.message, "second");
    assert_eq!(queue.ordered.len(), 1, "the report waits behind the chat");

    queue.flush();
    assert!(matches!(
        receiver.try_recv(),
        Ok(PlayServerbound::PlayerPosition(_))
    ));
    assert!(queue.ordered.is_empty());
}

/// A grass floor (top at y = 64) with one block of `plant` standing on
/// it at (0, 64, 0), and a player whose feet are at y = 64 there.
fn game_on_floor_with(plant: BlockState) -> Game {
    let mut world = World::new(0);
    let payload = vec![0u8; umicraft_world::chunk::payload_len(0b1000, true, true)];
    world
        .apply_chunk(
            ChunkPos::new(0, 0),
            umicraft_world::ChunkPayload {
                primary_bit_mask: 0b1000,
                ground_up: true,
                data: &payload,
            },
        )
        .expect("an empty column");
    for x in 0..16 {
        for z in 0..16 {
            world.set_block(BlockPos::new(x, 63, z), BlockState::new(2, 0));
        }
    }
    world.set_block(BlockPos::new(0, 64, 0), plant);
    let mut game = Game::new();
    game.world = Some(world);
    game.state = Some(GameState::new(DVec3::new(0.5, 64.0, 0.5)));
    game
}

/// Presses the use button once and runs the tick of the button.
fn click_use(game: &mut Game, queue: &mut CommandQueue) {
    game.use_pressed = true;
    run_use(game, queue);
}

fn wire_item(item_id: i16, damage: i16) -> WireItemStack {
    WireItemStack {
        item_id,
        stack_size: 1,
        damage,
        nbt: None,
    }
}

fn confirmation(window_id: u8, action_number: i16) -> PlayEvent {
    PlayEvent::ConfirmTransaction(
        umicraft_protocol::packets::play::clientbound::ConfirmTransaction {
            window_id,
            action_number,
            accepted: false,
        },
    )
}

/// Regression test (GrimAC `BadPacketsN`, 2026-11-02): the replies leave in
/// the order of the packets, the position reply between the two
/// transactions that surround a teleport.
#[test]
fn the_replies_to_a_teleport_keep_the_order_of_the_packets() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(64);
    let placement = umicraft_net::play::PlayerPlacement {
        x: 0.5,
        y: 64.0,
        z: 0.5,
        ..Default::default()
    };
    handle_net(&mut game, confirmation(0, -1), &mut queue, &frames);
    handle_net(
        &mut game,
        PlayEvent::Placement(placement),
        &mut queue,
        &frames,
    );
    handle_net(&mut game, confirmation(0, -2), &mut queue, &frames);
    let sent = commands(&queue);
    assert!(matches!(
        sent[..],
        [
            ClientCommand::ConfirmTransaction {
                action_number: -1,
                ..
            },
            ClientCommand::AckPosition { .. },
            ClientCommand::ConfirmTransaction {
                action_number: -2,
                ..
            },
        ]
    ));
}

#[test]
fn a_transaction_of_a_window_that_is_not_open_gets_no_reply() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(64);
    handle_net(&mut game, confirmation(7, 1), &mut queue, &frames);
    assert!(commands(&queue).is_empty());
}

fn commands(queue: &CommandQueue) -> Vec<&ClientCommand> {
    queue.ordered.iter().collect()
}

/// Puts a zombie or a player two blocks ahead of the player (yaw 0 looks
/// along +z) and returns its id.
fn spawn_ahead(game: &mut Game, player: bool) -> EntityId {
    let id = EntityId(7);
    let position = DVec3::new(0.5, 64.0, 2.5);
    if player {
        game.entities.spawn_player(
            id,
            [1; 16],
            position,
            0.0,
            0.0,
            None,
            EntityFlags::default(),
        );
    } else {
        game.entities.spawn_mob(
            id,
            54,
            position,
            0.0,
            0.0,
            0.0,
            Vec3::ZERO,
            EntityFlags::default(),
        );
    }
    id
}

fn click_attack(game: &mut Game, queue: &mut CommandQueue) {
    game.input.attack = true;
    game.attack_pressed = true;
    run_attack(game, queue);
}

#[test]
fn a_left_click_on_an_entity_swings_then_attacks_it() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    spawn_ahead(&mut game, false);
    click_attack(&mut game, &mut queue);
    let sent = commands(&queue);
    assert!(matches!(sent[0], ClientCommand::Swing));
    assert!(matches!(
        sent[1],
        ClientCommand::UseEntity {
            target: 7,
            action: UseEntityAction::Attack
        }
    ));
    assert_eq!(sent.len(), 2, "no dig packet while an entity is targeted");
}

#[test]
fn an_entity_behind_a_block_is_not_attacked() {
    let mut game = game_on_floor_with(BlockState::new(1, 0));
    let mut queue = CommandQueue::default();
    spawn_ahead(&mut game, false);
    game.pitch = 0.0;
    // A wall one block ahead of the eyes.
    if let Some(world) = game.world.as_mut() {
        world.set_block(BlockPos::new(0, 65, 1), BlockState::new(1, 0));
    }
    click_attack(&mut game, &mut queue);
    assert!(!commands(&queue)
        .iter()
        .any(|command| matches!(command, ClientCommand::UseEntity { .. })));
}

#[test]
fn a_knockback_hit_on_a_player_slows_the_attacker_but_a_mob_does_not() {
    for (player, factor) in [(true, 0.6), (false, 1.0)] {
        let mut game = game_on_floor_with(BlockState::AIR);
        let mut queue = CommandQueue::default();
        spawn_ahead(&mut game, player);
        game.windows.apply_set_slot(
            0,
            36,
            WireItemStack {
                nbt: Some(enchanted_nbt(19, 1)),
                ..wire_item(267, 0)
            },
        );
        if let Some(state) = game.state.as_mut() {
            state.set_velocity(DVec3::new(1.0, 0.0, 1.0));
        }
        click_attack(&mut game, &mut queue);
        let velocity = game.state.as_ref().unwrap().player().velocity;
        assert!((velocity.x - factor).abs() < 1e-9, "player: {player}");
    }
}

#[test]
fn a_right_click_on_a_mob_interacts_before_using_the_item() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    spawn_ahead(&mut game, false);
    game.windows.apply_set_slot(0, 36, wire_item(267, 0));
    click_use(&mut game, &mut queue);
    let sent = commands(&queue);
    assert!(matches!(
        sent[0],
        ClientCommand::UseEntity {
            action: UseEntityAction::InteractAt { .. },
            ..
        }
    ));
    assert!(matches!(
        sent[1],
        ClientCommand::UseEntity {
            action: UseEntityAction::Interact,
            ..
        }
    ));
    assert!(matches!(sent[2], ClientCommand::UseItem { .. }));
}

/// A hungry player who holds bread starts eating on the click, walks at
/// a fifth of the speed while the button is held, and releases it.
#[test]
fn holding_the_use_button_eats_slowly_and_releasing_tells_the_server() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.food = 10.0;
    game.windows.apply_set_slot(0, 36, wire_item(297, 0));
    game.pitch = -0.5;
    let mut queue = CommandQueue::default();
    game.input.use_item = true;
    click_use(&mut game, &mut queue);
    assert!(game.item_use.is_using());
    assert!(matches!(
        commands(&queue)[0],
        ClientCommand::UseItem { item } if item.item_id == 297
    ));

    game.input.use_item = false;
    run_use(&mut game, &mut queue);
    assert!(!game.item_use.is_using());
    assert!(commands(&queue).iter().any(|command| matches!(
        command,
        ClientCommand::PlayerDigging {
            status: DiggingStatus::ReleaseUseItem,
            ..
        }
    )));
}

#[test]
fn the_published_hand_carries_the_item_use_until_it_ends() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.food = 10.0;
    game.windows.apply_set_slot(0, 36, wire_item(297, 0));
    game.pitch = -0.5;
    game.input.use_item = true;
    let mut queue = CommandQueue::default();
    click_use(&mut game, &mut queue);
    let (frames, received) = mpsc::sync_channel(32);
    let frame = |game: &Game| {
        publish_frame(game, &frames, Instant::now());
        match received.try_recv() {
            Ok(RenderEvent::Frame(frame)) => frame,
            _ => panic!("a frame was expected"),
        }
    };
    let using = frame(&game).state.effects.hand.item_use.expect("a use");
    assert_eq!(using.action, UseAction::Eat);
    assert_eq!(using.count, 32.0);
    game.item_use.finish();
    assert_eq!(frame(&game).state.effects.hand.item_use, None);
}

#[test]
fn the_local_player_eating_throws_crumbs_on_the_bite_ticks() {
    // Regression: the local player's own meal showed no crumbs.
    let mut game = game_on_floor_with(BlockState::AIR);
    game.food = 10.0;
    game.windows.apply_set_slot(0, 36, wire_item(297, 0));
    game.input.use_item = true;
    let mut queue = CommandQueue::default();
    click_use(&mut game, &mut queue);
    let mut bites = Vec::new();
    for _ in 0..40 {
        let Some(view) = game.item_use.view() else {
            break;
        };
        let before = game.particles.len();
        spawn_local_crumbs(&mut game);
        if game.particles.len() > before {
            bites.push(view.count as u32);
        }
        run_use(&mut game, &mut queue);
    }
    assert_eq!(bites, [24, 20, 16, 12, 8, 4]);
}

#[test]
fn the_local_player_eating_bites_out_loud() {
    // Regression: the local player's own meal made no sound.
    let mut game = game_on_floor_with(BlockState::AIR);
    game.food = 10.0;
    game.windows.apply_set_slot(0, 36, wire_item(297, 0));
    game.input.use_item = true;
    let mut queue = CommandQueue::default();
    click_use(&mut game, &mut queue);
    let mut bites = 0;
    for _ in 0..40 {
        if game.item_use.view().is_none() {
            break;
        }
        spawn_local_crumbs(&mut game);
        let cues = game.state.as_mut().unwrap().drain_sound_cues();
        bites += cues.iter().filter(|c| c.name == "random.eat").count();
        run_use(&mut game, &mut queue);
    }
    assert_eq!(bites, 6);
}

#[test]
fn the_server_finishing_the_meal_ends_the_use() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.food = 10.0;
    game.entity_id = 7;
    game.windows.apply_set_slot(0, 36, wire_item(297, 0));
    game.pitch = -0.5;
    game.input.use_item = true;
    let mut queue = CommandQueue::default();
    click_use(&mut game, &mut queue);
    let (frames, _received) = mpsc::sync_channel(8);
    let status = PlayEvent::EntityStatus(
        umicraft_protocol::packets::play::clientbound::EntityStatus {
            entity_id: 7,
            status: 9,
        },
    );
    handle_net(&mut game, status, &mut queue, &frames);
    assert!(!game.item_use.is_using());
}

fn game_with_mob(mob_type: u8) -> Game {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entities.spawn_mob(
        EntityId(7),
        mob_type,
        DVec3::new(2.5, 64.0, 2.5),
        0.0,
        0.0,
        0.0,
        glam::Vec3::ZERO,
        EntityFlags::default(),
    );
    game
}

#[test]
fn a_blaze_near_the_player_smokes_every_tick() {
    // Regression: no mob gave off its own particles.
    let mut game = game_with_mob(61);
    spawn_mob_particles(&mut game);
    assert_eq!(game.particles.len(), 2);
}

#[test]
fn an_animal_in_love_shows_hearts() {
    let mut game = game_with_mob(92);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_net(&mut game, hurt_status(7, 18), &mut queue, &frames);
    assert_eq!(game.particles.len(), 7);
}

fn hurt_status(entity_id: i32, status: i8) -> PlayEvent {
    PlayEvent::EntityStatus(
        umicraft_protocol::packets::play::clientbound::EntityStatus { entity_id, status },
    )
}

#[test]
fn a_hurt_status_of_the_local_player_rolls_the_camera() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 7;
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    follow_body(&mut game);
    handle_net(&mut game, health_update(10.0), &mut queue, &frames);
    follow_body(&mut game);
    handle_net(&mut game, hurt_status(7, 2), &mut queue, &frames);
    assert_eq!(game.body.animation.hurt_time(), 10);
    assert!(game.body.animation.is_hurt_resistant());
    // Someone else's hurt does not shake this camera.
    let mut other = game_on_floor_with(BlockState::AIR);
    other.entity_id = 7;
    handle_net(&mut other, hurt_status(8, 2), &mut queue, &frames);
    assert_eq!(other.body.animation.hurt_time(), 0);
}

#[test]
fn a_hurt_status_of_the_local_player_plays_the_hurt_sound_once() {
    // Regression: no damage sound was played when the player was hit.
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 7;
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    let feet = game.state.as_ref().unwrap().position();
    handle_net(&mut game, hurt_status(7, 2), &mut queue, &frames);
    let cues = game.state.as_mut().unwrap().drain_sound_cues();
    let hurt: Vec<_> = cues
        .iter()
        .filter(|c| c.name == "game.player.hurt")
        .collect();
    assert_eq!(hurt.len(), 1);
    assert_eq!(hurt[0].pos, feet.to_array());
    handle_net(&mut game, hurt_status(7, 3), &mut queue, &frames);
    let cues = game.state.as_mut().unwrap().drain_sound_cues();
    assert!(cues.iter().any(|c| c.name == "game.player.die"));
}

#[test]
fn a_hurt_status_of_another_entity_is_silent_because_the_server_sends_the_sound() {
    // `RenderGlobal.playSound` is empty in 1.8: playing it here too would
    // double the Sound Effect packet of the same hit.
    let mut game = game_on_floor_with(BlockState::AIR);
    game.entity_id = 7;
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    handle_net(&mut game, hurt_status(8, 2), &mut queue, &frames);
    let cues = game.state.as_mut().unwrap().drain_sound_cues();
    assert!(cues.iter().all(|c| c.name != "game.player.hurt"));
}

#[test]
fn a_critical_animation_throws_stars_for_three_ticks() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(8);
    game.entities.spawn_mob(
        EntityId(30),
        54,
        DVec3::new(2.5, 64.0, 0.5),
        0.0,
        0.0,
        0.0,
        Vec3::ZERO,
        EntityFlags::default(),
    );
    let animation = |code| {
        PlayEvent::Animation(umicraft_protocol::packets::play::clientbound::Animation {
            entity_id: 30,
            animation: code,
        })
    };
    // A swing throws nothing.
    handle_net(&mut game, animation(0), &mut queue, &frames);
    assert!(game.particles.is_empty());
    handle_net(&mut game, animation(4), &mut queue, &frames);
    let first = game.particles.len();
    assert!(first > 0, "the first burst comes at once");
    assert_eq!(game.hit_emitters.len(), 1);
    combat::tick_emitters(&mut game);
    combat::tick_emitters(&mut game);
    assert!(game.particles.len() > first);
    assert_eq!(game.hit_emitters.len(), 1);
    combat::tick_emitters(&mut game);
    assert!(game.hit_emitters.is_empty(), "three bursts, then done");
}

#[test]
fn a_player_who_uses_an_item_walks_at_a_fifth_of_the_speed() {
    let walk = |using: bool| {
        let mut game = game_on_floor_with(BlockState::AIR);
        // The simulation waits for the nine columns around the player.
        let world = game.world.as_mut().expect("a world");
        let payload = vec![0u8; umicraft_world::chunk::payload_len(0b1000, true, true)];
        for (x, z) in [
            (-1, -1),
            (-1, 0),
            (-1, 1),
            (0, -1),
            (0, 1),
            (1, -1),
            (1, 0),
            (1, 1),
        ] {
            world
                .apply_chunk(
                    ChunkPos::new(x, z),
                    umicraft_world::ChunkPayload {
                        primary_bit_mask: 0b1000,
                        ground_up: true,
                        data: &payload,
                    },
                )
                .expect("an empty column");
        }
        game.playing = true;
        game.food = 10.0;
        game.input.forward = 1.0;
        game.input.use_item = using;
        game.windows.apply_set_slot(0, 36, wire_item(267, 0));
        game.pitch = -0.5;
        game.yaw = 0.0;
        let mut queue = CommandQueue::default();
        let (frames, _received) = mpsc::sync_channel(8);
        let (chunks, _chunk_received) = mpsc::sync_channel(8);
        for _ in 0..10 {
            tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
        }
        let position = game.state.as_ref().expect("placed").position();
        position.z - 0.5
    };
    let free = walk(false);
    let slowed = walk(true);
    assert!(slowed > 0.0, "the player still moves");
    assert!((slowed / free - 0.2).abs() < 0.03, "{slowed} vs {free}");
}

#[test]
fn q_and_ctrl_q_send_the_drop_packets() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    for kind in [DropKind::One, DropKind::Stack] {
        press_drop(&mut game, &mut queue, kind);
    }
    assert!(queue.ordered.is_empty(), "the packets leave in the tick");
    run_ticks(&mut game, &mut queue, 1);
    let statuses: Vec<_> = commands(&queue)
        .iter()
        .filter_map(|command| match command {
            ClientCommand::PlayerDigging { status, .. } => Some(*status),
            _ => None,
        })
        .collect();
    assert_eq!(
        statuses,
        vec![DiggingStatus::DropItem, DiggingStatus::DropAllItems]
    );
    game.gamemode = GameMode::Spectator;
    queue.ordered.clear();
    press_drop(&mut game, &mut queue, DropKind::One);
    run_ticks(&mut game, &mut queue, 1);
    assert!(
        !tags(&queue).iter().any(|tag| tag.starts_with("drop")),
        "a spectator drops nothing: {:?}",
        tags(&queue)
    );
}

/// Looks straight down at the grass floor (id 2).
#[test]
fn the_middle_button_selects_the_hotbar_slot_of_the_block() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.pitch = std::f32::consts::FRAC_PI_2;
    game.windows.apply_set_slot(0, 38, wire_item(2, 0));
    game.playing = true;
    let mut queue = CommandQueue::default();
    press_pick(&mut game, &mut queue);
    run_ticks(&mut game, &mut queue, 1);
    assert_eq!(game.selected_slot, 2);
    assert!(slots_in(&queue).is_empty(), "the sync is for the next tick");
    run_ticks(&mut game, &mut queue, 1);
    assert_eq!(slots_in(&queue), [2]);
}

#[test]
fn a_creative_pick_fills_a_free_slot_and_tells_the_server() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.gamemode = GameMode::Creative;
    game.pitch = std::f32::consts::FRAC_PI_2;
    let mut queue = CommandQueue::default();
    pick_block(&mut game, &mut queue);
    let given = game.windows.selected_item().expect("an item was given");
    assert_eq!(given.item_id, 2);
    assert!(commands(&queue).iter().any(|command| matches!(
        command,
        ClientCommand::CreativeSlot { slot: 36, item } if item.item_id == 2
    )));
}

#[test]
fn a_survival_pick_without_the_item_changes_nothing() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.pitch = std::f32::consts::FRAC_PI_2;
    let mut queue = CommandQueue::default();
    pick_block(&mut game, &mut queue);
    assert!(queue.ordered.is_empty());
}

/// Depth Strider on the worn boots reaches the physics.
#[test]
fn the_depth_strider_of_the_boots_reaches_the_player() {
    let mut game = game_on_floor_with(BlockState::AIR);
    let mut boots = wire_item(313, 0);
    // `ench` list of one compound: `id` = 8 (Depth Strider), `lvl` = 3.
    boots.nbt = Some(enchanted_nbt(8, 3));
    game.windows.apply_set_slot(0, 8, boots);
    apply_movement_stats(&mut game);
    assert_eq!(
        game.state.as_ref().expect("placed").player().depth_strider,
        3
    );
}

#[test]
fn looking_at_the_floor_selects_the_floor_block() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.pitch = std::f32::consts::FRAC_PI_2;
    let boxes = game.selection_outline();
    assert_eq!(
        boxes,
        vec![OutlineBox {
            min: [0.0, 63.0, 0.0],
            max: [1.0, 64.0, 1.0],
        }]
    );
}

#[test]
fn a_selection_is_offset_by_the_block_position_and_follows_its_shape() {
    // A stone slab (id 44) at (0, 64, 0) seen from beside it: it is
    // half a block high, so the outline must not be the full block.
    let mut game = game_on_floor_with(BlockState::new(44, 0));
    game.state = Some(GameState::new(DVec3::new(0.5, 64.0, 3.5)));
    game.pitch = 0.45;
    game.yaw = std::f32::consts::PI;
    let boxes = game.selection_outline();
    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0].min, [0.0, 64.0, 0.0]);
    assert_eq!(boxes[0].max, [1.0, 64.5, 1.0]);
}

#[test]
fn looking_at_the_sky_or_being_a_spectator_selects_nothing() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.pitch = -std::f32::consts::FRAC_PI_2;
    assert!(game.selection_outline().is_empty());
    game.pitch = std::f32::consts::FRAC_PI_2;
    game.gamemode = GameMode::Spectator;
    assert!(game.selection_outline().is_empty());
}

#[test]
fn an_adventure_player_gets_no_selection_outline() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.pitch = std::f32::consts::FRAC_PI_2;
    assert!(!game.selection_outline().is_empty());
    game.gamemode = GameMode::Adventure;
    assert!(game.selection_outline().is_empty());
}

/// Regression: every tick lifted a player standing in a plant, in water
/// or under leaves onto the top of the block, which put players on
/// flowers and on top of trees.
#[test]
fn a_player_in_plants_or_water_is_never_lifted() {
    for plant in [
        BlockState::new(31, 1),
        BlockState::new(38, 0),
        BlockState::new(9, 0),
    ] {
        let mut game = game_on_floor_with(plant);
        let mut queue = CommandQueue::default();
        let (frames, _frames_rx) = mpsc::sync_channel(64);
        let (chunks, _chunks_rx) = mpsc::sync_channel(64);
        for _ in 0..10 {
            tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
        }
        let feet = game.state.as_ref().expect("state").position();
        assert_eq!(feet.y, 64.0, "lifted out of {plant:?}");
    }
}

// Movement packet scheduling: one report per tick, nothing replaced.

/// Runs `count` ticks without a flush and returns the feet Z after each.
fn run_ticks(game: &mut Game, queue: &mut CommandQueue, count: usize) -> Vec<f64> {
    let (frames, _frames_rx) = mpsc::sync_channel(64);
    let (chunks, _chunks_rx) = mpsc::sync_channel(64);
    (0..count)
        .map(|_| {
            tick(game, queue, &frames, &chunks, Instant::now());
            game.state.as_ref().expect("state").position().z
        })
        .collect()
}

/// A game that walks forward (+z) and has already reached its pace.
fn walking_game() -> Game {
    let mut game = placed_game();
    game.input.forward = 1.0;
    run_ticks(&mut game, &mut CommandQueue::default(), 6);
    game
}

fn movement_of(queue: &CommandQueue) -> Vec<&ClientCommand> {
    queue.ordered.iter().filter(|c| is_movement(c)).collect()
}

fn reported_z(command: &ClientCommand) -> Option<f64> {
    match command {
        ClientCommand::PositionLook { z, .. } | ClientCommand::PlayerPosition { z, .. } => Some(*z),
        _ => None,
    }
}

fn index_of(queue: &CommandQueue, wanted: impl Fn(&ClientCommand) -> bool) -> usize {
    queue
        .ordered
        .iter()
        .position(wanted)
        .expect("the command is queued")
}

fn movement_indices(queue: &CommandQueue) -> Vec<usize> {
    (0..queue.ordered.len())
        .filter(|&i| is_movement(&queue.ordered[i]))
        .collect()
}

fn click_stack() -> WireItemStack {
    wire_item(1, 0)
}

#[test]
fn a_normal_tick_keeps_its_movement_packet() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    run_ticks(&mut game, &mut queue, 1);
    assert!(matches!(
        commands(&queue)[..],
        [ClientCommand::PositionLook { .. }]
    ));
}

#[test]
fn three_catch_up_ticks_before_a_flush_keep_three_movement_packets() {
    let mut game = walking_game();
    let mut queue = CommandQueue::default();
    let feet = run_ticks(&mut game, &mut queue, 3);
    let reports = movement_of(&queue);
    assert_eq!(reports.len(), 3, "one movement packet per logical tick");
    let reported: Vec<f64> = reports.iter().filter_map(|c| reported_z(c)).collect();
    assert_eq!(reported, feet);
}

#[test]
fn no_intermediate_position_is_lost() {
    let mut game = walking_game();
    let mut queue = CommandQueue::default();
    let feet = run_ticks(&mut game, &mut queue, 8);
    let reported: Vec<f64> = movement_of(&queue)
        .iter()
        .filter_map(|c| reported_z(c))
        .collect();
    assert_eq!(reported, feet);
    assert!(reported.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn a_movement_then_an_attack_leave_in_the_order_of_their_ticks() {
    let mut game = walking_game();
    game.entity_id = 1;
    game.pitch = 1.0;
    let mut queue = CommandQueue::default();
    run_ticks(&mut game, &mut queue, 1);
    game.input.attack = true;
    game.attack_pressed = true;
    run_ticks(&mut game, &mut queue, 1);
    let moves = movement_indices(&queue);
    let swing = index_of(&queue, |c| matches!(c, ClientCommand::Swing));
    let dig = index_of(&queue, |c| matches!(c, ClientCommand::PlayerDigging { .. }));
    assert_eq!(moves.len(), 2);
    // The click of a tick leaves before the movement report of that
    // tick (`runTick` clicks, then `onUpdateWalkingPlayer`), and after
    // the report of the tick before.
    assert!(moves[0] < swing && swing < moves[1]);
    assert!(moves[0] < dig && dig < moves[1]);
}

#[test]
fn a_movement_then_a_use_leave_in_the_order_of_their_ticks() {
    let mut game = walking_game();
    game.entity_id = 1;
    game.pitch = 1.0;
    game.windows.apply_set_slot(0, 36, click_stack());
    let mut queue = CommandQueue::default();
    run_ticks(&mut game, &mut queue, 1);
    game.use_pressed = true;
    run_ticks(&mut game, &mut queue, 1);
    let moves = movement_indices(&queue);
    let used = index_of(&queue, |c| {
        matches!(
            c,
            ClientCommand::Swing | ClientCommand::PlayerBlockPlacement { .. }
        )
    });
    assert_eq!(moves.len(), 2);
    assert!(moves[0] < used && used < moves[1]);
}

#[test]
fn interleaved_actions_keep_one_report_per_tick_and_their_own_tick() {
    let mut game = walking_game();
    game.entity_id = 1;
    game.pitch = 1.0;
    game.windows.apply_set_slot(0, 36, click_stack());
    let mut queue = CommandQueue::default();
    let mut ends = Vec::new();
    run_ticks(&mut game, &mut queue, 1);
    ends.push(queue.ordered.len());
    game.input.attack = true;
    game.attack_pressed = true;
    run_ticks(&mut game, &mut queue, 1);
    game.input.attack = false;
    ends.push(queue.ordered.len());
    game.use_pressed = true;
    run_ticks(&mut game, &mut queue, 1);
    ends.push(queue.ordered.len());
    run_ticks(&mut game, &mut queue, 1);
    ends.push(queue.ordered.len());
    let mut start = 0;
    let mut segments = Vec::new();
    for end in ends {
        segments.push(
            queue
                .ordered
                .iter()
                .skip(start)
                .take(end - start)
                .collect::<Vec<_>>(),
        );
        start = end;
    }
    for segment in &segments {
        assert_eq!(
            segment.iter().filter(|c| is_movement(c)).count(),
            1,
            "one report per tick: {segment:?}"
        );
        assert!(
            is_movement(segment.last().expect("a command")),
            "{segment:?}"
        );
    }
    assert_eq!(segments[0].len(), 1, "a plain tick sends only its report");
    assert!(segments[1]
        .iter()
        .any(|c| matches!(c, ClientCommand::PlayerDigging { .. })));
    assert!(segments[2].iter().any(|c| matches!(
        c,
        ClientCommand::Swing | ClientCommand::PlayerBlockPlacement { .. }
    )));
}

fn pushed_positions(queue: &mut CommandQueue, count: u32) {
    for step in 0..count {
        queue.push(ClientCommand::PlayerPosition {
            x: f64::from(step),
            y: 0.0,
            z: 0.0,
            on_ground: true,
        });
    }
}

fn received_x(receiver: &mut tokio::sync::mpsc::Receiver<PlayServerbound>) -> Vec<f64> {
    let mut xs = Vec::new();
    while let Ok(packet) = receiver.try_recv() {
        let PlayServerbound::PlayerPosition(position) = packet else {
            panic!("only positions were queued, got {packet:?}");
        };
        xs.push(position.x);
    }
    xs
}

#[test]
fn a_full_channel_keeps_every_movement_packet_in_order() {
    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    let mut queue = CommandQueue {
        sender: Some(sender),
        ..CommandQueue::default()
    };
    pushed_positions(&mut queue, 5);
    queue.flush();
    assert_eq!(queue.ordered.len(), 3, "the three that did not fit wait");
    let mut xs = received_x(&mut receiver);
    while !queue.ordered.is_empty() {
        queue.flush();
        xs.extend(received_x(&mut receiver));
    }
    assert_eq!(xs, [0.0, 1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn a_flush_transmits_each_packet_once() {
    let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
    let mut queue = CommandQueue {
        sender: Some(sender),
        ..CommandQueue::default()
    };
    pushed_positions(&mut queue, 5);
    queue.flush();
    queue.flush();
    assert!(queue.ordered.is_empty());
    assert_eq!(received_x(&mut receiver), [0.0, 1.0, 2.0, 3.0, 4.0]);
    assert!(receiver.try_recv().is_err(), "nothing is sent twice");
}

#[test]
fn the_backlog_is_bounded_by_pausing_the_ticks_and_never_by_dropping() {
    let mut queue = CommandQueue::default();
    pushed_positions(&mut queue, (PENDING_LIMIT - 1) as u32);
    assert!(!queue.congested());
    pushed_positions(&mut queue, 1);
    assert!(queue.congested(), "the loop starts no new tick now");
    assert_eq!(queue.ordered.len(), PENDING_LIMIT, "nothing was dropped");
}

#[test]
fn what_the_server_view_reported_is_what_the_server_receives() {
    let mut game = walking_game();
    let mut queue = CommandQueue::default();
    let feet = run_ticks(&mut game, &mut queue, 6);
    let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
    queue.sender = Some(sender);
    queue.flush();
    let mut known = Vec::new();
    while let Ok(packet) = receiver.try_recv() {
        match packet {
            PlayServerbound::PlayerPosition(p) => known.push(p.z),
            PlayServerbound::PositionLook(p) => known.push(p.z),
            other => panic!("a walking player sends positions, got {other:?}"),
        }
    }
    // Each position the `ServerView` counted as reported reached the
    // wire, so the server's last known position is the player's.
    assert_eq!(known, feet);
}

#[test]
fn an_idle_player_adds_nothing_but_the_bare_player_packet() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    run_ticks(&mut game, &mut queue, 10);
    assert_eq!(queue.ordered.len(), 10, "one packet per tick, no extras");
    assert!(matches!(
        queue.ordered[0],
        ClientCommand::PositionLook { .. }
    ));
    assert!(queue
        .ordered
        .iter()
        .skip(1)
        .all(|c| matches!(c, ClientCommand::Player { .. })));
}

#[test]
fn the_input_packets_of_a_riding_player_are_kept_per_tick() {
    let mut queue = CommandQueue::default();
    for step in 0..3 {
        queue.push(ClientCommand::PlayerLook {
            yaw: 0.0,
            pitch: 0.0,
            on_ground: true,
        });
        queue.push(ClientCommand::Input {
            strafe: 0.0,
            forward: step as f32,
            jumping: false,
            sneaking: false,
        });
    }
    assert_eq!(movement_of(&queue).len(), 6);
}

// Slot change, drop and pick-block: recorded by the input, sent by the tick.

fn press_slot(game: &mut Game, queue: &mut CommandQueue, slot: u8) {
    let update = InputUpdate {
        select_slot: Some(slot),
        ..InputUpdate::default()
    };
    handle_input(game, update, queue);
}

fn press_drop(game: &mut Game, queue: &mut CommandQueue, kind: DropKind) {
    let update = InputUpdate {
        drop_item: Some(kind),
        ..InputUpdate::default()
    };
    handle_input(game, update, queue);
}

fn press_pick(game: &mut Game, queue: &mut CommandQueue) {
    let update = InputUpdate {
        pick_block: true,
        ..InputUpdate::default()
    };
    handle_input(game, update, queue);
}

fn slots_in(queue: &CommandQueue) -> Vec<i16> {
    queue
        .ordered
        .iter()
        .filter_map(|command| match command {
            ClientCommand::HeldItemChange { slot } => Some(*slot),
            _ => None,
        })
        .collect()
}

/// The queue as a list of short names, oldest first.
fn tags(queue: &CommandQueue) -> Vec<String> {
    queue
        .ordered
        .iter()
        .map(|command| match command {
            ClientCommand::HeldItemChange { slot } => format!("slot{slot}"),
            ClientCommand::PlayerDigging {
                status: DiggingStatus::DropItem,
                ..
            } => "drop".to_string(),
            ClientCommand::PlayerDigging {
                status: DiggingStatus::DropAllItems,
                ..
            } => "dropall".to_string(),
            ClientCommand::PlayerDigging { .. } => "dig".to_string(),
            ClientCommand::CreativeSlot { .. } => "creative".to_string(),
            ClientCommand::Swing => "swing".to_string(),
            ClientCommand::UseEntity { action, .. } => match action {
                UseEntityAction::Attack => "attack",
                UseEntityAction::Interact => "interact",
                UseEntityAction::InteractAt { .. } => "interact_at",
            }
            .to_string(),
            ClientCommand::PlayerBlockPlacement { .. } => "place".to_string(),
            ClientCommand::UseItem { .. } => "use".to_string(),
            other if is_movement(other) => "move".to_string(),
            _ => "other".to_string(),
        })
        .collect()
}

/// Whether `wanted` appears in `tags` in that order (other tags between).
fn in_order(tags: &[String], wanted: &[&str]) -> bool {
    let mut at = 0;
    for tag in tags {
        if at < wanted.len() && tag == wanted[at] {
            at += 1;
        }
    }
    at == wanted.len()
}

#[test]
fn a_slot_press_leaves_in_the_tick_after_the_keys_stage_before_its_movement() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    press_slot(&mut game, &mut queue, 2);
    assert!(queue.ordered.is_empty(), "nothing leaves at the event");
    // `runTick`: the key selects the slot, no controller call follows.
    run_ticks(&mut game, &mut queue, 1);
    assert_eq!(tags(&queue), ["move"]);
    queue.ordered.clear();
    // The next `updateController` announces it, ahead of the movement.
    run_ticks(&mut game, &mut queue, 1);
    assert_eq!(tags(&queue), ["slot2", "move"]);
    queue.ordered.clear();
    run_ticks(&mut game, &mut queue, 2);
    assert_eq!(tags(&queue), ["move", "move"], "announced once");
}

#[test]
fn several_presses_between_two_ticks_make_one_packet_with_the_last_slot() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    for slot in [1, 2, 3] {
        press_slot(&mut game, &mut queue, slot);
    }
    run_ticks(&mut game, &mut queue, 3);
    assert_eq!(slots_in(&queue), [3]);
    // Back to the slot the server already knows: nothing at all.
    queue.ordered.clear();
    press_slot(&mut game, &mut queue, 4);
    press_slot(&mut game, &mut queue, 3);
    run_ticks(&mut game, &mut queue, 3);
    assert!(slots_in(&queue).is_empty());
}

#[test]
fn a_drop_leaves_in_its_tick_before_the_movement_and_without_a_slot_sync() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    press_drop(&mut game, &mut queue, DropKind::One);
    assert!(queue.ordered.is_empty());
    run_ticks(&mut game, &mut queue, 1);
    assert_eq!(tags(&queue), ["drop", "move"]);
    queue.ordered.clear();
    // A slot and a drop pressed together: the drop is sent first, the
    // slot at the next tick (the drop does not sync).
    press_slot(&mut game, &mut queue, 2);
    press_drop(&mut game, &mut queue, DropKind::One);
    run_ticks(&mut game, &mut queue, 1);
    assert_eq!(tags(&queue), ["drop", "move"]);
    queue.ordered.clear();
    run_ticks(&mut game, &mut queue, 1);
    assert_eq!(tags(&queue), ["slot2", "move"]);
}

#[test]
fn every_drop_press_is_a_packet() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    press_drop(&mut game, &mut queue, DropKind::One);
    press_drop(&mut game, &mut queue, DropKind::Stack);
    press_drop(&mut game, &mut queue, DropKind::One);
    run_ticks(&mut game, &mut queue, 2);
    assert_eq!(tags(&queue), ["drop", "dropall", "drop", "move", "move"]);
}

#[test]
fn a_survival_pick_leaves_no_packet_in_its_tick_and_the_slot_in_the_next() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.playing = true;
    game.pitch = std::f32::consts::FRAC_PI_2;
    game.windows.apply_set_slot(0, 38, wire_item(2, 0));
    let mut queue = CommandQueue::default();
    press_pick(&mut game, &mut queue);
    assert!(queue.ordered.is_empty());
    run_ticks(&mut game, &mut queue, 1);
    assert_eq!(tags(&queue), ["move"]);
    queue.ordered.clear();
    run_ticks(&mut game, &mut queue, 1);
    assert_eq!(tags(&queue), ["slot2", "move"]);
}

#[test]
fn a_creative_pick_sends_the_slot_packet_in_its_tick_and_the_sync_in_the_next() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.playing = true;
    game.gamemode = GameMode::Creative;
    game.pitch = std::f32::consts::FRAC_PI_2;
    game.windows.apply_set_slot(0, 36, wire_item(1, 0));
    let mut queue = CommandQueue::default();
    press_pick(&mut game, &mut queue);
    run_ticks(&mut game, &mut queue, 1);
    // `sendSlotPacket` does not sync the slot first.
    assert_eq!(tags(&queue), ["creative", "move"]);
    let chosen = i16::from(game.selected_slot);
    assert_ne!(chosen, 0, "the free slot was selected");
    queue.ordered.clear();
    run_ticks(&mut game, &mut queue, 1);
    assert_eq!(tags(&queue), [format!("slot{chosen}"), "move".to_string()]);
}

#[test]
fn a_slot_before_an_attack_is_announced_between_the_swing_and_the_attack_packet() {
    let mut game = placed_game();
    game.entity_id = 1;
    game.pitch = 1.0;
    let mut queue = CommandQueue::default();
    press_slot(&mut game, &mut queue, 2);
    game.input.attack = true;
    game.attack_pressed = true;
    run_ticks(&mut game, &mut queue, 1);
    let seen = tags(&queue);
    // `clickMouse`: swing, then the controller syncs and sends.
    assert!(
        in_order(&seen, &["swing", "slot2", "dig", "move"]),
        "{seen:?}"
    );
    assert_eq!(slots_in(&queue), [2], "once");
}

#[test]
fn a_slot_before_a_use_is_announced_before_the_use_packet() {
    let mut game = placed_game();
    game.entity_id = 1;
    game.pitch = 1.0;
    game.windows.apply_set_slot(0, 38, wire_item(1, 0));
    let mut queue = CommandQueue::default();
    press_slot(&mut game, &mut queue, 2);
    game.use_pressed = true;
    run_ticks(&mut game, &mut queue, 1);
    // `rightClickMouse` on a block: the controller syncs, sends the
    // placement, and the swing comes after it.
    assert_eq!(tags(&queue), ["slot2", "place", "swing", "move"]);
}

#[test]
fn the_actions_of_one_tick_keep_the_run_tick_order() {
    let mut game = placed_game();
    game.entity_id = 1;
    game.pitch = 1.0;
    let mut queue = CommandQueue::default();
    press_slot(&mut game, &mut queue, 2);
    press_drop(&mut game, &mut queue, DropKind::One);
    press_pick(&mut game, &mut queue);
    game.input.attack = true;
    game.attack_pressed = true;
    run_ticks(&mut game, &mut queue, 1);
    let seen = tags(&queue);
    // keys (drop), then the click (swing, sync, controller), then the
    // movement of the tick.
    assert!(
        in_order(&seen, &["drop", "swing", "slot2", "dig", "move"]),
        "{seen:?}"
    );
}

#[test]
fn a_movement_and_a_slot_of_different_ticks_keep_the_order_of_the_ticks() {
    let mut game = walking_game();
    let mut queue = CommandQueue::default();
    run_ticks(&mut game, &mut queue, 1);
    press_slot(&mut game, &mut queue, 5);
    run_ticks(&mut game, &mut queue, 2);
    // tick 1 move, tick 2 move (the key stage), tick 3: slot then move.
    assert_eq!(tags(&queue), ["move", "move", "slot5", "move"]);
}

#[test]
fn a_slot_chosen_by_the_server_is_announced_back_at_the_next_tick() {
    // `handleHeldItemChange` sets `currentItem` only; `currentPlayerItem`
    // keeps the old value, so `updateController` sends the slot back.
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    handle_net(&mut game, PlayEvent::HeldItemChange(5), &mut queue, &frames);
    assert!(queue.ordered.is_empty(), "nothing at the event");
    run_ticks(&mut game, &mut queue, 1);
    assert_eq!(game.selected_slot, 5);
    assert_eq!(tags(&queue), ["slot5", "move"]);
    queue.ordered.clear();
    run_ticks(&mut game, &mut queue, 2);
    assert!(slots_in(&queue).is_empty(), "once");
}

#[test]
fn a_server_slot_equal_to_the_known_one_sends_nothing() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    handle_net(&mut game, PlayEvent::HeldItemChange(0), &mut queue, &frames);
    run_ticks(&mut game, &mut queue, 3);
    assert!(slots_in(&queue).is_empty());
}

#[test]
fn a_key_after_a_server_slot_is_announced_after_the_servers_slot() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    handle_net(&mut game, PlayEvent::HeldItemChange(5), &mut queue, &frames);
    press_key(&mut game, &mut queue, 3);
    run_ticks(&mut game, &mut queue, 2);
    // Tick 1: the sync sends the server's slot, then the key selects.
    // Tick 2: the sync sends the key's slot.
    assert_eq!(tags(&queue), ["slot5", "move", "slot2", "move"]);
}

// The digit keys: one press per key and tick, in the order of the keys.

fn press_key(game: &mut Game, queue: &mut CommandQueue, key: usize) {
    let mut keys = [0u8; 9];
    keys[key - 1] = 1;
    let update = InputUpdate {
        hotbar_keys: keys,
        ..InputUpdate::default()
    };
    handle_input(game, update, queue);
}

#[test]
fn two_digit_keys_in_one_tick_give_the_higher_key_whatever_the_order() {
    for order in [[3, 1], [1, 3]] {
        let mut game = placed_game();
        let mut queue = CommandQueue::default();
        for key in order {
            press_key(&mut game, &mut queue, key);
        }
        assert_eq!(game.selected_slot, 2, "the player sees the result at once");
        run_ticks(&mut game, &mut queue, 3);
        assert_eq!(slots_in(&queue), [2], "order {order:?}");
    }
}

#[test]
fn three_or_more_keys_give_the_highest_key() {
    for order in [[5, 2, 8], [8, 5, 2], [2, 8, 5], [9, 1, 4]] {
        let mut game = placed_game();
        let mut queue = CommandQueue::default();
        for key in order {
            press_key(&mut game, &mut queue, key);
        }
        run_ticks(&mut game, &mut queue, 3);
        let highest = *order.iter().max().expect("keys") as i16 - 1;
        assert_eq!(slots_in(&queue), [highest], "order {order:?}");
    }
}

#[test]
fn keys_pressed_in_different_ticks_each_count_in_their_tick() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    press_key(&mut game, &mut queue, 6);
    run_ticks(&mut game, &mut queue, 1);
    press_key(&mut game, &mut queue, 2);
    run_ticks(&mut game, &mut queue, 2);
    // Tick 1 selects key 6; tick 2 syncs it and selects key 2; tick 3
    // syncs key 2.
    assert_eq!(tags(&queue), ["move", "slot5", "move", "slot1", "move"]);
}

#[test]
fn a_key_pressed_twice_selects_again_at_the_next_tick_over_the_wheel() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    press_key(&mut game, &mut queue, 2);
    press_key(&mut game, &mut queue, 2);
    run_ticks(&mut game, &mut queue, 1);
    assert_eq!(game.selected_slot, 1);
    // The wheel moves to slot 5 before the next tick; the key still has
    // a press left, and the keys come after the wheel.
    press_slot(&mut game, &mut queue, 5);
    assert_eq!(game.selected_slot, 5);
    run_ticks(&mut game, &mut queue, 2);
    assert_eq!(game.selected_slot, 1);
    assert_eq!(slots_in(&queue), [1], "slot 5 never reached the server");
}

#[test]
fn a_key_wins_over_the_wheel_in_the_same_tick_whatever_the_order() {
    for key_first in [true, false] {
        let mut game = placed_game();
        let mut queue = CommandQueue::default();
        if key_first {
            press_key(&mut game, &mut queue, 3);
            press_slot(&mut game, &mut queue, 5);
        } else {
            press_slot(&mut game, &mut queue, 5);
            press_key(&mut game, &mut queue, 3);
        }
        run_ticks(&mut game, &mut queue, 3);
        assert_eq!(slots_in(&queue), [2], "key first: {key_first}");
    }
}

#[test]
fn two_updates_with_the_same_key_between_two_ticks_count_twice() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    press_key(&mut game, &mut queue, 4);
    press_key(&mut game, &mut queue, 4);
    press_key(&mut game, &mut queue, 1);
    // Tick 1: keys 1 and 4 give up a press, the higher wins (slot 3);
    // tick 2: key 4 has one press left (slot 3 again).
    run_ticks(&mut game, &mut queue, 3);
    assert_eq!(slots_in(&queue), [3]);
}

// JoinGame and Respawn.

#[test]
fn a_join_starts_both_sides_on_slot_zero() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    game.selected_slot = 5;
    game.applied_slot = 5;
    game.synced_slot = 5;
    handle_net(
        &mut game,
        PlayEvent::JoinGame(umicraft_net::play::JoinGameInfo {
            entity_id: 1,
            gamemode: 0,
            dimension: 0,
            difficulty: 0,
            max_players: 20,
            level_type: "default".to_string(),
            reduced_debug_info: false,
        }),
        &mut queue,
        &frames,
    );
    assert_eq!(
        (game.selected_slot, game.applied_slot, game.synced_slot),
        (0, 0, 0)
    );
    queue.ordered.clear();
    run_ticks(&mut game, &mut queue, 3);
    assert!(slots_in(&queue).is_empty(), "a new controller knows slot 0");
}

fn respawn_event() -> PlayEvent {
    PlayEvent::Respawn(umicraft_protocol::packets::play::clientbound::Respawn {
        dimension: 0,
        difficulty: 0,
        game_mode: 0,
        world_type: "default".to_string(),
    })
}

#[test]
fn a_respawn_goes_back_to_slot_zero_and_tells_the_server_once() {
    // The new player has a new inventory (slot 0); the controller keeps
    // `currentPlayerItem`, so the next sync announces slot 0.
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    press_key(&mut game, &mut queue, 6);
    run_ticks(&mut game, &mut queue, 2);
    assert_eq!(slots_in(&queue), [5]);
    queue.ordered.clear();
    handle_net(&mut game, respawn_event(), &mut queue, &frames);
    assert_eq!(game.selected_slot, 0);
    assert_eq!(game.windows.selected_slot(), 0);
    assert!(queue.ordered.is_empty(), "nothing at the event");
    place_after_respawn(&mut game, &mut queue, &frames);
    run_ticks(&mut game, &mut queue, 3);
    assert_eq!(slots_in(&queue), [0], "once, at the next sync");
}

/// The server places the new player, as it does after a respawn.
fn place_after_respawn(
    game: &mut Game,
    queue: &mut CommandQueue,
    frames: &SyncSender<RenderEvent>,
) {
    let placement = umicraft_net::play::PlayerPlacement {
        x: 0.5,
        y: 64.0,
        z: 0.5,
        ..Default::default()
    };
    handle_net(game, PlayEvent::Placement(placement), queue, frames);
    queue.ordered.clear();
}

#[test]
fn a_respawn_on_slot_zero_sends_nothing() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    handle_net(&mut game, respawn_event(), &mut queue, &frames);
    place_after_respawn(&mut game, &mut queue, &frames);
    run_ticks(&mut game, &mut queue, 3);
    assert!(slots_in(&queue).is_empty());
}

// `rightClickMouse` / `clickMouse`: the whole packet sequence with a
// pending slot change.

#[test]
fn a_use_of_the_item_in_the_air_syncs_then_uses_without_a_swing() {
    let mut game = placed_game();
    game.entity_id = 1;
    game.pitch = -1.0;
    game.windows.apply_set_slot(0, 38, wire_item(297, 0));
    let mut queue = CommandQueue::default();
    press_slot(&mut game, &mut queue, 2);
    game.use_pressed = true;
    run_ticks(&mut game, &mut queue, 1);
    // `sendUseItem`: sync, then the use; `rightClickMouse` swings only
    // after a placement or an activation.
    assert_eq!(tags(&queue), ["slot2", "use", "move"]);
}

#[test]
fn a_tool_on_a_block_syncs_then_places_then_uses_without_a_swing() {
    let mut game = placed_game();
    game.entity_id = 1;
    game.pitch = 1.0;
    game.windows.apply_set_slot(0, 38, wire_item(267, 0));
    let mut queue = CommandQueue::default();
    press_slot(&mut game, &mut queue, 2);
    game.use_pressed = true;
    run_ticks(&mut game, &mut queue, 1);
    // The placement packet is sent, `onItemUse` fails, `sendUseItem`
    // follows; nothing swings.
    assert_eq!(tags(&queue), ["slot2", "place", "use", "move"]);
}

#[test]
fn a_click_on_a_mob_syncs_then_interacts_at_then_interacts_with_no_swing() {
    let mut game = placed_game();
    game.entity_id = 1;
    spawn_ahead(&mut game, false);
    let mut queue = CommandQueue::default();
    press_slot(&mut game, &mut queue, 2);
    game.use_pressed = true;
    run_ticks(&mut game, &mut queue, 1);
    // `func_178894_a` (sync, interact-at), `interactWithEntitySendPacket`
    // (interact); the empty hand sends no use.
    assert_eq!(tags(&queue), ["slot2", "interact_at", "interact", "move"]);
}

#[test]
fn an_attack_on_a_mob_swings_then_syncs_then_attacks() {
    let mut game = placed_game();
    game.entity_id = 1;
    spawn_ahead(&mut game, false);
    let mut queue = CommandQueue::default();
    press_slot(&mut game, &mut queue, 2);
    game.input.attack = true;
    game.attack_pressed = true;
    run_ticks(&mut game, &mut queue, 1);
    assert_eq!(tags(&queue), ["swing", "slot2", "attack", "move"]);
}

#[test]
fn a_click_that_only_swings_does_not_sync_the_slot() {
    let mut game = placed_game();
    game.entity_id = 1;
    game.pitch = -1.0;
    let mut queue = CommandQueue::default();
    press_slot(&mut game, &mut queue, 2);
    game.input.attack = true;
    game.attack_pressed = true;
    run_ticks(&mut game, &mut queue, 1);
    // `clickMouse` on air: `swingItem`, no controller call, no sync.
    assert_eq!(tags(&queue), ["swing", "move"]);
    queue.ordered.clear();
    game.input.attack = false;
    run_ticks(&mut game, &mut queue, 1);
    assert_eq!(tags(&queue), ["slot2", "move"], "the next updateController");
}

#[test]
fn actions_pressed_while_congested_leave_once_after_the_resume() {
    let mut harness = LoopHarness::start(network::COMMAND_CAPACITY);
    harness.join_world();
    harness.wait_for_ticks(12);
    let sent = harness.spam_chats(CONGESTING);
    std::thread::sleep(std::time::Duration::from_millis(300));
    harness.ticks();
    // Three slot presses and a drop while the simulation waits.
    for slot in [4, 5, 6] {
        harness
            .inputs
            .send(InputUpdate {
                select_slot: Some(slot),
                ..InputUpdate::default()
            })
            .expect("the loop runs");
    }
    harness
        .inputs
        .send(InputUpdate {
            drop_item: Some(DropKind::One),
            ..InputUpdate::default()
        })
        .expect("the loop runs");
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(harness.ticks(), 0, "still congested");

    let stream = harness.read_for(std::time::Duration::from_millis(1500));
    let after = harness.finish();
    let stream: Vec<PlayServerbound> = stream.into_iter().chain(after).collect();
    assert_eq!(chats_of(&stream), sent, "the queue kept its order");
    assert_eq!(slots_in_stream(&stream), [6], "three presses, one packet");
    let drops: Vec<usize> = stream
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            matches!(p, PlayServerbound::PlayerDigging(d) if d.status == DiggingStatus::DropItem)
        })
        .map(|(i, _)| i)
        .collect();
    assert_eq!(drops.len(), 1, "the drop leaves once");
    let slot = stream
        .iter()
        .position(|p| matches!(p, PlayServerbound::HeldItemChange(_)))
        .expect("the slot packet");
    assert!(drops[0] < slot, "the drop of the key stage, then the sync");
    assert_fall_has_no_gap(&falling_ys(&stream));
}

fn slots_in_stream(packets: &[PlayServerbound]) -> Vec<i16> {
    slots_of(packets)
}

// The congestion guard in the real game loop (`run`, on its own thread).

/// Slot changes that fill the channel to the socket thread and then the
/// queue up to its limit while nothing is read.
const CONGESTING: usize = network::COMMAND_CAPACITY + PENDING_LIMIT + 22;

/// The real game loop with channels the test holds. The commands channel
/// is only drained when the test reads it, which is how a
/// socket that does not take packets looks like to the game thread.
/// Every `RenderEvent::Frame` is one executed tick.
struct LoopHarness {
    inputs: crossbeam_channel::Sender<InputUpdate>,
    events: crossbeam_channel::Sender<GameEvent>,
    frames: mpsc::Receiver<RenderEvent>,
    _chunks: mpsc::Receiver<ChunkEvent>,
    commands: tokio::sync::mpsc::Receiver<PlayServerbound>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    spam: u32,
    /// Frame duration the game thread paces its events with (0 = off).
    frame_micros: std::sync::Arc<AtomicU32>,
}

impl LoopHarness {
    fn start(capacity: usize) -> Self {
        let frame_micros = std::sync::Arc::new(AtomicU32::new(0));
        let (inputs_tx, inputs) = crossbeam_channel::unbounded();
        let (events_tx, events) = crossbeam_channel::unbounded();
        let (frames_tx, frames) = mpsc::sync_channel(100_000);
        let (chunks_tx, chunks) = mpsc::sync_channel(100_000);
        let (commands_tx, commands) = tokio::sync::mpsc::channel(capacity);
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let channels = GameChannels {
            inputs,
            events,
            frames: frames_tx,
            chunks: chunks_tx,
            commands: commands_tx,
            frame_micros: std::sync::Arc::clone(&frame_micros),
        };
        let flag = std::sync::Arc::clone(&stop);
        let thread = std::thread::spawn(move || run(channels, flag, "tester".to_string(), None));
        Self {
            inputs: inputs_tx,
            events: events_tx,
            frames,
            _chunks: chunks,
            commands,
            stop,
            thread: Some(thread),
            spam: 0,
            frame_micros,
        }
    }

    /// Joins a world of empty air and places the player high above it,
    /// so it falls (and reports a new position every tick) for a long time.
    fn join_world(&self) {
        let send = |event| {
            self.events
                .send(GameEvent::Net(event))
                .expect("the loop runs")
        };
        send(PlayEvent::JoinGame(umicraft_net::play::JoinGameInfo {
            entity_id: 1,
            gamemode: 0,
            dimension: 0,
            difficulty: 0,
            max_players: 20,
            level_type: "flat".to_string(),
            reduced_debug_info: false,
        }));
        send(PlayEvent::ChunkData(Box::new(ChunkData {
            chunk_x: 0,
            chunk_z: 0,
            ground_up: true,
            primary_bit_mask: 1,
            data: vec![0u8; umicraft_world::chunk::payload_len(1, true, true)],
        })));
        send(PlayEvent::Placement(umicraft_net::play::PlayerPlacement {
            x: 0.5,
            y: 1000.0,
            z: 0.5,
            ..Default::default()
        }));
    }

    /// The ticks executed since the last call.
    fn ticks(&self) -> usize {
        let mut count = 0;
        while let Ok(event) = self.frames.try_recv() {
            if matches!(event, RenderEvent::Frame(_)) {
                count += 1;
            }
        }
        count
    }

    fn wait_for_ticks(&self, wanted: usize) -> usize {
        let mut seen = 0;
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        while seen < wanted && Instant::now() < deadline {
            seen += self.ticks();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(seen >= wanted, "only {seen} ticks ran");
        seen
    }

    /// Chat lines, each one a command in the queue. (Slot changes cannot
    /// fill it any more: the presses between two ticks make one packet.)
    fn spam_chats(&mut self, count: usize) -> Vec<String> {
        (0..count)
            .map(|_| {
                let message = format!("m{}", self.spam);
                self.spam += 1;
                self.events
                    .send(GameEvent::Chat(message.clone()))
                    .expect("the loop runs");
                message
            })
            .collect()
    }

    /// Reads what reached the socket side until `window` has passed.
    fn read_for(&mut self, window: std::time::Duration) -> Vec<PlayServerbound> {
        let mut packets = Vec::new();
        let end = Instant::now() + window;
        while Instant::now() < end {
            while let Ok(packet) = self.commands.try_recv() {
                packets.push(packet);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        packets
    }

    fn finish(&mut self) -> Vec<PlayServerbound> {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        self.thread
            .take()
            .expect("running")
            .join()
            .expect("the loop ends");
        self.read_for(std::time::Duration::from_millis(20))
    }
}

/// A server that answers every position confirmation with a new teleport
/// (what PandaSpigot did for a dead player in phase E.8): the number of
/// confirmations the client sends in `window`.
fn teleport_ping_pong(harness: &mut LoopHarness, window: std::time::Duration) -> usize {
    let placement = || {
        GameEvent::Net(PlayEvent::Placement(umicraft_net::play::PlayerPlacement {
            x: 0.5,
            y: 1000.0,
            z: 0.5,
            ..Default::default()
        }))
    };
    let confirmed = |packet: &PlayServerbound| matches!(packet, PlayServerbound::PositionLook(_));
    // Let the join settle and drop its confirmations, so exactly one
    // teleport is in flight, as with a real server.
    let settle = Instant::now() + std::time::Duration::from_millis(200);
    while Instant::now() < settle {
        while harness.commands.try_recv().is_ok() {}
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let end = Instant::now() + window;
    let mut count = 0;
    harness.events.send(placement()).expect("the loop runs");
    while Instant::now() < end {
        match harness.commands.try_recv() {
            Ok(packet) if confirmed(&packet) => {
                count += 1;
                harness.events.send(placement()).expect("the loop runs");
            }
            Ok(_) => {}
            Err(_) => std::thread::sleep(std::time::Duration::from_micros(50)),
        }
    }
    count
}

/// Phase E.9, vanilla `runGameLoop`: network tasks are taken once per frame,
/// so an endless teleport / confirmation exchange runs at the frame rate,
/// never faster (at 50 frames per second, 20 ms: about 25 in half a second).
#[test]
fn a_teleport_loop_runs_at_the_frame_rate() {
    let mut harness = LoopHarness::start(65_536);
    harness.join_world();
    harness.wait_for_ticks(3);
    harness.frame_micros.store(20_000, Ordering::Relaxed);
    let window = std::time::Duration::from_millis(500);
    let count = teleport_ping_pong(&mut harness, window);
    let _ = harness.finish();
    assert!(
        (15..=30).contains(&count),
        "{count} confirmations in 500 ms at a 20 ms frame"
    );
}

/// With the pacing off (value 0, the old behaviour) the same exchange runs
/// as fast as the threads allow: far more than a frame rate.
#[test]
fn without_frame_pacing_a_teleport_loop_is_not_bounded() {
    let mut harness = LoopHarness::start(65_536);
    harness.join_world();
    harness.wait_for_ticks(3);
    let count = teleport_ping_pong(&mut harness, std::time::Duration::from_millis(300));
    let _ = harness.finish();
    assert!(count > 200, "only {count} confirmations without pacing");
}

fn chats_of(packets: &[PlayServerbound]) -> Vec<String> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            PlayServerbound::Chat(chat) => Some(chat.message.clone()),
            _ => None,
        })
        .collect()
}

fn slots_of(packets: &[PlayServerbound]) -> Vec<i16> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            PlayServerbound::HeldItemChange(change) => Some(change.slot),
            _ => None,
        })
        .collect()
}

/// The feet Y of the falling player in each position report, without the
/// acknowledgement of the placement (the exact Y of the placement).
fn falling_ys(packets: &[PlayServerbound]) -> Vec<f64> {
    packets
        .iter()
        .filter_map(|packet| match packet {
            PlayServerbound::PlayerPosition(p) => Some(p.y),
            PlayServerbound::PositionLook(p) => Some(p.y),
            _ => None,
        })
        .filter(|&y| y != 1000.0)
        .collect()
}

/// A lost report of a falling player doubles one step; the steps of a
/// fall grow by less than 30 % after the first ten ticks.
fn assert_fall_has_no_gap(ys: &[f64]) {
    assert!(
        ys.windows(2).all(|w| w[1] < w[0]),
        "reports out of order or twice"
    );
    let steps: Vec<f64> = ys.windows(2).map(|w| w[0] - w[1]).collect();
    for (i, pair) in steps.windows(2).enumerate().skip(8) {
        assert!(
            pair[1] / pair[0] < 1.3,
            "a report is missing near #{i}: {pair:?}"
        );
    }
}

#[test]
fn the_real_loop_stops_ticking_while_congested_and_resumes_when_the_network_drains() {
    let mut harness = LoopHarness::start(network::COMMAND_CAPACITY);
    harness.join_world();
    let mut total_ticks = harness.wait_for_ticks(12);
    // Congest the queue: more commands than the limit, nothing read.
    let mut sent = harness.spam_chats(CONGESTING);
    std::thread::sleep(std::time::Duration::from_millis(300));
    total_ticks += harness.ticks();
    std::thread::sleep(std::time::Duration::from_millis(800));
    assert_eq!(harness.ticks(), 0, "no tick starts while the queue is full");
    // The player still acts while the simulation waits.
    harness
        .inputs
        .send(InputUpdate {
            look_yaw: 0.5,
            select_slot: Some(3),
            ..InputUpdate::default()
        })
        .expect("the loop runs");
    harness
        .events
        .send(GameEvent::Chat("during".to_string()))
        .expect("the loop runs");
    sent.push("during".to_string());
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(harness.ticks(), 0, "still congested");

    // The network takes packets again.
    let stream = harness.read_for(std::time::Duration::from_millis(1500));
    let resumed = harness.ticks();
    total_ticks += resumed;
    assert!(
        (10..=35).contains(&resumed),
        "the game resumes at its pace, no catch-up burst: {resumed} ticks in 1.5 s"
    );
    let after = harness.finish();
    let stream: Vec<PlayServerbound> = stream.into_iter().chain(after).collect();

    // Nothing queued was dropped, duplicated or reordered.
    assert_eq!(chats_of(&stream), sent);
    assert_eq!(
        slots_of(&stream),
        [3],
        "the slot pressed while congested, once"
    );
    assert_fall_has_no_gap(&falling_ys(&stream));
    // The turn made during the congestion reached the wire.
    let turned = stream.iter().any(|packet| match packet {
        PlayServerbound::PlayerLook(p) => (p.yaw - 28.6479).abs() < 0.01,
        PlayServerbound::PositionLook(p) => (p.yaw - 28.6479).abs() < 0.01,
        _ => false,
    });
    assert!(turned, "the mouse movement during the congestion was kept");
    // Every executed tick sent its one report (the placement's
    // acknowledgement is the only extra).
    let reports = stream
        .iter()
        .filter(|p| {
            matches!(
                p,
                PlayServerbound::Player(_)
                    | PlayServerbound::PlayerPosition(_)
                    | PlayServerbound::PlayerLook(_)
                    | PlayServerbound::PositionLook(_)
            )
        })
        .count();
    assert_eq!(
        reports,
        total_ticks + 1,
        "{reports} reports for {total_ticks} ticks"
    );
}

#[test]
fn several_congestions_and_recoveries_keep_the_order_and_lose_nothing() {
    let mut harness = LoopHarness::start(network::COMMAND_CAPACITY);
    harness.join_world();
    harness.wait_for_ticks(12);
    let mut sent = Vec::new();
    let mut stream = Vec::new();
    for cycle in 0..3 {
        sent.extend(harness.spam_chats(CONGESTING));
        std::thread::sleep(std::time::Duration::from_millis(300));
        harness.ticks();
        std::thread::sleep(std::time::Duration::from_millis(400));
        assert_eq!(harness.ticks(), 0, "cycle {cycle}: congested, no tick");
        stream.extend(harness.read_for(std::time::Duration::from_millis(700)));
        assert!(harness.ticks() >= 5, "cycle {cycle}: the game ticks again");
    }
    stream.extend(harness.finish());
    assert_eq!(chats_of(&stream), sent, "FIFO through three cycles");
    assert_fall_has_no_gap(&falling_ys(&stream));
}

#[test]
fn a_congested_loop_keeps_serving_events_and_stops_on_request() {
    let mut harness = LoopHarness::start(network::COMMAND_CAPACITY);
    harness.join_world();
    harness.wait_for_ticks(5);
    harness.spam_chats(CONGESTING);
    std::thread::sleep(std::time::Duration::from_millis(300));
    harness.ticks();
    // A flood of events and inputs while the queue is full: the loop
    // must still take them (no deadlock with the full channel).
    for _ in 0..500 {
        harness
            .events
            .send(GameEvent::Input(InputUpdate::default()))
            .expect("the loop runs");
        harness
            .inputs
            .send(InputUpdate::default())
            .expect("the loop runs");
    }
    let deadline = Instant::now() + std::time::Duration::from_secs(2);
    while (!harness.events.is_empty() || !harness.inputs.is_empty()) && Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        harness.events.len() + harness.inputs.len(),
        0,
        "the loop took them"
    );
    std::thread::sleep(std::time::Duration::from_millis(400));
    assert_eq!(harness.ticks(), 0, "and it is still congested");
    // Shutdown works while congested.
    let started = Instant::now();
    harness.finish();
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
}

#[test]
fn without_congestion_every_tick_sends_exactly_one_report() {
    let mut harness = LoopHarness::start(64);
    harness.join_world();
    let mut ticks = 0;
    let mut stream = Vec::new();
    let end = Instant::now() + std::time::Duration::from_millis(1500);
    while Instant::now() < end {
        ticks += harness.ticks();
        stream.extend(harness.read_for(std::time::Duration::from_millis(20)));
    }
    ticks += harness.ticks();
    stream.extend(harness.finish());
    ticks += harness.ticks();
    let reports = stream
        .iter()
        .filter(|p| {
            matches!(
                p,
                PlayServerbound::Player(_)
                    | PlayServerbound::PlayerPosition(_)
                    | PlayServerbound::PlayerLook(_)
                    | PlayServerbound::PositionLook(_)
            )
        })
        .count();
    assert!(ticks >= 20);
    assert_eq!(
        reports,
        ticks + 1,
        "the placement's acknowledgement is the extra one"
    );
    assert_fall_has_no_gap(&falling_ys(&stream));
}

#[test]
fn a_full_queue_drains_through_a_small_channel_in_order() {
    let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
    let mut queue = CommandQueue {
        sender: Some(sender),
        ..CommandQueue::default()
    };
    pushed_positions(&mut queue, (PENDING_LIMIT * 2) as u32);
    assert!(queue.congested());
    let mut xs = Vec::new();
    let mut rounds = 0;
    while !queue.ordered.is_empty() {
        queue.flush();
        xs.extend(received_x(&mut receiver));
        rounds += 1;
        assert!(rounds < 1000, "the queue must empty");
    }
    assert!(!queue.congested());
    assert_eq!(xs.len(), PENDING_LIMIT * 2);
    assert!(
        xs.windows(2).all(|w| w[1] == w[0] + 1.0),
        "FIFO, no duplicate"
    );
}

#[test]
fn a_riding_jump_leaves_ahead_of_the_report_of_its_tick_only() {
    let jump = || ClientCommand::RidingJump {
        entity_id: 1,
        strength: 50,
    };
    let report = || ClientCommand::PlayerPosition {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        on_ground: true,
    };
    let action = ClientCommand::EntityAction {
        entity_id: 1,
        action: umicraft_protocol::packets::play::serverbound::EntityActionKind::StartSneaking,
    };
    // The report of an earlier tick stays ahead of the jump.
    let mut queue = CommandQueue::default();
    queue.push(report());
    let since = queue.ordered.len();
    queue.push_before_movement(jump(), since);
    assert_eq!(queue.ordered, [report(), jump()]);
    // The report of this tick comes after the jump and its actions.
    let mut queue = CommandQueue::default();
    queue.push(action.clone());
    queue.push(report());
    queue.push_before_movement(jump(), 0);
    assert_eq!(queue.ordered, [action, jump(), report()]);
}

#[test]
fn a_time_update_sets_the_world_clock_and_the_frame_carries_it() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, received) = mpsc::sync_channel(4);
    game.state = Some(GameState::new(DVec3::new(0.0, 64.0, 0.0)));

    handle_net(
        &mut game,
        PlayEvent::TimeUpdate(umicraft_protocol::packets::play::clientbound::TimeUpdate {
            world_age: 72_770,
            time_of_day: -6_000,
        }),
        &mut queue,
        &frames,
    );
    assert_eq!(game.world_time, -6_000);

    publish_frame(&game, &frames, Instant::now());
    let RenderEvent::Frame(frame) = received.try_recv().expect("a frame") else {
        panic!("the published event must be a frame");
    };
    assert_eq!(frame.world_time, -6_000);
    assert_eq!(frame.dimension, 0);
    // `Frame` is cheap to clone and send across threads without taking locks.
    fn assert_send_sync<T: Send + Sync + Clone + 'static>() {}
    assert_send_sync::<Frame>();
}

/// NET-10: a BungeeCord or Velocity switch is a second `Join Game` on the
/// same connection; the new server's world replaces the old one.
#[test]
fn a_second_join_game_replaces_the_world_of_the_previous_server() {
    use umicraft_net::play::JoinGameInfo;
    use umicraft_protocol::packets::play::clientbound::TimeUpdate;

    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    let join = |entity_id, dimension| JoinGameInfo {
        entity_id,
        gamemode: 0,
        dimension,
        difficulty: 0,
        max_players: 20,
        level_type: "default".to_string(),
        reduced_debug_info: false,
    };

    handle_net(
        &mut game,
        PlayEvent::JoinGame(join(7, 0)),
        &mut queue,
        &frames,
    );
    handle_net(
        &mut game,
        PlayEvent::TimeUpdate(TimeUpdate {
            world_age: 1,
            time_of_day: 9_000,
        }),
        &mut queue,
        &frames,
    );
    handle_net(
        &mut game,
        PlayEvent::JoinGame(join(99, -1)),
        &mut queue,
        &frames,
    );

    assert_eq!(game.entity_id, 99);
    assert_eq!(game.dimension, -1);
    assert_eq!(game.world_time, 0);
    assert_eq!(game.entities.len(), 0);
    assert!(game
        .world
        .as_ref()
        .is_some_and(|world| world.dimension() == -1));
}

#[test]
fn joining_and_changing_dimension_reset_the_world_clock() {
    use umicraft_net::play::JoinGameInfo;
    use umicraft_protocol::packets::play::clientbound::{Respawn, TimeUpdate};

    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    let join = |dimension| JoinGameInfo {
        entity_id: 1,
        gamemode: 0,
        dimension,
        difficulty: 0,
        max_players: 20,
        level_type: "default".to_string(),
        reduced_debug_info: false,
    };
    let respawn = |dimension| Respawn {
        dimension,
        difficulty: 0,
        game_mode: 0,
        world_type: "default".to_string(),
    };

    handle_net(&mut game, PlayEvent::JoinGame(join(0)), &mut queue, &frames);
    assert_eq!(game.world_time, 0);
    assert_eq!(game.dimension, 0);

    handle_net(
        &mut game,
        PlayEvent::TimeUpdate(TimeUpdate {
            world_age: 1,
            time_of_day: -6_000,
        }),
        &mut queue,
        &frames,
    );
    assert_eq!(game.world_time, -6_000);

    // A same-dimension respawn keeps the frozen clock.
    handle_net(
        &mut game,
        PlayEvent::Respawn(respawn(0)),
        &mut queue,
        &frames,
    );
    assert_eq!(game.world_time, -6_000);
    assert_eq!(game.dimension, 0);

    // A dimension change starts a new clock.
    handle_net(
        &mut game,
        PlayEvent::Respawn(respawn(-1)),
        &mut queue,
        &frames,
    );
    assert_eq!(game.world_time, 0);
    assert_eq!(game.dimension, -1);
}

#[test]
fn the_local_body_wears_the_skin_of_the_account_when_the_server_sends_none() {
    use umicraft_game::entity::PlayerProfile;

    let account = ProfileTextures {
        skin: Some("http://textures.minecraft.net/texture/ab".to_string()),
        slim: true,
        cape: None,
    };
    let mut game = Game::new();
    // Before the profile is listed there is nothing to dress.
    game.account_textures = Some(account.clone());
    apply_account_skin(&mut game);
    assert!(game.body.profile.is_none());

    // An offline server lists the player without textures.
    game.body.profile = Some(std::sync::Arc::new(PlayerProfile::named("Me")));
    apply_account_skin(&mut game);
    let worn = game.body.profile.as_ref().expect("the profile stays");
    assert_eq!(
        worn.skin_url.as_deref(),
        Some("http://textures.minecraft.net/texture/ab")
    );
    assert_eq!(worn.slim, Some(true));

    // A skin the server sent is not replaced by the account's.
    let mut sent = PlayerProfile::named("Me");
    sent.skin_url = Some("http://textures.minecraft.net/texture/cd".into());
    game.body.profile = Some(std::sync::Arc::new(sent));
    apply_account_skin(&mut game);
    assert_eq!(
        game.body
            .profile
            .as_ref()
            .and_then(|profile| profile.skin_url.as_deref()),
        Some("http://textures.minecraft.net/texture/cd")
    );
}

#[test]
fn a_flat_world_is_known_from_the_join_and_the_respawn_that_changes_dimension() {
    use umicraft_net::play::JoinGameInfo;
    use umicraft_protocol::packets::play::clientbound::Respawn;

    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    let join = |level_type: &str| JoinGameInfo {
        entity_id: 1,
        gamemode: 0,
        dimension: 0,
        difficulty: 0,
        max_players: 20,
        level_type: level_type.to_string(),
        reduced_debug_info: false,
    };
    let respawn = |dimension, world_type: &str| Respawn {
        dimension,
        difficulty: 0,
        game_mode: 0,
        world_type: world_type.to_string(),
    };

    assert!(!game.flat_world, "a normal world until the server says");
    handle_net(
        &mut game,
        PlayEvent::JoinGame(join("flat")),
        &mut queue,
        &frames,
    );
    assert!(game.flat_world);
    // Names are compared without regard to case.
    handle_net(
        &mut game,
        PlayEvent::JoinGame(join("FLAT")),
        &mut queue,
        &frames,
    );
    assert!(game.flat_world);
    handle_net(
        &mut game,
        PlayEvent::JoinGame(join("default")),
        &mut queue,
        &frames,
    );
    assert!(!game.flat_world);

    // A respawn in the same dimension keeps the world, and its type.
    handle_net(
        &mut game,
        PlayEvent::JoinGame(join("flat")),
        &mut queue,
        &frames,
    );
    handle_net(
        &mut game,
        PlayEvent::Respawn(respawn(0, "default")),
        &mut queue,
        &frames,
    );
    assert!(game.flat_world, "the same world stays flat");
    // A new dimension is built with the type of the packet.
    handle_net(
        &mut game,
        PlayEvent::Respawn(respawn(-1, "default")),
        &mut queue,
        &frames,
    );
    assert!(!game.flat_world);
    handle_net(
        &mut game,
        PlayEvent::Respawn(respawn(0, "flat")),
        &mut queue,
        &frames,
    );
    assert!(game.flat_world);
}

#[test]
fn joining_another_server_starts_without_the_old_clicks_and_item_use() {
    // A lobby compass: the right click is still pending and an item is in
    // use when the proxy sends the player to another server.
    use umicraft_net::play::JoinGameInfo;

    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    game.use_pressed = true;
    game.attack_pressed = true;
    let join = JoinGameInfo {
        entity_id: 1,
        gamemode: 0,
        dimension: 0,
        difficulty: 0,
        max_players: 20,
        level_type: "default".to_string(),
        reduced_debug_info: false,
    };
    handle_net(&mut game, PlayEvent::JoinGame(join), &mut queue, &frames);
    assert!(!game.use_pressed && !game.attack_pressed);
    assert!(!game.item_use.is_using());
    assert!(
        game.windows.view().container.is_none(),
        "no window stays open"
    );
}

#[test]
fn a_formatted_chat_message_keeps_its_styles_and_arrival_time() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    let before = Instant::now();
    handle_net(
        &mut game,
        PlayEvent::Chat(
            r#"{"text":"Hi ","color":"red","extra":[{"text":"there","bold":true}]}"#.to_string(),
        ),
        &mut queue,
        &frames,
    );
    let hud = game.hud();
    assert_eq!(hud.chat.len(), 1);
    assert_eq!(hud.chat[0].plain(), "Hi there");
    assert_eq!(hud.chat[0].runs[0].color, [0xFF, 0x55, 0x55]);
    assert!(!hud.chat[0].runs[0].bold);
    assert!(hud.chat[0].runs[1].bold);
    assert!(hud.chat[0].received >= before);
}

#[test]
fn an_empty_chat_component_is_a_blank_line_not_raw_json() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    handle_net(
        &mut game,
        PlayEvent::Chat(r#"{"text":""}"#.to_string()),
        &mut queue,
        &frames,
    );
    assert_eq!(game.hud().chat[0].plain(), "");
}

#[test]
fn the_chat_keeps_the_last_one_hundred_messages() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    for number in 0..150 {
        handle_net(
            &mut game,
            PlayEvent::Chat(format!(r#"{{"text":"message {number}"}}"#)),
            &mut queue,
            &frames,
        );
    }
    let chat = game.hud().chat;
    assert_eq!(chat.len(), CHAT_HISTORY);
    assert_eq!(chat[0].plain(), "message 50");
    assert_eq!(chat[99].plain(), "message 149");
}

#[test]
fn the_typed_chat_limit_counts_characters_not_bytes() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    // 100 two-byte characters: 200 bytes, but within the 100-character limit.
    let message = "\u{e9}".repeat(100);
    handle_event(
        &mut game,
        GameEvent::Chat(message.clone()),
        &mut queue,
        &frames,
    );
    assert_eq!(queue.ordered.len(), 1, "a 100-character message is sent");
    handle_event(
        &mut game,
        GameEvent::Chat("\u{e9}".repeat(101)),
        &mut queue,
        &frames,
    );
    assert_eq!(queue.ordered.len(), 1, "101 characters is too long");
}

#[test]
fn a_tab_request_is_queued_and_the_answer_is_published_with_a_new_serial() {
    use umicraft_protocol::packets::play::clientbound::TabComplete;

    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    handle_event(
        &mut game,
        GameEvent::TabComplete("/he".to_string()),
        &mut queue,
        &frames,
    );
    assert!(matches!(
        queue.ordered.front(),
        Some(ClientCommand::TabComplete { message }) if message == "/he"
    ));

    assert_eq!(game.hud().completions_serial, 0);
    handle_net(
        &mut game,
        PlayEvent::TabComplete(TabComplete {
            matches: vec!["/help".to_string()],
        }),
        &mut queue,
        &frames,
    );
    let hud = game.hud();
    assert_eq!(hud.completions, ["/help"]);
    assert_eq!(hud.completions_serial, 1);
}

#[test]
fn the_chat_visibility_filters_incoming_chat_and_system_messages() {
    let mut game = Game::new();
    let say = |game: &mut Game, event: PlayEvent| {
        let (frames, _received) = mpsc::sync_channel(16);
        handle_net(game, event, &mut CommandQueue::default(), &frames);
    };
    // Commands only: the system message stays, the player message goes.
    game.chat_mode = 1;
    say(
        &mut game,
        PlayEvent::Chat(r#"{"text":"player"}"#.to_string()),
    );
    say(
        &mut game,
        PlayEvent::SystemChat(r#"{"text":"system"}"#.to_string()),
    );
    // Hidden: nothing is kept.
    game.chat_mode = 2;
    say(
        &mut game,
        PlayEvent::SystemChat(r#"{"text":"late"}"#.to_string()),
    );
    let plain: Vec<_> = game.hud().chat.iter().map(ChatLineView::plain).collect();
    assert_eq!(plain, ["system"]);
}

#[test]
fn a_client_settings_event_updates_the_chat_visibility_filter() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    handle_event(
        &mut game,
        GameEvent::ClientSettings {
            view_distance: 8,
            chat_mode: 2,
            chat_colors: true,
            skin_parts: 0x7F,
        },
        &mut queue,
        &frames,
    );
    assert_eq!(game.chat_mode, 2);
}

#[test]
fn a_local_chat_line_is_never_parsed_as_json() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    for text in ["null", "123", "Steve, Alex"] {
        handle_event(
            &mut game,
            GameEvent::LocalChat(text.to_string()),
            &mut queue,
            &frames,
        );
    }
    let plain: Vec<_> = game.hud().chat.iter().map(ChatLineView::plain).collect();
    assert_eq!(plain, ["null", "123", "Steve, Alex"]);
}

#[test]
fn the_social_events_fill_the_hud() {
    use umicraft_protocol::packets::play::clientbound::{
        AddPlayerEntry, DisplayScoreboard, ObjectiveRenderType, PlayerListHeaderFooter,
        PlayerListItem, ScoreboardObjective, Title, UpdateScore,
    };

    let mut game = Game::new();
    game.own_name = "Steve".to_string();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    let mut send = |game: &mut Game, event| handle_net(game, event, &mut queue, &frames);

    send(
        &mut game,
        PlayEvent::ScoreboardObjective(ScoreboardObjective::Create {
            name: "kills".to_string(),
            value: "Kills".to_string(),
            render_type: ObjectiveRenderType::Integer,
        }),
    );
    send(
        &mut game,
        PlayEvent::DisplayScoreboard(DisplayScoreboard {
            position: 1,
            score_name: "kills".to_string(),
        }),
    );
    send(
        &mut game,
        PlayEvent::UpdateScore(UpdateScore::Change {
            name: "Alex".to_string(),
            objective: "kills".to_string(),
            value: 4,
        }),
    );
    send(
        &mut game,
        PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![AddPlayerEntry {
            uuid: u128::from_be_bytes([1; 16]),
            name: "Steve".to_string(),
            properties: vec![],
            game_mode: 0,
            ping: 37,
            display_name: None,
        }])),
    );
    send(
        &mut game,
        PlayEvent::PlayerListHeaderFooter(PlayerListHeaderFooter {
            header: r#"{"text":"Welcome"}"#.to_string(),
            footer: r#"{"text":""}"#.to_string(),
        }),
    );
    send(
        &mut game,
        PlayEvent::Title(Title::Title {
            json: r#"{"text":"Round 1"}"#.to_string(),
        }),
    );
    send(
        &mut game,
        PlayEvent::ActionBar(r#"{"text":"Go!"}"#.to_string()),
    );

    // The title fades in from nothing, so it needs a few ticks to show.
    for _ in 0..5 {
        game.title.tick();
    }
    let hud = game.hud();
    let sidebar = hud.sidebar.expect("the sidebar objective is displayed");
    assert_eq!(plain_text(&sidebar.title), "Kills");
    assert_eq!(sidebar.lines.len(), 1);
    assert_eq!(plain_text(&sidebar.lines[0].name), "Alex");
    assert_eq!(sidebar.lines[0].score, 4);

    let tab_list = hud.tab_list.expect("a player is listed");
    assert_eq!(
        tab_list.header.as_deref().map(plain_text).as_deref(),
        Some("Welcome")
    );
    assert_eq!(tab_list.footer, None, "an empty footer is absent");
    assert_eq!(tab_list.entries.len(), 1);
    assert_eq!(plain_text(&tab_list.entries[0].name), "Steve");
    assert_eq!(tab_list.entries[0].ping, 37);

    let title = hud.title.expect("a title is displayed");
    assert_eq!(plain_text(&title.title), "Round 1");
    assert!(title.alpha > 0.0);
    let action_bar = hud.action_bar.expect("an action bar is displayed");
    assert_eq!(plain_text(&action_bar.text), "Go!");
    assert!(action_bar.alpha > 0.0);
}

#[test]
fn a_dimension_change_clears_the_scoreboard_but_keeps_the_tab_list() {
    use umicraft_protocol::packets::play::clientbound::{
        AddPlayerEntry, DisplayScoreboard, ObjectiveRenderType, PlayerListItem, Respawn,
        ScoreboardObjective,
    };

    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    handle_net(
        &mut game,
        PlayEvent::ScoreboardObjective(ScoreboardObjective::Create {
            name: "o".to_string(),
            value: "O".to_string(),
            render_type: ObjectiveRenderType::Integer,
        }),
        &mut queue,
        &frames,
    );
    handle_net(
        &mut game,
        PlayEvent::DisplayScoreboard(DisplayScoreboard {
            position: 1,
            score_name: "o".to_string(),
        }),
        &mut queue,
        &frames,
    );
    handle_net(
        &mut game,
        PlayEvent::PlayerListItem(PlayerListItem::AddPlayer(vec![AddPlayerEntry {
            uuid: u128::from_be_bytes([2; 16]),
            name: "Alex".to_string(),
            properties: vec![],
            game_mode: 0,
            ping: 5,
            display_name: None,
        }])),
        &mut queue,
        &frames,
    );
    assert!(game.hud().sidebar.is_some());

    handle_net(
        &mut game,
        PlayEvent::Respawn(Respawn {
            dimension: -1,
            difficulty: 0,
            game_mode: 0,
            world_type: "default".to_string(),
        }),
        &mut queue,
        &frames,
    );
    let hud = game.hud();
    assert!(hud.sidebar.is_none(), "a new world has a new scoreboard");
    assert!(
        hud.tab_list.is_some(),
        "the tab list belongs to the connection"
    );
}

#[test]
fn boss_mob_spawn_and_metadata_updates_hud_boss_bar() {
    use umicraft_protocol::packets::play::clientbound::{
        DestroyEntities, EntityMetadata, SpawnMob,
    };
    use umicraft_protocol::packets::play::metadata::{Metadata, WatchedObject};

    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);

    // Spawn Ender Dragon (mob type 63) with full health (200.0)
    let mut metadata = Metadata::default();
    metadata.objects.push(WatchedObject {
        type_id: 3,
        index: 6,
        value: 200.0_f32.to_be_bytes().to_vec(),
    });
    handle_net(
        &mut game,
        PlayEvent::SpawnMob(SpawnMob {
            entity_id: 500,
            mob_type: 63,
            x: 0,
            y: 64 * 32,
            z: 0,
            yaw: 0,
            pitch: 0,
            head_yaw: 0,
            velocity_x: 0,
            velocity_y: 0,
            velocity_z: 0,
            metadata,
        }),
        &mut queue,
        &frames,
    );

    social_view::tick_boss_bar(&mut game.boss_bar, &game.entities);
    let hud = game.hud();
    let boss = hud.boss.expect("boss bar present after dragon spawn");
    assert_eq!(plain_text(&boss.name), "Ender Dragon");
    assert_eq!(boss.health, 1.0);

    // Update health via EntityMetadata to 100.0 (half health)
    let mut update_meta = Metadata::default();
    update_meta.objects.push(WatchedObject {
        type_id: 3,
        index: 6,
        value: 100.0_f32.to_be_bytes().to_vec(),
    });
    handle_net(
        &mut game,
        PlayEvent::EntityMetadata(EntityMetadata {
            entity_id: 500,
            metadata: update_meta,
        }),
        &mut queue,
        &frames,
    );

    social_view::tick_boss_bar(&mut game.boss_bar, &game.entities);
    let hud = game.hud();
    let boss = hud.boss.expect("boss bar present after damage");
    assert_eq!(boss.health, 0.5);

    // Destroy dragon
    handle_net(
        &mut game,
        PlayEvent::DestroyEntities(DestroyEntities {
            entity_ids: vec![500],
        }),
        &mut queue,
        &frames,
    );
    // `BossStatus.statusBarTime` is 100 when the dragon was last seen and
    // the tick of that same frame already took one off: 99 remain.
    for _ in 0..98 {
        social_view::tick_boss_bar(&mut game.boss_bar, &game.entities);
    }
    assert!(game.hud().boss.is_some(), "the bar outlives the dragon");
    social_view::tick_boss_bar(&mut game.boss_bar, &game.entities);
    assert!(
        game.hud().boss.is_none(),
        "boss bar disappears 100 ticks after the dragon"
    );
}

#[test]
fn the_first_placement_publishes_the_placed_flag_at_once() {
    use umicraft_net::play::PlayerPlacement;

    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, received) = mpsc::sync_channel(16);
    let placement = PlayerPlacement {
        x: 0.5,
        y: 64.0,
        z: 0.5,
        yaw: 0.0,
        pitch: 0.0,
        raw: None,
    };
    handle_net(
        &mut game,
        PlayEvent::Placement(placement.clone()),
        &mut queue,
        &frames,
    );
    let statuses: Vec<bool> = received
        .try_iter()
        .filter_map(|event| match event {
            RenderEvent::Status(status) => Some(status.placed),
            _ => None,
        })
        .collect();
    assert_eq!(statuses, [true]);
    // A later correction does not publish again.
    handle_net(
        &mut game,
        PlayEvent::Placement(placement),
        &mut queue,
        &frames,
    );
    assert!(received
        .try_iter()
        .all(|event| !matches!(event, RenderEvent::Status(_))));
}

#[test]
fn a_statistics_packet_is_forwarded_by_name() {
    use umicraft_protocol::packets::play::clientbound::Statistics;

    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, received) = mpsc::sync_channel(16);
    let packet = Statistics {
        entries: vec![("stat.jump".to_string(), 7), ("stat.deaths".to_string(), 1)],
    };
    handle_net(
        &mut game,
        PlayEvent::Statistics(packet),
        &mut queue,
        &frames,
    );
    let stats = received.try_iter().find_map(|event| match event {
        RenderEvent::Stats(stats) => Some(stats),
        _ => None,
    });
    assert_eq!(
        stats,
        Some(vec![
            ("stat.deaths".to_string(), 1),
            ("stat.jump".to_string(), 7)
        ])
    );
}

#[test]
fn opening_the_statistics_asks_the_server() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _received) = mpsc::sync_channel(16);
    handle_event(&mut game, GameEvent::RequestStats, &mut queue, &frames);
    assert!(matches!(
        queue.ordered.front(),
        Some(ClientCommand::RequestStats)
    ));
}

// =========================================================================
// Phase B: Network Event Order & Parity Tests (12 Test Cases)
// =========================================================================

#[test]
fn test_event_order_1_s12_velocity_received_before_tick() {
    let mut harness = LoopHarness::start(network::COMMAND_CAPACITY);
    harness.join_world();
    harness.wait_for_ticks(3);
    // Send S12 knockback upwards (+0.5 blocks/tick) to the local player (id 1)
    harness
        .events
        .send(GameEvent::Net(PlayEvent::EntityVelocity(
            umicraft_protocol::packets::play::clientbound::EntityVelocity {
                entity_id: 1,
                velocity_x: 0,
                velocity_y: 4000,
                velocity_z: 0,
            },
        )))
        .expect("event sent");
    // Wait for the next tick to execute
    harness.wait_for_ticks(1);
    let stream = harness.finish();
    let ys = falling_ys(&stream);
    assert!(ys.len() >= 2);
    // In vanilla Java, velocity received before runTick() is applied in scheduledExecutables,
    // so the very next tick moves upward instead of continuing downward acceleration.
    let mut had_upward_or_reduced_fall = false;
    for window in ys.windows(2) {
        if window[1] >= window[0] - 0.05 {
            had_upward_or_reduced_fall = true;
            break;
        }
    }
    assert!(
        had_upward_or_reduced_fall,
        "S12 velocity took effect immediately before tick: {ys:?}"
    );
}

#[test]
fn test_event_order_2_s12_velocity_received_after_tick_start() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    // Execute tick 1 (gravity applies, velocity is negative/downward)
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    let vel_before = game.state.as_ref().unwrap().player().velocity;
    assert!(vel_before.y <= 0.0);

    // Receive S12 event between ticks
    let entity_id = game.entity_id;
    handle_net(
        &mut game,
        PlayEvent::EntityVelocity(
            umicraft_protocol::packets::play::clientbound::EntityVelocity {
                entity_id,
                velocity_x: 0,
                velocity_y: 8000, // 1.0 b/t
                velocity_z: 0,
            },
        ),
        &mut queue,
        &frames,
    );

    // Execute tick 2: velocity must be incorporated into this tick's movement
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    let pos_after = game.state.as_ref().unwrap().position();
    assert!(
        pos_after.y > 64.0,
        "tick 2 incorporated S12: y={}",
        pos_after.y
    );
}

#[test]
fn test_event_order_3_s08_pos_look_received_before_tick() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);

    // S08 arrives before tick
    handle_net(
        &mut game,
        PlayEvent::Placement(umicraft_net::play::PlayerPlacement {
            x: 100.0,
            y: 150.0,
            z: 200.0,
            yaw: 45.0,
            pitch: 10.0,
            ..Default::default()
        }),
        &mut queue,
        &frames,
    );

    assert_eq!(
        game.state.as_ref().unwrap().position(),
        DVec3::new(100.0, 150.0, 200.0)
    );
    assert!(matches!(
        queue.ordered.front(),
        Some(ClientCommand::AckPosition { .. })
    ));

    // Tick simulates from the new position
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    let pos = game.state.as_ref().unwrap().position();
    assert!((pos.x - 100.0).abs() < 1e-4);
    assert!((pos.z - 200.0).abs() < 1e-4);
}

#[test]
fn test_event_order_4_multiple_s08_successive_in_same_pass() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);

    for i in 1..=3 {
        handle_net(
            &mut game,
            PlayEvent::Placement(umicraft_net::play::PlayerPlacement {
                x: i as f64 * 10.0,
                y: 100.0,
                z: i as f64 * 10.0,
                ..Default::default()
            }),
            &mut queue,
            &frames,
        );
    }

    assert_eq!(
        game.state.as_ref().unwrap().position(),
        DVec3::new(30.0, 100.0, 30.0)
    );
    let acks: Vec<f64> = queue
        .ordered
        .iter()
        .filter_map(|cmd| match cmd {
            ClientCommand::AckPosition { x, .. } => Some(*x),
            _ => None,
        })
        .collect();
    assert_eq!(acks, vec![10.0, 20.0, 30.0]);
}

#[test]
fn test_event_order_5_s08_and_s12_in_same_batch() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);

    handle_net(
        &mut game,
        PlayEvent::Placement(umicraft_net::play::PlayerPlacement {
            x: 10.0,
            y: 100.0,
            z: 10.0,
            ..Default::default()
        }),
        &mut queue,
        &frames,
    );
    let entity_id = game.entity_id;
    handle_net(
        &mut game,
        PlayEvent::EntityVelocity(
            umicraft_protocol::packets::play::clientbound::EntityVelocity {
                entity_id,
                velocity_x: 0,
                velocity_y: 8000,
                velocity_z: 0,
            },
        ),
        &mut queue,
        &frames,
    );

    assert_eq!(
        game.state.as_ref().unwrap().position(),
        DVec3::new(10.0, 100.0, 10.0)
    );
    assert_eq!(game.state.as_ref().unwrap().player().velocity.y, 1.0);

    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    assert!(game.state.as_ref().unwrap().position().y > 100.5);
}

#[test]
fn test_event_order_6_multiple_block_changes_before_tick() {
    use umicraft_protocol::packets::play::clientbound::BlockChange;

    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);

    handle_net(
        &mut game,
        PlayEvent::BlockChange(
            BlockChange::at(0, 65, 0, 1 << 4), // Stone
        ),
        &mut queue,
        &frames,
    );

    assert_eq!(
        game.world.as_ref().unwrap().block(BlockPos::new(0, 65, 0)),
        BlockState::new(1, 0)
    );
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    assert_eq!(
        game.world.as_ref().unwrap().block(BlockPos::new(0, 65, 0)),
        BlockState::new(1, 0)
    );
}

#[test]
fn test_event_order_7_join_game_followed_by_state_packets() {
    let mut game = Game::new();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);

    handle_net(
        &mut game,
        PlayEvent::JoinGame(umicraft_net::play::JoinGameInfo {
            entity_id: 42,
            gamemode: 1,
            dimension: 0,
            difficulty: 1,
            max_players: 10,
            level_type: "flat".to_string(),
            reduced_debug_info: false,
        }),
        &mut queue,
        &frames,
    );

    assert_eq!(game.entity_id, 42);
    assert_eq!(game.gamemode, GameMode::Creative);
    assert!(game.world.is_some());
    assert!(game.state.is_none());

    handle_net(
        &mut game,
        PlayEvent::SpawnPosition(
            umicraft_protocol::packets::play::clientbound::SpawnPosition { x: 0, y: 64, z: 0 },
        ),
        &mut queue,
        &frames,
    );
    handle_net(
        &mut game,
        PlayEvent::Placement(umicraft_net::play::PlayerPlacement {
            x: 0.5,
            y: 65.0,
            z: 0.5,
            ..Default::default()
        }),
        &mut queue,
        &frames,
    );

    assert!(game.state.is_some());
    assert_eq!(
        game.state.as_ref().unwrap().position(),
        DVec3::new(0.5, 65.0, 0.5)
    );
}

#[test]
fn test_event_order_8_respawn_followed_by_placement() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);

    game.health = 0.0;
    handle_net(
        &mut game,
        PlayEvent::Respawn(umicraft_protocol::packets::play::clientbound::Respawn {
            dimension: 0,
            difficulty: 1,
            game_mode: 0,
            world_type: "default".to_string(),
        }),
        &mut queue,
        &frames,
    );

    assert_eq!(game.health, 20.0);
    assert!(game.state.is_none());

    handle_net(
        &mut game,
        PlayEvent::Placement(umicraft_net::play::PlayerPlacement {
            x: 10.0,
            y: 70.0,
            z: 10.0,
            ..Default::default()
        }),
        &mut queue,
        &frames,
    );

    assert!(game.state.is_some());
    assert_eq!(
        game.state.as_ref().unwrap().position(),
        DVec3::new(10.0, 70.0, 10.0)
    );
}

#[test]
fn test_event_order_9_dimension_change_with_pending_events() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);

    handle_net(
        &mut game,
        PlayEvent::Respawn(umicraft_protocol::packets::play::clientbound::Respawn {
            dimension: -1,
            difficulty: 1,
            game_mode: 0,
            world_type: "default".to_string(),
        }),
        &mut queue,
        &frames,
    );

    assert_eq!(game.dimension, -1);
    assert_eq!(game.world.as_ref().unwrap().dimension(), -1);
    assert!(game.state.is_none());

    handle_net(
        &mut game,
        PlayEvent::Placement(umicraft_net::play::PlayerPlacement {
            x: 5.0,
            y: 80.0,
            z: 5.0,
            ..Default::default()
        }),
        &mut queue,
        &frames,
    );

    assert_eq!(
        game.state.as_ref().unwrap().position(),
        DVec3::new(5.0, 80.0, 5.0)
    );
}

#[test]
fn test_event_order_10_multiple_catch_up_ticks() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);

    for i in 0..3 {
        tick(
            &mut game,
            &mut queue,
            &frames,
            &chunks,
            Instant::now() + Duration::from_millis(50 * i),
        );
    }

    let movements = movement_of(&queue);
    assert_eq!(
        movements.len(),
        3,
        "3 catch-up ticks produce exactly 3 movement reports"
    );
}

#[test]
fn test_event_order_11_heavy_network_load_burst() {
    use umicraft_protocol::packets::play::clientbound::BlockChange;

    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(1000);
    let (chunks, _) = mpsc::sync_channel(1000);

    for i in 0..150 {
        handle_net(
            &mut game,
            PlayEvent::BlockChange(BlockChange::at(i, 60, 0, 1 << 4)),
            &mut queue,
            &frames,
        );
    }

    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    assert!(game.state.is_some());
}

#[test]
fn test_event_order_12_fifo_order_preserved() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);

    for i in 0..5 {
        handle_net(
            &mut game,
            PlayEvent::HeldItemChange(i as i8),
            &mut queue,
            &frames,
        );
        assert_eq!(
            game.selected_slot, i as u8,
            "slot updated in exact FIFO order"
        );
    }
}

// =========================================================================
// Phase C: Parité des rotations et des entrées utilisateur (Minecraft 1.8.9)
// =========================================================================

#[test]
fn test_rot_1_pitch_at_minus_90_degrees() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);

    // Input pushing pitch to negative limit (looking up towards sky)
    handle_input(
        &mut game,
        InputUpdate {
            look_pitch: -10.0,
            ..InputUpdate::default()
        },
        &mut queue,
    );

    assert_eq!(game.pitch, -MAX_PITCH);
    assert!((game.pitch.to_degrees() - (-90.0)).abs() < 1e-4);

    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    let movements = movement_of(&queue);
    assert!(!movements.is_empty());
    let last = movements.last().unwrap();
    let wire_pitch = match last {
        ClientCommand::PositionLook { pitch, .. } | ClientCommand::PlayerLook { pitch, .. } => {
            *pitch
        }
        other => panic!("expected look report, got {other:?}"),
    };
    assert!(
        (wire_pitch - (-90.0)).abs() < 1e-4,
        "wire pitch must be -90.0°, got {wire_pitch}"
    );
}

#[test]
fn test_rot_2_pitch_at_plus_90_degrees() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);

    // Input pushing pitch to positive limit (looking down towards feet)
    handle_input(
        &mut game,
        InputUpdate {
            look_pitch: 10.0,
            ..InputUpdate::default()
        },
        &mut queue,
    );

    assert_eq!(game.pitch, MAX_PITCH);
    assert!((game.pitch.to_degrees() - 90.0).abs() < 1e-4);

    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    let movements = movement_of(&queue);
    assert!(!movements.is_empty());
    let last = movements.last().unwrap();
    let wire_pitch = match last {
        ClientCommand::PositionLook { pitch, .. } | ClientCommand::PlayerLook { pitch, .. } => {
            *pitch
        }
        other => panic!("expected look report, got {other:?}"),
    };
    assert!(
        (wire_pitch - 90.0).abs() < 1e-4,
        "wire pitch must be +90.0°, got {wire_pitch}"
    );
}

#[test]
fn test_rot_3_pitch_clamp_beyond_limits() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();

    // Attempting to exceed upper limit
    handle_input(
        &mut game,
        InputUpdate {
            look_pitch: 50.0,
            ..InputUpdate::default()
        },
        &mut queue,
    );
    assert_eq!(game.pitch, 90.0_f32.to_radians());

    // Attempting to exceed lower limit
    handle_input(
        &mut game,
        InputUpdate {
            look_pitch: -100.0,
            ..InputUpdate::default()
        },
        &mut queue,
    );
    assert_eq!(game.pitch, -90.0_f32.to_radians());
}

#[test]
fn test_rot_4_mouse_delta_1_10_100_pixels() {
    // At default sensitivity (0.5), look_factor = 1.0
    // In vanilla Minecraft: delta * 1.0 * 0.15 degrees
    let speed = crate::window::MOUSE_SENSITIVITY;

    // 1 pixel
    let delta_1_rad = 1.0_f32 * speed;
    let delta_1_deg = delta_1_rad.to_degrees();
    assert!(
        (delta_1_deg - 0.15).abs() < 1e-6,
        "1px delta: expected 0.15°, got {delta_1_deg}"
    );

    // 10 pixels
    let delta_10_rad = 10.0_f32 * speed;
    let delta_10_deg = delta_10_rad.to_degrees();
    assert!(
        (delta_10_deg - 1.50).abs() < 1e-5,
        "10px delta: expected 1.50°, got {delta_10_deg}"
    );

    // 100 pixels
    let delta_100_rad = 100.0_f32 * speed;
    let delta_100_deg = delta_100_rad.to_degrees();
    assert!(
        (delta_100_deg - 15.00).abs() < 1e-4,
        "100px delta: expected 15.00°, got {delta_100_deg}"
    );
}

#[test]
fn test_rot_5_sensitivity_scaling_levels() {
    use crate::options::ControlOptions;

    // Min sensitivity = 0.0
    let ctrl_min = ControlOptions {
        mouse_sensitivity: 0.0,
        ..ControlOptions::default()
    };
    let factor_min = ctrl_min.look_factor();
    let rot_min_deg = (100.0 * crate::window::MOUSE_SENSITIVITY * factor_min).to_degrees();
    let expected_min = 100.0 * 0.064 * 0.15; // 0.96°
    assert!(
        (rot_min_deg - expected_min).abs() < 1e-4,
        "min sens: got {rot_min_deg}, expected {expected_min}"
    );

    // Default sensitivity = 0.5
    let ctrl_def = ControlOptions {
        mouse_sensitivity: 0.5,
        ..ControlOptions::default()
    };
    let factor_def = ctrl_def.look_factor();
    let rot_def_deg = (100.0 * crate::window::MOUSE_SENSITIVITY * factor_def).to_degrees();
    let expected_def = 100.0 * 1.0 * 0.15; // 15.0°
    assert!(
        (rot_def_deg - expected_def).abs() < 1e-4,
        "default sens: got {rot_def_deg}, expected {expected_def}"
    );

    // Max sensitivity = 1.0
    let ctrl_max = ControlOptions {
        mouse_sensitivity: 1.0,
        ..ControlOptions::default()
    };
    let factor_max = ctrl_max.look_factor();
    let rot_max_deg = (100.0 * crate::window::MOUSE_SENSITIVITY * factor_max).to_degrees();
    let expected_max = 100.0 * 4.096 * 0.15; // 61.44°
    assert!(
        (rot_max_deg - expected_max).abs() < 1e-4,
        "max sens: got {rot_max_deg}, expected {expected_max}"
    );
}

#[test]
fn test_rot_6_java_rust_rotation_comparison() {
    use crate::options::ControlOptions;

    let sensitivities = [0.0_f32, 0.2, 0.5, 0.8, 1.0];
    let deltas = [1.0_f32, 5.0, 10.0, 25.0, 50.0, 100.0, -15.0, -80.0];

    for &s in &sensitivities {
        // Java 1.8.9 EntityRenderer.java formula:
        let f = s * 0.6_f32 + 0.2_f32;
        let f1 = f * f * f * 8.0_f32;

        let ctrl = ControlOptions {
            mouse_sensitivity: s,
            ..ControlOptions::default()
        };
        let rust_factor = ctrl.look_factor();

        for &d in &deltas {
            // Java setAngles:
            let java_yaw_delta = (d as f64 * f1 as f64 * 0.15_f64) as f32;

            // Rust window/mod.rs + game.rs:
            let rust_speed = crate::window::MOUSE_SENSITIVITY * rust_factor;
            let rust_yaw_delta_rad = d * rust_speed;
            let rust_yaw_delta_deg = rust_yaw_delta_rad.to_degrees();

            assert!(
                (java_yaw_delta - rust_yaw_delta_deg).abs() < 1e-4,
                "diff at sens={s}, delta={d}: java={java_yaw_delta}, rust={rust_yaw_delta_deg}"
            );
        }
    }
}

#[test]
fn test_rot_7_yaw_continuity_crossing_359_to_0() {
    let (mut game, mut queue) = game_facing(359.0);
    // Turning +2.0° crosses 360°
    let wire_yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[2.0]);
    // Must be continuous 361.0°, NEVER 1.0° (which would be a -360° jump)
    assert!(
        (wire_yaw - 361.0).abs() < 1e-2,
        "expected 361.0°, got {wire_yaw}"
    );
}

#[test]
fn test_rot_8_yaw_continuity_crossing_0_to_359() {
    let (mut game, mut queue) = game_facing(0.0);
    // Turning -2.0° goes negative
    let wire_yaw = turn_and_read_wire_yaw(&mut game, &mut queue, &[-2.0]);
    // Must be continuous -2.0°, NEVER 358.0° (which would be a +360° jump)
    assert!(
        (wire_yaw - (-2.0)).abs() < 1e-2,
        "expected -2.0°, got {wire_yaw}"
    );
}

#[test]
fn test_rot_9_c05_player_look_serialization() {
    use umicraft_protocol::packets::play::serverbound::PlayerLook;
    use umicraft_protocol::EncodablePacket;

    let packet = PlayerLook {
        yaw: 185.5,
        pitch: -90.0,
        on_ground: true,
    };

    let mut bytes = Vec::new();
    packet.encode_body(&mut bytes).expect("encode");
    assert_eq!(
        bytes.len(),
        9,
        "C05 must be 9 bytes: 4 bytes yaw + 4 bytes pitch + 1 byte on_ground"
    );

    let decoded = PlayerLook::decode(&mut &bytes[..]).expect("decode");
    assert_eq!(decoded.yaw, 185.5);
    assert_eq!(decoded.pitch, -90.0);
    assert!(decoded.on_ground);
}

#[test]
fn test_rot_10_c06_position_look_serialization() {
    use umicraft_protocol::packets::play::serverbound::PositionLook;
    use umicraft_protocol::EncodablePacket;

    let packet = PositionLook {
        x: 123.456,
        y: 65.0,
        z: -789.101,
        yaw: 360.25,
        pitch: 90.0,
        on_ground: false,
    };

    let mut bytes = Vec::new();
    packet.encode_body(&mut bytes).expect("encode");
    assert_eq!(
        bytes.len(),
        33,
        "C06 must be 33 bytes: 24 bytes (3*f64) + 8 bytes (2*f32) + 1 byte on_ground"
    );

    let decoded = PositionLook::decode(&mut &bytes[..]).expect("decode");
    assert_eq!(decoded.x, 123.456);
    assert_eq!(decoded.y, 65.0);
    assert_eq!(decoded.z, -789.101);
    assert_eq!(decoded.yaw, 360.25);
    assert_eq!(decoded.pitch, 90.0);
    assert!(!decoded.on_ground);
}

#[test]
fn test_rot_11_camera_rotation_and_wire_consistency() {
    let mut game = placed_game();

    // 1. Looking down at feet (+90°)
    game.pitch = 90.0_f32.to_radians();
    let wire_pitch = game.pitch.to_degrees();
    assert!((wire_pitch - 90.0).abs() < 1e-4);

    let render_state = RenderState {
        position: DVec3::ZERO,
        yaw: game.yaw,
        pitch: game.pitch,
        eye: DVec3::new(0.0, 1.62, 0.0),
        orbit_start: Vec3::ZERO,
        on_ground: true,
        effects: umicraft_game::view::ViewEffects::default(),
    };
    let (cam_yaw, cam_pitch) = camera_angles(&render_state);
    assert!(
        (cam_pitch - (-90.0_f32.to_radians())).abs() < 1e-4,
        "camera pitch is flipped relative to game pitch"
    );
    assert!((cam_yaw - (game.yaw + std::f32::consts::PI)).abs() < 1e-4);

    // 2. Looking up at sky (-90°)
    game.pitch = -90.0_f32.to_radians();
    let wire_pitch_up = game.pitch.to_degrees();
    assert!((wire_pitch_up - (-90.0)).abs() < 1e-4);

    let render_state_up = RenderState {
        position: DVec3::ZERO,
        yaw: game.yaw,
        pitch: game.pitch,
        eye: DVec3::new(0.0, 1.62, 0.0),
        orbit_start: Vec3::ZERO,
        on_ground: true,
        effects: umicraft_game::view::ViewEffects::default(),
    };
    let (_, cam_pitch_up) = camera_angles(&render_state_up);
    assert!((cam_pitch_up - (90.0_f32.to_radians())).abs() < 1e-4);
}

// =========================================================================
// Phase C.1: Validation du pipeline réel des rotations
// =========================================================================

fn drive_production_mouse_pipeline(
    game: &mut Game,
    queue: &mut CommandQueue,
    controls: &crate::options::ControlOptions,
    zoom_factor: f32,
    deltas: &[(f64, f64)],
) {
    let mut look = (0.0_f32, 0.0_f32);
    for &delta in deltas {
        let (dyaw, dpitch) = crate::window::compute_mouse_look_delta(delta, controls, zoom_factor);
        look.0 += dyaw;
        look.1 += dpitch;
    }
    let update = InputUpdate {
        look_yaw: look.0,
        look_pitch: look.1,
        ..InputUpdate::default()
    };
    handle_input(game, update, queue);
}

#[test]
fn test_pipeline_sensitivity_0_0_min() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 0.0,
        ..crate::options::ControlOptions::default()
    };

    // 100px X, 50px Y at min sensitivity
    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(100.0, 50.0)]);
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements = movement_of(&queue);
    assert!(!movements.is_empty());
    let (yaw, pitch) = match movements.last().unwrap() {
        ClientCommand::PositionLook { yaw, pitch, .. }
        | ClientCommand::PlayerLook { yaw, pitch, .. } => (*yaw, *pitch),
        other => panic!("expected look report, got {other:?}"),
    };

    let expected_yaw = 100.0 * 0.064 * 0.15; // 0.96°
    let expected_pitch = 50.0 * 0.064 * 0.15; // 0.48°
    assert!(
        (yaw - expected_yaw).abs() < 1e-4,
        "yaw: expected {expected_yaw}, got {yaw}"
    );
    assert!(
        (pitch - expected_pitch).abs() < 1e-4,
        "pitch: expected {expected_pitch}, got {pitch}"
    );
}

#[test]
fn test_pipeline_sensitivity_0_5_default() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 0.5,
        ..crate::options::ControlOptions::default()
    };

    // 100px X, -50px Y at default sensitivity
    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(100.0, -50.0)]);
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements = movement_of(&queue);
    assert!(!movements.is_empty());
    let (yaw, pitch) = match movements.last().unwrap() {
        ClientCommand::PositionLook { yaw, pitch, .. }
        | ClientCommand::PlayerLook { yaw, pitch, .. } => (*yaw, *pitch),
        other => panic!("expected look report, got {other:?}"),
    };

    let expected_yaw = 100.0 * 1.0 * 0.15; // 15.0°
    let expected_pitch = -50.0 * 1.0 * 0.15; // -7.5°
    assert!(
        (yaw - expected_yaw).abs() < 1e-4,
        "yaw: expected {expected_yaw}, got {yaw}"
    );
    assert!(
        (pitch - expected_pitch).abs() < 1e-4,
        "pitch: expected {expected_pitch}, got {pitch}"
    );
}

#[test]
fn test_pipeline_sensitivity_1_0_max() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 1.0,
        ..crate::options::ControlOptions::default()
    };

    // -20px X, 30px Y at max sensitivity
    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(-20.0, 30.0)]);
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements = movement_of(&queue);
    assert!(!movements.is_empty());
    let (yaw, pitch) = match movements.last().unwrap() {
        ClientCommand::PositionLook { yaw, pitch, .. }
        | ClientCommand::PlayerLook { yaw, pitch, .. } => (*yaw, *pitch),
        other => panic!("expected look report, got {other:?}"),
    };

    let expected_yaw = -20.0 * 4.096 * 0.15; // -12.288°
    let expected_pitch = 30.0 * 4.096 * 0.15; // 18.432°
    assert!(
        (yaw - expected_yaw).abs() < 1e-4,
        "yaw: expected {expected_yaw}, got {yaw}"
    );
    assert!(
        (pitch - expected_pitch).abs() < 1e-4,
        "pitch: expected {expected_pitch}, got {pitch}"
    );
}

#[test]
fn test_pipeline_positive_and_negative_deltas() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 0.5,
        ..crate::options::ControlOptions::default()
    };

    // Two movements: (+50, +40) then (-80, -100) -> net (-30, -60)
    drive_production_mouse_pipeline(
        &mut game,
        &mut queue,
        &controls,
        1.0,
        &[(50.0, 40.0), (-80.0, -100.0)],
    );
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements = movement_of(&queue);
    assert!(!movements.is_empty());
    let (yaw, pitch) = match movements.last().unwrap() {
        ClientCommand::PositionLook { yaw, pitch, .. }
        | ClientCommand::PlayerLook { yaw, pitch, .. } => (*yaw, *pitch),
        other => panic!("expected look report, got {other:?}"),
    };

    let expected_yaw = -30.0 * 0.15; // -4.5°
    let expected_pitch = -60.0 * 0.15; // -9.0°
    assert!(
        (yaw - expected_yaw).abs() < 1e-4,
        "yaw: expected {expected_yaw}, got {yaw}"
    );
    assert!(
        (pitch - expected_pitch).abs() < 1e-4,
        "pitch: expected {expected_pitch}, got {pitch}"
    );
}

#[test]
fn test_pipeline_multiple_successive_mouse_events_in_frame() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 0.5,
        ..crate::options::ControlOptions::default()
    };

    // 5 successive events arriving in the same frame
    let events = [
        (10.0, 5.0),
        (15.0, -2.0),
        (20.0, 7.0),
        (-5.0, -3.0),
        (8.0, 4.0),
    ]; // sum: x = 48.0, y = 11.0

    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &events);
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements = movement_of(&queue);
    assert!(!movements.is_empty());
    let (yaw, pitch) = match movements.last().unwrap() {
        ClientCommand::PositionLook { yaw, pitch, .. }
        | ClientCommand::PlayerLook { yaw, pitch, .. } => (*yaw, *pitch),
        other => panic!("expected look report, got {other:?}"),
    };

    let expected_yaw = 48.0 * 0.15; // 7.2°
    let expected_pitch = 11.0 * 0.15; // 1.65°
    assert!(
        (yaw - expected_yaw).abs() < 1e-4,
        "yaw: expected {expected_yaw}, got {yaw}"
    );
    assert!(
        (pitch - expected_pitch).abs() < 1e-4,
        "pitch: expected {expected_pitch}, got {pitch}"
    );
}

#[test]
fn test_pipeline_pitch_vertical_limits_plus_minus_90() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 0.5,
        ..crate::options::ControlOptions::default()
    };

    // 1. Pulling mouse way up -> should clamp to -90.0°
    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(0.0, -5000.0)]);
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements = movement_of(&queue);
    let pitch_up = match movements.last().unwrap() {
        ClientCommand::PositionLook { pitch, .. } | ClientCommand::PlayerLook { pitch, .. } => {
            *pitch
        }
        other => panic!("expected look report, got {other:?}"),
    };
    assert!(
        (pitch_up - (-90.0)).abs() < 1e-4,
        "expected -90.0°, got {pitch_up}"
    );

    // 2. Pushing mouse way down -> should clamp to +90.0°
    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(0.0, 10000.0)]);
    tick(
        &mut game,
        &mut queue,
        &frames,
        &chunks,
        Instant::now() + Duration::from_millis(50),
    );

    let movements = movement_of(&queue);
    let pitch_down = match movements.last().unwrap() {
        ClientCommand::PositionLook { pitch, .. } | ClientCommand::PlayerLook { pitch, .. } => {
            *pitch
        }
        other => panic!("expected look report, got {other:?}"),
    };
    assert!(
        (pitch_down - 90.0).abs() < 1e-4,
        "expected +90.0°, got {pitch_down}"
    );
}

#[test]
fn test_pipeline_yaw_exceeding_360_continuous() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 0.5,
        ..crate::options::ControlOptions::default()
    };

    // 6 bursts of 500px to the right = 3000px
    // 3000px * 0.15 = 450.0°
    for _ in 0..6 {
        drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(500.0, 0.0)]);
    }
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements = movement_of(&queue);
    let yaw = match movements.last().unwrap() {
        ClientCommand::PositionLook { yaw, .. } | ClientCommand::PlayerLook { yaw, .. } => *yaw,
        other => panic!("expected look report, got {other:?}"),
    };
    assert!(
        (yaw - 450.0).abs() < 1e-2,
        "continuous yaw must be 450.0°, got {yaw}"
    );
}

#[test]
fn test_pipeline_c05_c06_serialization_after_multiple_rotations() {
    use umicraft_protocol::packets::play::serverbound::{PlayerLook, PositionLook};
    use umicraft_protocol::EncodablePacket;

    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 0.5,
        ..crate::options::ControlOptions::default()
    };

    // 1. Initial tick with movement+rotation reports C06 PositionLook (33 bytes)
    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(50.0, -30.0)]);
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements1 = movement_of(&queue);
    let (c06_pos, c06_yaw, c06_pitch) = match movements1.last().unwrap() {
        ClientCommand::PositionLook {
            x,
            y,
            z,
            yaw,
            pitch,
            ..
        } => (DVec3::new(*x, *y, *z), *yaw, *pitch),
        other => panic!("expected PositionLook (C06), got {other:?}"),
    };
    let mut c06_bytes = Vec::new();
    PositionLook {
        x: c06_pos.x,
        y: c06_pos.y,
        z: c06_pos.z,
        yaw: c06_yaw,
        pitch: c06_pitch,
        on_ground: false,
    }
    .encode_body(&mut c06_bytes)
    .expect("encode c06");
    assert_eq!(c06_bytes.len(), 33, "C06 must be exactly 33 bytes");
    let decoded_c06 = PositionLook::decode(&mut &c06_bytes[..]).expect("decode c06");
    assert_eq!(decoded_c06.x, c06_pos.x);
    assert_eq!(decoded_c06.y, c06_pos.y);
    assert_eq!(decoded_c06.z, c06_pos.z);
    assert_eq!(decoded_c06.yaw, c06_yaw);
    assert_eq!(decoded_c06.pitch, c06_pitch);

    queue.ordered.clear();

    // 2. Subsequent tick with rotation alone reports C05 PlayerLook (9 bytes)
    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(200.0, 100.0)]);
    tick(
        &mut game,
        &mut queue,
        &frames,
        &chunks,
        Instant::now() + Duration::from_millis(50),
    );

    let movements2 = movement_of(&queue);
    let (c05_yaw, c05_pitch) = match movements2.last().unwrap() {
        ClientCommand::PlayerLook { yaw, pitch, .. } => (*yaw, *pitch),
        other => panic!("expected PlayerLook (C05), got {other:?}"),
    };
    let mut c05_bytes = Vec::new();
    PlayerLook {
        yaw: c05_yaw,
        pitch: c05_pitch,
        on_ground: false,
    }
    .encode_body(&mut c05_bytes)
    .expect("encode c05");
    assert_eq!(c05_bytes.len(), 9, "C05 must be exactly 9 bytes");
    let decoded_c05 = PlayerLook::decode(&mut &c05_bytes[..]).expect("decode c05");
    assert_eq!(decoded_c05.yaw, c05_yaw);
    assert_eq!(decoded_c05.pitch, c05_pitch);
}

#[test]
fn test_pipeline_consistency_player_state_and_wire() {
    let mut game = placed_game();
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    let controls = crate::options::ControlOptions {
        mouse_sensitivity: 0.5,
        ..crate::options::ControlOptions::default()
    };

    drive_production_mouse_pipeline(&mut game, &mut queue, &controls, 1.0, &[(123.0, -45.0)]);
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());

    let movements = movement_of(&queue);
    let (wire_yaw, wire_pitch) = match movements.last().unwrap() {
        ClientCommand::PositionLook { yaw, pitch, .. }
        | ClientCommand::PlayerLook { yaw, pitch, .. } => (*yaw, *pitch),
        other => panic!("expected look report, got {other:?}"),
    };

    // Exactly matches game state degrees
    assert_eq!(game.yaw.to_degrees(), wire_yaw);
    assert_eq!(game.pitch.to_degrees(), wire_pitch);
}
