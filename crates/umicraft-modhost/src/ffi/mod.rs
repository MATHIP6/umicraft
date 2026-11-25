// SPDX-License-Identifier: MIT OR Apache-2.0

//! Every `unsafe` operation of the host: the raw-pointer edge of the C ABI.
//!
//! Host functions handed to mods, symbol lookup in loaded libraries, and the one
//! `Send` claim about mod pointers live here so the rest of the crate stays safe
//! (ADR-0087; the event-specific part is in `event`). Policy (what is allowed, what is logged) lives in
//! [`crate::context`]; this module only converts raw arguments into safe ones.
#![allow(unsafe_code)]

mod camera;
mod event;
mod hud;
mod menu;
mod packet;
mod physics;
mod post;
mod service;
mod ui;
mod world;
mod world_layer;

#[cfg(test)]
pub(crate) use camera::testing as camera_testing;
pub(crate) use camera::{capturing, CAMERA_ACTIVE};
#[cfg(test)]
pub(crate) use event::testing;
pub(crate) use hud::with_hud_cx;
#[cfg(test)]
pub(crate) use physics::testing as physics_testing;
pub(crate) use physics::PHYSICS_ACTIVE;
pub(crate) use service::{with_service_cx, StatePtr};
pub(crate) use world::{lend_snapshot, lend_world};
pub(crate) use world_layer::with_world_cx;

use core::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use libloading::Library;
use umicraft_modapi_sys::{
    UmiEventId, UmiHandlerFn, UmiHost, UmiLevel, UmiModApiVersionFn, UmiModHandle, UmiModInitFn,
    UmiModShutdownFn, UmiMode, UmiStatus, UmiStr, UMI_API_VERSION, UMI_MOD_API_VERSION_SYMBOL,
    UMI_MOD_INIT_SYMBOL, UMI_MOD_SHUTDOWN_SYMBOL,
};

use crate::context::ModContext;
use crate::error::{DeclareError, LoadError};

/// The `user` pointer a mod gave to `subscribe`, opaque to the host.
///
/// The host never dereferences it; it only hands it back to the handler it came
/// with.
#[derive(Debug, Clone, Copy)]
pub struct UserPtr(*mut c_void);

// SAFETY: the pointer is never dereferenced by the host. The mod promises its
// handlers may be called from the thread that owns the event, which is the only
// place the dispatcher calls them (ADR-0086, thread ownership); moving the
// pointer between host threads just carries the value.
unsafe impl Send for UserPtr {}
// SAFETY: as above; sharing a `*mut c_void` value across threads reads no data.
unsafe impl Sync for UserPtr {}

impl UserPtr {
    /// Wraps a mod-provided pointer.
    pub fn new(ptr: *mut c_void) -> Self {
        Self(ptr)
    }

    /// The pointer to pass back to the handler.
    pub fn as_ptr(self) -> *mut c_void {
        self.0
    }
}

/// The handle a mod receives for `context`.
///
/// The context must stay at this address for as long as the mod can call the host.
pub fn handle_of(context: &ModContext) -> UmiModHandle {
    UmiModHandle(core::ptr::from_ref(context).cast_mut().cast())
}

/// Borrows the context behind `handle`, or `None` for a null handle.
fn context_of<'a>(handle: UmiModHandle) -> Option<&'a ModContext> {
    // SAFETY: a non-null handle was produced by `handle_of` from a context that the
    // host keeps alive and unmoved until the client exits (`LoadedMod` holds an
    // `Arc` and mods are never dropped before the host). Only shared access is
    // made, and `ModContext` is `Sync`.
    unsafe { handle.0.cast::<ModContext>().as_ref() }
}

/// Borrows at most `max` bytes of a mod-provided string.
fn bytes_of<'a>(s: UmiStr, max: usize) -> &'a [u8] {
    if s.ptr.is_null() || s.len == 0 {
        return &[];
    }
    // SAFETY: the ABI promises `ptr` points to `len` readable bytes for the duration
    // of the call; null was excluded above and `max <= len` is enforced by `min`.
    unsafe { core::slice::from_raw_parts(s.ptr, s.len.min(max)) }
}

/// Reads a mod-provided string of at most `max` bytes as UTF-8, `None` if it is longer
/// or not valid UTF-8.
fn utf8_of<'a>(s: UmiStr, max: usize) -> Option<&'a str> {
    if s.len > max {
        return None;
    }
    std::str::from_utf8(bytes_of(s, max)).ok()
}

/// Like [`utf8_of`] but for a declaration: the failure says which text was wrong.
/// An empty string passes; the caller decides whether empty is allowed.
fn text_of<'a>(s: UmiStr, what: &'static str, max: usize) -> Result<&'a str, DeclareError> {
    utf8_of(s, max).ok_or(DeclareError::Text { what, max })
}

