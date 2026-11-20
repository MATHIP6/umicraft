// SPDX-License-Identifier: MIT OR Apache-2.0

//! The function table the host hands to a mod.

use core::ffi::c_void;

use crate::{
    UmiBytes, UmiCommandFn, UmiEntityVisitFn, UmiEventHeader, UmiEventId, UmiHudCx, UmiHudDrawFn,
    UmiHudPlacement, UmiModHandle, UmiPhysicsCx, UmiPhysicsFn, UmiServiceMode, UmiSettingDef,
    UmiSettingValue, UmiStatus, UmiStr, UmiVersion,
};

/// How a handler takes part in an event.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiMode(pub u32);

impl UmiMode {
    /// The handler only watches the event.
    pub const OBSERVE: Self = Self(0);
    /// The handler may change or cancel the event.
    pub const INTERCEPT: Self = Self(1);
}

/// Severity of a log message.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiLevel(pub u32);

impl UmiLevel {
    /// Something failed.
    pub const ERROR: Self = Self(1);
    /// Something is suspicious.
    pub const WARN: Self = Self(2);
    /// Normal progress information.
    pub const INFO: Self = Self(3);
    /// Developer diagnostics.
    pub const DEBUG: Self = Self(4);
    /// Very verbose diagnostics.
    pub const TRACE: Self = Self(5);
}

/// Event callback: receives the payload and the `user` pointer given to
/// `subscribe`, which the host passes back untouched.
pub type UmiHandlerFn = extern "C" fn(event: *mut UmiEventHeader, user: *mut c_void) -> UmiStatus;

