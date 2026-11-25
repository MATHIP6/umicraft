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
    umicraft_mod, Anchor, Block, ButtonPressed, Chat, ChatReceived, ChatSend, Connected,
    Disconnected, Frame, GameMode, InitCx, KeyPressed, Mod, ModError, Module, ModuleSpec,
    ModuleToggled, PacketReceived, PacketSend, Packets, Physics, Placement, PostPass, PostTick,
    PreTick, ScreenClosed, ScreenOpened, Setting, World,
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
/// Make the physics layer fail, or register it as a replacing layer.
static FAIL_PHYSICS: AtomicBool = AtomicBool::new(false);
static REPLACE_PHYSICS: AtomicBool = AtomicBool::new(false);
/// Make the physics layer toggle flight and the game mode (API 1.1).
static FLIGHT_LAYER: AtomicBool = AtomicBool::new(false);
/// Calls of the fake `physics_next`.
static NEXT_CALLS: AtomicUsize = AtomicUsize::new(0);
/// The physics layers the fake host registered, with their mode.
static LAYERS: Mutex<Vec<Layer>> = Mutex::new(Vec::new());
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
/// Packets the fake host was asked to inject, as `(id, body)`.
static INJECTED: Mutex<Vec<(u32, Vec<u8>)>> = Mutex::new(Vec::new());
/// The status `packet_send` answers with.
static PACKET_STATUS: Mutex<UmiStatus> = Mutex::new(UmiStatus::OK);
/// Post passes the fake host accepted, as `key:label:wgsl`.
static POST_PASSES: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Every `post_set_param` call, as `key:index:x,y,z,w`.
static POST_PARAMS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// What the fake host answered a refused `post_pass` with.
static POST_REFUSALS: Mutex<Vec<Result<PostPass, ModError>>> = Mutex::new(Vec::new());
/// What the mod registered, for the tests to read through.
static UI: Mutex<Option<Ui>> = Mutex::new(None);
/// Every menu registration (API 1.2) the fake host received, as text.
static MENU: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// The module the mod declared, for the tests to read through.
static RADAR: Mutex<Option<Module>> = Mutex::new(None);

#[derive(Clone, Copy)]
struct Ui {
    show: Setting<bool>,
    tint: Setting<u32>,
    volume: Setting<f32>,
    mode: Setting<u32>,
    unknown: Setting<u32>,
    chat: Chat,
    packets: Packets,
    world: World,
    post: PostPass,
}

#[derive(Clone, Copy)]
struct Layer {
    mode: UmiServiceMode,
    step: UmiPhysicsFn,
    user: usize,
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
    cx.observe::<PacketReceived>(0, |e| {
        see(format!("rx={:#04x} {:?}", e.id(), e.data()));
    })?;
    cx.observe::<PacketSend>(0, |e| {
        see(format!("tx={:#04x} {:?}", e.id(), e.data()));
    })?;
    cx.observe::<Connected>(0, |e| see(format!("connected={}", e.address())))?;
    cx.observe::<Disconnected>(0, |e| see(format!("disconnected={}", e.reason())))?;
    cx.observe::<Frame>(0, |e| {
        see(format!(
            "frame={} {:?} fps={} ms={} yaw={} pitch={} clicks={}/{} ping={:?} sprint={}",
            e.now_ms(),
            e.position(),
            e.fps(),
            e.frame_ms(),
            e.yaw(),
            e.pitch(),
            e.left_clicks(),
            e.right_clicks(),
            e.ping_ms(),
            e.sprinting()
        ));
    })?;
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
        packets: cx.packets(),
        world: cx.world(),
        post: cx.post_pass("tint", "Tint", "fn fs_main() {}")?,
    };
    let mut refusals = POST_REFUSALS.lock().unwrap();
    refusals.push(cx.post_pass("tint", "Again", "fn fs_main() {}"));
    refusals.push(cx.post_pass("broken", "Broken", "broken"));
    refusals.push(cx.post_pass(&"k".repeat(65), "Long", "fn fs_main() {}"));
    drop(refusals);
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
        hud.clip(0.0, 0.0, 20.0, 10.0, |hud| {
            hud.circle(5.0, 5.0, 3.0, 0xFF_0000, 1.0);
            hud.ring(5.0, 5.0, 4.0, 0xFF_0000, 0.5, 1.5);
        });
        hud.polygon(&[[0.0, 0.0], [4.0, 0.0], [0.0, 4.0]], 0x12_3456, 1.0);
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

/// A tab, a module with a setting, a button and an attached widget, and a button
/// on the mod's own card (API 1.2).
fn register_menu(cx: &mut InitCx<'_>) -> Result<(), ModError> {
    let pvp = cx.category("pvp", "PvP")?;
    let radar = cx.module(
        ModuleSpec::new("radar", "Radar")
            .description("Players around")
            .category(pvp),
    )?;
    let mut card = cx.card(radar);
    card.toggle("names", "Names", true)?;
    card.button("clear", "Marks", "Clear")?;
    card.attach_widget("clock")?;
    let post = UI.lock().unwrap().map(|ui| ui.post);
    if let Some(post) = post {
        card.attach_post_pass(&post)?;
    }
    cx.button("reload", "List", "Reload")?;
    cx.observe::<ModuleToggled>(0, |e| see(format!("module={}:{}", e.module(), e.enabled())))?;
    cx.observe::<ButtonPressed>(0, |e| see(format!("button={}:{}", e.module(), e.key())))?;
    *RADAR.lock().unwrap() = Some(radar);
    Ok(())
}

