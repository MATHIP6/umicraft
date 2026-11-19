// SPDX-License-Identifier: MIT OR Apache-2.0

//! Dispatch tests with fake `extern "C"` handlers: no library is loaded. A handler
//! learns which subscription it is from the `user` pointer, which the tests use as a
//! plain integer token and never dereference.

use core::ffi::c_void;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use umicraft_modapi_sys::{UmiEventHeader, UmiStatus, UmiStr, UMI_EVENT_CANCELLED};

use super::*;
use crate::budget::MAX_CONSECUTIVE_OVERRUNS;
use crate::ffi::{host_table, testing};
use crate::worker::QUEUE_CAPACITY;

thread_local! {
    /// Tokens of the intercept handlers that ran on this (test) thread, in order.
    static RAN: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    /// Header flags an intercept handler saw, in order.
    static FLAGS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

/// What observers saw on their worker threads: token, thread name, flags, status of
/// an `event_set_text` attempt.
type Observation = (usize, Option<String>, u32, i32);
static OBSERVED: Mutex<Vec<Observation>> = Mutex::new(Vec::new());
static GATE_OPEN: AtomicBool = AtomicBool::new(false);

fn ran() -> Vec<usize> {
    RAN.with(|ran| ran.borrow().clone())
}

extern "C" fn record(_: *mut UmiEventHeader, user: *mut c_void) -> UmiStatus {
    RAN.with(|ran| ran.borrow_mut().push(user as usize));
    UmiStatus::OK
}

extern "C" fn cancel(event: *mut UmiEventHeader, user: *mut c_void) -> UmiStatus {
    testing::set_flag(event, UMI_EVENT_CANCELLED);
    record(event, user)
}

extern "C" fn replace_text(event: *mut UmiEventHeader, user: *mut c_void) -> UmiStatus {
    record(event, user);
    (host_table().event_set_text)(event, UmiStr::from_static("replaced"))
}

extern "C" fn fail_with_user_status(event: *mut UmiEventHeader, user: *mut c_void) -> UmiStatus {
    record(event, user);
    UmiStatus(2)
}

extern "C" fn see_flags(event: *mut UmiEventHeader, user: *mut c_void) -> UmiStatus {
    FLAGS.with(|flags| flags.borrow_mut().push(testing::flags_of(event)));
    record(event, user)
}

extern "C" fn sleep_2ms(event: *mut UmiEventHeader, user: *mut c_void) -> UmiStatus {
    std::thread::sleep(Duration::from_millis(2));
    record(event, user)
}

/// Observer: notes the thread, the flags and what `event_set_text` answers.
extern "C" fn observe(event: *mut UmiEventHeader, user: *mut c_void) -> UmiStatus {
    let set_text = (host_table().event_set_text)(event, UmiStr::from_static("x"));
    let entry = (
        user as usize,
        std::thread::current().name().map(str::to_owned),
        testing::flags_of(event),
        set_text.0,
    );
    OBSERVED.lock().unwrap().push(entry);
    UmiStatus::OK
}

/// Observer that holds the worker until the gate opens.
extern "C" fn observe_gated(event: *mut UmiEventHeader, user: *mut c_void) -> UmiStatus {
    while !GATE_OPEN.load(Ordering::Acquire) {
        std::thread::yield_now();
    }
    observe(event, user)
}

fn observed(token: usize) -> Vec<(Option<String>, u32, i32)> {
    let all = OBSERVED.lock().unwrap();
    all.iter()
        .filter(|entry| entry.0 == token)
        .map(|entry| (entry.1.clone(), entry.2, entry.3))
        .collect()
}

/// A started mod with the given `(event, mode, priority, handler, token)` handlers.
fn mod_with(
    id: &str,
    handlers: &[(UmiEventId, UmiMode, i32, UmiHandlerFn, usize)],
) -> Arc<ModContext> {
    let context = ModContext::new(id.to_owned().try_into().unwrap());
    for &(event, mode, priority, handler, token) in handlers {
        let user = UserPtr::new(token as *mut c_void);
        let status = context.subscribe(event, mode, priority, Some(handler), user);
        assert_eq!(status, UmiStatus::OK);
    }
    context.finish_init(true);
    context
}

fn intercepting(
    event: UmiEventId,
    priority: i32,
    handler: UmiHandlerFn,
    token: usize,
) -> (UmiEventId, UmiMode, i32, UmiHandlerFn, usize) {
    (event, UmiMode::INTERCEPT, priority, handler, token)
}

fn install(contexts: &[&Arc<ModContext>], thread: EventThread) -> EventBus {
    let mut bus = EventBus::default();
    for context in contexts {
        install_mod(&mut bus, thread, context);
    }
    bus
}

#[test]
fn intercept_runs_by_priority_across_mods_and_ties_in_load_order() {
    let late = mod_with("late", &[intercepting(UmiEventId::PRE_TICK, 10, record, 1)]);
    let early = mod_with(
        "early",
        &[intercepting(UmiEventId::PRE_TICK, -5, record, 2)],
    );
    let tie = mod_with("tie", &[intercepting(UmiEventId::PRE_TICK, 10, record, 3)]);
    let mut bus = install(&[&late, &early, &tie], EventThread::Game);

    assert!(!bus.dispatch(&mut PreTick::new(0)));

    assert_eq!(ran(), [2, 1, 3]);
}

#[test]
fn cancelling_stops_later_handlers_and_is_visible_to_the_emitter() {
    let cx = mod_with(
        "canceller",
        &[
            intercepting(UmiEventId::CHAT_SEND, 0, cancel, 1),
            intercepting(UmiEventId::CHAT_SEND, 1, record, 2),
        ],
    );
    let mut bus = install(&[&cx], EventThread::Game);
    let mut event = ChatSend::new("/spam");

    assert!(bus.dispatch(&mut event));

    assert_eq!(ran(), [1]);
    assert!(event.is_cancelled());
}

#[test]
fn a_non_cancellable_event_ignores_the_cancel_mark() {
    let cx = mod_with(
        "rogue",
        &[
            intercepting(UmiEventId::POST_TICK, 0, cancel, 1),
            intercepting(UmiEventId::POST_TICK, 1, record, 2),
        ],
    );
    let mut bus = install(&[&cx], EventThread::Game);

    assert!(!bus.dispatch(&mut PostTick::new(0)));

    assert_eq!(ran(), [1, 2]);
}

#[test]
fn set_text_is_seen_by_later_handlers_and_by_the_emitter() {
    let cx = mod_with(
        "editor",
        &[intercepting(UmiEventId::CHAT_SEND, 0, replace_text, 1)],
    );
    let mut bus = install(&[&cx], EventThread::Game);
    let seen_later = Arc::new(Mutex::new(String::new()));
    let sink = seen_later.clone();
    bus.on::<ChatSend, _>(ModuleId::new("builtin"), 5, move |event, _| {
        *sink.lock().unwrap() = event.text().to_owned();
    });
    let mut event = ChatSend::new("original");

    assert!(!bus.dispatch(&mut event));

    assert_eq!(*seen_later.lock().unwrap(), "replaced");
    assert_eq!(event.text(), "replaced");
    assert!(cx.is_enabled());
}

#[test]
fn set_text_on_chat_received_replaces_the_json() {
    let cx = mod_with(
        "editor_rx",
        &[intercepting(UmiEventId::CHAT_RECEIVED, 0, replace_text, 1)],
    );
    let mut bus = install(&[&cx], EventThread::Game);
    let mut event = ChatReceived::new("{}", 0);

    bus.dispatch(&mut event);

    assert_eq!(event.json(), "replaced");
}

#[test]
fn set_text_on_an_immutable_event_is_denied_and_disables_the_mod() {
    let cx = mod_with(
        "misuse",
        &[intercepting(UmiEventId::PRE_TICK, 0, replace_text, 1)],
    );
    let mut bus = install(&[&cx], EventThread::Game);

    bus.dispatch(&mut PreTick::new(0));

    assert!(!cx.is_enabled(), "DENIED is a non-OK status");
}

#[test]
fn flags_are_lent_for_the_call_and_cleared_after() {
    let cx = mod_with(
        "flags",
        &[
            intercepting(UmiEventId::CHAT_SEND, 0, see_flags, 1),
            intercepting(UmiEventId::PRE_TICK, 0, see_flags, 2),
        ],
    );
    let mut bus = install(&[&cx], EventThread::Game);
    let mut chat = ChatSend::new("x");
    let mut tick = PreTick::new(0);

    bus.dispatch(&mut chat);
    bus.dispatch(&mut tick);

    assert_eq!(
        FLAGS.with(|f| f.borrow().clone()),
        [UMI_EVENT_MUTABLE | UMI_EVENT_CANCELLABLE, 0]
    );
    assert_eq!(chat.header().flags, 0);
    assert_eq!(tick.header().flags, 0);
}

#[test]
fn a_failing_handler_disables_its_mod_and_later_dispatches_skip_it() {
    let bad = mod_with(
        "bad",
        &[intercepting(
            UmiEventId::PRE_TICK,
            0,
            fail_with_user_status,
            1,
        )],
    );
    let good = mod_with("good", &[intercepting(UmiEventId::PRE_TICK, 1, record, 2)]);
    let mut bus = install(&[&bad, &good], EventThread::Game);

    bus.dispatch(&mut PreTick::new(0));
    bus.dispatch(&mut PreTick::new(1));

    assert!(!bad.is_enabled());
    assert!(good.is_enabled());
    assert_eq!(ran(), [1, 2, 2], "the failing handler ran once");
}

#[test]
fn a_handler_over_budget_fifty_times_in_a_row_is_disabled() {
    let slow = mod_with(
        "slow",
        &[intercepting(UmiEventId::PRE_TICK, 0, sleep_2ms, 1)],
    );
    let mut bus = install(&[&slow], EventThread::Game);

    for tick in 0..MAX_CONSECUTIVE_OVERRUNS + 5 {
        bus.dispatch(&mut PreTick::new(tick));
    }

    assert!(!slow.is_enabled());
    let stats = slow.stats();
    assert_eq!(stats.calls, MAX_CONSECUTIVE_OVERRUNS);
    assert_eq!(stats.overruns, MAX_CONSECUTIVE_OVERRUNS);
    assert!(stats.max_ns >= 2_000_000 && stats.total_ns >= stats.max_ns);
}

#[test]
fn only_the_subscriptions_of_the_installed_thread_are_registered() {
    let cx = mod_with(
        "threads",
        &[
            intercepting(UmiEventId::PRE_TICK, 0, record, 1),
            intercepting(UmiEventId::SCREEN_OPENED, 0, record, 2),
        ],
    );

    let game = install(&[&cx], EventThread::Game);
    let render = install(&[&cx], EventThread::Render);

    assert!(game.has_subscribers::<PreTick>() && !game.has_subscribers::<ScreenOpened>());
    assert!(render.has_subscribers::<ScreenOpened>() && !render.has_subscribers::<PreTick>());
}

#[test]
fn a_disabled_mod_registers_nothing() {
    let cx = mod_with("off", &[intercepting(UmiEventId::PRE_TICK, 0, record, 1)]);
    cx.set_wanted(false);

    let bus = install(&[&cx], EventThread::Game);

    assert!(!bus.has_subscribers::<PreTick>());
}

#[test]
fn observers_run_on_the_worker_with_a_copy_and_no_capabilities() {
    let cx = mod_with(
        "watcher",
        &[(
            UmiEventId::CHAT_SEND,
            UmiMode::OBSERVE,
            0,
            observe as UmiHandlerFn,
            7001,
        )],
    );
    let mut bus = install(&[&cx], EventThread::Game);
    let mut event = ChatSend::new("hi");

    assert!(!bus.dispatch(&mut event));
    cx.worker().stop();

    let seen = observed(7001);
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0.as_deref(), Some("umicraft-mod-watcher"));
    assert_eq!(seen[0].1, 0, "no MUTABLE, no CANCELLABLE");
    assert_eq!(seen[0].2, UmiStatus::DENIED.0, "a copy cannot be changed");
    assert_eq!(event.text(), "hi");
}

