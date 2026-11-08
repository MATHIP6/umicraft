// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tests of the registry to menu bridge.

use umicraft_builtin_mods::{
    mods, BuiltinMod, ModCategory, ModId, ModMeta, SettingDef, SettingKind,
};

use super::*;

/// A mod with a slider: no built-in mod has one at the moment, but the menu must
/// keep handling them.
struct SliderMod;

const SLIDER: &[SettingDef] = &[SettingDef {
    key: "size",
    label_key: "umicraft.mods.test.size",
    kind: SettingKind::Slider {
        min: 0.0,
        max: 3.0,
        step: 1.0,
        default: 1.0,
    },
}];

impl BuiltinMod for SliderMod {
    fn meta(&self) -> &'static ModMeta {
        static META: ModMeta = ModMeta {
            id: ModId::new("slider_test"),
            name_key: "umicraft.mods.test.name",
            description_key: "umicraft.mods.test.desc",
            category: ModCategory::Visual,
            default_enabled: false,
        };
        &META
    }

    fn settings(&self) -> &'static [SettingDef] {
        SLIDER
    }
}

/// The built-in mods plus [`SliderMod`].
fn registry_with_slider() -> ModRegistry {
    let mut all = mods::all();
    all.push(Box::new(SliderMod));
    ModRegistry::new(all).expect("distinct ids")
}

fn registry() -> ModRegistry {
    ModRegistry::new(mods::all()).expect("the built-in mods have distinct ids")
}

fn lang() -> Lang {
    crate::language::load(Lang::DEFAULT_CODE)
}

fn card_of<'v>(view: &'v ModsView, id: &str) -> &'v ModCardView {
    view.cards.iter().find(|c| c.id == id).expect("listed")
}

fn setting<'v>(card: &'v ModCardView, key: &str) -> &'v SettingControl {
    &card
        .settings
        .iter()
        .find(|s| s.key == key)
        .expect("has the setting")
        .control
}

#[test]
fn every_mod_is_listed_with_translated_texts() {
    let view = build(&registry(), &lang());
    assert_eq!(view.cards.len(), mods::all().len());
    let fps = card_of(&view, "fps");
    assert_eq!(fps.name, "FPS");
    assert_ne!(fps.description, "umicraft.mods.fps.desc", "translated");
    assert_eq!(view.categories.len(), ModCategory::ALL.len());
    assert_eq!(view.categories[0], "HUD");
}

#[test]
fn cards_follow_the_order_of_their_categories() {
    let view = build(&registry(), &lang());
    let categories: Vec<_> = view.cards.iter().map(|c| c.category).collect();
    assert!(categories.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(view
        .cards
        .iter()
        .all(|c| c.category < view.categories.len()));
}

#[test]
fn settings_carry_their_kind_and_current_value() {
    let mut registry = registry();
    registry
        .set_setting("coordinates", "decimals", &SettingValue::Bool(false))
        .expect("known");
    registry
        .set_setting("cps", "mode", &SettingValue::Text("right".into()))
        .expect("known");
    let view = build(&registry, &lang());
    assert!(matches!(
        setting(card_of(&view, "coordinates"), "decimals"),
        SettingControl::Toggle(false)
    ));
    assert!(matches!(
        setting(card_of(&view, "coordinates"), "direction"),
        SettingControl::Toggle(true)
    ));
    assert!(matches!(
        setting(card_of(&view, "fps"), "color"),
        SettingControl::Color(hex) if hex.starts_with('#')
    ));
    let SettingControl::Choice { options, selected } = setting(card_of(&view, "cps"), "mode")
    else {
        panic!("cps mode is a choice");
    };
    assert_eq!(options[*selected].0, "right");
    assert_eq!(
        options[*selected].1, "Right clicks",
        "the option label is translated"
    );
}

#[test]
fn a_slider_setting_carries_its_range_and_value() {
    let mut registry = registry_with_slider();
    registry
        .set_setting("slider_test", "size", &SettingValue::Number(2.0))
        .expect("known");
    let view = build(&registry, &lang());
    assert!(matches!(
        setting(card_of(&view, "slider_test"), "size"),
        SettingControl::Slider { min, max, value, .. }
            if *min == 0.0 && *max == 3.0 && (*value - 2.0).abs() < 1e-6
    ));
}

#[test]
fn a_toggle_outcome_switches_the_mod_and_asks_to_save() {
    let mut registry = registry();
    let off = ModsOutcome::Toggle {
        id: "fps".into(),
        enabled: false,
    };
    assert_eq!(apply(&mut registry, &off), Ok(true));
    let view = build(&registry, &lang());
    assert!(!card_of(&view, "fps").enabled);
    assert_eq!(registry.config().0["fps"].enabled, Some(false));
}

#[test]
fn a_slider_outcome_out_of_bounds_is_clamped_by_the_registry() {
    let mut registry = registry_with_slider();
    let set = |value| ModsOutcome::Set {
        id: "slider_test".into(),
        key: "size".into(),
        value: ModValue::Number(value),
    };
    assert_eq!(apply(&mut registry, &set(99.0)), Ok(true));
    let view = build(&registry, &lang());
    let SettingControl::Slider { value, .. } = setting(card_of(&view, "slider_test"), "size")
    else {
        panic!("a slider");
    };
    assert!((*value - 3.0).abs() < 1e-6);
    assert_eq!(apply(&mut registry, &set(f32::NAN)), Ok(true));
}

#[test]
fn an_unknown_mod_or_setting_is_an_error_not_a_panic() {
    let mut registry = registry();
    let ghost = ModsOutcome::Toggle {
        id: "ghost".into(),
        enabled: true,
    };
    assert!(matches!(
        apply(&mut registry, &ghost),
        Err(RegistryError::UnknownMod(_))
    ));
    let bad_key = ModsOutcome::Set {
        id: "fps".into(),
        key: "nope".into(),
        value: ModValue::Bool(true),
    };
    assert!(matches!(
        apply(&mut registry, &bad_key),
        Err(RegistryError::Setting(_))
    ));
}

#[test]
fn closing_changes_nothing() {
    let mut registry = registry();
    let before = registry.config();
    assert_eq!(apply(&mut registry, &ModsOutcome::Close), Ok(false));
    assert_eq!(registry.config(), before);
}
