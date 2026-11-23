// SPDX-License-Identifier: MIT OR Apache-2.0

use core::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use umicraft_modapi_sys::{UmiHost, UmiPhysicsCx};

use super::*;
use crate::ffi::physics_testing::{input_yaw, scale_velocity_x, set_velocity_x};
use crate::ffi::{handle_of, host_table};
use crate::manifest::Permissions;
use crate::ui::shared::HostShared;

fn state() -> UmiPlayerPhysics {
    UmiPlayerPhysics {
        size: core::mem::size_of::<UmiPlayerPhysics>() as u32,
        flags: 0,
        position: [0.0; 3],
        velocity: [1.0, 0.0, 0.0],
        yaw: 0.0,
        pitch: 0.0,
        fall_distance: 0.0,
        fly_speed: 0.05,
        game_mode: 0,
    }
}

fn input() -> UmiPhysicsInput {
    UmiPhysicsInput {
        size: core::mem::size_of::<UmiPhysicsInput>() as u32,
        flags: 0,
        forward: 1.0,
        strafe: 0.0,
        yaw: 0.0,
        pitch: 0.0,
    }
}

fn table() -> UmiHost {
    host_table()
}

/// The vanilla stand-in: adds one to the x velocity.
fn vanilla(state: &mut UmiPlayerPhysics) {
    state.velocity[0] += 1.0;
}

fn id(name: &str) -> crate::manifest::ModId {
    name.to_owned().try_into().unwrap()
}

fn load(wraps: bool, replaces: bool) -> Load {
    let service = || vec![PLAYER_PHYSICS.to_owned().try_into().unwrap()];
    Load {
        wraps: if wraps { service() } else { Vec::new() },
        replaces: if replaces { service() } else { Vec::new() },
        ..Load::default()
    }
}

fn mod_with(name: &str, movement: bool, load: &Load) -> Arc<ModContext> {
    let permissions = Permissions {
        movement,
        ..Permissions::default()
    };
    ModContext::with_services(id(name), permissions, load, Arc::new(HostShared::default()))
}

/// A mod that wraps `player_physics` with `step`, initialised.
fn wrapper(name: &str, step: UmiPhysicsFn) -> Arc<ModContext> {
    layer(name, UmiServiceMode::WRAP, step)
}

fn layer(name: &str, mode: UmiServiceMode, step: UmiPhysicsFn) -> Arc<ModContext> {
    let cx = mod_with(name, true, &load(true, true));
    let user = UserPtr::new(core::ptr::null_mut());
    assert_eq!(cx.register_physics(mode, Some(step), user), UmiStatus::OK);
    cx.finish_init(true);
    cx
}

fn chain(contexts: &[&Arc<ModContext>]) -> PhysicsChain {
    PhysicsChain::new(contexts.iter().copied()).unwrap()
}

fn run(chain: &PhysicsChain) -> UmiPlayerPhysics {
    let mut s = state();
    chain.step(&mut s, &input(), &mut vanilla);
    s
}

/// Order in which fake layers ran, as a letter each (lowercase on the way in,
/// uppercase on the way out).
static TRACE: Mutex<String> = Mutex::new(String::new());
static TRACE_GUARD: Mutex<()> = Mutex::new(());

fn trace(c: char) {
    TRACE.lock().unwrap().push(c);
}

/// Calls `next` and doubles the x velocity afterwards.
extern "C" fn doubler(
    cx: *mut UmiPhysicsCx,
    state: *mut UmiPlayerPhysics,
    _: *const UmiPhysicsInput,
    _: *mut c_void,
) -> UmiStatus {
    let status = (table().physics_next)(cx);
    scale_velocity_x(state, 2.0);
    status
}

extern "C" fn never_next(
    _: *mut UmiPhysicsCx,
    state: *mut UmiPlayerPhysics,
    _: *const UmiPhysicsInput,
    _: *mut c_void,
) -> UmiStatus {
    set_velocity_x(state, 50.0);
    UmiStatus::OK
}

static SECOND_CALL: Mutex<Option<UmiStatus>> = Mutex::new(None);

extern "C" fn next_twice(
    cx: *mut UmiPhysicsCx,
    _: *mut UmiPlayerPhysics,
    _: *const UmiPhysicsInput,
    _: *mut c_void,
) -> UmiStatus {
    assert_eq!((table().physics_next)(cx), UmiStatus::OK);
    *SECOND_CALL.lock().unwrap() = Some((table().physics_next)(cx));
    UmiStatus::OK
}

static REPLACER_NEXT: Mutex<Option<UmiStatus>> = Mutex::new(None);

