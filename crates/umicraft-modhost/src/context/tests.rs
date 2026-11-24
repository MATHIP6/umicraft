// SPDX-License-Identifier: MIT OR Apache-2.0

use core::ffi::c_void;

use umicraft_modapi_sys::UmiEventHeader;

use super::*;
use crate::ui::shared::HostShared;

extern "C" fn noop(_: *mut UmiEventHeader, _: *mut c_void) -> UmiStatus {
    UmiStatus::OK
}

fn context() -> Arc<ModContext> {
    ModContext::new("hud".to_owned().try_into().unwrap())
}

fn subscribe(cx: &ModContext, event: UmiEventId, handler: Option<UmiHandlerFn>) -> UmiStatus {
    cx.subscribe(
        event,
        UmiMode::OBSERVE,
        0,
        handler,
        UserPtr::new(std::ptr::null_mut()),
    )
}

#[test]
fn subscribe_records_a_valid_handler() {
    let cx = context();
    assert_eq!(
        subscribe(&cx, UmiEventId::PRE_TICK, Some(noop)),
        UmiStatus::OK
    );
    assert_eq!(cx.subscriptions(UmiEventId::PRE_TICK).len(), 1);
    assert!(cx.subscriptions(UmiEventId::POST_TICK).is_empty());
}

#[test]
fn subscribe_refuses_null_handler_unknown_event_and_unknown_mode() {
    let cx = context();
    assert_eq!(
        subscribe(&cx, UmiEventId::PRE_TICK, None),
        UmiStatus::INVALID_ARGUMENT
    );
    assert_eq!(
        subscribe(&cx, UmiEventId(999), Some(noop)),
        UmiStatus::INVALID_ARGUMENT
    );
    let bad_mode = cx.subscribe(
        UmiEventId::PRE_TICK,
        UmiMode(7),
        0,
        Some(noop),
        UserPtr::new(std::ptr::null_mut()),
    );
    assert_eq!(bad_mode, UmiStatus::INVALID_ARGUMENT);
    assert!(cx.subscriptions(UmiEventId::PRE_TICK).is_empty());
}

#[test]
fn subscribe_after_init_is_denied() {
    let cx = context();
    cx.finish_init(true);
    assert_eq!(
        subscribe(&cx, UmiEventId::PRE_TICK, Some(noop)),
        UmiStatus::DENIED
    );
}

#[test]
fn failed_init_drops_subscriptions_and_stays_disabled() {
    let cx = context();
    subscribe(&cx, UmiEventId::PRE_TICK, Some(noop));
    cx.finish_init(false);
    assert!(!cx.is_enabled());
    assert!(cx.is_failed());
    assert!(cx.subscriptions(UmiEventId::PRE_TICK).is_empty());
    assert!(
        !cx.set_wanted(true),
        "a mod that failed to initialise stays off"
    );
}

#[test]
fn the_player_can_switch_a_mod_off_and_on_again() {
    let cx = context();
    subscribe(&cx, UmiEventId::PRE_TICK, Some(noop));
    cx.finish_init(true);
    assert!(cx.is_enabled());
    assert!(cx.set_wanted(false));
    assert!(!cx.is_enabled() && !cx.is_wanted() && !cx.is_failed());
    // The list stays: a dispatch in flight may still read it, and re-enabling needs it.
    assert_eq!(cx.subscriptions(UmiEventId::PRE_TICK).len(), 1);
    assert!(cx.set_wanted(true));
    assert!(cx.is_enabled());
}

#[test]
fn a_failed_mod_stays_off_whatever_the_player_wants() {
    let cx = context();
    cx.finish_init(true);
    cx.fail("test");
    cx.fail("again, logged once");
    assert!(cx.is_failed() && cx.is_wanted() && !cx.is_enabled());
    assert_eq!(
        cx.failure().as_deref(),
        Some("test"),
        "the first reason wins"
    );
    assert!(!cx.set_wanted(true));
    assert!(!cx.is_enabled());
}

