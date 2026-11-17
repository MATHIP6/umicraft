// SPDX-License-Identifier: MIT OR Apache-2.0

//! The camera, the perspectives and the body of the local player.

use super::*;

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
    game.controls.input.forward = 1.0;
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
    game.controls.input.forward = 1.0;
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

    game.controls.input.forward = 0.0;
    for _ in 0..60 {
        tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    }
    assert!(game.body.frame_data().limbs.amount < 0.05);
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