/// Runs `body` with the context behind `handle`, turning a null handle into
/// `INVALID_ARGUMENT` and a panic into `PANICKED`, so nothing unwinds into the mod.
fn guarded(handle: UmiModHandle, body: impl FnOnce(&ModContext) -> UmiStatus) -> UmiStatus {
    catch_unwind(AssertUnwindSafe(|| match context_of(handle) {
        Some(context) => body(context),
        None => UmiStatus::INVALID_ARGUMENT,
    }))
    .unwrap_or(UmiStatus::PANICKED)
}

/// Longest message read from a mod; the context caps again when logging.
const MAX_MESSAGE_BYTES: usize = 8192;

extern "C" fn host_log(me: UmiModHandle, level: UmiLevel, msg: UmiStr) {
    // A panic must not unwind into the mod's C frames.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if let Some(context) = context_of(me) {
            context.log(level, bytes_of(msg, MAX_MESSAGE_BYTES));
        }
    }));
}

extern "C" fn host_subscribe(
    me: UmiModHandle,
    event: UmiEventId,
    mode: UmiMode,
    priority: i32,
    handler: Option<UmiHandlerFn>,
    user: *mut c_void,
) -> UmiStatus {
    guarded(me, |context| {
        context.subscribe(event, mode, priority, handler, UserPtr::new(user))
    })
}

/// Builds the function table handed to every mod.
pub fn host_table() -> UmiHost {
    UmiHost {
        size: core::mem::size_of::<UmiHost>(),
        version: UMI_API_VERSION,
        log: host_log,
        subscribe: host_subscribe,
        event_set_text: event::host_event_set_text,
        register_setting: ui::host_register_setting,
        setting_get: ui::host_setting_get,
        register_hud_widget: ui::host_register_hud_widget,
        hud_line: hud::host_hud_line,
        hud_label: hud::host_hud_label,
        hud_cell: hud::host_hud_cell,
        hud_tick: hud::host_hud_tick,
        hud_reserve: hud::host_hud_reserve,
        register_key: ui::host_register_key,
        register_command: ui::host_register_command,
        chat_print: ui::host_chat_print,
        packet_send: packet::host_packet_send,
        register_physics: physics::host_register_physics,
        physics_next: physics::host_physics_next,
        world_block: world::host_world_block,
        world_entities: world::host_world_entities,
        register_post_pass: post::host_register_post_pass,
        post_set_param: post::host_post_set_param,
        register_camera: camera::host_register_camera,
        camera_next: camera::host_camera_next,
        world_entities_snapshot: world::host_world_entities_snapshot,
        hud_outline: hud::host_hud_outline,
        hud_segment: hud::host_hud_segment,
        register_category: menu::host_register_category,
        register_module: menu::host_register_module,
        register_setting_in: menu::host_register_setting_in,
        register_button: menu::host_register_button,
        module_attach: menu::host_module_attach,
        module_enabled: menu::host_module_enabled,
        camera_capture: camera::host_camera_capture,
        world_local_player: world::host_world_local_player,
        hud_circle: hud::host_hud_circle,
        hud_polygon: hud::host_hud_polygon,
        hud_clip_push: hud::host_hud_clip_push,
        hud_clip_pop: hud::host_hud_clip_pop,
        register_world_layer: world_layer::host_register_world_layer,
        world_line: world_layer::host_world_line,
        world_box: world_layer::host_world_box,
        world_polygon: world_layer::host_world_polygon,
        world_highlight: world_layer::host_world_highlight,
    }
}

/// The three entry points of a loaded mod library.
///
/// The pointers are only valid while the [`Library`] they came from stays mapped;
/// `LoadedMod` keeps both together and never unmaps the library before the host dies.
#[derive(Clone, Copy)]
pub struct Exports {
    /// `umi_mod_api_version`.
    pub api_version: UmiModApiVersionFn,
    /// `umi_mod_init`.
    pub init: UmiModInitFn,
    /// `umi_mod_shutdown`.
    pub shutdown: UmiModShutdownFn,
}

/// Maps the library at `path`.
pub fn open_library(path: &Path) -> Result<Library, LoadError> {
    // SAFETY: loading a library runs its initialisers, which is the inherent risk of
    // a native mod. The caller only gets here after the library's SHA-256 was
    // trusted by the player (see `load::authorize`).
    unsafe { Library::new(path) }.map_err(LoadError::Open)
}

/// Looks up the three exports of `library`.
///
/// # Safety contract (checked by the caller)
///
/// The returned pointers must not outlive `library`.
pub fn exports_of(library: &Library) -> Result<Exports, LoadError> {
    Ok(Exports {
        api_version: symbol(library, UMI_MOD_API_VERSION_SYMBOL, "umi_mod_api_version")?,
        init: symbol(library, UMI_MOD_INIT_SYMBOL, "umi_mod_init")?,
        shutdown: symbol(library, UMI_MOD_SHUTDOWN_SYMBOL, "umi_mod_shutdown")?,
    })
}