/// Reports the flight facts, then starts flying and predicts creative.
fn flight_layer(physics: &mut Physics<'_>) -> Result<(), ModError> {
    let state = physics.state_mut();
    see(format!(
        "flight {} {} {} {:?}",
        state.flying(),
        state.allow_flying(),
        state.fly_speed(),
        state.game_mode()
    ));
    state.set_flying(true);
    state.set_game_mode(GameMode::Creative);
    Ok(())
}

/// Reads the input, changes the writable state and runs the inner layers.
fn physics_layer(physics: &mut Physics<'_>) -> Result<(), ModError> {
    if FLIGHT_LAYER.load(SeqCst) {
        return flight_layer(physics);
    }
    let input = physics.input();
    see(format!(
        "input {} {} {} {} {} {} {}",
        input.forward(),
        input.strafe(),
        input.jump(),
        input.sneak(),
        input.sprint(),
        input.yaw(),
        input.pitch()
    ));
    assert!(!FAIL_PHYSICS.load(SeqCst), "layer failed");
    physics.state_mut().set_on_ground(true);
    physics.state_mut().set_sprinting(false);
    physics.state_mut().set_fall_distance(2.5);
    let result = physics.next();
    let state = physics.state();
    see(format!(
        "state {:?} {:?} {} {} {} {} {} {} {}",
        state.position(),
        state.velocity(),
        state.on_ground(),
        state.sprinting(),
        state.sneaking(),
        state.flying(),
        state.in_water(),
        state.yaw(),
        state.pitch()
    ));
    see(format!("fall {}", state.fall_distance()));
    physics.state_mut().set_velocity([1.0, 2.0, 3.0]);
    physics.state_mut().set_position([4.0, 5.0, 6.0]);
    result
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
        register_menu(cx)?;
        if REPLACE_PHYSICS.load(SeqCst) {
            cx.replace_physics(physics_layer)?;
        } else {
            cx.wrap_physics(physics_layer)?;
        }
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

extern "C" fn fake_packet_send(_me: UmiModHandle, id: u32, data: UmiBytes) -> UmiStatus {
    let body = if data.ptr.is_null() {
        assert_eq!(data.len, 0, "a null pointer carries no bytes");
        Vec::new()
    } else {
        // SAFETY: the module under test passes a live slice for the call.
        unsafe { std::slice::from_raw_parts(data.ptr, data.len) }.to_vec()
    };
    INJECTED.lock().unwrap().push((id, body));
    *PACKET_STATUS.lock().unwrap()
}

extern "C" fn fake_register_physics(
    _me: UmiModHandle,
    mode: UmiServiceMode,
    step: Option<UmiPhysicsFn>,
    user: *mut c_void,
) -> UmiStatus {
    LAYERS.lock().unwrap().push(Layer {
        mode,
        step: step.expect("trampoline is never null"),
        user: user as usize,
    });
    UmiStatus::OK
}

/// Like the host: a replacing layer has nothing inside; the state gains 1 in y.
extern "C" fn fake_physics_next(cx: *mut UmiPhysicsCx) -> UmiStatus {
    assert!(!cx.is_null());
    if REPLACE_PHYSICS.load(SeqCst) {
        return UmiStatus::UNSUPPORTED;
    }
    NEXT_CALLS.fetch_add(1, SeqCst);
    // SAFETY: the test passes this cx pointer to the state it owns (see
    // `run_layer`), as the real host does.
    unsafe { (*cx.cast::<UmiPlayerPhysics>()).velocity[1] += 1.0 };
    UmiStatus::OK
}

/// `x == 0` is stone with metadata 3, `x == 1` an unloaded chunk, else refused.
extern "C" fn fake_world_block(
    _me: UmiModHandle,
    x: i32,
    _y: i32,
    _z: i32,
    out: *mut u32,
) -> UmiStatus {
    match x {
        0 => {
            // SAFETY: the module under test passes a writable `u32`.
            unsafe { *out = 1 << 4 | 3 };
            UmiStatus::OK
        }
        1 => UmiStatus::UNAVAILABLE,
        _ => UmiStatus::UNSUPPORTED,
    }
}

/// Visits three entities; a non-`OK` answer stops and is returned, as the
/// real host does.
extern "C" fn fake_world_entities(
    _me: UmiModHandle,
    visit: Option<UmiEntityVisitFn>,
    user: *mut c_void,
) -> UmiStatus {
    let visit = visit.expect("trampoline is never null");
    for (id, kind, name) in [(7, "player", "Ann"), (8, "zombie", ""), (9, "item", "x")] {
        let entity = UmiEntity {
            size: size_of::<UmiEntity>() as u32,
            id,
            kind: text(kind),
            position: [f64::from(id), 64.0, -1.5],
            yaw: 90.0,
            pitch: -10.0,
            name: text(name),
            health: 0.0,
            max_health: 0.0,
            armor: 0.0,
            team: UmiStr::EMPTY,
            width: 0.0,
            height: 0.0,
            velocity: [0.0; 3],
            flags: 0,
            is_player: 0,
            render_position: [f64::from(id), 64.0, -1.5],
            ..entity_1_4_tail()
        };
        let status = visit(&entity, user);
        if status != UmiStatus::OK {
            return status;
        }
    }
    UmiStatus::OK
}

extern "C" fn fake_register_post_pass(
    _me: UmiModHandle,
    key: UmiStr,
    label: UmiStr,
    wgsl: UmiStr,
) -> UmiStatus {
    let mut passes = POST_PASSES.lock().unwrap();
    let entry = format!("{}:{}:{}", text_of(key), text_of(label), text_of(wgsl));
    if passes
        .iter()
        .any(|p| p.starts_with(&format!("{}:", text_of(key))))
    {
        return UmiStatus::CONFLICT;
    }
    if text_of(wgsl) == "broken" {
        return UmiStatus::INVALID_ARGUMENT;
    }
    passes.push(entry);
    UmiStatus::OK
}

extern "C" fn fake_post_set_param(
    _me: UmiModHandle,
    key: UmiStr,
    index: u32,
    x: f32,
    y: f32,
    z: f32,
    w: f32,
) -> UmiStatus {
    if index > 3 {
        return UmiStatus::INVALID_ARGUMENT;
    }
    POST_PARAMS
        .lock()
        .unwrap()
        .push(format!("{}:{index}:{x},{y},{z},{w}", text_of(key)));
    UmiStatus::OK
}

fn menu(line: String) -> UmiStatus {
    MENU.lock().unwrap().push(line);
    UmiStatus::OK
}

extern "C" fn fake_register_category(_me: UmiModHandle, key: UmiStr, label: UmiStr) -> UmiStatus {
    menu(format!("category:{}:{}", text_of(key), text_of(label)))
}

extern "C" fn fake_register_module(_me: UmiModHandle, def: *const UmiModuleDef) -> UmiStatus {
    // SAFETY: the module under test passes a definition that lives for the call.
    let def = unsafe { *def };
    assert_eq!(def.size as usize, size_of::<UmiModuleDef>());
    menu(format!(
        "module:{}:{}:{}:{}:{}",
        text_of(def.key),
        text_of(def.label),
        text_of(def.description),
        text_of(def.category),
        def.flags
    ))
}

extern "C" fn fake_register_setting_in(
    _me: UmiModHandle,
    module: UmiStr,
    def: *const UmiSettingDef,
) -> UmiStatus {
    // SAFETY: the module under test passes a definition that lives for the call.
    let def = unsafe { *def };
    menu(format!("setting:{}:{}", text_of(module), text_of(def.key)))
}

extern "C" fn fake_register_button(
    _me: UmiModHandle,
    module: UmiStr,
    key: UmiStr,
    label: UmiStr,
    text: UmiStr,
) -> UmiStatus {
    menu(format!(
        "button:{}:{}:{}:{}",
        text_of(module),
        text_of(key),
        text_of(label),
        text_of(text)
    ))
}

extern "C" fn fake_module_attach(
    _me: UmiModHandle,
    module: UmiStr,
    kind: u32,
    key: UmiStr,
) -> UmiStatus {
    menu(format!(
        "attach:{}:{kind}:{}",
        text_of(module),
        text_of(key)
    ))
}

/// Every module is off, so a mod reading it sees the host and not its default.
extern "C" fn fake_module_enabled(_me: UmiModHandle, module: UmiStr, out: *mut u8) -> UmiStatus {
    if text_of(module) != "radar" {
        return UmiStatus::INVALID_ARGUMENT;
    }
    // SAFETY: the module under test passes a writable byte.
    unsafe { *out = 0 };
    UmiStatus::OK
}

/// The fields API 1.4 appended to `UmiEntity`, as a host fills them for an
/// entity it knows nothing more about.
fn entity_1_4_tail() -> UmiEntity {
    UmiEntity {
        size: 0,
        id: 0,
        kind: UmiStr::EMPTY,
        position: [0.0; 3],
        yaw: 0.0,
        pitch: 0.0,
        name: UmiStr::EMPTY,
        health: 0.0,
        max_health: 0.0,
        armor: 0.0,
        team: UmiStr::EMPTY,
        width: 0.0,
        height: 0.0,
        velocity: [0.0; 3],
        flags: 0,
        is_player: 0,
        render_position: [0.0; 3],
        hurt_time: 0,
        death_time: 0,
        held_item: -1,
        held_damage: 0,
        display_name: UmiStr::EMPTY,
        team_color: UMI_NO_COLOR,
        ping: -1,
        uuid_version: 0,
        effect_count: 0,
        effects: [UmiEffect::default(); UMI_MAX_ENTITY_EFFECTS],
    }
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
        packet_send: fake_packet_send,
        register_physics: fake_register_physics,
        physics_next: fake_physics_next,
        world_block: fake_world_block,
        world_entities: fake_world_entities,
        register_post_pass: fake_register_post_pass,
        post_set_param: fake_post_set_param,
        register_camera: fake_register_camera,
        camera_next: fake_camera_next,
        camera_capture: fake_camera_next,
        world_entities_snapshot: fake_world_entities_snapshot,
        hud_outline: fake_hud_outline,
        hud_segment: fake_hud_segment,
        register_category: fake_register_category,
        register_module: fake_register_module,
        register_setting_in: fake_register_setting_in,
        register_button: fake_register_button,
        module_attach: fake_module_attach,
        module_enabled: fake_module_enabled,
        world_local_player: fake_world_local_player,
        hud_circle: fake_hud_circle,
        hud_polygon: fake_hud_polygon,
        hud_clip_push: fake_hud_clip_push,
        hud_clip_pop: fake_hud_clip_pop,
        register_world_layer: fake_register_world_layer,
        world_line: fake_world_line,
        world_box: fake_world_box,
        world_polygon: fake_world_polygon,
        world_highlight: fake_world_highlight,
    }
}