#[test]
fn shutdown_is_owed_once_and_only_after_a_successful_init() {
    let cx = context();
    cx.finish_init(true);
    cx.fail("test");
    assert!(!cx.is_enabled());
    assert!(
        cx.take_shutdown_due(),
        "a disabled mod still owes its shutdown"
    );
    assert!(!cx.take_shutdown_due());

    let failed = context();
    assert_eq!(failed.failure(), None);
    failed.finish_init(false);
    assert!(failed.failure().unwrap().contains("init"));
    assert!(!failed.take_shutdown_due());
}

#[test]
fn subscribe_accepts_the_thirteen_open_events_and_refuses_unknown_ids() {
    let cx = context();
    for id in (1..=9).chain(12..=15) {
        assert_eq!(subscribe(&cx, UmiEventId(id), Some(noop)), UmiStatus::OK);
    }
    assert_eq!(
        subscribe(&cx, UmiEventId(16), Some(noop)),
        UmiStatus::INVALID_ARGUMENT
    );
    assert_eq!(cx.all_subscriptions().len(), 13);
}

fn with_permissions(id: &str, permissions: Permissions) -> Arc<ModContext> {
    ModContext::with_shared(
        id.to_owned().try_into().unwrap(),
        permissions,
        Arc::default(),
    )
}

const PACKETS: Permissions = Permissions {
    network_packets: true,
    movement: false,
    filesystem: false,
    network_http: false,
    camera: false,
};
const MOVEMENT: Permissions = Permissions {
    network_packets: false,
    movement: true,
    filesystem: false,
    network_http: false,
    camera: false,
};

#[test]
fn packet_events_need_the_network_packets_permission() {
    let plain = context();
    for event in [UmiEventId::PACKET_RECEIVED, UmiEventId::PACKET_SEND] {
        assert_eq!(subscribe(&plain, event, Some(noop)), UmiStatus::DENIED);
    }
    assert!(plain.all_subscriptions().is_empty());

    let allowed = with_permissions("net", PACKETS);
    for event in [UmiEventId::PACKET_RECEIVED, UmiEventId::PACKET_SEND] {
        assert_eq!(subscribe(&allowed, event, Some(noop)), UmiStatus::OK);
    }
    // Argument validation comes first: a null handler is not a permission question.
    assert_eq!(
        subscribe(&plain, UmiEventId::PACKET_SEND, None),
        UmiStatus::INVALID_ARGUMENT
    );
}

#[test]
fn permissions_are_kept_and_alter_game_follows_packets_and_movement() {
    assert!(!context().alters_game());
    assert_eq!(context().permissions(), Permissions::default());
    assert!(with_permissions("net", PACKETS).alters_game());
    assert!(with_permissions("mv", MOVEMENT).alters_game());
    let http = Permissions {
        network_http: true,
        filesystem: true,
        ..Permissions::default()
    };
    assert!(!with_permissions("http", http).alters_game());
    // The camera changes what is drawn, not what the server sees (ADR-0094).
    let camera = Permissions {
        camera: true,
        ..Permissions::default()
    };
    assert!(!with_permissions("cam", camera).alters_game());
}

#[test]
fn server_compat_holds_off_only_mods_that_alter_the_game() {
    let shared = Arc::<HostShared>::default();
    let make = |id: &str, permissions| {
        let cx = ModContext::with_shared(
            id.to_owned().try_into().unwrap(),
            permissions,
            shared.clone(),
        );
        cx.finish_init(true);
        cx
    };
    let altering = make("net", PACKETS);
    let harmless = make("plain", Permissions::default());

    shared.set_server_compat(true);
    assert!(!altering.is_enabled() && altering.is_held_by_compat());
    assert!(altering.is_wanted(), "the player's switch is untouched");
    assert!(harmless.is_enabled() && !harmless.is_held_by_compat());

    shared.set_server_compat(false);
    assert!(altering.is_enabled() && !altering.is_held_by_compat());
}

#[test]
fn log_text_replaces_invalid_utf8_and_caps_length() {
    assert_eq!(log_text(b"ok\xff"), "ok\u{fffd}");
    assert_eq!(log_text(&vec![b'a'; 10_000]).len(), MAX_LOG_BYTES);
}
