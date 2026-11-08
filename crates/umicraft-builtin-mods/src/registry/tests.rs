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
        },
    );
    r.apply_config(&saved);
    let counter = r.mods().next().expect("counter");
    assert!(!counter.settings.bool("big"));
    assert!(counter.enabled, "never chosen -> the default");
    assert!(counter.placement.is_some_and(|p| p.scale.is_finite()));
}
