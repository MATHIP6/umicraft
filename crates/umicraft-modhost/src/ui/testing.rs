// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fakes shared by the tests of the `ui` modules: contexts and `extern "C"` mod
//! callbacks, so no library is loaded.

use core::ffi::c_void;
use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::time::Duration;

use umicraft_modapi_sys::{UmiHudCx, UmiStatus, UmiStr};

use super::settings::{SettingSpec, SettingSpecKind};
use crate::context::ModContext;
use crate::ffi::{host_table, testing, UserPtr};
use crate::manifest::{ModId, Permissions};
use crate::ui::shared::HostShared;

thread_local! {
    /// Arguments the fake command received, in order.
    pub(crate) static ARGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// The `UmiHudCx` the last fake draw function received.
    pub(crate) static SAVED_CX: Cell<usize> = const { Cell::new(0) };
}

pub(crate) fn mod_id(text: &str) -> ModId {
    text.to_owned().try_into().unwrap()
}

pub(crate) fn user() -> UserPtr {
    UserPtr::new(std::ptr::null_mut())
}

/// A not yet initialised mod inside the host-wide state `shared`.
pub(crate) fn fresh(id: &str, shared: &Arc<HostShared>) -> Arc<ModContext> {
    ModContext::with_shared(mod_id(id), Permissions::default(), Arc::clone(shared))
}

pub(crate) fn toggle(key: &str) -> SettingSpec {
    SettingSpec {
        key: key.to_owned(),
        label: "Label".to_owned(),
        kind: SettingSpecKind::Toggle { default: false },
    }
}

/// Draws two lines and records the cx it was given.
pub(crate) extern "C" fn draw_lines(cx: *mut UmiHudCx, _: *mut c_void) -> UmiStatus {
    SAVED_CX.with(|saved| saved.set(cx as usize));
    let host = host_table();
    (host.hud_line)(cx, UmiStr::from_static("fps 60"), 0xff_ff_ff);
    (host.hud_line)(cx, UmiStr::from_static("ping 12"), 0x00_ff_00);
    UmiStatus::OK
}

pub(crate) extern "C" fn draw_error(_: *mut UmiHudCx, _: *mut c_void) -> UmiStatus {
    UmiStatus::ERROR
}

pub(crate) extern "C" fn draw_panicked(_: *mut UmiHudCx, _: *mut c_void) -> UmiStatus {
    UmiStatus::PANICKED
}

/// Takes longer than the HUD budget.
pub(crate) extern "C" fn draw_slow(_: *mut UmiHudCx, _: *mut c_void) -> UmiStatus {
    std::thread::sleep(Duration::from_millis(2));
    UmiStatus::OK
}

/// Succeeds and records its arguments.
pub(crate) extern "C" fn run_ok(args: UmiStr, _: *mut c_void) -> UmiStatus {
    ARGS.with(|all| all.borrow_mut().push(testing::text(args)));
    UmiStatus::OK
}

pub(crate) extern "C" fn run_error(_: UmiStr, _: *mut c_void) -> UmiStatus {
    UmiStatus::ERROR
}

/// A command that reports a caught panic.
pub(crate) extern "C" fn run_panicked(_: UmiStr, _: *mut c_void) -> UmiStatus {
    UmiStatus::PANICKED
}