/// The local player: entity 1, named `Me`, on team `blue`.
extern "C" fn fake_world_local_player(_me: UmiModHandle, out: *mut UmiEntity) -> UmiStatus {
    let entity = UmiEntity {
        size: size_of::<UmiEntity>() as u32,
        id: 1,
        kind: text("player"),
        name: text("Me"),
        team: text("blue"),
        display_name: text("\u{a7}9Me"),
        team_color: 0x5555ff,
        ..entity_1_4_tail()
    };
    // SAFETY: the module under test passes a writable entity.
    unsafe { out.write(entity) };
    UmiStatus::OK
}

extern "C" fn fake_hud_circle(
    _hud: *mut UmiHudCx,
    x: f32,
    y: f32,
    radius: f32,
    color: u32,
    alpha: f32,
    thickness: f32,
) -> UmiStatus {
    hud_call(format!(
        "circle {x} {y} {radius} {color:06x} {alpha} {thickness}"
    ))
}

extern "C" fn fake_hud_polygon(
    _hud: *mut UmiHudCx,
    points: *const f32,
    count: usize,
    color: u32,
    alpha: f32,
) -> UmiStatus {
    // SAFETY: the module under test passes `count` points of two floats.
    let floats = unsafe { std::slice::from_raw_parts(points, 2 * count) };
    hud_call(format!("polygon {floats:?} {color:06x} {alpha}"))
}

