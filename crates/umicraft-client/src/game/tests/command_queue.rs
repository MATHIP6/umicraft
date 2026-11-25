// SPDX-License-Identifier: MIT OR Apache-2.0

//! The order of the serverbound commands and the backpressure of the queue.

use super::*;

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
    game.pitch = 57.3;
    let mut queue = CommandQueue::default();
    run_ticks(&mut game, &mut queue, 1);
    game.controls.input.attack = true;
    game.controls.attack_presses = 1;
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
    game.pitch = 57.3;
    game.windows.apply_set_slot(0, 36, click_stack());
    let mut queue = CommandQueue::default();
    run_ticks(&mut game, &mut queue, 1);
    game.controls.use_pressed = true;
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
    game.pitch = 57.3;
    game.windows.apply_set_slot(0, 36, click_stack());
    let mut queue = CommandQueue::default();
    let mut ends = Vec::new();
    run_ticks(&mut game, &mut queue, 1);
    ends.push(queue.ordered.len());
    game.controls.input.attack = true;
    game.controls.attack_presses = 1;
    run_ticks(&mut game, &mut queue, 1);
    game.controls.input.attack = false;
    ends.push(queue.ordered.len());
    game.controls.use_pressed = true;
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
