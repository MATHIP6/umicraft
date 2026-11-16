// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the registry, with two small test mods.

use std::fmt::Write;

use super::*;
use crate::hud::Anchor;
use crate::id::ModCategory;
use crate::settings::{SettingDef, SettingKind};
use crate::traits::HudSpec;

const SETTINGS: &[SettingDef] = &[SettingDef {
    key: "big",
    label_key: "k",
    kind: SettingKind::Toggle { default: false },
}];

/// A HUD mod that prints the fps.
struct Counter;

impl BuiltinMod for Counter {
    fn meta(&self) -> &'static ModMeta {
        static META: ModMeta = ModMeta {
            id: ModId::new("counter"),
            name_key: "n",
            description_key: "d",
            category: ModCategory::Hud,
            default_enabled: true,
        };
        &META
    }
    fn settings(&self) -> &'static [SettingDef] {
        SETTINGS
    }
    fn hud(&self) -> Option<HudSpec> {
        Some(HudSpec {
            default_placement: HudPlacement::at(Anchor::TopRight, -2.0, 2.0),
        })
    }
    fn draw(&self, ctx: &ModContext, _: &ModSettings, out: &mut HudContent) {
        write!(out.line(0xffffff), "{} FPS", ctx.fps).expect("string write");
    }
}

/// A behaviour mod that forces sprinting while moving forward.
struct AlwaysSprint;

impl BuiltinMod for AlwaysSprint {
    fn meta(&self) -> &'static ModMeta {
        static META: ModMeta = ModMeta {
            id: ModId::new("always_sprint"),
            name_key: "n",
            description_key: "d",
            category: ModCategory::Gameplay,
            default_enabled: false,
        };
        &META
    }
    fn modify_input(&mut self, input: &mut ModInput, _: &ModSettings) {
        input.sprint |= input.forward;
    }
}

fn registry() -> ModRegistry {
    ModRegistry::new(vec![Box::new(Counter), Box::new(AlwaysSprint)]).expect("distinct ids")
}

#[test]
fn mods_start_at_their_defaults() {
    let r = registry();
    let views: Vec<_> = r.mods().collect();
    assert!(views[0].enabled && !views[1].enabled);
    assert_eq!(views[0].placement.map(|p| p.anchor), Some(Anchor::TopRight));
    assert_eq!(views[1].placement, None, "a behaviour mod has no widget");
}

#[test]
fn a_duplicate_id_is_refused() {
    let result = ModRegistry::new(vec![Box::new(Counter), Box::new(Counter)]);
    assert_eq!(
        result.err(),
        Some(RegistryError::DuplicateId(ModId::new("counter")))
    );
}

#[test]
fn update_fills_the_widget_of_enabled_mods_only() {
    let mut r = registry();
    r.update(&ModContext {
        fps: 144,
        ..ModContext::default()
    });
    let widgets: Vec<_> = r.hud_widgets().collect();
    assert_eq!(widgets.len(), 1);
    assert_eq!(widgets[0].content.lines()[0].text, "144 FPS");
    r.set_enabled("counter", false).expect("known");
    assert_eq!(r.hud_widgets().count(), 0);
}

#[test]
fn the_widget_text_does_not_pile_up_across_frames() {
    let mut r = registry();
    r.update(&ModContext {
        fps: 1,
        ..ModContext::default()
    });
    r.update(&ModContext {
        fps: 2,
        ..ModContext::default()
    });
    let lines = r.hud_widgets().next().expect("one widget").content.lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, "2 FPS");
}

#[test]
fn input_is_rewritten_only_by_enabled_mods() {
    let mut r = registry();
    let mut input = ModInput {
        forward: true,
        sprint: false,
    };
    r.modify_input(&mut input);
    assert!(!input.sprint, "disabled by default");
    r.set_enabled("always_sprint", true).expect("known");
    r.modify_input(&mut input);
    assert!(input.sprint);
}

#[test]
fn unknown_mods_and_settings_are_errors() {
    let mut r = registry();
    assert_eq!(
        r.set_enabled("nope", true),
        Err(RegistryError::UnknownMod("nope".into()))
    );
    assert_eq!(
        r.set_setting("counter", "nope", &SettingValue::Bool(true)),
        Err(RegistryError::Setting(SettingsError::UnknownKey(
            "nope".into()
        )))
    );
}

#[test]
fn a_placement_is_sanitized_and_can_be_reset() {
    let mut r = registry();
    let wild = HudPlacement {
        scale: 50.0,
        ..HudPlacement::default()
    };
    r.set_placement("counter", wild).expect("known");
    assert!(r
        .mods()
        .next()
        .and_then(|v| v.placement)
        .is_some_and(|p| p.scale <= 3.0));
    r.reset_placement("counter").expect("known");
    assert_eq!(
        r.mods().next().and_then(|v| v.placement).map(|p| p.anchor),
        Some(Anchor::TopRight)
    );
}

#[test]
fn the_state_survives_a_save_and_load() {
    let mut r = registry();
    r.set_enabled("always_sprint", true).expect("known");
    r.set_setting("counter", "big", &SettingValue::Bool(true))
        .expect("known");
    r.set_placement("counter", HudPlacement::at(Anchor::BottomLeft, 3.0, -3.0))
        .expect("known");
    let saved = r.config();

    let mut fresh = registry();
    fresh.apply_config(&saved);
    assert_eq!(fresh.config(), saved);
    let counter = fresh.mods().next().expect("counter");
    assert!(counter.settings.bool("big"));
    assert_eq!(
        counter.placement.map(|p| p.anchor),
        Some(Anchor::BottomLeft)
    );
}

