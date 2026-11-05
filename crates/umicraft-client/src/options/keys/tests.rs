// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the key bindings.

use winit::keyboard::KeyCode;

use super::names::BINDABLE;
use super::*;

#[test]
fn no_default_binding_is_shared_or_reserved() {
    let bindings = KeyBindings::default();
    for action in Action::ALL {
        assert!(bindings.conflicts(action).is_empty(), "{action:?}");
        assert!(bindings.get(action).is_bindable(), "{action:?}");
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

#[test]
fn the_attack_use_and_pick_defaults_are_mouse_buttons_one_to_three() {
    let bindings = KeyBindings::default();
    assert_eq!(bindings.get(Action::Attack), Binding::Mouse(0));
    assert_eq!(bindings.get(Action::Use), Binding::Mouse(1));
    assert_eq!(bindings.get(Action::PickBlock), Binding::Mouse(2));
    assert_eq!(bindings.get(Action::Screenshot), KeyCode::F2);
    assert_eq!(bindings.get(Action::Fullscreen), KeyCode::F11);
    assert_eq!(bindings.get(Action::Drop), KeyCode::KeyQ);
}

#[test]
fn winit_buttons_map_to_the_vanilla_numbering() {
    use winit::event::MouseButton;
    assert_eq!(
        Binding::from_mouse(MouseButton::Left),
        Some(Binding::Mouse(0))
    );
    assert_eq!(
        Binding::from_mouse(MouseButton::Right),
        Some(Binding::Mouse(1))
    );
    assert_eq!(
        Binding::from_mouse(MouseButton::Middle),
        Some(Binding::Mouse(2))
    );
    assert_eq!(
        Binding::from_mouse(MouseButton::Back),
        Some(Binding::Mouse(3))
    );
    assert_eq!(Binding::from_mouse(MouseButton::Other(200)), None);
}

#[test]
fn any_action_takes_a_mouse_button_and_a_key_can_take_attack() {
    let mut bindings = KeyBindings::default();
    assert!(bindings.set(Action::Forward, Binding::Mouse(4)));
    assert!(bindings.set(Action::Attack, KeyCode::KeyJ));
    assert_eq!(
        bindings.action_for(Binding::Mouse(4)),
        Some(Action::Forward)
    );
    assert_eq!(bindings.action_for(KeyCode::KeyJ), Some(Action::Attack));
    assert_eq!(bindings.action_for(Binding::Mouse(0)), None);
    assert!(!bindings.set(Action::Jump, Binding::Mouse(40)));
}

#[test]
fn a_mouse_button_shared_by_two_actions_is_a_conflict() {
    let mut bindings = KeyBindings::default();
    bindings.set(Action::Use, Binding::Mouse(0));
    assert_eq!(bindings.conflicts(Action::Attack), [Action::Use]);
    assert_eq!(bindings.conflicts(Action::Use), [Action::Attack]);
}

#[test]
fn bindings_round_trip_through_the_map_and_old_files_still_load() {
    let mut bindings = KeyBindings::default();
    bindings.set(Action::Attack, KeyCode::KeyJ);
    bindings.set(Action::Sneak, Binding::Mouse(3));
    let map = bindings.to_map();
    assert_eq!(map["sneak"], "Mouse4");
    assert_eq!(KeyBindings::from_map(&map), bindings);

    // A file from before mouse bindings: only keys, no attack/use/pick.
    let old: std::collections::BTreeMap<String, String> =
        [("forward".to_string(), "ArrowUp".to_string())].into();
    let loaded = KeyBindings::from_map(&old);
    assert_eq!(loaded.get(Action::Forward), KeyCode::ArrowUp);
    assert_eq!(loaded.get(Action::Attack), Binding::Mouse(0));
    assert_eq!(Binding::from_name("Mouse0"), None);
    assert_eq!(Binding::from_name("Mouse33"), None);
}

#[test]
fn a_mouse_button_label_uses_the_language() {
    let lang = umicraft_core::lang::Lang::builtin();
    assert_eq!(Binding::Mouse(0).label(&lang), "Button 1");
    assert_eq!(Binding::Mouse(2).label(&lang), "Button 3");
    assert_eq!(Binding::Key(KeyCode::KeyW).label(&lang), "W");
}