/// A replacing layer: sets the velocity itself and finds `next` unsupported.
extern "C" fn replacer(
    cx: *mut UmiPhysicsCx,
    state: *mut UmiPlayerPhysics,
    _: *const UmiPhysicsInput,
    _: *mut c_void,
) -> UmiStatus {
    *REPLACER_NEXT.lock().unwrap() = Some((table().physics_next)(cx));
    set_velocity_x(state, 7.0);
    UmiStatus::OK
}

/// Scribbles on the state, then reports an error without calling `next`.
extern "C" fn scribble_then_fail(
    _: *mut UmiPhysicsCx,
    state: *mut UmiPlayerPhysics,
    _: *const UmiPhysicsInput,
    _: *mut c_void,
) -> UmiStatus {
    set_velocity_x(state, 999.0);
    UmiStatus::PANICKED
}

/// Calls `next`, then reports an error.
extern "C" fn next_then_fail(
    cx: *mut UmiPhysicsCx,
    state: *mut UmiPlayerPhysics,
    _: *const UmiPhysicsInput,
    _: *mut c_void,
) -> UmiStatus {
    (table().physics_next)(cx);
    set_velocity_x(state, 999.0);
    UmiStatus::ERROR
}

extern "C" fn trace_a(
    cx: *mut UmiPhysicsCx,
    _: *mut UmiPlayerPhysics,
    _: *const UmiPhysicsInput,
    _: *mut c_void,
) -> UmiStatus {
    trace('a');
    let status = (table().physics_next)(cx);
    trace('A');
    status
}

extern "C" fn trace_b(
    cx: *mut UmiPhysicsCx,
    _: *mut UmiPlayerPhysics,
    _: *const UmiPhysicsInput,
    _: *mut c_void,
) -> UmiStatus {
    trace('b');
    let status = (table().physics_next)(cx);
    trace('B');
    status
}

/// Remembers the cx it was given, to call `next` after the layer returned.
static KEPT_CX: Mutex<usize> = Mutex::new(0);

extern "C" fn keeps_cx(
    cx: *mut UmiPhysicsCx,
    _: *mut UmiPlayerPhysics,
    _: *const UmiPhysicsInput,
    _: *mut c_void,
) -> UmiStatus {
    *KEPT_CX.lock().unwrap() = cx as usize;
    (table().physics_next)(cx)
}

/// Sleeps 3 ms (over the 1 ms budget) outside `next`.
extern "C" fn slow_itself(
    cx: *mut UmiPhysicsCx,
    _: *mut UmiPlayerPhysics,
    _: *const UmiPhysicsInput,
    _: *mut c_void,
) -> UmiStatus {
    std::thread::sleep(Duration::from_millis(3));
    (table().physics_next)(cx)
}

extern "C" fn identity(
    cx: *mut UmiPhysicsCx,
    _: *mut UmiPlayerPhysics,
    _: *const UmiPhysicsInput,
    _: *mut c_void,
) -> UmiStatus {
    (table().physics_next)(cx)
}

static INPUT_YAW: AtomicU32 = AtomicU32::new(0);

extern "C" fn reads_input(
    cx: *mut UmiPhysicsCx,
    _: *mut UmiPlayerPhysics,
    input: *const UmiPhysicsInput,
    user: *mut c_void,
) -> UmiStatus {
    INPUT_YAW.store(input_yaw(input).to_bits(), Ordering::Relaxed);
    assert_eq!(user as usize, 0x1234);
    (table().physics_next)(cx)
}

// ---- registration ------------------------------------------------------------------

fn register(cx: &ModContext, mode: UmiServiceMode, step: Option<UmiPhysicsFn>) -> UmiStatus {
    cx.register_physics(mode, step, UserPtr::new(core::ptr::null_mut()))
}

#[test]
fn registering_a_wrapper_through_the_table_succeeds() {
    let cx = mod_with("tweak", true, &load(true, false));
    let status = (table().register_physics)(
        handle_of(&cx),
        UmiServiceMode::WRAP,
        Some(identity),
        core::ptr::null_mut(),
    );
    assert_eq!(status, UmiStatus::OK);
    cx.finish_init(true);
    assert!(PhysicsChain::new([&cx]).is_some());
}

#[test]
fn registration_needs_the_movement_permission() {
    let cx = mod_with("tweak", false, &load(true, false));
    assert_eq!(
        register(&cx, UmiServiceMode::WRAP, Some(identity)),
        UmiStatus::DENIED
    );
}