#[test]
fn a_full_queue_drops_observations_and_counts_them() {
    let cx = mod_with(
        "flooded",
        &[(
            UmiEventId::PRE_TICK,
            UmiMode::OBSERVE,
            0,
            observe_gated as UmiHandlerFn,
            7002,
        )],
    );
    let mut bus = install(&[&cx], EventThread::Game);
    let sent = QUEUE_CAPACITY + 40;

    for tick in 0..sent as u64 {
        bus.dispatch(&mut PreTick::new(tick));
    }
    let dropped = cx.stats().dropped as usize;
    GATE_OPEN.store(true, Ordering::Release);
    cx.worker().stop();

    // The worker may have taken one job off the queue before it parked.
    let delivered = observed(7002).len();
    assert!((39..=40).contains(&dropped), "dropped {dropped}");
    assert_eq!(delivered + dropped, sent);
}

#[test]
fn a_disabled_mods_observer_gets_nothing() {
    let cx = mod_with(
        "muted",
        &[(
            UmiEventId::PRE_TICK,
            UmiMode::OBSERVE,
            0,
            observe as UmiHandlerFn,
            7003,
        )],
    );
    let mut bus = install(&[&cx], EventThread::Game);
    cx.set_wanted(false);

    bus.dispatch(&mut PreTick::new(0));
    cx.worker().stop();

    assert!(observed(7003).is_empty());
    assert_eq!(cx.stats().dropped, 0);
}