/// The host function table, append-only (ADR-0087).
///
/// A mod must check `size` before using a field added in a later minor.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UmiHost {
    /// Size in bytes of this struct as the host built it.
    pub size: usize,
    /// API version the host implements.
    pub version: UmiVersion,
    /// Logs `msg` on behalf of mod `me` at `level`.
    pub log: extern "C" fn(me: UmiModHandle, level: UmiLevel, msg: UmiStr),
    /// Registers `handler` for `event`; lower `priority` runs first. Returns
    /// `INVALID_ARGUMENT` for an unknown event or mode, or a null handler.
    ///
    /// `handler` is an `Option` because a C mod may pass `NULL`: a bare
    /// function pointer must never be null in Rust, and `Option` of it has
    /// the same ABI with null as `None`.
    pub subscribe: extern "C" fn(
        me: UmiModHandle,
        event: UmiEventId,
        mode: UmiMode,
        priority: i32,
        handler: Option<UmiHandlerFn>,
        user: *mut c_void,
    ) -> UmiStatus,
    /// Replaces the text of the event being intercepted (`CHAT_RECEIVED`:
    /// the JSON; `CHAT_SEND`: the line). Later handlers and the client see the
    /// new text. Returns `DENIED` unless the event carries
    /// `UMI_EVENT_MUTABLE`, `INVALID_ARGUMENT` for another event or invalid
    /// UTF-8. Added in API 0.2.
    pub event_set_text: extern "C" fn(event: *mut UmiEventHeader, text: UmiStr) -> UmiStatus,
    /// Declares a setting (during `umi_mod_init` only). `CONFLICT` for a
    /// key already used, `INVALID_ARGUMENT` for an invalid definition.
    /// Added in API 0.3.
    pub register_setting: extern "C" fn(me: UmiModHandle, def: *const UmiSettingDef) -> UmiStatus,
    /// Reads the current value of setting `key` into `out`; callable from any
    /// thread at any time. `INVALID_ARGUMENT` for an unknown key.
    pub setting_get:
        extern "C" fn(me: UmiModHandle, key: UmiStr, out: *mut UmiSettingValue) -> UmiStatus,
    /// Declares a HUD widget `key` (during `umi_mod_init` only), drawn by
    /// `draw` each frame while the mod is enabled. The player can move and
    /// scale it in the mods menu; `placement` is the default.
    pub register_hud_widget: extern "C" fn(
        me: UmiModHandle,
        key: UmiStr,
        label: UmiStr,
        placement: UmiHudPlacement,
        draw: Option<UmiHudDrawFn>,
        user: *mut c_void,
    ) -> UmiStatus,
    /// Adds a text line to the widget being drawn (`color`: `0xRRGGBB`).
    pub hud_line: extern "C" fn(hud: *mut UmiHudCx, text: UmiStr, color: u32) -> UmiStatus,
    /// Adds free text at `(x, y)` inside the widget, in widget pixels.
    pub hud_label: extern "C" fn(
        hud: *mut UmiHudCx,
        x: f32,
        y: f32,
        text: UmiStr,
        color: u32,
        alpha: f32,
        scale: f32,
    ) -> UmiStatus,
    /// Adds a boxed cell (a key of a keystrokes display, for instance).
    pub hud_cell: extern "C" fn(
        hud: *mut UmiHudCx,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        pressed: u8,
        text: UmiStr,
        color: u32,
    ) -> UmiStatus,
    /// Adds a filled rectangle.
    pub hud_tick: extern "C" fn(
        hud: *mut UmiHudCx,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: u32,
        alpha: f32,
    ) -> UmiStatus,
    /// Reserves at least `width` x `height` widget pixels for labels, cells
    /// and ticks, which do not size the widget by themselves.
    pub hud_reserve: extern "C" fn(hud: *mut UmiHudCx, width: f32, height: f32) -> UmiStatus,
    /// Declares a key binding `key` (during `umi_mod_init` only), listed on
    /// the controls page under the mod's name. `default_binding` is a key
    /// name such as `KeyG` or `Mouse4`, or empty for unbound. A press fires
    /// `KEY_PRESSED` for this mod only.
    pub register_key: extern "C" fn(
        me: UmiModHandle,
        key: UmiStr,
        label: UmiStr,
        default_binding: UmiStr,
    ) -> UmiStatus,
    /// Declares the client command `/<name>` (during `umi_mod_init` only);
    /// the line is not sent to the server. `CONFLICT` if another mod took
    /// the name.
    pub register_command: extern "C" fn(
        me: UmiModHandle,
        name: UmiStr,
        usage: UmiStr,
        run: Option<UmiCommandFn>,
        user: *mut c_void,
    ) -> UmiStatus,
    /// Shows `text` as a local chat line (never sent); callable from any
    /// thread.
    pub chat_print: extern "C" fn(me: UmiModHandle, text: UmiStr) -> UmiStatus,
    /// Queues the serverbound play packet `id` with body `data` (no length
    /// prefix) for the network thread; callable from any thread after init.
    /// `DENIED` without the `network_packets` permission or while the mod is
    /// disabled, `INVALID_ARGUMENT` for a body the client would not send
    /// itself, `BUSY` when the queue is full. Added in API 0.4.
    pub packet_send: extern "C" fn(me: UmiModHandle, id: u32, data: UmiBytes) -> UmiStatus,
    /// Wraps or replaces the `player_physics` service (during `umi_mod_init`
    /// only). `DENIED` without the `movement` permission or when `mod.toml` does
    /// not declare the service in `wraps` (or `replaces`), `CONFLICT` for a
    /// second layer of the same mod. Added in API 0.5.
    pub register_physics: extern "C" fn(
        me: UmiModHandle,
        mode: UmiServiceMode,
        step: Option<UmiPhysicsFn>,
        user: *mut c_void,
    ) -> UmiStatus,
    /// Runs the inner layers (and the built-in step) on the state the running
    /// wrapper was given, once. `UNSUPPORTED` for a replacing layer, `DENIED`
    /// for a second call. Added in API 0.5.
    pub physics_next: extern "C" fn(cx: *mut UmiPhysicsCx) -> UmiStatus,
    /// Reads the block at `(x, y, z)` into `out` as `id << 4 | metadata`.
    /// Only on the game thread, inside a callback (a game-event intercept
    /// handler, a command or a physics layer), else `UNSUPPORTED`;
    /// `UNAVAILABLE` for a chunk that is not loaded. Added in API 0.5.
    pub world_block:
        extern "C" fn(me: UmiModHandle, x: i32, y: i32, z: i32, out: *mut u32) -> UmiStatus,
    /// Calls `visit` for every entity of the world but the local player; same
    /// thread rule as `world_block`. Added in API 0.5.
    pub world_entities: extern "C" fn(
        me: UmiModHandle,
        visit: Option<UmiEntityVisitFn>,
        user: *mut c_void,
    ) -> UmiStatus,
}
