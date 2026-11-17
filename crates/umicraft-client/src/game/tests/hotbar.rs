// SPDX-License-Identifier: MIT OR Apache-2.0

//! The hotbar: slot keys, wheel, drop and pick-block, and their order on the wire.

use super::*;

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
