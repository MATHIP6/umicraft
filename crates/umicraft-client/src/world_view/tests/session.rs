// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the view keeps of the session: the HUD, the status, the toasts, the
//! connect stage, the sounds, and what clearing a session forgets.

use super::*;

#[test]
fn the_hud_the_status_and_the_end_are_kept() {
    let mut harness = Harness::new();
    let hud = HudState {
        health: 7.0,
        ..HudState::default()
    };
    let status = GameStatus {
        chunks: 3,
        placed: true,
        ..GameStatus::default()
    };
    for event in [
        RenderEvent::Hud(Box::new(hud)),
        RenderEvent::Status(Box::new(status)),
        RenderEvent::Stats(vec![("stat.jump".to_string(), 4)]),
        RenderEvent::Ended(ConnectionEnd::kicked("kicked")),
    ] {
        harness.frames.0.send(event).expect("frame queue");
    }
    harness.sync();
    assert_eq!(harness.world_view.hud().health, 7.0);
    assert_eq!(harness.world_view.status().chunks, 3);
    assert!(harness.world_view.placed());
    assert_eq!(harness.world_view.stats(), [("stat.jump".to_string(), 4)]);
    assert_eq!(
        harness.world_view.ended(),
        Some(&ConnectionEnd::kicked("kicked"))
    );
}

#[test]
fn only_an_achievement_unlocked_after_the_first_answer_makes_a_toast() {
    let mut harness = Harness::new();
    let answer = |names: &[&str]| {
        RenderEvent::Stats(
            names
                .iter()
                .map(|name| (format!("achievement.{name}"), 1))
                .collect(),
        )
    };
    harness
        .frames
        .0
        .send(answer(&["openInventory"]))
        .expect("queue");
    harness.sync();
    assert!(
        harness.world_view.toast().is_none(),
        "the first answer is the baseline"
    );
    harness
        .frames
        .0
        .send(answer(&["openInventory", "mineWood"]))
        .expect("queue");
    harness.sync();
    assert_eq!(
        harness.world_view.toast().map(|(id, _)| id),
        Some("mineWood")
    );
    harness.world_view.clear_session(&mut harness.target);
    assert!(harness.world_view.toast().is_none());
}

#[test]
fn the_connect_stage_follows_the_event_and_resets_with_the_session() {
    let mut harness = Harness::new();
    assert_eq!(harness.world_view.connect_stage(), ConnectStage::Connecting);
    let event = RenderEvent::Connect(ConnectStage::LoggingIn);
    harness.frames.0.send(event).expect("frame queue");
    harness.sync();
    assert_eq!(harness.world_view.connect_stage(), ConnectStage::LoggingIn);
    harness.world_view.clear_session(&mut harness.target);
    assert_eq!(harness.world_view.connect_stage(), ConnectStage::Connecting);
}

#[test]
fn clearing_a_session_forgets_its_state_but_keeps_the_workers() {
    let mut harness = Harness::new();
    let status = GameStatus {
        placed: true,
        chunks: 4,
        ..GameStatus::default()
    };
    for event in [
        RenderEvent::Hud(Box::new(HudState {
            health: 3.0,
            ..HudState::default()
        })),
        RenderEvent::Status(Box::new(status)),
        RenderEvent::Stats(vec![("stat.jump".to_string(), 4)]),
        RenderEvent::Ended(ConnectionEnd::kicked("kicked")),
    ] {
        harness.frames.0.send(event).expect("frame queue");
    }
    harness.sync();
    harness.world_view.clear_session(&mut harness.target);
    assert!(!harness.world_view.placed());
    assert!(harness.world_view.stats().is_empty());
    assert_eq!(harness.world_view.ended(), None);
    assert_eq!(harness.world_view.hud().health, 20.0);
    assert_eq!(harness.target.clears, 1);

    harness.send_chunk(ORIGIN, &[BlockPos::new(2, 1, 2)]);
    harness.sync_until(|h| h.target.uploaded.contains_key(&ORIGIN));
}

#[test]
fn sound_cues_wait_for_the_window_in_order_and_are_taken_once() {
    use std::borrow::Cow;
    use umicraft_game::sound::cue::CueCategory;

    let cue = |name: &'static str| SoundCue {
        name: Cow::Borrowed(name),
        pos: [0.0; 3],
        volume: 1.0,
        pitch: 1.0,
        category: CueCategory::Blocks,
    };
    let mut harness = Harness::new();
    for cues in [
        vec![cue("step.stone")],
        vec![cue("dig.stone"), cue("random.pop")],
    ] {
        harness
            .frames
            .0
            .send(RenderEvent::Sounds(cues))
            .expect("frame queue");
    }
    harness.sync();
    let names: Vec<_> = harness
        .world_view
        .take_sounds()
        .into_iter()
        .map(|cue| cue.name)
        .collect();
    assert_eq!(names, ["step.stone", "dig.stone", "random.pop"]);
    assert!(harness.world_view.take_sounds().is_empty());
}
