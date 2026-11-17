// SPDX-License-Identifier: MIT OR Apache-2.0

//! The real game loop: frame pacing, congestion and recovery.

use super::*;

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
            // 100 mouse pixels at the default sensitivity: 15 degrees.
            look_yaw: 100.0,
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
        PlayServerbound::PlayerLook(p) => p.yaw == 15.0,
        PlayServerbound::PositionLook(p) => p.yaw == 15.0,
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
