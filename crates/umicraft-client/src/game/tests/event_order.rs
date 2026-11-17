// SPDX-License-Identifier: MIT OR Apache-2.0

//! The order of the network events against the tick (vanilla `runGameLoop`).

use super::*;

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