extern "C" fn fake_hud_clip_push(
    _hud: *mut UmiHudCx,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> UmiStatus {
    hud_call(format!("clip {x} {y} {width} {height}"))
}

extern "C" fn fake_hud_clip_pop(_hud: *mut UmiHudCx) -> UmiStatus {
    hud_call("pop".to_owned())
}

extern "C" fn fake_register_world_layer(
    _me: UmiModHandle,
    _key: UmiStr,
    _draw: Option<UmiWorldDrawFn>,
    _user: *mut c_void,
) -> UmiStatus {
    UmiStatus::UNSUPPORTED
}

#[allow(clippy::too_many_arguments)]
extern "C" fn fake_world_line(
    _world: *mut UmiWorldCx,
    _x1: f64,
    _y1: f64,
    _z1: f64,
    _x2: f64,
    _y2: f64,
    _z2: f64,
    _color: u32,
    _alpha: f32,
    _thickness: f32,
    _flags: u32,
) -> UmiStatus {
    UmiStatus::UNSUPPORTED
}

#[allow(clippy::too_many_arguments)]
extern "C" fn fake_world_box(
    _world: *mut UmiWorldCx,
    _min_x: f64,
    _min_y: f64,
    _min_z: f64,
    _max_x: f64,
    _max_y: f64,
    _max_z: f64,
    _color: u32,
    _alpha: f32,
    _flags: u32,
) -> UmiStatus {
    UmiStatus::UNSUPPORTED
}

extern "C" fn fake_world_polygon(
    _world: *mut UmiWorldCx,
    _points: *const f64,
    _count: usize,
    _color: u32,
    _alpha: f32,
    _flags: u32,
) -> UmiStatus {
    UmiStatus::UNSUPPORTED
}

extern "C" fn fake_world_highlight(
    _world: *mut UmiWorldCx,
    _entity: i32,
    _color: u32,
    _alpha: f32,
    _flags: u32,
) -> UmiStatus {
    UmiStatus::UNSUPPORTED
}

extern "C" fn fake_register_camera(
    _me: UmiModHandle,
    _mode: UmiServiceMode,
    _step: Option<UmiCameraFn>,
    _user: *mut c_void,
) -> UmiStatus {
    UmiStatus::UNSUPPORTED
}

extern "C" fn fake_camera_next(_cx: *mut UmiCameraCx) -> UmiStatus {
    UmiStatus::UNSUPPORTED
}

/// What the fake snapshot answers instead of its rows, `OK` for the rows.
static SNAPSHOT_STATUS: Mutex<UmiStatus> = Mutex::new(UmiStatus::OK);

/// The three entities of the fake snapshot, nearest first: id, kind, health.
const SNAPSHOT_ROWS: [(i32, &str, f32); 3] =
    [(7, "zombie", 12.5), (8, "player", 20.0), (9, "item", 0.0)];

/// Answers the three rows of [`SNAPSHOT_ROWS`] as the real host does: at most
/// `capacity` of them, the total in `*count`, and nothing for a refusal.
extern "C" fn fake_world_entities_snapshot(
    _me: UmiModHandle,
    out: *mut UmiEntity,
    capacity: usize,
    count: *mut usize,
) -> UmiStatus {
    let status = *SNAPSHOT_STATUS.lock().unwrap();
    if status != UmiStatus::OK {
        return status;
    }
    for (index, (id, kind, health)) in SNAPSHOT_ROWS.into_iter().enumerate().take(capacity) {
        let living = kind != "item";
        let entity = UmiEntity {
            size: size_of::<UmiEntity>() as u32,
            id,
            kind: text(kind),
            position: [f64::from(id), 64.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
            name: if kind == "player" {
                text("Ann")
            } else {
                UmiStr::EMPTY
            },
            health,
            max_health: if living { 20.0 } else { 0.0 },
            armor: 5.0,
            team: if kind == "player" {
                text("red")
            } else {
                UmiStr::EMPTY
            },
            width: 0.6,
            height: 1.8,
            velocity: [0.0, -0.0784, 0.25],
            flags: if living {
                UMI_ENTITY_LIVING | UMI_ENTITY_ON_FIRE | UMI_ENTITY_SPRINTING
            } else {
                0
            },
            is_player: u8::from(kind == "player"),
            render_position: [f64::from(id) + 0.25, 64.0, 0.0],
            ..entity_1_4_tail()
        };
        // SAFETY: the module under test passes room for `capacity` entities.
        unsafe { out.add(index).write(entity) };
    }
    // SAFETY: the module under test passes a writable `usize`.
    unsafe { *count = SNAPSHOT_ROWS.len() };
    UmiStatus::OK
}

extern "C" fn fake_hud_outline(
    _hud: *mut UmiHudCx,
    _x: f32,
    _y: f32,
    _width: f32,
    _height: f32,
    _color: u32,
    _alpha: f32,
) -> UmiStatus {
    UmiStatus::UNSUPPORTED
}

#[allow(clippy::too_many_arguments)]
extern "C" fn fake_hud_segment(
    _hud: *mut UmiHudCx,
    _x1: f32,
    _y1: f32,
    _x2: f32,
    _y2: f32,
    _color: u32,
    _alpha: f32,
    _thickness: f32,
) -> UmiStatus {
    UmiStatus::UNSUPPORTED
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
    for list in [&SEEN, &SET_CALLS, &DEFS, &HUD_CALLS, &KEYS, &CHAT, &MENU] {
        list.lock().unwrap().clear();
    }
    WIDGETS.lock().unwrap().clear();
    COMMANDS.lock().unwrap().clear();
    LAYERS.lock().unwrap().clear();
    NEXT_CALLS.store(0, SeqCst);
    INJECTED.lock().unwrap().clear();
    for list in [&POST_PASSES, &POST_PARAMS] {
        list.lock().unwrap().clear();
    }
    POST_REFUSALS.lock().unwrap().clear();
    *PACKET_STATUS.lock().unwrap() = UmiStatus::OK;
    *SNAPSHOT_STATUS.lock().unwrap() = UmiStatus::OK;
    *VALUES.lock().unwrap() = Some(HashMap::new());
    *UI.lock().unwrap() = None;
    *RADAR.lock().unwrap() = None;
    SET_RESULTS.lock().unwrap().clear();
    for counter in [&DROPS, &SHUTDOWNS] {
        counter.store(0, SeqCst);
    }
    for flag in [
        &FAIL_INIT,
        &PANIC_ON_TICK,
        &REFUSE_SUBSCRIBE,
        &REFUSE_HUD,
        &FAIL_PHYSICS,
        &REPLACE_PHYSICS,
        &FLIGHT_LAYER,
    ] {
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
    assert_eq!(subs.len(), 15);
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
    // A real host drops a failed mod's passes; the fake keeps them.
    POST_PASSES.lock().unwrap().clear();
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

fn frame_payload(ping_ms: i32, sprinting: u8) -> UmiFrame {
    UmiFrame {
        header: header(UmiEventId::FRAME, size_of::<UmiFrame>(), 0),
        now_ms: 5000,
        position: [1.5, 64.0, -2.0],
        fps: 120,
        frame_ms: 8.5,
        yaw: -90.0,
        pitch: 10.0,
        left_clicks: 3,
        right_clicks: 1,
        ping_ms,
        sprinting,
        eye: [1.5, 65.62, -2.0],
        render_origin: [1.0, 65.0, -2.0],
        view_proj: IDENTITY,
        viewport: [854, 480],
        gui_scale: 2.0,
        fov_y: 70.0,
        aspect: 854.0 / 480.0,
        view_bob: IDENTITY,
    }
}

const IDENTITY: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

#[test]
fn frame_view_reads_every_field() {
    let _guard = fresh();
    init_ok();
    let sub = sub_for(UmiEventId::FRAME);
    assert_eq!(call(sub, &mut frame_payload(42, 1)), UmiStatus::OK);
    assert_eq!(call(sub, &mut frame_payload(-1, 0)), UmiStatus::OK);
    assert_eq!(
        seen(),
        [
            "frame=5000 [1.5, 64.0, -2.0] fps=120 ms=8.5 yaw=-90 pitch=10 clicks=3/1 ping=Some(42) sprint=true",
            "frame=5000 [1.5, 64.0, -2.0] fps=120 ms=8.5 yaw=-90 pitch=10 clicks=3/1 ping=None sprint=false",
        ]
    );
}

#[test]
fn the_frame_view_refuses_another_payload() {
    let _guard = fresh();
    init_ok();
    let mut tick = pre_tick(1);
    assert_eq!(
        call(sub_for(UmiEventId::FRAME), &mut tick),
        UmiStatus::INVALID_ARGUMENT
    );
    assert!(seen().is_empty());
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
            "clip 0 0 20 10",
            "circle 5 5 3 ff0000 1 0",
            "circle 5 5 4 ff0000 0.5 1.5",
            "pop",
            "polygon [0.0, 0.0, 4.0, 0.0, 0.0, 4.0] 123456 1",
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

fn packet(id: UmiEventId, packet_id: u32, data: UmiBytes) -> UmiPacket {
    UmiPacket {
        header: header(id, size_of::<UmiPacket>(), UMI_EVENT_CANCELLABLE),
        id: packet_id,
        data,
    }
}

#[test]
fn packet_views_read_id_and_body() {
    let _guard = fresh();
    init_ok();
    let body = [1_u8, 2, 3];
    let bytes = UmiBytes {
        ptr: body.as_ptr(),
        len: body.len(),
    };
    let mut rx = packet(UmiEventId::PACKET_RECEIVED, 0x21, bytes);
    let sub = sub_for(UmiEventId::PACKET_RECEIVED);
    assert_eq!(call(sub, &mut rx), UmiStatus::OK);
    let mut tx = packet(UmiEventId::PACKET_SEND, 0x0A, UmiBytes::EMPTY);
    let sub = sub_for(UmiEventId::PACKET_SEND);
    assert_eq!(call(sub, &mut tx), UmiStatus::OK);
    assert_eq!(seen(), ["rx=0x21 [1, 2, 3]", "tx=0x0a []"]);
}

#[test]
fn packet_view_survives_a_null_pointer_with_a_length_and_a_wide_id() {
    let _guard = fresh();
    init_ok();
    let hostile = UmiBytes {
        ptr: std::ptr::null(),
        len: 9,
    };
    let mut rx = packet(UmiEventId::PACKET_RECEIVED, 0x1_0000, hostile);
    assert_eq!(
        call(sub_for(UmiEventId::PACKET_RECEIVED), &mut rx),
        UmiStatus::OK
    );
    assert_eq!(seen(), ["rx=0xff []"]);
}

#[test]
fn packets_send_passes_id_and_body_to_the_host() {
    let _guard = fresh();
    init_ok();
    assert_eq!(ui().packets.send(0x0A, &[]), Ok(()));
    assert_eq!(ui().packets.send(0x09, &[0, 4]), Ok(()));
    assert_eq!(
        *INJECTED.lock().unwrap(),
        [(0x0A, vec![]), (0x09, vec![0, 4])]
    );
}

#[test]
fn packets_send_maps_host_statuses_to_errors() {
    let _guard = fresh();
    init_ok();
    for (status, expected) in [
        (UmiStatus::BUSY, ModError::Busy),
        (UmiStatus::DENIED, ModError::HostRefused(UmiStatus::DENIED)),
        (
            UmiStatus::INVALID_ARGUMENT,
            ModError::HostRefused(UmiStatus::INVALID_ARGUMENT),
        ),
    ] {
        *PACKET_STATUS.lock().unwrap() = status;
        assert_eq!(ui().packets.send(0xFF, &[1]), Err(expected));
    }
}

fn physics_state() -> UmiPlayerPhysics {
    UmiPlayerPhysics {
        size: size_of::<UmiPlayerPhysics>() as u32,
        flags: UMI_PHYSICS_SPRINTING | UMI_PHYSICS_SNEAKING | UMI_PHYSICS_IN_WATER,
        position: [1.0, 2.0, 3.0],
        velocity: [0.0; 3],
        yaw: 30.0,
        pitch: 40.0,
        fall_distance: 0.5,
        fly_speed: 0.05,
        game_mode: UMI_GAME_MODE_SURVIVAL,
    }
}

fn physics_input() -> UmiPhysicsInput {
    UmiPhysicsInput {
        size: size_of::<UmiPhysicsInput>() as u32,
        flags: UMI_INPUT_JUMP | UMI_INPUT_SPRINT,
        forward: 1.0,
        strafe: -1.0,
        yaw: 30.0,
        pitch: 40.0,
    }
}

/// Calls the registered layer on `state`; the fake host's `cx` is the state.
fn run_layer(state: &mut UmiPlayerPhysics, input: &UmiPhysicsInput) -> UmiStatus {
    let layer = LAYERS.lock().unwrap()[0];
    let cx: *mut UmiPhysicsCx = (state as *mut UmiPlayerPhysics).cast();
    (layer.step)(cx, state, input, layer.user as *mut c_void)
}

#[test]
fn wrapper_is_registered_and_runs_with_views_and_next() {
    let _guard = fresh();
    init_ok();
    assert_eq!(LAYERS.lock().unwrap()[0].mode, UmiServiceMode::WRAP);
    let mut state = physics_state();
    assert_eq!(run_layer(&mut state, &physics_input()), UmiStatus::OK);
    assert_eq!(NEXT_CALLS.load(SeqCst), 1);
    assert_eq!(
        seen(),
        [
            "input 1 -1 true false true 30 40",
            // after next(): velocity y +1; flags: on ground set, sprint off,
            // read-only flags untouched
            "state [1.0, 2.0, 3.0] [0.0, 1.0, 0.0] true false true false true 30 40",
            "fall 2.5",
        ]
    );
    assert_eq!(state.velocity, [1.0, 2.0, 3.0]);
    assert_eq!(state.position, [4.0, 5.0, 6.0]);
    assert_eq!(
        state.flags,
        UMI_PHYSICS_ON_GROUND | UMI_PHYSICS_SNEAKING | UMI_PHYSICS_IN_WATER
    );
    assert_eq!((state.yaw, state.pitch), (30.0, 40.0));
}

#[test]
fn replacing_layer_registers_as_replace_and_next_is_refused() {
    let _guard = fresh();
    REPLACE_PHYSICS.store(true, SeqCst);
    init_ok();
    assert_eq!(LAYERS.lock().unwrap()[0].mode, UmiServiceMode::REPLACE);
    let mut state = physics_state();
    // `next` is refused, the closure passes that error on: the layer fails.
    assert_eq!(run_layer(&mut state, &physics_input()), UmiStatus::ERROR);
    assert_eq!(NEXT_CALLS.load(SeqCst), 0);
}

#[test]
fn physics_layer_error_and_panic_map_to_statuses() {
    let _guard = fresh();
    init_ok();
    FAIL_PHYSICS.store(true, SeqCst);
    let mut state = physics_state();
    assert_eq!(run_layer(&mut state, &physics_input()), UmiStatus::PANICKED);
}

#[test]
fn physics_layer_refuses_bad_pointers_and_small_structs() {
    let _guard = fresh();
    init_ok();
    let layer = LAYERS.lock().unwrap()[0];
    let user = layer.user as *mut c_void;
    let (mut state, input) = (physics_state(), physics_input());
    let cx: *mut UmiPhysicsCx = (&mut state as *mut UmiPlayerPhysics).cast();
    assert_eq!(
        (layer.step)(cx, std::ptr::null_mut(), &input, user),
        UmiStatus::INVALID_ARGUMENT
    );
    assert_eq!(
        (layer.step)(cx, &mut state, std::ptr::null(), user),
        UmiStatus::INVALID_ARGUMENT
    );
    assert_eq!(
        (layer.step)(cx, &mut state, &input, std::ptr::null_mut()),
        UmiStatus::INVALID_ARGUMENT
    );
    let small = UmiPhysicsInput { size: 4, ..input };
    assert_eq!(
        (layer.step)(cx, &mut state, &small, user),
        UmiStatus::INVALID_ARGUMENT
    );
    let small = UmiPlayerPhysics {
        size: 4,
        ..physics_state()
    };
    let mut small = small;
    assert_eq!(
        (layer.step)(cx, &mut small, &input, user),
        UmiStatus::INVALID_ARGUMENT
    );
    assert_eq!(NEXT_CALLS.load(SeqCst), 0);
}

#[test]
fn a_layer_reads_and_writes_the_flight_and_the_game_mode() {
    let _guard = fresh();
    FLIGHT_LAYER.store(true, SeqCst);
    init_ok();
    let mut state = UmiPlayerPhysics {
        flags: UMI_PHYSICS_ALLOW_FLYING,
        ..physics_state()
    };
    assert_eq!(run_layer(&mut state, &physics_input()), UmiStatus::OK);
    assert_eq!(seen(), ["flight false true 0.05 Survival"]);
    assert_eq!(
        state.flags & (UMI_PHYSICS_FLYING | UMI_PHYSICS_ALLOW_FLYING),
        UMI_PHYSICS_FLYING | UMI_PHYSICS_ALLOW_FLYING
    );
    assert_eq!(state.game_mode, UMI_GAME_MODE_CREATIVE);
    assert_eq!(state.fly_speed, 0.05, "read-only");
}

#[test]
fn a_1_0_sized_state_is_refused_before_a_layer_could_reach_past_it() {
    // The safe view spans the 1.1 struct; a 1.0 host (72 bytes) cannot load a
    // 1.1 mod anyway, and the view's own 1.0 fallbacks are unit-tested.
    let _guard = fresh();
    FLIGHT_LAYER.store(true, SeqCst);
    init_ok();
    let layer = LAYERS.lock().unwrap()[0];
    let mut state = UmiPlayerPhysics {
        size: 72,
        ..physics_state()
    };
    let cx: *mut UmiPhysicsCx = (&mut state as *mut UmiPlayerPhysics).cast();
    let input = physics_input();
    assert_eq!(
        (layer.step)(cx, &mut state, &input, layer.user as *mut c_void),
        UmiStatus::INVALID_ARGUMENT
    );
    assert!(seen().is_empty());
    assert_eq!(state.flags & UMI_PHYSICS_FLYING, 0);
}

#[test]
fn world_block_decodes_the_wire_state_and_maps_statuses() {
    let _guard = fresh();
    init_ok();
    let world = ui().world;
    assert_eq!(world.block(0, 64, 0), Ok(Block { id: 1, meta: 3 }));
    assert_eq!(world.block(1, 64, 0), Err(ModError::Unavailable));
    assert_eq!(
        world.block(2, 64, 0),
        Err(ModError::HostRefused(UmiStatus::UNSUPPORTED))
    );
}

#[test]
fn world_entities_visits_all_and_exposes_the_views() {
    let _guard = fresh();
    init_ok();
    let mut lines = Vec::new();
    ui().world
        .entities(|e| {
            lines.push(format!(
                "{} {} {:?} {} {} [{}]",
                e.id(),
                e.kind(),
                e.position(),
                e.yaw(),
                e.pitch(),
                e.name()
            ));
            true
        })
        .unwrap();
    assert_eq!(
        lines,
        [
            "7 player [7.0, 64.0, -1.5] 90 -10 [Ann]",
            "8 zombie [8.0, 64.0, -1.5] 90 -10 []",
            "9 item [9.0, 64.0, -1.5] 90 -10 [x]",
        ]
    );
}

#[test]
fn world_entities_stops_when_the_visitor_says_so_or_panics() {
    let _guard = fresh();
    init_ok();
    let mut visited = 0;
    let result = ui().world.entities(|_| {
        visited += 1;
        false
    });
    assert_eq!((result, visited), (Ok(()), 1));
    let mut visited = 0;
    let result = ui().world.entities(|_| {
        visited += 1;
        panic!("visitor boom");
    });
    assert_eq!((result, visited), (Ok(()), 1));
}

#[test]
fn unavailable_has_a_message() {
    assert_eq!(ModError::Unavailable.to_string(), "the chunk is not loaded");
}

#[test]
fn post_pass_registers_and_maps_refusals() {
    let _guard = fresh();
    init_ok();
    assert_eq!(
        *POST_PASSES.lock().unwrap(),
        ["tint:Tint:fn fs_main() {}".to_owned()]
    );
    let refusals = POST_REFUSALS.lock().unwrap();
    assert_eq!(
        *refusals,
        [
            Err(ModError::HostRefused(UmiStatus::CONFLICT)),
            Err(ModError::HostRefused(UmiStatus::INVALID_ARGUMENT)),
            Err(ModError::HostRefused(UmiStatus::INVALID_ARGUMENT)),
        ]
    );
}

#[test]
fn post_pass_sets_params_by_key_and_reports_a_bad_index() {
    let _guard = fresh();
    init_ok();
    let post = ui().post;
    assert_eq!(post.set_param(2, [1.0, 0.5, 0.0, -1.0]), Ok(()));
    assert_eq!(
        *POST_PARAMS.lock().unwrap(),
        ["tint:2:1,0.5,0,-1".to_owned()]
    );
    assert_eq!(
        post.set_param(4, [0.0; 4]),
        Err(ModError::HostRefused(UmiStatus::INVALID_ARGUMENT))
    );
}

#[test]
fn snapshot_visits_the_rows_with_the_metadata_getters() {
    let _guard = fresh();
    init_ok();
    let mut lines = Vec::new();
    ui().world
        .snapshot(|e| {
            lines.push(format!(
                "{} {} [{}] [{}] hp {}/{} armor {} {}x{} v{:?} fire {} sprint {} sneak {} item {} invisible {} living {} player {} at {:?} drawn {:?} flags {}",
                e.id(),
                e.kind(),
                e.name(),
                e.team(),
                e.health(),
                e.max_health(),
                e.armor(),
                e.width(),
                e.height(),
                e.velocity(),
                e.on_fire(),
                e.sprinting(),
                e.sneaking(),
                e.using_item(),
                e.invisible(),
                e.is_living(),
                e.is_player(),
                e.position(),
                e.render_position(),
                e.flags(),
            ));
            true
        })
        .unwrap();
    assert_eq!(
        lines,
        [
            "7 zombie [] [] hp 12.5/20 armor 5 0.6x1.8 v[0.0, -0.0784, 0.25] fire true sprint true sneak false item false invisible false living true player false at [7.0, 64.0, 0.0] drawn [7.25, 64.0, 0.0] flags 37",
            "8 player [Ann] [red] hp 20/20 armor 5 0.6x1.8 v[0.0, -0.0784, 0.25] fire true sprint true sneak false item false invisible false living true player true at [8.0, 64.0, 0.0] drawn [8.25, 64.0, 0.0] flags 37",
            "9 item [] [] hp 0/0 armor 5 0.6x1.8 v[0.0, -0.0784, 0.25] fire false sprint false sneak false item false invisible false living false player false at [9.0, 64.0, 0.0] drawn [9.25, 64.0, 0.0] flags 0",
        ]
    );
}

#[test]
fn snapshot_stops_when_the_visitor_says_so() {
    let _guard = fresh();
    init_ok();
    let mut visited = 0;
    let result = ui().world.snapshot(|_| {
        visited += 1;
        visited < 2
    });
    assert_eq!((result, visited), (Ok(()), 2));
}

#[test]
fn snapshot_maps_the_host_answers_to_errors() {
    let _guard = fresh();
    init_ok();
    for (status, error) in [
        (UmiStatus::UNAVAILABLE, ModError::Unavailable),
        (
            UmiStatus::UNSUPPORTED,
            ModError::HostRefused(UmiStatus::UNSUPPORTED),
        ),
        (UmiStatus::DENIED, ModError::HostRefused(UmiStatus::DENIED)),
    ] {
        *SNAPSHOT_STATUS.lock().unwrap() = status;
        let mut visited = 0;
        let result = ui().world.snapshot(|_| {
            visited += 1;
            true
        });
        assert_eq!((result, visited), (Err(error), 0));
    }
}

#[test]
fn a_visitor_may_read_the_snapshot_again() {
    let _guard = fresh();
    init_ok();
    let world = ui().world;
    let mut inner = 0;
    world
        .snapshot(|_| {
            world
                .snapshot(|_| {
                    inner += 1;
                    true
                })
                .unwrap();
            false
        })
        .unwrap();
    assert_eq!(inner, 3);
}

#[test]
fn tabs_modules_and_buttons_reach_the_host_in_order() {
    let _guard = fresh();
    init_ok();
    assert_eq!(
        *MENU.lock().unwrap(),
        [
            "category:pvp:PvP",
            "module:radar:Radar:Players around:pvp:1",
            "setting:radar:names",
            "button:radar:clear:Marks:Clear",
            "attach:radar:1:clock",
            "attach:radar:2:tint",
            "button::reload:List:Reload",
        ]
    );
    let radar = RADAR.lock().unwrap().expect("declared");
    assert_eq!(radar.key(), "radar");
    assert!(!radar.is_enabled(), "the host's answer, not the default");
}

#[test]
fn module_and_button_events_are_viewed() {
    let _guard = fresh();
    init_ok();
    let mut toggled = UmiModuleToggled {
        header: header(UmiEventId::MODULE_TOGGLED, size_of::<UmiModuleToggled>(), 0),
        module: text("radar"),
        enabled: 1,
    };
    assert_eq!(
        call(sub_for(UmiEventId::MODULE_TOGGLED), &mut toggled),
        UmiStatus::OK
    );
    let mut pressed = UmiButtonPressed {
        header: header(UmiEventId::BUTTON_PRESSED, size_of::<UmiButtonPressed>(), 0),
        module: text(""),
        key: text("reload"),
    };
    assert_eq!(
        call(sub_for(UmiEventId::BUTTON_PRESSED), &mut pressed),
        UmiStatus::OK
    );
    assert_eq!(seen(), ["module=radar:true", "button=:reload"]);
}
