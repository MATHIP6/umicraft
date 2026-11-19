// SPDX-License-Identifier: MIT OR Apache-2.0

//! Drives the exports of `umicraft_mod!` through a fake host, the way the
//! real loader does.

// The fake host reads a raw slice, as a real host does.
#![allow(unsafe_code)]

use core::ffi::c_void;
use std::collections::HashMap;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
use std::sync::{Mutex, MutexGuard, PoisonError};

use umicraft_modapi::{
    umicraft_mod, Anchor, Chat, ChatReceived, ChatSend, Connected, Disconnected, InitCx,
    KeyPressed, Mod, ModError, Placement, PostTick, PreTick, ScreenClosed, ScreenOpened, Setting,
};
use umicraft_modapi_sys::*;

static SERIAL: Mutex<()> = Mutex::new(());
static LOGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static SUBS: Mutex<Vec<Sub>> = Mutex::new(Vec::new());
static TICKS: Mutex<Vec<u64>> = Mutex::new(Vec::new());
/// What the view-reading handlers saw, one `name=value` line per call.
static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Results of `set_*` calls made by the handlers.
static SET_RESULTS: Mutex<Vec<Result<(), ModError>>> = Mutex::new(Vec::new());
/// Texts the fake host was asked to set.
static SET_CALLS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static DROPS: AtomicUsize = AtomicUsize::new(0);
static DROPS_AT_SHUTDOWN: AtomicUsize = AtomicUsize::new(usize::MAX);
static SHUTDOWNS: AtomicUsize = AtomicUsize::new(0);
static FAIL_INIT: AtomicBool = AtomicBool::new(false);
static PANIC_ON_TICK: AtomicBool = AtomicBool::new(false);
static REFUSE_SUBSCRIBE: AtomicBool = AtomicBool::new(false);
static REFUSE_HUD: AtomicBool = AtomicBool::new(false);
/// Setting definitions the fake host received, one `kind:key:label:details` each.
static DEFS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// The values `setting_get` answers with, by key; a missing key is unknown.
static VALUES: Mutex<Option<HashMap<String, UmiSettingValue>>> = Mutex::new(None);
static WIDGETS: Mutex<Vec<Widget>> = Mutex::new(Vec::new());
/// Every `hud_*` call the fake host received, as text.
static HUD_CALLS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static KEYS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static COMMANDS: Mutex<Vec<Command>> = Mutex::new(Vec::new());
static CHAT: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// What the mod registered, for the tests to read through.
static UI: Mutex<Option<Ui>> = Mutex::new(None);

#[derive(Clone, Copy)]
struct Ui {
    show: Setting<bool>,
    tint: Setting<u32>,
    volume: Setting<f32>,
    mode: Setting<u32>,
    unknown: Setting<u32>,
    chat: Chat,
}

struct Widget {
    key: String,
    label: String,
    placement: UmiHudPlacement,
    draw: UmiHudDrawFn,
    user: usize,
}

#[derive(Clone, Copy)]
struct Command {
    run: UmiCommandFn,
    user: usize,
}

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

fn see(line: String) {
    SEEN.lock().unwrap().push(line);
}

/// Observers read every view; the chat intercepts also write through it.
fn register_view_handlers(cx: &mut InitCx<'_>) -> Result<(), ModError> {
    cx.intercept::<ChatSend>(0, |e, _| {
        see(format!("send={}", e.text()));
        let result = e.set_text("replaced");
        see(format!("send_after={}", e.text()));
        SET_RESULTS.lock().unwrap().push(result);
    })?;
    cx.observe::<ChatReceived>(0, |e| {
        see(format!("chat={} {:?}", e.json(), e.position()));
    })?;
    cx.intercept::<ChatReceived>(1, |e, _| {
        let result = e.set_json("{\"text\":\"new\"}");
        see(format!("chat_after={}", e.json()));
        SET_RESULTS.lock().unwrap().push(result);
    })?;
    cx.observe::<Connected>(0, |e| see(format!("connected={}", e.address())))?;
    cx.observe::<Disconnected>(0, |e| see(format!("disconnected={}", e.reason())))?;
    cx.observe::<ScreenOpened>(0, |e| see(format!("opened={}", e.name())))?;
    cx.observe::<ScreenClosed>(0, |e| see(format!("closed={}", e.name())))
}

