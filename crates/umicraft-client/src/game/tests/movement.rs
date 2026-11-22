// SPDX-License-Identifier: MIT OR Apache-2.0

//! The movement packets of the tick, riding, and the motion the server gives.

use super::*;

#[test]
fn the_player_reports_every_tick_while_its_column_is_missing() {
    // Regression: Umicraft froze and sent nothing until the player's column
    // arrived. Vanilla keeps ticking (`ChunkProviderClient.chunkExists` is
    // always true), sinks at 0.1 a tick and reports it.
    let mut game = Game::new();
    game.playing = true;
    game.state = Some(GameState::new(DVec3::new(0.5, 4.0, 0.5)));
    game.world = Some(World::new(0));
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    for _ in 0..3 {
        tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    }
    let reports = movement_of(&queue);
    assert_eq!(reports.len(), 3, "one report a tick: {reports:?}");
    let sunk = game.state.as_ref().expect("placed").position().y;
    assert!(sunk < 4.0 && sunk > 3.6, "the player sinks slowly: {sunk}");
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
fn a_riding_tick_steers_with_the_damped_axes() {
    // Regression: the raw 1.0 was sent, where vanilla sends `moveForward`
    // after `EntityLivingBase` damped it to 0.98.
    let mut game = placed_game();
    game.vehicle = Some(42);
    let mut queue = CommandQueue::default();
    let forward = RawInput {
        forward: 1.0,
        strafe: -1.0,
        ..RawInput::default()
    }
    .to_intent();
    report_movement(&mut game, &forward, &mut queue);
    assert!(matches!(
        commands(&queue)[..],
        [
            ClientCommand::PlayerLook { .. },
            ClientCommand::Input {
                strafe: -0.98,
                forward: 0.98,
                jumping: false,
                sneaking: false,
            }
        ]
    ));
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
fn a_boat_holds_the_player_up() {
    // Regression: boats were not solid for the local player (MCP
    // `World.getCollidingBoundingBoxes` adds `EntityBoat.getCollisionBoundingBox`).
    let mut game = placed_game();
    game.state = Some(GameState::new(DVec3::new(3.5, 66.0, 3.5)));
    game.entities.spawn_object(
        EntityId(9),
        1,
        DVec3::new(3.5, 64.0, 3.5),
        0.0,
        0.0,
        Vec3::ZERO,
        0,
    );
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let (chunks, _) = mpsc::sync_channel(64);
    for _ in 0..30 {
        tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
        queue.ordered.clear();
    }
    let player = game.state.as_ref().expect("placed").player();
    assert!(player.on_ground);
    assert_eq!(
        player.position.y,
        64.0 + f64::from(0.6_f32),
        "standing on the boat"
    );
}

/// Seats the local player (entity 7) on a boat (entity 9) on the floor of
/// [`placed_game`].
fn boarded_boat() -> (Game, CommandQueue) {
    let mut game = placed_game();
    game.entity_id = 7;
    game.state = Some(GameState::new(DVec3::new(3.5, 65.0, 3.5)));
    game.entities.spawn_object(
        EntityId(9),
        1,
        DVec3::new(3.5, 64.0, 3.5),
        0.0,
        0.0,
        Vec3::ZERO,
        0,
    );
    let mut queue = CommandQueue::default();
    let (frames, _) = mpsc::sync_channel(64);
    let attach = PlayEvent::AttachEntity(
        umicraft_protocol::packets::play::clientbound::AttachEntity {
            leash: 0,
            entity_id: 7,
            vehicle_id: 9,
        },
    );
    handle_net(&mut game, attach, &mut queue, &frames);
    (game, queue)
}

#[test]
fn a_boarded_boat_answers_the_keys_on_the_client() {
    // Regression: the client waited for the server to move the boat it rode,
    // where `EntityBoat` simulates it locally once `isBoatEmpty` is false.
    let (mut game, mut queue) = boarded_boat();
    let (frames, _) = mpsc::sync_channel(256);
    let (chunks, _) = mpsc::sync_channel(64);
    game.controls.input.forward = 1.0;
    for _ in 0..20 {
        tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
        queue.ordered.clear();
    }
    let boat = &game.entities.entities()[&EntityId(9)];
    assert!(boat.boat.driven && boat.boat.boarded);
    // Yaw 0 paddles towards +z, slowly on dry land.
    assert!(boat.position.z > 3.5 + 0.01, "{}", boat.position.z);
    let player = game.state.as_ref().expect("placed").player();
    assert!(
        (player.position.z - boat.position.z).abs() < 0.5,
        "seated on it"
    );
}

#[test]
fn a_driven_boat_ignores_a_server_position_within_a_block() {
    use umicraft_protocol::packets::play::clientbound::EntityTeleport;

    let (mut game, mut queue) = boarded_boat();
    let (frames, _) = mpsc::sync_channel(64);
    let relative = PlayEvent::EntityRelativeMove(
        umicraft_protocol::packets::play::clientbound::EntityRelativeMove {
            entity_id: 9,
            dx: 16,
            dy: 0,
            dz: 0,
            on_ground: true,
        },
    );
    handle_net(&mut game, relative, &mut queue, &frames);
    game.entities.tick();
    let boat = &game.entities.entities()[&EntityId(9)];
    assert_eq!(
        boat.position,
        DVec3::new(3.5, 64.0, 3.5),
        "half a block off: kept"
    );
    // A teleport of a ridden boat puts it there at once.
    let teleport = PlayEvent::EntityTeleport(EntityTeleport {
        entity_id: 9,
        x: 10 * 32,
        y: 64 * 32,
        z: 3 * 32,
        yaw: 0,
        pitch: 0,
        on_ground: true,
    });
    handle_net(&mut game, teleport, &mut queue, &frames);
    let boat = &game.entities.entities()[&EntityId(9)];
    assert_eq!(boat.position, DVec3::new(10.0, 64.0, 3.0));
}

#[test]
fn another_rider_makes_the_boat_an_empty_one_again() {
    let (mut game, mut queue) = boarded_boat();
    let (frames, _) = mpsc::sync_channel(64);
    let other = PlayEvent::AttachEntity(
        umicraft_protocol::packets::play::clientbound::AttachEntity {
            leash: 0,
            entity_id: 30,
            vehicle_id: 9,
        },
    );
    handle_net(&mut game, other, &mut queue, &frames);
    assert!(!game.entities.entities()[&EntityId(9)].boat.driven);
}

#[test]
fn the_local_body_sits_in_its_boat() {
    // Regression: the local body never knew it rode, so it stood in the boat
    // in the second and third person.
    let (mut game, _) = boarded_boat();
    follow_body(&mut game);
    assert!(game.body.frame_data().riding);
    game.vehicle = None;
    follow_body(&mut game);
    assert!(!game.body.frame_data().riding);
}
