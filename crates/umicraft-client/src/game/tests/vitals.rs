// SPDX-License-Identifier: MIT OR Apache-2.0

//! Health, armour, death and respawn of the local player.

use super::*;

#[test]
fn the_armour_bar_follows_the_armour_slots() {
    let mut game = placed_game();
    game.windows.apply_set_slot(0, 5, wire_item(310, 0));
    game.windows.apply_set_slot(0, 6, wire_item(311, 0));
    sync_armor(&mut game);
    assert_eq!(game.vitals.armor, 11.0);
}

#[test]
fn the_hud_carries_the_rows_and_bars_of_the_tick() {
    let mut game = game_on_floor_with(BlockState::AIR);
    game.vitals.experience = 0.5;
    game.vitals.level = 4;
    let (frames, _received) = mpsc::sync_channel(16);
    handle_net(
        &mut game,
        health_update(7.0),
        &mut CommandQueue::default(),
        &frames,
    );
    game.vitals.health = 7.0;
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
    game.controls.input.forward = 1.0;
    game.controls.input.attack = true;
    game.controls.attack_pressed = true;
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
    assert_eq!(game.vitals.death_time, DEATH_TICKS);
    handle_net(&mut game, health_update(20.0), &mut queue, &frames);
    tick(&mut game, &mut queue, &frames, &chunks, Instant::now());
    assert_eq!(game.vitals.death_time, 0);
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
    assert_eq!(game.vitals.score, 0, "another entity's score is not ours");
    handle_net(&mut game, metadata(3, 57), &mut queue, &frames);
    assert_eq!(game.vitals.score, 57);
    assert_eq!(game.hud().score, 57);
}
