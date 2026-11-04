// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the key bindings.

use winit::keyboard::KeyCode;

use super::names::BINDABLE;
use super::*;

#[test]
fn no_default_key_is_shared_or_reserved() {
    let bindings = KeyBindings::default();
    for action in Action::ALL {
        assert!(bindings.conflicts(action).is_empty(), "{action:?}");
        assert!(!is_reserved(bindings.get(action)), "{action:?}");
        assert!(key_name(bindings.get(action)).is_some(), "{action:?}");
    }
}

#[test]
fn every_key_name_round_trips_and_has_a_label() {
    for (name, key) in BINDABLE {
        assert_eq!(key_name(*key), Some(*name));
        assert_eq!(key_from_name(name), Some(*key));
        assert!(!key_label(*key).is_empty());
    }
    assert_eq!(key_label(KeyCode::KeyW), "W");
    assert_eq!(key_label(KeyCode::Digit3), "3");
    assert_eq!(key_label(KeyCode::ShiftLeft), "LEFT SHIFT");
    assert_eq!(key_label(KeyCode::Slash), "/");
    assert_eq!(key_label(KeyCode::Escape), "?");
    assert_eq!(key_from_name("NoSuchKey"), None);
}

#[test]
fn a_rebound_key_is_used_and_the_old_one_is_free() {
    let mut bindings = KeyBindings::default();
    assert!(bindings.set(Action::Forward, KeyCode::ArrowUp));
    assert_eq!(bindings.get(Action::Forward), KeyCode::ArrowUp);
    assert_eq!(bindings.action_for(KeyCode::ArrowUp), Some(Action::Forward));
    assert_eq!(bindings.action_for(KeyCode::KeyW), None);
}

#[test]
fn reserved_and_unknown_keys_cannot_be_bound() {
    let mut bindings = KeyBindings::default();
    for key in [
        KeyCode::Escape,
        KeyCode::F1,
        KeyCode::F2,
        KeyCode::F3,
        KeyCode::F4,
        KeyCode::PrintScreen,
    ] {
        assert!(!bindings.set(Action::Jump, key), "{key:?}");
    }
    assert_eq!(bindings.get(Action::Jump), KeyCode::Space);
}

#[test]
fn two_actions_on_one_key_are_reported_as_a_conflict() {
    let mut bindings = KeyBindings::default();
    bindings.set(Action::Back, KeyCode::KeyW);
    assert_eq!(bindings.conflicts(Action::Forward), [Action::Back]);
    assert_eq!(bindings.conflicts(Action::Back), [Action::Forward]);
    assert!(bindings.conflicts(Action::Jump).is_empty());
    let both: Vec<_> = bindings.actions_for(KeyCode::KeyW).collect();
    assert_eq!(both, [Action::Forward, Action::Back]);
}

#[test]
fn reset_restores_every_default() {
    let mut bindings = KeyBindings::default();
    bindings.set(Action::Jump, KeyCode::KeyJ);
    bindings.reset();
    assert_eq!(bindings, KeyBindings::default());
}

#[test]
fn hotbar_actions_map_to_their_slot() {
    let slots: Vec<_> = Action::ALL
        .iter()
        .filter_map(|action| action.hotbar_slot())
        .collect();
    assert_eq!(slots, [0, 1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(Action::Jump.hotbar_slot(), None);
}