/// Settings, a widget, a key binding and a command, as a mod author writes them.
fn register_ui(cx: &mut InitCx<'_>) -> Result<(), ModError> {
    let ui = Ui {
        show: cx.toggle("show", "Show", true)?,
        tint: cx.color("tint", "Tint", 0x00FF_8800)?,
        volume: cx.slider("volume", "Volume", 0.0, 10.0, 0.5, 3.0)?,
        mode: cx.choice("mode", "Mode", &["a", "b", "c"], 1)?,
        unknown: cx.color("other", "Other", 7)?,
        chat: cx.chat(),
    };
    *UI.lock().unwrap() = Some(ui);
    let placement = Placement::new(Anchor::BottomRight)
        .offset(2.0, 3.0)
        .scale(1.5);
    cx.hud_widget("clock", "Clock", placement, move |hud| {
        if ui.show.get() {
            hud.line("on", ui.tint.get());
            hud.reserve(10.0, 5.0);
        }
    })?;
    cx.hud_widget("gauge", "Gauge", Placement::new(Anchor::Center), |hud| {
        hud.label(1.0, 2.0, "text", 0xFF_FFFF, 0.5, 2.0);
        hud.cell(3.0, 4.0, 5.0, 6.0, true, "W", 0x00_FF00);
        hud.tick(7.0, 8.0, 9.0, 10.0, 0x00_00FF, 0.25);
    })?;
    cx.key("greet", "Greet", "KeyH")?;
    cx.observe::<KeyPressed>(0, |e| see(format!("key={}", e.key())))?;
    cx.command("say", "/say <text>", move |args| match args {
        "boom" => panic!("boom"),
        "" => Err(ModError::msg("empty")),
        text => {
            ui.chat.print(text);
            Ok(())
        }
    })
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
        register_view_handlers(cx)?;
        register_ui(cx)?;
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

/// Enforces what the real host does: `MUTABLE` and a chat event, valid UTF-8.
/// Repoints the payload's string at a leaked copy, as the host repoints into
/// its owned text.
extern "C" fn fake_set_text(event: *mut UmiEventHeader, text: UmiStr) -> UmiStatus {
    // SAFETY: the module under test passes the live payload it was given.
    let header = unsafe { *event };
    if header.flags & UMI_EVENT_MUTABLE == 0 {
        return UmiStatus::DENIED;
    }
    // SAFETY: the module under test passes a live slice for the call.
    let bytes = unsafe { std::slice::from_raw_parts(text.ptr, text.len) };
    let Ok(copy) = std::str::from_utf8(bytes) else {
        return UmiStatus::INVALID_ARGUMENT;
    };
    let copy: &'static str = Box::leak(copy.to_owned().into_boxed_str());
    let repointed = UmiStr::from_static(copy);
    // SAFETY: the id says which payload type `event` points to.
    match header.id {
        UmiEventId::CHAT_SEND => unsafe { (*event.cast::<UmiChatSend>()).text = repointed },
        UmiEventId::CHAT_RECEIVED => unsafe { (*event.cast::<UmiChatReceived>()).json = repointed },
        _ => return UmiStatus::INVALID_ARGUMENT,
    }
    SET_CALLS.lock().unwrap().push(copy.to_owned());
    UmiStatus::OK
}

fn text_of(text: UmiStr) -> String {
    // SAFETY: the module under test passes a live UTF-8 slice for the call.
    let bytes = unsafe { std::slice::from_raw_parts(text.ptr, text.len) };
    String::from_utf8_lossy(bytes).into_owned()
}

extern "C" fn fake_register_setting(_me: UmiModHandle, def: *const UmiSettingDef) -> UmiStatus {
    // SAFETY: the module under test passes a definition that lives for the call.
    let def = unsafe { *def };
    let details = match def.kind {
        UmiSettingKind::TOGGLE => format!("{}", def.toggle_default),
        UmiSettingKind::SLIDER => format!(
            "{}..{}/{}={}",
            def.slider_min, def.slider_max, def.slider_step, def.slider_default
        ),
        UmiSettingKind::CHOICE => {
            // SAFETY: `choices` points at `choices_len` strings for the call.
            let labels = unsafe { std::slice::from_raw_parts(def.choices, def.choices_len) };
            let labels: Vec<_> = labels.iter().map(|l| text_of(*l)).collect();
            format!("{labels:?}={}", def.choice_default)
        }
        _ => format!("{:06x}", def.color_default),
    };
    assert_eq!(def.size as usize, size_of::<UmiSettingDef>());
    DEFS.lock().unwrap().push(format!(
        "{}:{}:{}:{details}",
        def.kind.0,
        text_of(def.key),
        text_of(def.label)
    ));
    UmiStatus::OK
}

extern "C" fn fake_setting_get(
    _me: UmiModHandle,
    key: UmiStr,
    out: *mut UmiSettingValue,
) -> UmiStatus {
    let values = VALUES.lock().unwrap();
    match values.as_ref().and_then(|v| v.get(&text_of(key))) {
        Some(value) => {
            // SAFETY: the module under test passes a writable value.
            unsafe { *out = *value };
            UmiStatus::OK
        }
        None => UmiStatus::INVALID_ARGUMENT,
    }
}

extern "C" fn fake_register_hud_widget(
    _me: UmiModHandle,
    key: UmiStr,
    label: UmiStr,
    placement: UmiHudPlacement,
    draw: Option<UmiHudDrawFn>,
    user: *mut c_void,
) -> UmiStatus {
    WIDGETS.lock().unwrap().push(Widget {
        key: text_of(key),
        label: text_of(label),
        placement,
        draw: draw.expect("trampoline is never null"),
        user: user as usize,
    });
    UmiStatus::OK
}

fn hud_call(call: String) -> UmiStatus {
    if REFUSE_HUD.load(SeqCst) {
        return UmiStatus::INVALID_ARGUMENT;
    }
    HUD_CALLS.lock().unwrap().push(call);
    UmiStatus::OK
}

extern "C" fn fake_hud_line(_hud: *mut UmiHudCx, text: UmiStr, color: u32) -> UmiStatus {
    hud_call(format!("line {} {color:06x}", text_of(text)))
}

extern "C" fn fake_hud_label(
    _hud: *mut UmiHudCx,
    x: f32,
    y: f32,
    text: UmiStr,
    color: u32,
    alpha: f32,
    scale: f32,
) -> UmiStatus {
    hud_call(format!(
        "label {x} {y} {} {color:06x} {alpha} {scale}",
        text_of(text)
    ))
}

extern "C" fn fake_hud_cell(
    _hud: *mut UmiHudCx,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    pressed: u8,
    text: UmiStr,
    color: u32,
) -> UmiStatus {
    hud_call(format!(
        "cell {x} {y} {width} {height} {pressed} {} {color:06x}",
        text_of(text)
    ))
}

extern "C" fn fake_hud_tick(
    _hud: *mut UmiHudCx,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: u32,
    alpha: f32,
) -> UmiStatus {
    hud_call(format!("tick {x} {y} {width} {height} {color:06x} {alpha}"))
}

extern "C" fn fake_hud_reserve(_hud: *mut UmiHudCx, width: f32, height: f32) -> UmiStatus {
    hud_call(format!("reserve {width} {height}"))
}

extern "C" fn fake_register_key(
    _me: UmiModHandle,
    key: UmiStr,
    label: UmiStr,
    default_binding: UmiStr,
) -> UmiStatus {
    KEYS.lock().unwrap().push(format!(
        "{}:{}:{}",
        text_of(key),
        text_of(label),
        text_of(default_binding)
    ));
    UmiStatus::OK
}

extern "C" fn fake_register_command(
    _me: UmiModHandle,
    name: UmiStr,
    usage: UmiStr,
    run: Option<UmiCommandFn>,
    user: *mut c_void,
) -> UmiStatus {
    DEFS.lock()
        .unwrap()
        .push(format!("command:{}:{}", text_of(name), text_of(usage)));
    COMMANDS.lock().unwrap().push(Command {
        run: run.expect("trampoline is never null"),
        user: user as usize,
    });
    UmiStatus::OK
}

extern "C" fn fake_chat_print(_me: UmiModHandle, text: UmiStr) -> UmiStatus {
    CHAT.lock().unwrap().push(text_of(text));
    UmiStatus::OK
}

fn host() -> UmiHost {
    UmiHost {
        size: size_of::<UmiHost>(),
        version: UMI_API_VERSION,
        log: fake_log,
        subscribe: fake_subscribe,
        event_set_text: fake_set_text,
        register_setting: fake_register_setting,
        setting_get: fake_setting_get,
        register_hud_widget: fake_register_hud_widget,
        hud_line: fake_hud_line,
        hud_label: fake_hud_label,
        hud_cell: fake_hud_cell,
        hud_tick: fake_hud_tick,
        hud_reserve: fake_hud_reserve,
        register_key: fake_register_key,
        register_command: fake_register_command,
        chat_print: fake_chat_print,
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
    for list in [&SEEN, &SET_CALLS, &DEFS, &HUD_CALLS, &KEYS, &CHAT] {
        list.lock().unwrap().clear();
    }
    WIDGETS.lock().unwrap().clear();
    COMMANDS.lock().unwrap().clear();
    *VALUES.lock().unwrap() = Some(HashMap::new());
    *UI.lock().unwrap() = None;
    SET_RESULTS.lock().unwrap().clear();
    for counter in [&DROPS, &SHUTDOWNS] {
        counter.store(0, SeqCst);
    }
    for flag in [&FAIL_INIT, &PANIC_ON_TICK, &REFUSE_SUBSCRIBE, &REFUSE_HUD] {
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

fn sub_with(event: UmiEventId, mode: UmiMode) -> Sub {
    *SUBS
        .lock()
        .unwrap()
        .iter()
        .find(|s| s.event == event && s.mode == mode)
        .unwrap()
}

fn text(s: &'static str) -> UmiStr {
    UmiStr::from_static(s)
}

fn chat_send(text: UmiStr, flags: u32) -> UmiChatSend {
    UmiChatSend {
        header: header(UmiEventId::CHAT_SEND, size_of::<UmiChatSend>(), flags),
        text,
    }
}

fn chat_received(json: UmiStr, position: u8, flags: u32) -> UmiChatReceived {
    UmiChatReceived {
        header: header(
            UmiEventId::CHAT_RECEIVED,
            size_of::<UmiChatReceived>(),
            flags,
        ),
        json,
        position,
    }
}

fn seen() -> Vec<String> {
    SEEN.lock().unwrap().clone()
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
    assert_eq!(subs.len(), 10);
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

#[test]
fn chat_received_view_reads_json_and_position() {
    let _guard = fresh();
    init_ok();
    let sub = sub_with(UmiEventId::CHAT_RECEIVED, UmiMode::OBSERVE);
    for (raw, name) in [
        (0, "Chat"),
        (1, "System"),
        (2, "ActionBar"),
        (9, "Other(9)"),
    ] {
        SEEN.lock().unwrap().clear();
        let mut payload = chat_received(text("{\"text\":\"hi\"}"), raw, 0);
        assert_eq!(call(sub, &mut payload), UmiStatus::OK);
        assert_eq!(seen(), [format!("chat={{\"text\":\"hi\"}} {name}")]);
    }
}

#[test]
fn chat_send_set_text_through_intercept_is_visible_and_ok() {
    let _guard = fresh();
    init_ok();
    let sub = sub_with(UmiEventId::CHAT_SEND, UmiMode::INTERCEPT);
    let mut payload = chat_send(text("hi :shrug:"), UMI_EVENT_MUTABLE);
    assert_eq!(call(sub, &mut payload), UmiStatus::OK);
    assert_eq!(seen(), ["send=hi :shrug:", "send_after=replaced"]);
    assert_eq!(*SET_RESULTS.lock().unwrap(), [Ok(())]);
    assert_eq!(*SET_CALLS.lock().unwrap(), ["replaced"]);
    // The host-side payload now points at the replacement.
    // SAFETY: the fake host leaked the replacement, so it lives forever.
    let now = unsafe { std::slice::from_raw_parts(payload.text.ptr, payload.text.len) };
    assert_eq!(now, b"replaced");
}

#[test]
fn chat_received_set_json_through_intercept_works() {
    let _guard = fresh();
    init_ok();
    let sub = sub_with(UmiEventId::CHAT_RECEIVED, UmiMode::INTERCEPT);
    let mut payload = chat_received(text("{}"), 0, UMI_EVENT_MUTABLE);
    assert_eq!(call(sub, &mut payload), UmiStatus::OK);
    assert_eq!(seen(), ["chat_after={\"text\":\"new\"}"]);
    assert_eq!(*SET_RESULTS.lock().unwrap(), [Ok(())]);
}

#[test]
fn set_text_without_the_mutable_flag_is_denied() {
    let _guard = fresh();
    init_ok();
    let sub = sub_with(UmiEventId::CHAT_SEND, UmiMode::INTERCEPT);
    let mut payload = chat_send(text("hi"), 0);
    assert_eq!(call(sub, &mut payload), UmiStatus::OK);
    assert_eq!(
        *SET_RESULTS.lock().unwrap(),
        [Err(ModError::HostRefused(UmiStatus::DENIED))]
    );
    assert_eq!(seen(), ["send=hi", "send_after=hi"]);
    assert!(SET_CALLS.lock().unwrap().is_empty());
}

#[test]
fn connection_and_screen_views_read_their_text() {
    let _guard = fresh();
    init_ok();
    let mut connected = UmiConnected {
        header: header(UmiEventId::CONNECTED, size_of::<UmiConnected>(), 0),
        address: text("play.example.org:25565"),
    };
    let mut disconnected = UmiDisconnected {
        header: header(UmiEventId::DISCONNECTED, size_of::<UmiDisconnected>(), 0),
        reason: text("Timed out"),
    };
    let screen = |id, name| UmiScreen {
        header: header(id, size_of::<UmiScreen>(), 0),
        name,
    };
    let mut opened = screen(UmiEventId::SCREEN_OPENED, text("inventory"));
    let mut closed = screen(UmiEventId::SCREEN_CLOSED, text("chat"));
    assert_eq!(
        call(sub_for(UmiEventId::CONNECTED), &mut connected),
        UmiStatus::OK
    );
    assert_eq!(
        call(sub_for(UmiEventId::DISCONNECTED), &mut disconnected),
        UmiStatus::OK
    );
    assert_eq!(
        call(sub_for(UmiEventId::SCREEN_OPENED), &mut opened),
        UmiStatus::OK
    );
    assert_eq!(
        call(sub_for(UmiEventId::SCREEN_CLOSED), &mut closed),
        UmiStatus::OK
    );
    assert_eq!(
        seen(),
        [
            "connected=play.example.org:25565",
            "disconnected=Timed out",
            "opened=inventory",
            "closed=chat"
        ]
    );
}

#[test]
fn the_two_screen_events_are_not_interchangeable() {
    let _guard = fresh();
    init_ok();
    let mut opened = UmiScreen {
        header: header(UmiEventId::SCREEN_OPENED, size_of::<UmiScreen>(), 0),
        name: text("chat"),
    };
    let closed_sub = sub_for(UmiEventId::SCREEN_CLOSED);
    assert_eq!(call(closed_sub, &mut opened), UmiStatus::INVALID_ARGUMENT);
    assert!(seen().is_empty());
}

#[test]
fn view_with_the_wrong_event_id_is_invalid() {
    let _guard = fresh();
    init_ok();
    let sub = sub_with(UmiEventId::CHAT_SEND, UmiMode::INTERCEPT);
    let mut payload = chat_received(text("{}"), 0, UMI_EVENT_MUTABLE);
    assert_eq!(call(sub, &mut payload), UmiStatus::INVALID_ARGUMENT);
}

#[test]
fn null_empty_and_invalid_utf8_strings_read_as_empty() {
    let _guard = fresh();
    init_ok();
    let sub = sub_with(UmiEventId::CHAT_SEND, UmiMode::INTERCEPT);
    let bad = [0xff_u8, 0xfe];
    for text in [
        UmiStr::EMPTY,
        UmiStr {
            ptr: bad.as_ptr(),
            len: 0,
        },
        UmiStr {
            ptr: bad.as_ptr(),
            len: bad.len(),
        },
    ] {
        SEEN.lock().unwrap().clear();
        assert_eq!(call(sub, &mut chat_send(text, 0)), UmiStatus::OK);
        assert_eq!(seen()[0], "send=");
    }
}

fn ui() -> Ui {
    UI.lock().unwrap().unwrap()
}

fn set_value(key: &str, value: UmiSettingValue) {
    VALUES
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .insert(key.to_owned(), value);
}

fn value(kind: UmiSettingKind) -> UmiSettingValue {
    UmiSettingValue {
        kind,
        toggle: 0,
        slider: 0.0,
        choice: 0,
        color: 0,
    }
}

fn widget() -> (UmiHudDrawFn, *mut c_void) {
    let widgets = WIDGETS.lock().unwrap();
    (widgets[0].draw, widgets[0].user as *mut c_void)
}

fn draw() -> UmiStatus {
    let (draw, user) = widget();
    let mut fake_cx = 0_u8;
    draw(std::ptr::from_mut(&mut fake_cx).cast(), user)
}

fn run_command(args: &'static str) -> UmiStatus {
    let command = COMMANDS.lock().unwrap()[0];
    (command.run)(text(args), command.user as *mut c_void)
}

#[test]
fn settings_are_registered_with_their_definitions() {
    let _guard = fresh();
    init_ok();
    assert_eq!(
        DEFS.lock().unwrap()[..4],
        [
            "1:show:Show:1",
            "4:tint:Tint:ff8800",
            "2:volume:Volume:0..10/0.5=3",
            "3:mode:Mode:[\"a\", \"b\", \"c\"]=1",
        ]
    );
}

#[test]
fn setting_get_reads_the_hosts_value() {
    let _guard = fresh();
    init_ok();
    set_value(
        "show",
        UmiSettingValue {
            toggle: 0,
            ..value(UmiSettingKind::TOGGLE)
        },
    );
    set_value(
        "tint",
        UmiSettingValue {
            color: 0x123456,
            ..value(UmiSettingKind::COLOR)
        },
    );
    set_value(
        "volume",
        UmiSettingValue {
            slider: 7.5,
            ..value(UmiSettingKind::SLIDER)
        },
    );
    set_value(
        "mode",
        UmiSettingValue {
            choice: 2,
            ..value(UmiSettingKind::CHOICE)
        },
    );
    let ui = ui();
    assert!(!ui.show.get());
    assert_eq!(ui.tint.get(), 0x123456);
    assert!((ui.volume.get() - 7.5).abs() < f32::EPSILON);
    assert_eq!(ui.mode.get(), 2);
}

#[test]
fn setting_get_falls_back_to_the_default_on_error_or_wrong_kind() {
    let _guard = fresh();
    init_ok();
    let ui = ui();
    // Unknown key: the fake host answers INVALID_ARGUMENT.
    assert!(ui.show.get());
    assert_eq!(ui.unknown.get(), 7);
    // A host answering another kind than asked is ignored, not misread.
    set_value(
        "tint",
        UmiSettingValue {
            choice: 1,
            ..value(UmiSettingKind::CHOICE)
        },
    );
    assert_eq!(ui.tint.get(), 0x00FF_8800);
}

#[test]
fn hud_widget_is_registered_and_draws_through_the_host() {
    let _guard = fresh();
    init_ok();
    {
        let widgets = WIDGETS.lock().unwrap();
        assert_eq!(
            (widgets[0].key.as_str(), widgets[0].label.as_str()),
            ("clock", "Clock")
        );
        assert_eq!(
            widgets[0].placement,
            UmiHudPlacement {
                anchor: 8,
                offset_x: 2.0,
                offset_y: 3.0,
                scale: 1.5
            }
        );
    }
    assert_eq!(draw(), UmiStatus::OK);
    assert_eq!(
        *HUD_CALLS.lock().unwrap(),
        ["line on ff8800", "reserve 10 5"]
    );
    // Toggled off: the draw closure sees the new value.
    HUD_CALLS.lock().unwrap().clear();
    set_value("show", value(UmiSettingKind::TOGGLE));
    assert_eq!(draw(), UmiStatus::OK);
    assert!(HUD_CALLS.lock().unwrap().is_empty());
}

#[test]
fn a_refused_hud_call_stops_the_frame_but_not_the_widget() {
    let _guard = fresh();
    init_ok();
    REFUSE_HUD.store(true, SeqCst);
    assert_eq!(draw(), UmiStatus::OK);
    assert!(HUD_CALLS.lock().unwrap().is_empty());
    REFUSE_HUD.store(false, SeqCst);
    assert_eq!(draw(), UmiStatus::OK);
    assert_eq!(HUD_CALLS.lock().unwrap().len(), 2);
}

#[test]
fn draw_trampoline_rejects_a_null_cx_and_a_null_user() {
    let _guard = fresh();
    init_ok();
    let (draw, user) = widget();
    assert_eq!(
        draw(std::ptr::null_mut(), user),
        UmiStatus::INVALID_ARGUMENT
    );
    let mut fake_cx = 0_u8;
    let status = draw(
        std::ptr::from_mut(&mut fake_cx).cast(),
        std::ptr::null_mut(),
    );
    assert_eq!(status, UmiStatus::INVALID_ARGUMENT);
}

#[test]
fn every_hud_primitive_reaches_the_host() {
    let _guard = fresh();
    init_ok();
    let (draw, user) = {
        let widgets = WIDGETS.lock().unwrap();
        (widgets[1].draw, widgets[1].user as *mut c_void)
    };
    let mut fake_cx = 0_u8;
    assert_eq!(
        draw(std::ptr::from_mut(&mut fake_cx).cast(), user),
        UmiStatus::OK
    );
    assert_eq!(
        *HUD_CALLS.lock().unwrap(),
        [
            "label 1 2 text ffffff 0.5 2",
            "cell 3 4 5 6 1 W 00ff00",
            "tick 7 8 9 10 0000ff 0.25",
        ]
    );
}

#[test]
fn key_binding_is_registered_and_key_pressed_is_viewed() {
    let _guard = fresh();
    init_ok();
    assert_eq!(*KEYS.lock().unwrap(), ["greet:Greet:KeyH"]);
    let mut pressed = UmiKeyPressed {
        header: header(UmiEventId::KEY_PRESSED, size_of::<UmiKeyPressed>(), 0),
        key: text("greet"),
    };
    assert_eq!(
        call(sub_for(UmiEventId::KEY_PRESSED), &mut pressed),
        UmiStatus::OK
    );
    assert_eq!(seen(), ["key=greet"]);
}

#[test]
fn command_is_registered_and_maps_results_to_statuses() {
    let _guard = fresh();
    init_ok();
    assert!(DEFS
        .lock()
        .unwrap()
        .contains(&"command:say:/say <text>".to_string()));
    assert_eq!(run_command("hi there"), UmiStatus::OK);
    assert_eq!(*CHAT.lock().unwrap(), ["hi there"]);
    assert_eq!(run_command(""), UmiStatus::ERROR);
    assert_eq!(run_command("boom"), UmiStatus::PANICKED);
    assert_eq!(CHAT.lock().unwrap().len(), 1);
}

#[test]
fn command_with_a_null_user_is_invalid() {
    let _guard = fresh();
    init_ok();
    let command = COMMANDS.lock().unwrap()[0];
    assert_eq!(
        (command.run)(text("x"), std::ptr::null_mut()),
        UmiStatus::INVALID_ARGUMENT
    );
}

#[test]
fn chat_handle_prints_through_the_host() {
    let _guard = fresh();
    init_ok();
    ui().chat.print("local line");
    assert_eq!(*CHAT.lock().unwrap(), ["local line"]);
}
