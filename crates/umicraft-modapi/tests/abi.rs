// SPDX-License-Identifier: MIT OR Apache-2.0

//! Drives the exports of `umicraft_mod!` through a fake host, the way the
//! real loader does.

// The fake host reads a raw slice, as a real host does.
#![allow(unsafe_code)]

use core::ffi::c_void;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
use std::sync::{Mutex, MutexGuard, PoisonError};

use umicraft_modapi::{umicraft_mod, InitCx, Mod, ModError, PostTick, PreTick};
use umicraft_modapi_sys::*;

static SERIAL: Mutex<()> = Mutex::new(());
static LOGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static SUBS: Mutex<Vec<Sub>> = Mutex::new(Vec::new());
static TICKS: Mutex<Vec<u64>> = Mutex::new(Vec::new());
static DROPS: AtomicUsize = AtomicUsize::new(0);
static DROPS_AT_SHUTDOWN: AtomicUsize = AtomicUsize::new(usize::MAX);
static SHUTDOWNS: AtomicUsize = AtomicUsize::new(0);
static FAIL_INIT: AtomicBool = AtomicBool::new(false);
static PANIC_ON_TICK: AtomicBool = AtomicBool::new(false);
static REFUSE_SUBSCRIBE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct Sub {
    event: UmiEventId,
    mode: UmiMode,
    priority: i32,
    handler: UmiHandlerFn,
    user: usize,
}

struct DropToken;

impl Drop for DropToken {
    fn drop(&mut self) {
        DROPS.fetch_add(1, SeqCst);
    }
}

struct TestMod;

impl Mod for TestMod {
    fn init(cx: &mut InitCx<'_>) -> Result<Self, ModError> {
        cx.logger().info("hello");
        let token = DropToken;
        cx.observe::<PreTick>(5, move |e| {
            let _keep = &token;
            assert!(!PANIC_ON_TICK.load(SeqCst), "boom");
            TICKS.lock().unwrap().push(e.tick());
        })?;
        let token = DropToken;
        cx.intercept::<PostTick>(-1, move |_e, control| {
            let _keep = &token;
            control.cancel();
        })?;
        if FAIL_INIT.load(SeqCst) {
            return Err(ModError::msg("nope"));
        }
        Ok(Self)
    }

    fn shutdown(&mut self) {
        DROPS_AT_SHUTDOWN.store(DROPS.load(SeqCst), SeqCst);
        SHUTDOWNS.fetch_add(1, SeqCst);
    }
}

umicraft_mod!(TestMod);

extern "C" fn fake_log(_me: UmiModHandle, level: UmiLevel, msg: UmiStr) {
    // SAFETY: the module under test passes a live UTF-8 slice for the call.
    let text = unsafe { std::slice::from_raw_parts(msg.ptr, msg.len) };
    let text = String::from_utf8_lossy(text);
    LOGS.lock().unwrap().push(format!("{}:{text}", level.0));
}

extern "C" fn fake_subscribe(
    _me: UmiModHandle,
    event: UmiEventId,
    mode: UmiMode,
    priority: i32,
    handler: Option<UmiHandlerFn>,
    user: *mut c_void,
) -> UmiStatus {
    if REFUSE_SUBSCRIBE.load(SeqCst) {
        return UmiStatus::DENIED;
    }
    SUBS.lock().unwrap().push(Sub {
        event,
        mode,
        priority,
        handler: handler.expect("trampoline is never null"),
        user: user as usize,
    });
    UmiStatus::OK
}

fn host() -> UmiHost {
    UmiHost {
        size: size_of::<UmiHost>(),
        version: UMI_API_VERSION,
        log: fake_log,
        subscribe: fake_subscribe,
    }
}

fn me() -> UmiModHandle {
    UmiModHandle(std::ptr::null_mut())
}

/// Serializes tests (they share statics and the one mod instance) and leaves
/// the mod shut down with every counter reset.
fn fresh() -> MutexGuard<'static, ()> {
    let guard = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    umi_mod_shutdown();
    LOGS.lock().unwrap().clear();
    SUBS.lock().unwrap().clear();
    TICKS.lock().unwrap().clear();
    for counter in [&DROPS, &SHUTDOWNS] {
        counter.store(0, SeqCst);
    }
    for flag in [&FAIL_INIT, &PANIC_ON_TICK, &REFUSE_SUBSCRIBE] {
        flag.store(false, SeqCst);
    }
    guard
}

fn init_ok() {
    assert_eq!(umi_mod_init(&host(), me()), UmiStatus::OK);
}

fn sub_for(event: UmiEventId) -> Sub {
    *SUBS
        .lock()
        .unwrap()
        .iter()
        .find(|s| s.event == event)
        .unwrap()
}

fn header(id: UmiEventId, size: usize, flags: u32) -> UmiEventHeader {
    UmiEventHeader {
        size: size as u32,
        id,
        flags,
    }
}

fn pre_tick(tick: u64) -> UmiPreTick {
    UmiPreTick {
        header: header(UmiEventId::PRE_TICK, size_of::<UmiPreTick>(), 0),
        tick,
    }
}

fn post_tick(flags: u32) -> UmiPostTick {
    UmiPostTick {
        header: header(UmiEventId::POST_TICK, size_of::<UmiPostTick>(), flags),
        tick: 0,
    }
}