#[test]
fn registration_needs_the_service_in_the_matching_manifest_list() {
    let wraps_only = mod_with("a", true, &load(true, false));
    assert_eq!(
        register(&wraps_only, UmiServiceMode::REPLACE, Some(identity)),
        UmiStatus::DENIED
    );
    let replaces_only = mod_with("b", true, &load(false, true));
    assert_eq!(
        register(&replaces_only, UmiServiceMode::WRAP, Some(identity)),
        UmiStatus::DENIED
    );
    let neither = mod_with("c", true, &load(false, false));
    assert_eq!(
        register(&neither, UmiServiceMode::WRAP, Some(identity)),
        UmiStatus::DENIED
    );
    assert_eq!(
        register(&replaces_only, UmiServiceMode::REPLACE, Some(identity)),
        UmiStatus::OK
    );
}

#[test]
fn a_second_layer_of_the_same_mod_conflicts() {
    let cx = mod_with("tweak", true, &load(true, true));
    assert_eq!(
        register(&cx, UmiServiceMode::WRAP, Some(identity)),
        UmiStatus::OK
    );
    assert_eq!(
        register(&cx, UmiServiceMode::REPLACE, Some(identity)),
        UmiStatus::CONFLICT
    );
}

#[test]
fn a_null_function_or_unknown_mode_is_invalid() {
    let cx = mod_with("tweak", true, &load(true, false));
    assert_eq!(
        register(&cx, UmiServiceMode::WRAP, None),
        UmiStatus::INVALID_ARGUMENT
    );
    assert_eq!(
        register(&cx, UmiServiceMode(9), Some(identity)),
        UmiStatus::INVALID_ARGUMENT
    );
    // Neither attempt used up the mod's one layer.
    assert_eq!(
        register(&cx, UmiServiceMode::WRAP, Some(identity)),
        UmiStatus::OK
    );
}

#[test]
fn registration_after_init_is_denied() {
    let cx = mod_with("tweak", true, &load(true, false));
    cx.finish_init(true);
    assert_eq!(
        register(&cx, UmiServiceMode::WRAP, Some(identity)),
        UmiStatus::DENIED
    );
    assert!(PhysicsChain::new([&cx]).is_none());
}

#[test]
fn a_failed_init_drops_the_layer() {
    let cx = mod_with("tweak", true, &load(true, false));
    assert_eq!(
        register(&cx, UmiServiceMode::WRAP, Some(identity)),
        UmiStatus::OK
    );
    cx.finish_init(false);
    assert!(PhysicsChain::new([&cx]).is_none());
}

#[test]
fn no_layer_means_no_chain() {
    let cx = mod_with("plain", true, &load(true, false));
    cx.finish_init(true);
    assert!(PhysicsChain::new([&cx]).is_none());
}

// ---- running -----------------------------------------------------------------------

#[test]
fn a_wrapper_runs_around_vanilla() {
    let cx = wrapper("w", doubler);
    // vanilla: 1 + 1 = 2, then doubled.
    assert_eq!(run(&chain(&[&cx])).velocity[0], 4.0);
    assert_eq!(cx.physics_stats().calls, 1);
}

#[test]
fn a_wrapper_that_never_calls_next_skips_the_rest() {
    let cx = wrapper("w", never_next);
    assert_eq!(run(&chain(&[&cx])).velocity[0], 50.0);
}

#[test]
fn the_second_physics_next_is_denied() {
    let cx = wrapper("w", next_twice);
    let s = run(&chain(&[&cx]));
    assert_eq!(*SECOND_CALL.lock().unwrap(), Some(UmiStatus::DENIED));
    assert_eq!(s.velocity[0], 2.0, "vanilla ran once");
    assert!(cx.is_enabled(), "a denied call is the mod's to handle");
}

#[test]
fn a_replacing_layer_takes_the_place_of_vanilla_and_has_no_next() {
    let cx = layer("r", UmiServiceMode::REPLACE, replacer);
    assert_eq!(run(&chain(&[&cx])).velocity[0], 7.0);
    assert_eq!(*REPLACER_NEXT.lock().unwrap(), Some(UmiStatus::UNSUPPORTED));
}

#[test]
fn wrappers_nest_in_load_order_around_a_replacement_loaded_first() {
    let _guard = TRACE_GUARD.lock().unwrap();
    TRACE.lock().unwrap().clear();
    let replacing = layer("r", UmiServiceMode::REPLACE, replacer);
    let outer = wrapper("outer", trace_a);
    let inner = wrapper("inner", trace_b);
    let s = run(&chain(&[&replacing, &outer, &inner]));
    assert_eq!(*TRACE.lock().unwrap(), "abBA");
    assert_eq!(
        s.velocity[0], 7.0,
        "the replacement, not vanilla, is innermost"
    );
}

