// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use super::*;
use crate::manifest::Permissions;

fn context(input: bool) -> Arc<ModContext> {
    let cx = ModContext::with_shared(
        "aim".to_owned().try_into().unwrap(),
        Permissions {
            input,
            ..Permissions::default()
        },
        Arc::default(),
    );
    cx.finish_init(true);
    cx.shared.set_game_focus(true);
    cx
}

fn drained(cx: &ModContext) -> Vec<InputRequest> {
    let mut out = Vec::new();
    InputQueue::new(cx.shared.input_inbox()).drain(|request| out.push(request));
    out
}

#[test]
fn requests_are_queued_in_call_order() {
    let cx = context(true);
    assert_eq!(cx.input_press(UMI_INPUT_ACTION_ATTACK, 2), UmiStatus::OK);
    assert_eq!(cx.input_clear_press(UMI_INPUT_ACTION_USE), UmiStatus::OK);
    assert_eq!(cx.input_add_look(1.5, -2.0), UmiStatus::OK);
    assert_eq!(
        drained(&cx),
        [
            InputRequest::Press {
                action: InputAction::Attack,
                count: 2
            },
            InputRequest::ClearPress {
                action: InputAction::Use
            },
            InputRequest::AddLook {
                yaw: 1.5,
                pitch: -2.0
            },
        ]
    );
}

#[test]
fn without_the_permission_every_call_is_denied() {
    let cx = context(false);
    assert_eq!(
        cx.input_press(UMI_INPUT_ACTION_ATTACK, 1),
        UmiStatus::DENIED
    );
    assert_eq!(
        cx.input_clear_press(UMI_INPUT_ACTION_ATTACK),
        UmiStatus::DENIED
    );
    assert_eq!(cx.input_add_look(1.0, 0.0), UmiStatus::DENIED);
    assert!(drained(&cx).is_empty());
}

#[test]
fn a_mod_the_player_turned_off_is_denied() {
    let cx = context(true);
    assert!(cx.set_wanted(false));
    assert_eq!(cx.input_press(UMI_INPUT_ACTION_USE, 1), UmiStatus::DENIED);
}

#[test]
fn without_the_game_focus_the_input_is_unavailable() {
    let cx = context(true);
    cx.shared.set_game_focus(false);
    assert_eq!(
        cx.input_press(UMI_INPUT_ACTION_ATTACK, 1),
        UmiStatus::UNAVAILABLE
    );
    assert_eq!(cx.input_add_look(1.0, 0.0), UmiStatus::UNAVAILABLE);
    assert!(drained(&cx).is_empty());
}

#[test]
fn bad_arguments_are_refused() {
    let cx = context(true);
    let refused = [
        cx.input_press(2, 1),
        cx.input_press(UMI_INPUT_ACTION_ATTACK, 0),
        cx.input_press(UMI_INPUT_ACTION_ATTACK, UMI_MAX_INPUT_PRESSES + 1),
        cx.input_press(UMI_INPUT_ACTION_ATTACK, 300),
        cx.input_clear_press(7),
        cx.input_add_look(f32::NAN, 0.0),
        cx.input_add_look(0.0, f32::INFINITY),
        cx.input_add_look(180.5, 0.0),
    ];
    assert!(refused
        .iter()
        .all(|status| *status == UmiStatus::INVALID_ARGUMENT));
    assert!(drained(&cx).is_empty());
    assert_eq!(
        cx.input_press(UMI_INPUT_ACTION_ATTACK, UMI_MAX_INPUT_PRESSES),
        UmiStatus::OK
    );
    assert_eq!(cx.input_add_look(-180.0, 180.0), UmiStatus::OK);
}

#[test]
fn a_full_queue_answers_busy_and_keeps_what_it_has() {
    let cx = context(true);
    for _ in 0..INPUT_QUEUE_CAPACITY {
        assert_eq!(cx.input_press(UMI_INPUT_ACTION_ATTACK, 1), UmiStatus::OK);
    }
    assert_eq!(cx.input_press(UMI_INPUT_ACTION_ATTACK, 1), UmiStatus::BUSY);
    assert_eq!(drained(&cx).len(), INPUT_QUEUE_CAPACITY);
}

#[test]
fn clear_discards_the_queue() {
    let cx = context(true);
    assert_eq!(cx.input_press(UMI_INPUT_ACTION_ATTACK, 1), UmiStatus::OK);
    InputQueue::new(cx.shared.input_inbox()).clear();
    assert!(drained(&cx).is_empty());
}

#[test]
fn server_compat_mode_does_not_hold_an_input_mod_off() {
    let cx = context(true);
    cx.shared.set_server_compat(true);
    assert!(!cx.is_held_by_compat());
    assert_eq!(cx.input_press(UMI_INPUT_ACTION_ATTACK, 1), UmiStatus::OK);
}