#[test]
fn key_pressed_only_reaches_the_mod_it_is_aimed_at() {
    let both = |token| {
        [
            intercepting(UmiEventId::KEY_PRESSED, 0, record, token),
            (
                UmiEventId::KEY_PRESSED,
                UmiMode::OBSERVE,
                0,
                observe as UmiHandlerFn,
                token + 1000,
            ),
        ]
    };
    let target = mod_with("target", &both(1));
    let bystander = mod_with("bystander", &both(2));
    let mut bus = install(&[&target, &bystander], EventThread::Render);
    let id: crate::ModId = "target".to_owned().try_into().unwrap();

    assert!(!bus.dispatch(&mut KeyPressed::new(id, "jump")));
    target.worker().stop();
    bystander.worker().stop();

    assert_eq!(ran(), [1], "only the target's intercept handler ran");
    assert_eq!(observed(1001).len(), 1);
    assert!(
        observed(1002).is_empty(),
        "the other mod's observer got nothing"
    );
}

#[test]
fn key_pressed_is_a_render_event_and_carries_its_key() {
    let cx = mod_with(
        "keys",
        &[intercepting(UmiEventId::KEY_PRESSED, 0, record, 1)],
    );
    let game = install(&[&cx], EventThread::Game);
    let render = install(&[&cx], EventThread::Render);

    assert!(!game.has_subscribers::<KeyPressed>());
    assert!(render.has_subscribers::<KeyPressed>());
    let event = KeyPressed::new("keys".to_owned().try_into().unwrap(), "toggle");
    assert_eq!(event.key(), "toggle");
    assert_eq!(event.target().map(crate::ModId::as_str), Some("keys"));
}
