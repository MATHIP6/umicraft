// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use umicraft_builtin_mods::{HudPlacement, SettingValue};
use umicraft_modapi_sys::{
    UmiModuleDef, UmiSettingDef, UmiSettingKind, UmiStatus, UmiStr, UMI_ATTACH_HUD_WIDGET,
    UMI_ATTACH_POST_PASS, UMI_MODULE_DEFAULT_ON,
};

use super::*;
use crate::ffi::{handle_of, host_table};
use crate::ui::config::ExternalModConfig;
use crate::ui::shared::HostShared;
use crate::ui::testing::{draw_lines, fresh, toggle, user};
use crate::ui::view::ModView;
use crate::ui::{MAX_CATEGORIES, MAX_MODULES};

const GREY: &str = "@fragment
fn fs_main(in: UmiVertexOut) -> @location(0) vec4<f32> {
    return textureSample(umi_scene, umi_sampler, in.uv);
}";

fn text(s: &str) -> UmiStr {
    UmiStr {
        ptr: s.as_ptr(),
        len: s.len(),
    }
}

fn context() -> Arc<ModContext> {
    fresh("suite", &Arc::<HostShared>::default())
}

fn module<'a>(key: &'a str, category: &'a str) -> ModuleDef<'a> {
    ModuleDef {
        key,
        label: "Module",
        description: "",
        category,
        default_on: true,
    }
}

fn raw_module(key: &str, category: &str, flags: u32) -> UmiModuleDef {
    UmiModuleDef {
        size: size_of::<UmiModuleDef>() as u32,
        flags,
        key: text(key),
        label: text("Radar"),
        description: text("Shows players around"),
        category: text(category),
    }
}

fn view(cx: &ModContext) -> ModView<'_> {
    static MANIFEST: std::sync::OnceLock<crate::manifest::Manifest> = std::sync::OnceLock::new();
    let manifest = MANIFEST.get_or_init(|| {
        crate::manifest::Manifest::parse(
            "[mod]\nid = \"suite\"\nname = \"Suite\"\nversion = \"1.0.0\"\napi_version = \"1.2\"\nauthors = []\n",
        )
        .unwrap()
    });
    ModView::new(manifest, "00", cx)
}

#[test]
fn categories_and_modules_register_through_the_host_table() {
    let (cx, host) = (context(), host_table());
    let me = handle_of(&cx);
    assert_eq!(
        (host.register_category)(me, text("pvp"), text("PvP")),
        UmiStatus::OK
    );
    let radar = raw_module("radar", "pvp", UMI_MODULE_DEFAULT_ON);
    assert_eq!((host.register_module)(me, &raw const radar), UmiStatus::OK);
    let plain = raw_module("plain", "", 0);
    assert_eq!((host.register_module)(me, &raw const plain), UmiStatus::OK);
    cx.finish_init(true);

    let modules: Vec<_> = view(&cx).modules().collect();
    assert_eq!(modules.len(), 2);
    assert_eq!(
        (modules[0].key, modules[0].label, modules[0].description),
        ("radar", "Radar", "Shows players around")
    );
    assert_eq!(modules[0].category, Some("PvP"));
    assert!(modules[0].enabled);
    assert_eq!(modules[1].category, None, "no category: the External tab");
    assert!(!modules[1].enabled, "flags 0: off by default");

    let mut out = 7;
    assert_eq!(
        (host.module_enabled)(me, text("radar"), &raw mut out),
        UmiStatus::OK
    );
    assert_eq!(out, 1);
    assert_eq!(
        (host.module_enabled)(me, text("nope"), &raw mut out),
        UmiStatus::INVALID_ARGUMENT
    );
    assert_eq!(
        (host.module_enabled)(me, text("radar"), core::ptr::null_mut()),
        UmiStatus::INVALID_ARGUMENT
    );
}

