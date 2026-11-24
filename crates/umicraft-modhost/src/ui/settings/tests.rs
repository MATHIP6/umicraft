// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

fn spec(kind: SettingSpecKind) -> SettingSpec {
    SettingSpec {
        key: "size".to_owned(),
        label: "Size".to_owned(),
        kind,
    }
}

fn slider(min: f32, max: f32, step: f32, default: f32) -> SettingSpec {
    spec(SettingSpecKind::Slider {
        min,
        max,
        step,
        default,
    })
}

fn choice(options: &[&str], default: usize) -> SettingSpec {
    spec(SettingSpecKind::Choice {
        options: options.iter().map(|o| (*o).to_owned()).collect(),
        default,
    })
}

#[test]
fn a_valid_spec_of_every_kind_passes() {
    assert!(spec(SettingSpecKind::Toggle { default: true })
        .validate()
        .is_ok());
    assert!(slider(0.0, 10.0, 0.5, 5.0).validate().is_ok());
    assert!(choice(&["a", "b"], 1).validate().is_ok());
    assert!(spec(SettingSpecKind::Color {
        default: 0xff_ff_ff
    })
    .validate()
    .is_ok());
}

#[test]
fn each_invalid_spec_is_refused() {
    let nan = f32::NAN;
    let invalid = [
        slider(1.0, 1.0, 0.5, 1.0),
        slider(2.0, 1.0, 0.5, 1.5),
        slider(0.0, 1.0, 0.0, 0.5),
        slider(0.0, 1.0, -1.0, 0.5),
        slider(0.0, 1.0, 0.1, 2.0),
        slider(0.0, 1.0, 0.1, -0.5),
        slider(nan, 1.0, 0.1, 0.5),
        slider(0.0, f32::INFINITY, 0.1, 0.5),
        choice(&[], 0),
        choice(&["a"], 1),
        choice(&vec!["x"; 33], 0),
        choice(&[""], 0),
        spec(SettingSpecKind::Color {
            default: 0x1_00_00_00,
        }),
        SettingSpec {
            key: "Bad Key".to_owned(),
            ..spec(SettingSpecKind::Toggle { default: false })
        },
        SettingSpec {
            label: String::new(),
            ..spec(SettingSpecKind::Toggle { default: false })
        },
        SettingSpec {
            label: "x".repeat(MAX_LABEL_BYTES + 1),
            ..spec(SettingSpecKind::Toggle { default: false })
        },
    ];
    for spec in invalid {
        assert!(spec.validate().is_err(), "{spec:?}");
    }
    assert!(choice(&vec!["x"; 32], 31).validate().is_ok());
}

#[test]
fn a_slider_value_is_clamped_and_snapped() {
    let slot = SettingSlot::new(slider(0.0, 10.0, 0.5, 5.0), None);
    slot.set(&SettingValue::Number(99.0));
    assert_eq!(slot.value(), SettingValue::Number(10.0));
    slot.set(&SettingValue::Number(2.3));
    assert_eq!(slot.value(), SettingValue::Number(2.5));
    assert!((slot.abi_value().unwrap().slider - 2.5).abs() < f32::EPSILON);
    assert_eq!(slot.abi_value().unwrap().kind, UmiSettingKind::SLIDER);
}

#[test]
fn bad_values_fall_back_to_the_default() {
    let slot = SettingSlot::new(choice(&["a", "b"], 1), None);
    slot.set(&SettingValue::Text("a".to_owned()));
    assert_eq!(slot.abi_value().unwrap().choice, 0);
    slot.set(&SettingValue::Text("zzz".to_owned()));
    assert_eq!(slot.value(), SettingValue::Text("b".to_owned()));
    slot.set(&SettingValue::Bool(true));
    assert_eq!(slot.abi_value().unwrap().choice, 1);

    let number = SettingSlot::new(slider(0.0, 1.0, 0.1, 0.5), None);
    number.set(&SettingValue::Number(f32::NAN));
    assert_eq!(number.value(), SettingValue::Number(0.5));
}

#[test]
fn colours_and_toggles_round_trip() {
    let colour = SettingSlot::new(
        spec(SettingSpecKind::Color {
            default: 0x00_ff_00,
        }),
        None,
    );
    assert_eq!(colour.value(), SettingValue::Text("#00ff00".to_owned()));
    colour.set(&SettingValue::Text("#1A2b3C".to_owned()));
    assert_eq!(colour.value(), SettingValue::Text("#1a2b3c".to_owned()));
    assert_eq!(colour.abi_value().unwrap().color, 0x1a_2b_3c);
    colour.set(&SettingValue::Text("red".to_owned()));
    assert_eq!(colour.abi_value().unwrap().color, 0x00_ff_00);

    let toggle = SettingSlot::new(spec(SettingSpecKind::Toggle { default: true }), None);
    assert_eq!(toggle.abi_value().unwrap().toggle, 1);
    toggle.set(&SettingValue::Bool(false));
    assert_eq!(toggle.value(), SettingValue::Bool(false));
    assert_eq!(toggle.abi_value().unwrap().kind, UmiSettingKind::TOGGLE);
}

#[test]
fn sanitize_matches_the_builtin_rules() {
    let spec = slider(0.0, 10.0, 0.5, 5.0);
    assert_eq!(
        spec.sanitize(&SettingValue::Number(-3.0)),
        SettingValue::Number(0.0)
    );
    assert_eq!(spec.default_value(), SettingValue::Number(5.0));
}

#[test]
fn a_button_needs_a_text_and_has_no_value() {
    let button = |text: &str| {
        spec(SettingSpecKind::Button {
            text: text.to_owned(),
        })
    };
    assert!(button("Reset").validate().is_ok());
    assert!(button("").validate().is_err());
    assert!(button(&"x".repeat(33)).validate().is_err());
    let slot = SettingSlot::new(button("Reset"), Some(0));
    assert!(slot.spec().is_button());
    assert_eq!(slot.module(), Some(0));
    assert_eq!(slot.abi_value(), None, "setting_get does not know a button");
    slot.set(&SettingValue::Bool(true));
    assert_eq!(slot.value(), SettingValue::Bool(false));
}
