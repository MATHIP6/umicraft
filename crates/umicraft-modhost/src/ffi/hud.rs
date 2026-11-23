// SPDX-License-Identifier: MIT OR Apache-2.0

//! The unsafe edge of widget drawing: the opaque `UmiHudCx` pointer and the `hud_*`
//! host functions (ADR-0087).
//!
//! The `UmiHudCx` a draw function receives points at a [`HudCall`] on the host's
//! stack that wraps the `&mut HudContent` of the widget being drawn. The pointer is
//! only valid during the draw call and only on the drawing thread; a mod that kept it
//! or hands it to another thread would otherwise cause a use after free, so every
//! `hud_*` function first compares it with the call registered as active on the
//! current thread and refuses (`DENIED`) anything else.
#![allow(unsafe_code)]

use core::ffi::c_void;
use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};

use umicraft_builtin_mods::HudContent;
use umicraft_modapi_sys::{UmiHudCx, UmiStatus, UmiStr};

use super::utf8_of;
use crate::ui::draw;

/// Longest text read from a mod for one HUD item; `draw` cuts it further.
const MAX_READ_BYTES: usize = 4096;

/// What a `UmiHudCx` points at: the content being filled.
struct HudCall<'a> {
    content: &'a mut HudContent,
}

thread_local! {
    /// The `HudCall` being drawn on this thread, or null.
    static ACTIVE: Cell<*mut c_void> = const { Cell::new(core::ptr::null_mut()) };
}

/// Restores the previously active call when a draw call ends, even by unwinding.
struct Restore(*mut c_void);

impl Drop for Restore {
    fn drop(&mut self) {
        ACTIVE.set(self.0);
    }
}

/// Makes `content` the target of the `hud_*` functions while `draw` runs, handing it
/// the opaque pointer a widget's draw function takes.
///
/// `content` is reborrowed through the pointer for the call, so the caller must not
/// touch it meanwhile; the borrow checker enforces that.
pub(crate) fn with_hud_cx(
    content: &mut HudContent,
    draw: impl FnOnce(*mut UmiHudCx) -> UmiStatus,
) -> UmiStatus {
    let mut call = HudCall { content };
    let cx = core::ptr::from_mut(&mut call).cast::<UmiHudCx>();
    let _restore = Restore(ACTIVE.replace(cx.cast()));
    draw(cx)
}

/// Runs `body` on the content behind `hud` if `hud` is the call active on this thread.
fn with_call(hud: *mut UmiHudCx, body: impl FnOnce(&mut HudContent) -> UmiStatus) -> UmiStatus {
    catch_unwind(AssertUnwindSafe(|| {
        if hud.is_null() {
            return UmiStatus::INVALID_ARGUMENT;
        }
        if ACTIVE.get() != hud.cast() {
            return UmiStatus::DENIED;
        }
        // SAFETY: `hud` equals the address of the `HudCall` that `with_hud_cx` put in
        // `ACTIVE` for this thread and removes when its call ends, so it is live and
        // was created from a `&mut HudContent` that nobody else uses until then. No
        // other reference to the `HudCall` exists: the host passed only the raw pointer
        // to the mod.
        let call = unsafe { &mut *hud.cast::<HudCall>() };
        body(call.content)
    }))
    .unwrap_or(UmiStatus::PANICKED)
}

/// Runs `body` with the decoded text, refusing invalid UTF-8.
fn with_text(
    hud: *mut UmiHudCx,
    text: UmiStr,
    body: impl FnOnce(&mut HudContent, &str) -> UmiStatus,
) -> UmiStatus {
    match utf8_of(text, MAX_READ_BYTES) {
        Some(text) => with_call(hud, |content| body(content, text)),
        None => UmiStatus::INVALID_ARGUMENT,
    }
}

pub(super) extern "C" fn host_hud_line(hud: *mut UmiHudCx, text: UmiStr, color: u32) -> UmiStatus {
    with_text(hud, text, |content, text| draw::line(content, text, color))
}

pub(super) extern "C" fn host_hud_label(
    hud: *mut UmiHudCx,
    x: f32,
    y: f32,
    text: UmiStr,
    color: u32,
    alpha: f32,
    scale: f32,
) -> UmiStatus {
    with_text(hud, text, |content, text| {
        draw::label(content, [x, y], text, color, alpha, scale)
    })
}

// The argument list is the ABI's (`UmiHost::hud_cell`), not a design choice.
#[allow(clippy::too_many_arguments)]
pub(super) extern "C" fn host_hud_cell(
    hud: *mut UmiHudCx,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    pressed: u8,
    text: UmiStr,
    color: u32,
) -> UmiStatus {
    with_text(hud, text, |content, text| {
        draw::cell(content, [x, y, width, height], pressed != 0, text, color)
    })
}

pub(super) extern "C" fn host_hud_tick(
    hud: *mut UmiHudCx,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: u32,
    alpha: f32,
) -> UmiStatus {
    with_call(hud, |content| {
        draw::tick(content, [x, y, width, height], color, alpha)
    })
}

pub(super) extern "C" fn host_hud_outline(
    hud: *mut UmiHudCx,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: u32,
    alpha: f32,
) -> UmiStatus {
    with_call(hud, |content| {
        draw::outline(content, [x, y, width, height], color, alpha)
    })
}

#[allow(clippy::too_many_arguments)] // the ABI signature of `UmiHost::hud_segment`
pub(super) extern "C" fn host_hud_segment(
    hud: *mut UmiHudCx,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    color: u32,
    alpha: f32,
    thickness: f32,
) -> UmiStatus {
    with_call(hud, |content| {
        draw::segment(content, [[x1, y1], [x2, y2]], color, alpha, thickness)
    })
}

pub(super) extern "C" fn host_hud_reserve(
    hud: *mut UmiHudCx,
    width: f32,
    height: f32,
) -> UmiStatus {
    with_call(hud, |content| draw::reserve(content, width, height))
}