#[test]
fn a_disabled_layer_is_skipped_and_the_rest_still_run() {
    let _guard = TRACE_GUARD.lock().unwrap();
    TRACE.lock().unwrap().clear();
    let first = wrapper("first", trace_a);
    let second = wrapper("second", trace_b);
    let chain = chain(&[&first, &second]);
    assert!(chain.is_active());
    assert!(first.set_wanted(false));
    let s = run(&chain);
    assert_eq!(*TRACE.lock().unwrap(), "bB");
    assert_eq!(s.velocity[0], 2.0);
    assert!(second.set_wanted(false));
    assert!(!chain.is_active());
    assert_eq!(run(&chain).velocity[0], 2.0, "plain vanilla");
    assert_eq!(first.physics_stats().calls, 0);
}

#[test]
fn a_disabled_replacement_leaves_vanilla() {
    let cx = layer("r", UmiServiceMode::REPLACE, replacer);
    assert!(cx.set_wanted(false));
    assert_eq!(run(&chain(&[&cx])).velocity[0], 2.0);
}

#[test]
fn a_failing_layer_is_undone_and_the_inner_chain_still_runs() {
    let _guard = TRACE_GUARD.lock().unwrap();
    TRACE.lock().unwrap().clear();
    let bad = wrapper("bad", scribble_then_fail);
    let good = wrapper("good", trace_b);
    let chain = chain(&[&bad, &good]);
    let s = run(&chain);
    assert_eq!(s.velocity[0], 2.0, "the scribble is gone, vanilla ran once");
    assert_eq!(*TRACE.lock().unwrap(), "bB");
    assert!(bad.is_failed() && !good.is_failed());
    // From now on the failed mod is skipped.
    assert_eq!(run(&chain).velocity[0], 2.0);
    assert_eq!(bad.physics_stats().calls, 0, "a failed call is not counted");
}

#[test]
fn a_layer_failing_after_next_keeps_what_the_inner_chain_produced() {
    let bad = wrapper("bad", next_then_fail);
    let mut vanilla_runs = 0;
    let mut s = state();
    chain(&[&bad]).step(&mut s, &input(), &mut |s| {
        vanilla_runs += 1;
        vanilla(s);
    });
    assert_eq!(vanilla_runs, 1);
    assert_eq!(s.velocity[0], 2.0, "vanilla's result, not the layer's 999");
    assert!(bad.is_failed());
}

#[test]
fn a_failing_replacement_falls_back_to_vanilla() {
    let bad = layer("r", UmiServiceMode::REPLACE, scribble_then_fail);
    assert_eq!(run(&chain(&[&bad])).velocity[0], 2.0);
    assert!(bad.is_failed());
}

#[test]
fn physics_next_outside_the_layer_call_is_denied() {
    let cx = wrapper("w", keeps_cx);
    run(&chain(&[&cx]));
    let kept = *KEPT_CX.lock().unwrap() as *mut UmiPhysicsCx;
    assert_eq!((table().physics_next)(kept), UmiStatus::DENIED);
    assert_eq!(
        (table().physics_next)(core::ptr::null_mut()),
        UmiStatus::INVALID_ARGUMENT
    );
}

#[test]
fn the_layer_gets_the_input_and_its_user_pointer() {
    let cx = mod_with("w", true, &load(true, false));
    let user = UserPtr::new(0x1234 as *mut c_void);
    assert_eq!(
        cx.register_physics(UmiServiceMode::WRAP, Some(reads_input), user),
        UmiStatus::OK
    );
    cx.finish_init(true);
    let mut wanted = input();
    wanted.yaw = 33.5;
    chain(&[&cx]).step(&mut state(), &wanted, &mut vanilla);
    assert_eq!(f32::from_bits(INPUT_YAW.load(Ordering::Relaxed)), 33.5);
}

// ---- budget ------------------------------------------------------------------------

#[test]
fn a_slow_inner_step_is_not_charged_to_the_outer_layer() {
    let cx = wrapper("outer", identity);
    let chain = chain(&[&cx]);
    for _ in 0..(crate::MAX_CONSECUTIVE_OVERRUNS + 5) {
        let mut s = state();
        chain.step(&mut s, &input(), &mut |_| {
            std::thread::sleep(Duration::from_millis(3));
        });
    }
    let stats = cx.physics_stats();
    assert_eq!(stats.overruns, 0, "{stats:?}");
    assert!(cx.is_enabled());
}

#[test]
fn a_layer_slow_on_its_own_time_is_failed_after_a_streak() {
    let cx = wrapper("slow", slow_itself);
    let chain = chain(&[&cx]);
    for _ in 0..crate::MAX_CONSECUTIVE_OVERRUNS {
        run(&chain);
    }
    assert!(cx.is_failed());
    let reason = cx.failure().unwrap();
    assert!(
        reason.ends_with("in its `player_physics` layer"),
        "{reason}"
    );
    assert!(cx.physics_stats().overruns >= crate::MAX_CONSECUTIVE_OVERRUNS);
}