#[test]
fn saved_tables_of_unknown_mods_are_preserved() {
    let mut r = registry();
    let mut saved = ModsConfig::default();
    saved.0.insert(
        "from_the_future".into(),
        ModConfig {
            enabled: Some(true),
            ..ModConfig::default()
        },
    );
    r.apply_config(&saved);
    assert!(r.config().0.contains_key("from_the_future"));
}

#[test]
fn a_corrupt_saved_value_does_not_break_the_registry() {
    let mut r = registry();
    let mut saved = ModsConfig::default();
    saved.0.insert(
        "counter".into(),
        ModConfig {
            enabled: None,
            settings: [("big".to_owned(), SettingValue::Number(9.0))].into(),
            placement: Some(HudPlacement {
                scale: f32::NAN,
                ..HudPlacement::default()
            }),
            ..ModConfig::default()
        },
    );
    r.apply_config(&saved);
    let counter = r.mods().next().expect("counter");
    assert!(!counter.settings.bool("big"));
    assert!(counter.enabled, "never chosen -> the default");
    assert!(counter.placement.is_some_and(|p| p.scale.is_finite()));
}

fn placement_of(r: &ModRegistry, id: &str) -> Option<HudPlacement> {
    r.mods().find(|m| m.meta.id.as_str() == id)?.placement
}

fn enabled(r: &ModRegistry, id: &str) -> bool {
    r.mods().any(|m| m.meta.id.as_str() == id && m.enabled)
}

/// The real mods, so the presets are checked against the widgets that exist.
fn real() -> ModRegistry {
    ModRegistry::new(crate::mods::all()).expect("the built-in mods")
}

#[test]
fn a_preset_turns_widgets_on_and_off_and_places_them() {
    let mut r = real();
    r.apply_preset(HudPreset::Pvp);
    assert!(enabled(&r, "fps") && enabled(&r, "cps") && enabled(&r, "ping"));
    assert!(!enabled(&r, "coordinates"));
    assert_eq!(
        placement_of(&r, "cps").map(|p| p.anchor),
        Some(Anchor::TopRight)
    );
    r.apply_preset(HudPreset::Minimal);
    assert!(enabled(&r, "fps") && !enabled(&r, "cps") && !enabled(&r, "ping"));
    // A preset leaves the mods without a widget alone.
    let zoom_before = enabled(&r, "zoom");
    r.apply_preset(HudPreset::Pvp);
    assert_eq!(enabled(&r, "zoom"), zoom_before);
}

#[test]
fn the_default_preset_is_the_layout_of_a_fresh_registry() {
    let mut r = real();
    r.apply_preset(HudPreset::Minimal);
    r.apply_preset(HudPreset::Default);
    let fresh = real();
    for id in ["fps", "cps", "ping", "coordinates"] {
        assert_eq!(placement_of(&r, id), placement_of(&fresh, id), "{id}");
        assert_eq!(enabled(&r, id), enabled(&fresh, id), "{id}");
    }
}

#[test]
fn the_custom_preset_is_the_saved_layout_and_survives_a_restart() {
    let mut r = real();
    assert!(!r.has_custom());
    // Nothing saved: the preset changes nothing.
    let before = r.config();
    r.apply_preset(HudPreset::Custom);
    assert_eq!(r.config(), before);

    let moved = HudPlacement::at(Anchor::BottomRight, -7.0, -9.0);
    r.set_placement("fps", moved).expect("fps");
    r.set_enabled("cps", true).expect("cps");
    r.save_custom();
    assert!(r.has_custom());
    r.apply_preset(HudPreset::Minimal);
    assert_ne!(placement_of(&r, "fps"), Some(moved));

    // Through the saved file, as after a restart.
    let text = toml::to_string(&r.config()).expect("serializes");
    let mut restarted = real();
    restarted.apply_config(&toml::from_str(&text).expect("parses"));
    assert!(restarted.has_custom());
    restarted.apply_preset(HudPreset::Custom);
    assert_eq!(placement_of(&restarted, "fps"), Some(moved));
    assert!(enabled(&restarted, "cps"));
}

#[test]
fn a_locked_widget_keeps_its_place_through_presets_and_restarts() {
    let mut r = real();
    let here = HudPlacement::at(Anchor::Center, 3.0, 3.0);
    r.set_placement("fps", here).expect("fps");
    r.set_locked("fps", true).expect("fps");
    r.apply_preset(HudPreset::Pvp);
    r.apply_preset(HudPreset::Default);
    assert_eq!(placement_of(&r, "fps"), Some(here));

    let text = toml::to_string(&r.config()).expect("serializes");
    let mut restarted = real();
    restarted.apply_config(&toml::from_str(&text).expect("parses"));
    assert!(restarted
        .mods()
        .any(|m| m.meta.id.as_str() == "fps" && m.locked));
    assert!(r.set_locked("nope", true).is_err());
}

/// What undo relies on: a config taken earlier brings back the placement, the
/// switch, the settings and the lock.
#[test]
fn a_restored_config_brings_back_every_part_of_the_layout() {
    let mut r = real();
    let before = r.config();
    r.set_placement("fps", HudPlacement::at(Anchor::Center, 0.0, 0.0))
        .expect("fps");
    r.set_enabled("fps", false).expect("fps");
    r.set_locked("fps", true).expect("fps");
    r.set_setting("fps", "background", &SettingValue::Bool(false))
        .expect("background");
    assert_ne!(r.config(), before);
    r.apply_config(&before);
    assert_eq!(r.config(), before);
}