fn call<T>(sub: Sub, payload: &mut T) -> UmiStatus {
    (sub.handler)((payload as *mut T).cast(), sub.user as *mut c_void)
}

#[test]
fn reports_the_api_version() {
    let _guard = fresh();
    assert_eq!(umi_mod_api_version(), UMI_API_VERSION);
}

#[test]
fn init_registers_the_subscriptions_and_logs() {
    let _guard = fresh();
    init_ok();
    let subs = SUBS.lock().unwrap();
    assert_eq!(subs.len(), 2);
    assert_eq!(
        (subs[0].event, subs[0].mode, subs[0].priority),
        (UmiEventId::PRE_TICK, UmiMode::OBSERVE, 5)
    );
    assert_eq!(
        (subs[1].event, subs[1].mode, subs[1].priority),
        (UmiEventId::POST_TICK, UmiMode::INTERCEPT, -1)
    );
    assert_eq!(*LOGS.lock().unwrap(), ["3:hello"]);
}

#[test]
fn trampoline_runs_the_closure() {
    let _guard = fresh();
    init_ok();
    assert_eq!(
        call(sub_for(UmiEventId::PRE_TICK), &mut pre_tick(42)),
        UmiStatus::OK
    );
    assert_eq!(*TICKS.lock().unwrap(), [42]);
}

#[test]
fn panicking_handler_returns_panicked() {
    let _guard = fresh();
    init_ok();
    PANIC_ON_TICK.store(true, SeqCst);
    assert_eq!(
        call(sub_for(UmiEventId::PRE_TICK), &mut pre_tick(1)),
        UmiStatus::PANICKED
    );
}

#[test]
fn init_error_maps_to_error_and_is_logged() {
    let _guard = fresh();
    FAIL_INIT.store(true, SeqCst);
    assert_eq!(umi_mod_init(&host(), me()), UmiStatus::ERROR);
    assert_eq!(*LOGS.lock().unwrap(), ["3:hello", "1:nope"]);
    // The failed init left no instance behind, so a retry is not a conflict.
    FAIL_INIT.store(false, SeqCst);
    init_ok();
}

#[test]
fn second_init_conflicts() {
    let _guard = fresh();
    init_ok();
    assert_eq!(umi_mod_init(&host(), me()), UmiStatus::CONFLICT);
}

#[test]
fn shutdown_calls_the_mod_then_frees_the_closures() {
    let _guard = fresh();
    init_ok();
    umi_mod_shutdown();
    assert_eq!(SHUTDOWNS.load(SeqCst), 1);
    assert_eq!(DROPS_AT_SHUTDOWN.load(SeqCst), 0);
    assert_eq!(DROPS.load(SeqCst), 2);
}

#[test]
fn shutdown_without_init_is_a_no_op() {
    let _guard = fresh();
    umi_mod_shutdown();
    assert_eq!(SHUTDOWNS.load(SeqCst), 0);
}

#[test]
fn wrong_event_id_or_small_payload_is_invalid() {
    let _guard = fresh();
    init_ok();
    let sub = sub_for(UmiEventId::PRE_TICK);
    assert_eq!(call(sub, &mut post_tick(0)), UmiStatus::INVALID_ARGUMENT);
    let mut short = pre_tick(1);
    short.header.size = 4;
    assert_eq!(call(sub, &mut short), UmiStatus::INVALID_ARGUMENT);
    let null = (sub.handler)(std::ptr::null_mut(), sub.user as *mut c_void);
    assert_eq!(null, UmiStatus::INVALID_ARGUMENT);
    assert!(TICKS.lock().unwrap().is_empty());
}

#[test]
fn cancel_sets_the_flag_only_when_cancellable() {
    let _guard = fresh();
    init_ok();
    let sub = sub_for(UmiEventId::POST_TICK);
    let mut plain = post_tick(0);
    assert_eq!(call(sub, &mut plain), UmiStatus::OK);
    assert_eq!(plain.header.flags, 0);
    let mut cancellable = post_tick(UMI_EVENT_CANCELLABLE);
    assert_eq!(call(sub, &mut cancellable), UmiStatus::OK);
    assert_eq!(
        cancellable.header.flags,
        UMI_EVENT_CANCELLABLE | UMI_EVENT_CANCELLED
    );
}

#[test]
fn small_or_null_host_is_refused() {
    let _guard = fresh();
    let mut small = host();
    small.size = size_of::<usize>();
    assert_eq!(umi_mod_init(&small, me()), UmiStatus::UNSUPPORTED);
    assert_eq!(
        umi_mod_init(std::ptr::null(), me()),
        UmiStatus::INVALID_ARGUMENT
    );
    let mut old = host();
    old.version = UmiVersion {
        major: 0,
        minor: 99,
    };
    assert_eq!(umi_mod_init(&old, me()), UmiStatus::UNSUPPORTED);
}

#[test]
fn host_refusal_frees_the_closure_at_once() {
    let _guard = fresh();
    REFUSE_SUBSCRIBE.store(true, SeqCst);
    assert_eq!(umi_mod_init(&host(), me()), UmiStatus::ERROR);
    assert_eq!(DROPS.load(SeqCst), 1);
    assert!(LOGS
        .lock()
        .unwrap()
        .contains(&"1:host refused the request (status 5)".to_string()));
}