#[test]
fn bad_declarations_are_refused_with_the_documented_status() {
    let (cx, host) = (context(), host_table());
    let me = handle_of(&cx);
    let register = |key: &str, label: &str| (host.register_category)(me, text(key), text(label));
    assert_eq!(register("Bad", "x"), UmiStatus::INVALID_ARGUMENT);
    assert_eq!(register("ok", ""), UmiStatus::INVALID_ARGUMENT);
    assert_eq!(register("ok", &"x".repeat(33)), UmiStatus::INVALID_ARGUMENT);
    for index in 0..MAX_CATEGORIES {
        assert_eq!(register(&format!("c{index}"), "Tab"), UmiStatus::OK);
    }
    assert_eq!(register("c0", "Tab"), UmiStatus::DENIED, "the limit first");

    let unknown_category = raw_module("radar", "missing", 0);
    assert_eq!(
        (host.register_module)(me, &raw const unknown_category),
        UmiStatus::INVALID_ARGUMENT
    );
    let small = UmiModuleDef {
        size: 4,
        ..raw_module("radar", "", 0)
    };
    assert_eq!(
        (host.register_module)(me, &raw const small),
        UmiStatus::INVALID_ARGUMENT
    );
    assert_eq!(
        (host.register_module)(me, core::ptr::null()),
        UmiStatus::INVALID_ARGUMENT
    );
    let radar = raw_module("radar", "c1", 0);
    assert_eq!((host.register_module)(me, &raw const radar), UmiStatus::OK);
    assert_eq!(
        (host.register_module)(me, &raw const radar),
        UmiStatus::CONFLICT
    );
    for index in 1..MAX_MODULES {
        cx.register_module(&module(&format!("m{index}"), ""))
            .unwrap();
    }
    assert_eq!(
        cx.register_module(&module("extra", ""))
            .unwrap_err()
            .status(),
        UmiStatus::DENIED
    );

    cx.finish_init(true);
    assert_eq!(register("late", "Late"), UmiStatus::DENIED);
}

#[test]
fn settings_and_buttons_sit_on_the_card_they_name() {
    let (cx, host) = (context(), host_table());
    let me = handle_of(&cx);
    cx.register_module(&module("radar", "")).unwrap();
    cx.register_setting(toggle("global")).unwrap();
    let def = UmiSettingDef {
        size: size_of::<UmiSettingDef>() as u32,
        kind: UmiSettingKind::TOGGLE,
        key: text("range"),
        label: text("Range"),
        toggle_default: 1,
        slider_min: 0.0,
        slider_max: 0.0,
        slider_step: 0.0,
        slider_default: 0.0,
        choices: core::ptr::null(),
        choices_len: 0,
        choice_default: 0,
        color_default: 0,
    };
    let into = |module: &str| (host.register_setting_in)(me, text(module), &raw const def);
    assert_eq!(into("missing"), UmiStatus::INVALID_ARGUMENT);
    assert_eq!(into("radar"), UmiStatus::OK);
    assert_eq!(
        into(""),
        UmiStatus::CONFLICT,
        "keys are unique across the mod"
    );
    let button = |module: &str, key: &str, label: &str, caption: &str| {
        (host.register_button)(me, text(module), text(key), text(label), text(caption))
    };
    assert_eq!(button("radar", "reset", "Counters", "Reset"), UmiStatus::OK);
    assert_eq!(button("", "reload", "List", "Reload"), UmiStatus::OK);
    assert_eq!(button("", "empty", "List", ""), UmiStatus::INVALID_ARGUMENT);
    assert_eq!(
        button("", "range", "Again", "Go"),
        UmiStatus::CONFLICT,
        "a button shares the settings' keys"
    );
    cx.finish_init(true);

    let keys = |settings: Vec<crate::SettingView<'_>>| {
        settings
            .iter()
            .map(|s| s.spec.key.clone())
            .collect::<Vec<_>>()
    };
    let mod_card = keys(view(&cx).settings().collect());
    assert_eq!(mod_card, ["global", "reload"]);
    let radar = view(&cx).modules().next().unwrap();
    assert_eq!(keys(radar.settings().collect()), ["range", "reset"]);

    assert!(cx.setting_get("range").is_some());
    assert_eq!(cx.setting_get("reset"), None, "a button has no value");
    assert_eq!(cx.button_module("reset").unwrap(), "radar");
    assert_eq!(cx.button_module("reload").unwrap(), "");
    assert!(cx.button_module("range").is_err(), "a toggle is no button");
}

