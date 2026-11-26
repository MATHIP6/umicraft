// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the routing of the mods menu outcomes and of the controls of the cards.

use umicraft_ui::menus::ModValue;
use umicraft_ui::Rect;

use super::*;

fn toggle(id: &str) -> ModsOutcome {
    ModsOutcome::Toggle {
        id: id.into(),
        enabled: true,
    }
}

fn placed(id: &str) -> ModsOutcome {
    ModsOutcome::Hud(HudEditorOutcome::Placed {
        id: id.into(),
        rect: Rect::new(10.0, 10.0, 20.0, 8.0),
        scale: 1.0,
        screen: [400.0, 300.0],
    })
}

#[test]
fn an_id_without_the_prefix_is_for_the_builtin_registry() {
    assert_eq!(route(&toggle("fps")), Route::BUILTIN);
    assert_eq!(route(&placed("fps")), Route::BUILTIN);
    assert_eq!(route(&ModsOutcome::Close), Route::BUILTIN);
}

#[test]
fn a_prefixed_id_is_for_the_external_mod_only() {
    let change = toggle("ext:hello");
    let routed = route(&change);
    assert!(!routed.builtin);
    assert_eq!(
        routed.external,
        Some(Request::Enable {
            mod_id: "hello",
            enabled: true
        })
    );
}

#[test]
fn a_setting_keeps_its_key_and_its_value() {
    let change = ModsOutcome::Set {
        id: "ext:hello".into(),
        key: "size".into(),
        value: ModValue::Number(3.0),
    };
    assert_eq!(
        route(&change).external,
        Some(Request::Setting {
            mod_id: "hello",
            key: "size",
            value: SettingValue::Number(3.0)
        })
    );
}

#[test]
fn a_widget_id_names_the_mod_and_the_widget() {
    let mod_id: ModId = "hello".parse().expect("valid id");
    let id = widget_id(&mod_id, "main");
    assert_eq!(id, "ext:hello:main");
    assert_eq!(parse(&id), Some(("hello", Some("main"))));
    assert_eq!(parse(&card_id(&mod_id)), Some(("hello", None)));
    assert_eq!(parse("fps"), None);
    assert!(matches!(
        route(&placed(&id)).external,
        Some(Request::Place {
            mod_id: "hello",
            widget: "main",
            ..
        })
    ));
}

#[test]
fn the_switch_in_the_hud_editor_toggles_the_whole_mod() {
    assert_eq!(
        route(&toggle("ext:hello:main")).external,
        Some(Request::Enable {
            mod_id: "hello",
            enabled: true
        })
    );
}

#[test]
fn reset_all_is_for_both_sides() {
    let change = ModsOutcome::Hud(HudEditorOutcome::ResetAll);
    let routed = route(&change);
    assert!(routed.builtin);
    assert_eq!(routed.external, Some(Request::ResetAll));
}

#[test]
fn an_external_widget_cannot_be_locked_and_the_registry_never_sees_it() {
    let lock = ModsOutcome::Hud(HudEditorOutcome::Lock {
        id: "ext:hello:main".into(),
        locked: true,
    });
    assert_eq!(route(&lock), Route::external(None));
}

#[test]
fn a_placement_of_the_whole_mod_without_a_widget_is_dropped() {
    assert_eq!(route(&placed("ext:hello")), Route::external(None));
}

#[test]
fn a_choice_selects_the_current_option_and_a_bad_value_the_first() {
    let kind = SettingSpecKind::Choice {
        options: vec!["a".into(), "b".into()],
        default: 0,
    };
    let selected = |value: SettingValue| match control(&kind, &value) {
        SettingControl::Choice { selected, .. } => selected,
        other => panic!("not a choice: {other:?}"),
    };
    assert_eq!(selected(SettingValue::Text("b".into())), 1);
    assert_eq!(selected(SettingValue::Bool(true)), 0);
}

#[test]
fn a_slider_and_a_toggle_show_their_values() {
    let slider = SettingSpecKind::Slider {
        min: 1.0,
        max: 5.0,
        step: 1.0,
        default: 2.0,
    };
    assert!(matches!(
        control(&slider, &SettingValue::Number(4.0)),
        SettingControl::Slider { value, .. } if value == 4.0
    ));
    assert_eq!(
        control(
            &SettingSpecKind::Toggle { default: false },
            &SettingValue::Bool(true)
        ),
        SettingControl::Toggle(true)
    );
}

#[test]
fn the_compatible_mode_card_is_its_own_request_not_a_mod() {
    let change = toggle(COMPAT_CARD);
    let routed = route(&change);
    assert!(!routed.builtin);
    assert_eq!(
        routed.external,
        Some(Request::ServerCompat { enabled: true })
    );
    // A mod id never contains a hyphen, so no mod card can collide with it.
    assert!(COMPAT_CARD
        .strip_prefix(PREFIX)
        .is_some_and(|rest| rest.parse::<ModId>().is_err()));
}

