// SPDX-License-Identifier: MIT OR Apache-2.0

use umicraft_builtin_mods::{Anchor, MAX_SCALE};
use umicraft_modapi_sys::UmiSettingKind;

use super::*;
use crate::ui::settings::{SettingSpec, SettingSpecKind};
use crate::ui::testing::{draw_lines, user};

fn spec(key: &str, kind: SettingSpecKind) -> SettingSpec {
    SettingSpec {
        key: key.to_owned(),
        label: key.to_owned(),
        kind,
    }
}

/// A mod with a slider, a choice, a toggle, a widget and a key binding.
fn declared() -> std::sync::Arc<ModContext> {
    let cx = ModContext::new("m".to_owned().try_into().unwrap());
    cx.register_setting(spec(
        "size",
        SettingSpecKind::Slider {
            min: 0.0,
            max: 10.0,
            step: 0.5,
            default: 5.0,
        },
    ))
    .unwrap();
    cx.register_setting(spec(
        "style",
        SettingSpecKind::Choice {
            options: vec!["a".to_owned(), "b".to_owned()],
            default: 1,
        },
    ))
    .unwrap();
    cx.register_setting(spec("on", SettingSpecKind::Toggle { default: false }))
        .unwrap();
    cx.register_widget(
        "main",
        "Main",
        HudPlacement::at(Anchor::TopLeft, 0.0, 0.0),
        Some(draw_lines),
        user(),
    )
    .unwrap();
    cx.register_key("toggle", "Toggle", "KeyG").unwrap();
    cx.finish_init(true);
    cx
}

fn number(cx: &ModContext, key: &str) -> f32 {
    cx.setting_get(key).unwrap().slider
}

#[test]
fn applying_a_config_sanitises_every_value() {
    let cx = declared();
    let config = ExternalModConfig {
        settings: BTreeMap::from([
            ("size".to_owned(), SettingValue::Number(99.0)),
            ("style".to_owned(), SettingValue::Text("nope".to_owned())),
            ("on".to_owned(), SettingValue::Number(1.0)),
        ]),
        placements: BTreeMap::from([(
            "main".to_owned(),
            HudPlacement {
                scale: 50.0,
                ..HudPlacement::at(Anchor::Center, 4.0, 5.0)
            },
        )]),
        keys: BTreeMap::from([("toggle".to_owned(), "x".repeat(100))]),
        ..ExternalModConfig::default()
    };

    cx.apply_config(&config);

    assert!((number(&cx, "size") - 10.0).abs() < f32::EPSILON, "clamped");
    let style = cx.setting_get("style").unwrap();
    assert_eq!(
        (style.kind, style.choice),
        (UmiSettingKind::CHOICE, 1),
        "default"
    );
    assert_eq!(
        cx.setting_get("on").unwrap().toggle,
        0,
        "wrong type: default"
    );
    let saved = cx.config();
    assert_eq!(saved.placements["main"].scale, MAX_SCALE);
    assert_eq!(saved.placements["main"].anchor, Anchor::Center);
    assert_eq!(
        saved.keys["toggle"], "KeyG",
        "an over-long name restores the default"
    );
}

#[test]
fn set_setting_changes_what_setting_get_returns() {
    let cx = declared();
    cx.set_setting("size", &SettingValue::Number(2.3)).unwrap();
    assert!(
        (number(&cx, "size") - 2.5).abs() < f32::EPSILON,
        "snapped to the step"
    );
    cx.set_setting("style", &SettingValue::Text("a".to_owned()))
        .unwrap();
    assert_eq!(cx.setting_get("style").unwrap().choice, 0);
    assert_eq!(
        cx.set_setting("nope", &SettingValue::Bool(true)),
        Err(ConfigureError::UnknownKey {
            kind: "setting",
            key: "nope".to_owned()
        })
    );
}

#[test]
fn placements_and_keys_have_their_own_setters() {
    let cx = declared();
    cx.set_placement("main", HudPlacement::at(Anchor::BottomRight, 1.0, 1.0))
        .unwrap();
    cx.set_key("toggle", "Mouse4").unwrap();
    let config = cx.config();
    assert_eq!(config.placements["main"].anchor, Anchor::BottomRight);
    assert_eq!(config.keys["toggle"], "Mouse4");
    assert!(cx.set_placement("nope", HudPlacement::default()).is_err());
    assert!(cx.set_key("nope", "KeyA").is_err());
}

#[test]
fn unknown_keys_survive_a_round_trip() {
    let cx = declared();
    let config = ExternalModConfig {
        settings: BTreeMap::from([("from_the_future".to_owned(), SettingValue::Number(3.0))]),
        placements: BTreeMap::from([("old_widget".to_owned(), HudPlacement::default())]),
        keys: BTreeMap::from([("old_key".to_owned(), "KeyZ".to_owned())]),
        ..ExternalModConfig::default()
    };

    cx.apply_config(&config);
    let saved = cx.config();

    assert_eq!(saved.settings["from_the_future"], SettingValue::Number(3.0));
    assert!(saved.placements.contains_key("old_widget"));
    assert_eq!(saved.keys["old_key"], "KeyZ");
    assert!(
        saved.settings.contains_key("size"),
        "declared ones are there too"
    );
}

#[test]
fn a_config_saved_to_disk_restores_the_same_state() {
    let cx = declared();
    cx.set_setting("size", &SettingValue::Number(7.5)).unwrap();
    cx.set_key("toggle", "KeyH").unwrap();
    cx.set_wanted(false);
    let path = std::env::temp_dir().join(format!("umi-modcfg-restore-{}.toml", std::process::id()));
    cx.config().save(&path).unwrap();

    let fresh = declared();
    fresh.apply_config(&ExternalModConfig::load(&path).unwrap());
    std::fs::remove_file(&path).unwrap();

    assert_eq!(fresh.config(), cx.config());
    assert!(!fresh.is_wanted());
}

#[test]
fn the_enabled_field_switches_the_mod_unless_it_failed() {
    let cx = declared();
    let off = ExternalModConfig {
        enabled: Some(false),
        ..ExternalModConfig::default()
    };
    cx.apply_config(&off);
    assert!(!cx.is_enabled());
    cx.apply_config(&ExternalModConfig {
        enabled: Some(true),
        ..ExternalModConfig::default()
    });
    assert!(cx.is_enabled());
    cx.apply_config(&ExternalModConfig::default());
    assert!(cx.is_enabled(), "no `enabled` keeps the switch");

    cx.fail("test");
    cx.apply_config(&ExternalModConfig {
        enabled: Some(true),
        ..ExternalModConfig::default()
    });
    assert!(!cx.is_enabled());
    assert_eq!(cx.config().enabled, Some(true), "the player's wish is kept");
}