#[test]
fn an_attached_widget_or_pass_follows_its_module() {
    let (cx, host) = (context(), host_table());
    let me = handle_of(&cx);
    cx.register_module(&module("radar", "")).unwrap();
    cx.register_widget(
        "map",
        "Map",
        HudPlacement::default(),
        Some(draw_lines),
        user(),
    )
    .unwrap();
    cx.register_widget(
        "free",
        "Free",
        HudPlacement::default(),
        Some(draw_lines),
        user(),
    )
    .unwrap();
    cx.register_post_pass("grey", "Grey", GREY).unwrap();
    let attach =
        |module: &str, kind, key: &str| (host.module_attach)(me, text(module), kind, text(key));
    assert_eq!(attach("radar", UMI_ATTACH_HUD_WIDGET, "map"), UmiStatus::OK);
    assert_eq!(attach("radar", UMI_ATTACH_POST_PASS, "grey"), UmiStatus::OK);
    assert_eq!(
        attach("radar", UMI_ATTACH_HUD_WIDGET, "map"),
        UmiStatus::CONFLICT
    );
    assert_eq!(
        attach("radar", UMI_ATTACH_HUD_WIDGET, "nope"),
        UmiStatus::INVALID_ARGUMENT
    );
    assert_eq!(
        attach("nope", UMI_ATTACH_HUD_WIDGET, "free"),
        UmiStatus::INVALID_ARGUMENT
    );
    assert_eq!(attach("radar", 9, "free"), UmiStatus::INVALID_ARGUMENT);
    cx.finish_init(true);

    let drawn = |cx: &ModContext| {
        cx.draw_widgets();
        let mut keys = Vec::new();
        cx.visit_widgets(|widget| keys.push(widget.key.to_owned()));
        keys
    };
    assert_eq!(drawn(&cx), ["map", "free"]);
    assert!(cx.post_passes().all(|pass| pass.enabled));

    assert_eq!(cx.set_module("radar", false), Ok(true));
    assert_eq!(cx.set_module("radar", false), Ok(false), "no change");
    assert_eq!(drawn(&cx), ["free"]);
    assert!(cx.post_passes().all(|pass| !pass.enabled));
    let shown: Vec<_> = view(&cx).widgets().map(|w| (w.key, w.shown)).collect();
    assert_eq!(shown, [("map", false), ("free", true)]);
    assert!(cx.set_module("nope", true).is_err());
}

#[test]
fn module_switches_round_trip_through_the_config_and_buttons_are_not_saved() {
    let cx = context();
    cx.register_module(&module("radar", "")).unwrap();
    cx.register_button("radar", "reset", "Counters", "Reset")
        .unwrap();
    cx.finish_init(true);
    cx.set_module("radar", false).unwrap();

    let config = cx.config();
    assert_eq!(config.modules.get("radar"), Some(&false));
    assert!(!config.settings.contains_key("reset"));

    let other = context();
    other.register_module(&module("radar", "")).unwrap();
    other.finish_init(true);
    let mut saved = config.clone();
    saved.modules.insert("gone".to_owned(), true);
    other.apply_config(&saved);
    assert_eq!(other.module_enabled("radar"), Some(false));
    assert_eq!(
        other.config().modules.get("gone"),
        Some(&true),
        "an unknown module's switch is kept"
    );
    assert_eq!(ExternalModConfig::default().modules.len(), 0);
    let _ = SettingValue::Bool(false);
}