#[test]
fn applying_the_compatible_mode_switches_the_host_and_changes_no_mod_file() {
    let nothing = umicraft_modhost::resolve(
        std::iter::empty(),
        umicraft_modhost::ApiVersion { major: 0, minor: 6 },
    );
    let dir = std::env::temp_dir();
    let (host, _) = ModHost::load(
        &nothing,
        &dir,
        &mut umicraft_modhost::TrustList::default(),
        |_, _| false,
    );
    let applied = apply(&host, Request::ServerCompat { enabled: true }).unwrap();
    assert!(applied.changed.is_empty() && applied.event.is_none());
    assert!(host.server_compat());
    // A press or a module switch for a mod that is not loaded is refused.
    let press = Request::Press {
        mod_id: "hello",
        key: "reset",
    };
    assert!(apply(&host, press).is_err());
    let module = Request::Module {
        mod_id: "hello",
        module: "radar",
        enabled: false,
    };
    assert!(apply(&host, module).is_err());
}

#[test]
fn a_module_card_routes_its_switch_and_settings_to_its_mod() {
    let id: ModId = "hello".parse().expect("valid id");
    let card = module_card_id(&id, "radar");
    assert_eq!(card, "ext-module:hello:radar");
    assert_eq!(parse(&card), None, "never read as a mod's card");
    let switch = ModsOutcome::Toggle {
        id: card.clone(),
        enabled: false,
    };
    let routed = route(&switch);
    assert!(!routed.builtin);
    assert_eq!(
        routed.external,
        Some(Request::Module {
            mod_id: "hello",
            module: "radar",
            enabled: false
        })
    );
    let set = ModsOutcome::Set {
        id: card.clone(),
        key: "range".into(),
        value: ModValue::Bool(true),
    };
    assert_eq!(
        route(&set).external,
        Some(Request::Setting {
            mod_id: "hello",
            key: "range",
            value: SettingValue::Bool(true)
        })
    );
}

#[test]
fn a_press_goes_to_the_mod_of_the_card_and_never_to_the_builtin_registry() {
    for id in ["ext:hello", "ext-module:hello:radar"] {
        let press = ModsOutcome::Press {
            id: id.into(),
            key: "reset".into(),
        };
        let routed = route(&press);
        assert!(!routed.builtin);
        assert_eq!(
            routed.external,
            Some(Request::Press {
                mod_id: "hello",
                key: "reset"
            })
        );
    }
    let builtin = ModsOutcome::Press {
        id: "fps".into(),
        key: "reset".into(),
    };
    assert_eq!(route(&builtin), Route::BUILTIN);
}

#[test]
fn a_button_setting_becomes_a_button_control() {
    let kind = SettingSpecKind::Button {
        text: "Reset".into(),
    };
    assert_eq!(
        control(&kind, &SettingValue::Bool(false)),
        SettingControl::Button("Reset".into())
    );
}

/// The shipped English texts, so the tests read what the player reads.
fn english() -> Lang {
    Lang::from_texts(
        "en_US",
        None,
        Some(include_str!(
            "../../../../umicraft-assets/client-pack/umicraft/lang/en_US.lang"
        )),
    )
}

fn untrusted(trusted: bool) -> UntrustedMod {
    UntrustedMod {
        id: "hello".parse().expect("valid id"),
        name: "Hello".to_owned(),
        version: "1.2.0".to_owned(),
        permissions: Permissions {
            movement: true,
            network_http: true,
            ..Permissions::default()
        },
        hash: "0123456789abcdef".repeat(4),
        trusted,
    }
}

#[test]
fn turning_an_untrusted_card_on_asks_to_trust_it_and_off_is_dropped() {
    let on = toggle("ext-untrusted:hello");
    let routed = route(&on);
    assert!(!routed.builtin);
    assert_eq!(routed.external, Some(Request::Trust { mod_id: "hello" }));
    let off = ModsOutcome::Toggle {
        id: "ext-untrusted:hello".into(),
        enabled: false,
    };
    assert_eq!(route(&off), Route::external(None));
    // Its id cannot be read as a loaded mod's card.
    assert_eq!(parse("ext-untrusted:hello"), None);
}

#[test]
fn the_untrusted_card_shows_the_hash_and_what_the_mod_asks_for() {
    let lang = english();
    let card = untrusted_card(&untrusted(false), &lang);
    assert_eq!(card.id, "ext-untrusted:hello");
    assert_eq!(card.name, "Hello");
    assert!(!card.enabled);
    assert!(card.description.contains("#0123456789ab"));
    assert!(card.description.contains(&"0123456789abcdef".repeat(4)));
    assert!(card.description.contains("v1.2.0"));
    assert!(untrusted_card(&untrusted(true), &lang).enabled);
}

#[test]
fn only_the_declared_permissions_are_listed() {
    let lang = english();
    let words = permission_words(untrusted(false).permissions, &lang);
    assert_eq!(words.len(), 2);
    assert!(permission_words(Permissions::default(), &lang).is_empty());
}

#[test]
fn the_input_permission_has_its_badge() {
    let lang = english();
    let input = Permissions {
        input: true,
        ..Permissions::default()
    };
    assert_eq!(permission_words(input, &lang), ["attack & use input"]);
}

#[test]
fn a_failure_reason_comes_after_the_details() {
    let lang = english();
    let text = details(
        "1.0.0",
        &["Ana".to_owned()],
        Permissions::default(),
        Some(&"f".repeat(64)),
        Some("panicked in its handler for event 3"),
        &lang,
    );
    assert!(text.starts_with("v1.0.0"));
    assert!(text.contains("#ffffffffffff |"));
    assert!(text.ends_with("panicked in its handler for event 3"));
}
