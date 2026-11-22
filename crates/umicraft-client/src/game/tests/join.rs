// SPDX-License-Identifier: MIT OR Apache-2.0

//! Joining, respawning and changing dimension.

use super::*;

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
    assert_eq!(game.clock.time(), 6_000);
    assert!(game.clock.frozen());

    publish_frame(&game, &frames, Instant::now());
    let RenderEvent::Frame(frame) = received.try_recv().expect("a frame") else {
        panic!("the published event must be a frame");
    };
    assert_eq!(frame.world_time, 6_000);
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
    assert_eq!(game.clock.time(), 0);
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
    assert_eq!(game.clock.time(), 0);
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
    assert_eq!(game.clock.time(), 6_000);
    assert!(game.clock.frozen());

    // A same-dimension respawn keeps the frozen clock.
    handle_net(
        &mut game,
        PlayEvent::Respawn(respawn(0)),
        &mut queue,
        &frames,
    );
    assert_eq!(game.clock.time(), 6_000);
    assert!(game.clock.frozen());
    assert_eq!(game.dimension, 0);

    // A dimension change starts a new clock.
    handle_net(
        &mut game,
        PlayEvent::Respawn(respawn(-1)),
        &mut queue,
        &frames,
    );
    assert_eq!(game.clock.time(), 0);
    assert_eq!(game.dimension, -1);
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
    game.controls.use_pressed = true;
    game.controls.attack_pressed = true;
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
    assert!(!game.controls.use_pressed && !game.controls.attack_pressed);
    assert!(!game.item_use.is_using());
    assert!(
        game.windows.view().container.is_none(),
        "no window stays open"
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