fn symbol<T: Copy>(library: &Library, name: &[u8], label: &'static str) -> Result<T, LoadError> {
    // SAFETY: `T` is the function-pointer type ADR-0087 defines for `name`; a mod
    // exporting the symbol with another signature breaks the ABI contract. Copying
    // the pointer out of the `Symbol` is sound because the caller keeps the library
    // mapped for as long as the pointer is used.
    unsafe { library.get::<T>(name) }
        .map(|s| *s)
        .map_err(|source| LoadError::MissingSymbol {
            symbol: label,
            source,
        })
}

#[cfg(test)]
mod tests {
    use umicraft_modapi_sys::{UmiBytes, UmiEventHeader};

    use super::*;

    extern "C" fn noop(_: *mut UmiEventHeader, _: *mut c_void) -> UmiStatus {
        UmiStatus::OK
    }

    fn context() -> std::sync::Arc<ModContext> {
        ModContext::new("hud".to_owned().try_into().unwrap())
    }

    #[test]
    fn table_describes_itself() {
        let table = host_table();
        assert_eq!(table.size, core::mem::size_of::<UmiHost>());
        assert_eq!(table.version, UMI_API_VERSION);
    }

    #[test]
    fn subscribe_through_the_table_reaches_the_context() {
        let cx = context();
        let table = host_table();
        let status = (table.subscribe)(
            handle_of(&cx),
            UmiEventId::POST_TICK,
            UmiMode::INTERCEPT,
            5,
            Some(noop),
            core::ptr::null_mut(),
        );
        assert_eq!(status, UmiStatus::OK);
        assert_eq!(cx.subscriptions(UmiEventId::POST_TICK)[0].priority, 5);
    }

    #[test]
    fn subscribe_refuses_unknown_event_and_a_null_handle() {
        let cx = context();
        let table = host_table();
        let call = |me, event| {
            (table.subscribe)(
                me,
                event,
                UmiMode::OBSERVE,
                0,
                Some(noop),
                core::ptr::null_mut(),
            )
        };
        assert_eq!(
            call(handle_of(&cx), UmiEventId(77)),
            UmiStatus::INVALID_ARGUMENT
        );
        let null = UmiModHandle(core::ptr::null_mut());
        assert_eq!(
            call(null, UmiEventId::PRE_TICK),
            UmiStatus::INVALID_ARGUMENT
        );
    }

    fn permitted() -> std::sync::Arc<ModContext> {
        let permissions = crate::manifest::Permissions {
            network_packets: true,
            ..Default::default()
        };
        let cx = ModContext::with_shared(
            "net".to_owned().try_into().unwrap(),
            permissions,
            std::sync::Arc::default(),
        );
        cx.finish_init(true);
        cx
    }

    #[test]
    fn packet_send_through_the_table_reaches_the_outbox() {
        let cx = permitted();
        let table = host_table();
        // Animation (0x0A) has an empty body, given as null, empty and non-null views.
        for data in [UmiBytes::EMPTY, UmiBytes::from_static(&[])] {
            assert_eq!(
                (table.packet_send)(handle_of(&cx), 0x0A, data),
                UmiStatus::OK
            );
        }
        assert_eq!(cx.stats().injected, 2);

        let junk = UmiBytes::from_static(&[1]);
        assert_eq!(
            (table.packet_send)(handle_of(&cx), 0x0A, junk),
            UmiStatus::INVALID_ARGUMENT
        );
        let huge = UmiBytes {
            ptr: junk.ptr,
            len: usize::MAX,
        };
        assert_eq!(
            (table.packet_send)(handle_of(&cx), 0x0A, huge),
            UmiStatus::INVALID_ARGUMENT
        );
        // Regression: a null pointer with a length is refused, not read as empty.
        let dangling = UmiBytes {
            ptr: core::ptr::null(),
            len: 3,
        };
        assert_eq!(
            (table.packet_send)(handle_of(&cx), 0x0A, dangling),
            UmiStatus::INVALID_ARGUMENT
        );
        let null = UmiModHandle(core::ptr::null_mut());
        assert_eq!(
            (table.packet_send)(null, 0x0A, UmiBytes::EMPTY),
            UmiStatus::INVALID_ARGUMENT
        );
        assert_eq!(
            (table.packet_send)(handle_of(&context()), 0x0A, UmiBytes::EMPTY),
            UmiStatus::DENIED
        );
    }

    #[test]
    fn log_accepts_invalid_utf8_empty_and_null() {
        let cx = context();
        let table = host_table();
        let bad = [0xffu8, 0xfe];
        let me = handle_of(&cx);
        (table.log)(
            me,
            UmiLevel::INFO,
            UmiStr {
                ptr: bad.as_ptr(),
                len: 2,
            },
        );
        (table.log)(me, UmiLevel::WARN, UmiStr::EMPTY);
        (table.log)(
            UmiModHandle(core::ptr::null_mut()),
            UmiLevel(99),
            UmiStr::from_static("x"),
        );
    }
}
