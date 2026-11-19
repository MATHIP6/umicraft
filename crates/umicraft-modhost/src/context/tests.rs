// SPDX-License-Identifier: MIT OR Apache-2.0

use core::ffi::c_void;

use umicraft_modapi_sys::UmiEventHeader;

use super::*;

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
    failed.finish_init(false);
    assert!(!failed.take_shutdown_due());
}

#[test]
fn subscribe_accepts_the_nine_events_of_api_0_3() {
    let cx = context();
    for id in 1..=9 {
        assert_eq!(subscribe(&cx, UmiEventId(id), Some(noop)), UmiStatus::OK);
    }
    assert_eq!(
        subscribe(&cx, UmiEventId(10), Some(noop)),
        UmiStatus::INVALID_ARGUMENT
    );
    assert_eq!(cx.all_subscriptions().len(), 9);
}

#[test]
fn log_text_replaces_invalid_utf8_and_caps_length() {
    assert_eq!(log_text(b"ok\xff"), "ok\u{fffd}");
    assert_eq!(log_text(&vec![b'a'; 10_000]).len(), MAX_LOG_BYTES);
}
