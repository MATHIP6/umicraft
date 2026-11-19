// SPDX-License-Identifier: MIT OR Apache-2.0

use umicraft_builtin_mods::Anchor;
use umicraft_modapi_sys::{UmiSettingKind, UmiStatus};

use super::*;
use crate::ui::settings::SettingSpecKind;
use crate::ui::shared::HostShared;
use crate::ui::testing::{draw_lines, fresh, run_ok, toggle, user};
use crate::ui::{MAX_COMMANDS, MAX_KEYS, MAX_SETTINGS, MAX_WIDGETS, OUTBOX_CAPACITY};

fn context() -> std::sync::Arc<ModContext> {
    ModContext::new("m".to_owned().try_into().unwrap())
}

#[test]
fn a_setting_is_readable_during_init_and_after_without_changes() {
    let cx = context();
    cx.register_setting(toggle("on")).unwrap();
    let during = cx.setting_get("on").unwrap();
    assert_eq!(during.kind, UmiSettingKind::TOGGLE);
    assert_eq!(during.toggle, 0);
    cx.finish_init(true);
    assert_eq!(cx.setting_get("on"), Some(during));
    assert_eq!(cx.setting_get("missing"), None);
}

#[test]
fn registrations_after_init_are_denied_whatever_they_are() {
    let cx = context();
    cx.finish_init(true);

    let results = [
        cx.register_setting(toggle("on")),
        cx.register_widget("w", "W", HudPlacement::default(), Some(draw_lines), user()),
        cx.register_key("k", "K", "KeyG"),
        cx.register_command("c", "", Some(run_ok), user()),
    ];

    for result in results {
        let error = result.unwrap_err();
        assert_eq!(error, DeclareError::Late);
        assert_eq!(error.status(), UmiStatus::DENIED);
    }
    assert_eq!(cx.setting_get("on"), None);
}

#[test]
fn duplicates_and_limits_are_refused_per_kind() {
    let cx = context();
    cx.register_setting(toggle("on")).unwrap();
    cx.register_key("k", "K", "").unwrap();
    assert!(matches!(
        cx.register_setting(toggle("on")),
        Err(DeclareError::Duplicate { .. })
    ));
    assert!(matches!(
        cx.register_key("k", "Again", ""),
        Err(DeclareError::Duplicate { .. })
    ));
    // Settings, widgets and keys have separate namespaces.
    cx.register_widget(
        "on",
        "On",
        HudPlacement::default(),
        Some(draw_lines),
        user(),
    )
    .unwrap();

    for n in 1..MAX_SETTINGS {
        cx.register_setting(toggle(&format!("s{n}"))).unwrap();
    }
    let too_many = cx.register_setting(toggle("one_more")).unwrap_err();
    assert!(matches!(too_many, DeclareError::TooMany { .. }));
    assert_eq!(too_many.status(), UmiStatus::DENIED);

    for n in 1..MAX_WIDGETS {
        let key = format!("w{n}");
        cx.register_widget(&key, "W", HudPlacement::default(), Some(draw_lines), user())
            .unwrap();
    }
    assert!(cx
        .register_widget(
            "w_more",
            "W",
            HudPlacement::default(),
            Some(draw_lines),
            user()
        )
        .is_err());
    for n in 1..MAX_KEYS {
        cx.register_key(&format!("k{n}"), "K", "").unwrap();
    }
    assert!(cx.register_key("k_more", "K", "").is_err());
}

#[test]
fn invalid_declarations_report_invalid_argument() {
    let cx = context();
    let bad_slider = SettingSpec {
        kind: SettingSpecKind::Slider {
            min: 1.0,
            max: 0.0,
            step: 1.0,
            default: 0.5,
        },
        ..toggle("slider")
    };
    let errors = [
        cx.register_setting(bad_slider).unwrap_err(),
        cx.register_key("Bad", "K", "").unwrap_err(),
        cx.register_key("ok", "K", &"x".repeat(33)).unwrap_err(),
        cx.register_command("Bad", "", Some(run_ok), user())
            .unwrap_err(),
        cx.register_command("ok", &"u".repeat(257), Some(run_ok), user())
            .unwrap_err(),
        cx.register_command("ok", "", None, user()).unwrap_err(),
    ];
    for error in errors {
        assert_eq!(error.status(), UmiStatus::INVALID_ARGUMENT, "{error}");
    }
    assert_eq!(cx.report(Err(DeclareError::Late)), UmiStatus::DENIED);
    assert_eq!(cx.report(Ok(())), UmiStatus::OK);
}

#[test]
fn commands_are_limited_and_withdrawn_when_init_fails() {
    let shared = std::sync::Arc::<HostShared>::default();
    let failing = fresh("failing", &shared);
    for n in 0..MAX_COMMANDS {
        failing
            .register_command(&format!("c{n}"), "", Some(run_ok), user())
            .unwrap();
    }
    assert!(failing
        .register_command("c_more", "", Some(run_ok), user())
        .is_err());

    failing.finish_init(false);

    let later = fresh("later", &shared);
    assert!(later
        .register_command("c0", "", Some(run_ok), user())
        .is_ok());
    later.finish_init(true);
    assert_eq!(shared.commands().complete("c1"), Vec::<String>::new());
    assert_eq!(shared.commands().complete("c0"), ["c0"]);
}

#[test]
fn chat_lines_queue_up_to_the_outbox_capacity_then_drop() {
    let cx = started_context();
    let outbox = cx.shared.chat_outbox();
    for n in 0..OUTBOX_CAPACITY + 3 {
        assert_eq!(cx.chat_print(&format!("line {n}")), UmiStatus::OK);
    }
    assert_eq!(cx.shared.dropped_lines(), 3);
    let lines: Vec<String> = outbox.try_iter().collect();
    assert_eq!(lines.len(), OUTBOX_CAPACITY);
    assert_eq!(lines[0], "line 0");
    assert_eq!(cx.chat_print("again"), UmiStatus::OK);
    assert_eq!(outbox.try_recv().as_deref(), Ok("again"));
}

#[test]
fn chat_print_refuses_long_lines_and_a_switched_off_mod() {
    let cx = started_context();
    assert_eq!(
        cx.chat_print(&"x".repeat(crate::ui::MAX_CHAT_BYTES + 1)),
        UmiStatus::INVALID_ARGUMENT
    );
    cx.set_wanted(false);
    assert_eq!(cx.chat_print("hi"), UmiStatus::DENIED);
    assert!(cx.shared.chat_outbox().try_recv().is_err());
}

#[test]
fn chat_print_is_allowed_during_init() {
    let cx = context();
    assert_eq!(cx.chat_print("loading"), UmiStatus::OK);
}

fn started_context() -> std::sync::Arc<ModContext> {
    let cx = context();
    cx.finish_init(true);
    cx
}

#[test]
fn a_widget_default_placement_is_kept_for_reset() {
    let cx = context();
    let placement = HudPlacement::at(Anchor::BottomLeft, 1.0, 2.0);
    cx.register_widget("w", "W", placement, Some(draw_lines), user())
        .unwrap();
    cx.finish_init(true);
    assert_eq!(
        cx.ui.declared().widget("w").unwrap().default_placement(),
        placement
    );
}
