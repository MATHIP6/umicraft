// SPDX-License-Identifier: MIT OR Apache-2.0

//! The unsafe edge of the world layers (API 1.4, ADR-0097): `register_world_layer`,
//! the opaque `UmiWorldCx` and the `world_*` drawing functions.
//!
//! As with `UmiHudCx`, the pointer a layer's draw function receives points at a
//! [`WorldCall`] on the host's stack, valid only during that call and on that
//! thread; every `world_*` function first compares it with the call registered as
//! active on the current thread and refuses (`DENIED`) anything else.
#![allow(unsafe_code)]

use core::ffi::c_void;
use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};

use umicraft_modapi_sys::{UmiModHandle, UmiStatus, UmiStr, UmiWorldCx, UmiWorldDrawFn};

use super::{guarded, text_of, UserPtr};
use crate::world_layer::{WorldCall, POLYGON_POINTS};

/// Longest key read from a mod; the key check refuses it past its own bound.
const MAX_KEY_BYTES: usize = 64;

thread_local! {
    /// The `WorldCall` being drawn on this thread, or null.
    static ACTIVE: Cell<*mut c_void> = const { Cell::new(core::ptr::null_mut()) };
}

/// Restores the previously active call when a draw call ends, even by unwinding.
struct Restore(*mut c_void);

impl Drop for Restore {
    fn drop(&mut self) {
        ACTIVE.set(self.0);
    }
}

/// Makes `call` the target of the `world_*` functions while `draw` runs, handing it
/// the opaque pointer a layer's draw function takes.
pub(crate) fn with_world_cx(
    call: &mut WorldCall<'_>,
    draw: impl FnOnce(*mut UmiWorldCx) -> UmiStatus,
) -> UmiStatus {
    let cx = core::ptr::from_mut(call).cast::<UmiWorldCx>();
    let _restore = Restore(ACTIVE.replace(cx.cast()));
    draw(cx)
}

/// Runs `body` on the call behind `world` if it is the one active on this thread.
fn with_call(
    world: *mut UmiWorldCx,
    body: impl FnOnce(&mut WorldCall<'_>) -> UmiStatus,
) -> UmiStatus {
    catch_unwind(AssertUnwindSafe(|| {
        if world.is_null() {
            return UmiStatus::INVALID_ARGUMENT;
        }
        if ACTIVE.get() != world.cast() {
            return UmiStatus::DENIED;
        }
        // SAFETY: `world` equals the address of the `WorldCall` that `with_world_cx`
        // put in `ACTIVE` for this thread and removes when its call ends, so it is
        // live, and it was made from a `&mut WorldCall` nobody else uses meanwhile:
        // the host passed only the raw pointer to the mod.
        let call = unsafe { &mut *world.cast::<WorldCall<'_>>() };
        body(call)
    }))
    .unwrap_or(UmiStatus::PANICKED)
}

pub(super) extern "C" fn host_register_world_layer(
    me: UmiModHandle,
    key: UmiStr,
    draw: Option<UmiWorldDrawFn>,
    user: *mut c_void,
) -> UmiStatus {
    guarded(me, |cx| {
        let result = text_of(key, "world layer key", MAX_KEY_BYTES)
            .and_then(|key| cx.register_world_layer(key, draw, UserPtr::new(user)));
        cx.report(result)
    })
}

#[allow(clippy::too_many_arguments)] // the ABI signature of `UmiHost::world_line`
pub(super) extern "C" fn host_world_line(
    world: *mut UmiWorldCx,
    x1: f64,
    y1: f64,
    z1: f64,
    x2: f64,
    y2: f64,
    z2: f64,
    color: u32,
    alpha: f32,
    thickness: f32,
    flags: u32,
) -> UmiStatus {
    with_call(world, |call| {
        call.line([[x1, y1, z1], [x2, y2, z2]], color, alpha, thickness, flags)
    })
}

#[allow(clippy::too_many_arguments)] // the ABI signature of `UmiHost::world_box`
pub(super) extern "C" fn host_world_box(
    world: *mut UmiWorldCx,
    min_x: f64,
    min_y: f64,
    min_z: f64,
    max_x: f64,
    max_y: f64,
    max_z: f64,
    color: u32,
    alpha: f32,
    flags: u32,
) -> UmiStatus {
    with_call(world, |call| {
        call.cuboid(
            [[min_x, min_y, min_z], [max_x, max_y, max_z]],
            color,
            alpha,
            flags,
        )
    })
}

pub(super) extern "C" fn host_world_polygon(
    world: *mut UmiWorldCx,
    points: *const f64,
    count: usize,
    color: u32,
    alpha: f32,
    flags: u32,
) -> UmiStatus {
    let (fewest, most) = POLYGON_POINTS;
    if points.is_null() || !(fewest..=most).contains(&count) {
        return UmiStatus::INVALID_ARGUMENT;
    }
    // SAFETY: `points` is non-null and the ABI promises `3 * count` readable
    // doubles; `count` is at most 64. `[f64; 3]` has the alignment of `f64`.
    let points = unsafe { core::slice::from_raw_parts(points.cast::<[f64; 3]>(), count) };
    with_call(world, |call| call.polygon(points, color, alpha, flags))
}

pub(super) extern "C" fn host_world_highlight(
    world: *mut UmiWorldCx,
    entity: i32,
    color: u32,
    alpha: f32,
    flags: u32,
) -> UmiStatus {
    with_call(world, |call| call.highlight(entity, color, alpha, flags))
}
