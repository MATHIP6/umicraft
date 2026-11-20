// SPDX-License-Identifier: MIT OR Apache-2.0

//! The unsafe edge of the `player_physics` service: the opaque `UmiPhysicsCx`, the
//! pointer to the state a layer edits, and the `register_physics` / `physics_next`
//! host functions (ADR-0087).
//!
//! The `UmiPhysicsCx` a layer receives points at the [`LayerRun`] on the host's stack
//! that describes the running layer. Like the HUD pointer it is valid for the call
//! and the calling thread only, so `physics_next` compares it with the run registered
//! as active on the current thread and refuses (`DENIED`) anything else: a mod that
//! kept the pointer, or passed it to another thread, cannot reach freed memory.
#![allow(unsafe_code)]

use core::ffi::c_void;
use core::marker::PhantomData;
use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};

use umicraft_modapi_sys::{
    UmiModHandle, UmiPhysicsCx, UmiPhysicsFn, UmiPlayerPhysics, UmiServiceMode, UmiStatus,
};

use super::{guarded, UserPtr};
use crate::physics::LayerRun;

/// The player state of the step being run, as the raw pointer layers receive.
///
/// Built from an exclusive borrow, so the pointer is valid for `'a`; the host uses
/// the state only through this handle while a step runs, one access at a time.
#[derive(Clone, Copy)]
pub(crate) struct StatePtr<'a> {
    ptr: *mut UmiPlayerPhysics,
    _borrow: PhantomData<&'a mut UmiPlayerPhysics>,
}

impl<'a> StatePtr<'a> {
    /// Takes over `state` for the duration of a step.
    pub(crate) fn new(state: &'a mut UmiPlayerPhysics) -> Self {
        Self {
            ptr: core::ptr::from_mut(state),
            _borrow: PhantomData,
        }
    }

    /// The pointer to give to a layer.
    pub(crate) fn as_ptr(self) -> *mut UmiPlayerPhysics {
        self.ptr
    }

    /// A copy of the state as it is now.
    pub(crate) fn read(self) -> UmiPlayerPhysics {
        // SAFETY: `ptr` came from a live `&'a mut` (see `new`). A layer that holds the
        // pointer only uses it during its own call, and the host reads between
        // calls, never while a layer runs.
        unsafe { *self.ptr }
    }

    /// Overwrites the state.
    pub(crate) fn write(self, value: UmiPlayerPhysics) {
        // SAFETY: as in `read`.
        unsafe { *self.ptr = value };
    }

    /// Lends the state to `f` as a mutable reference.
    pub(crate) fn with_mut<R>(self, f: impl FnOnce(&mut UmiPlayerPhysics) -> R) -> R {
        // SAFETY: as in `read`; the reference ends with `f`, which the host calls
        // only when no layer is running on this state.
        f(unsafe { &mut *self.ptr })
    }
}

thread_local! {
    /// The `LayerRun` of the layer running on this thread, or null.
    static ACTIVE: Cell<*mut c_void> = const { Cell::new(core::ptr::null_mut()) };
}

/// Restores the previously active run when a layer's call ends, even by unwinding;
/// nested layers (a wrapper calling `physics_next`) each restore their parent's.
struct Restore(*mut c_void);

impl Drop for Restore {
    fn drop(&mut self) {
        ACTIVE.set(self.0);
    }
}

/// Makes `run` the target of `physics_next` while `call` runs, handing it the opaque
/// pointer a layer takes.
pub(crate) fn with_physics_cx(
    run: &mut LayerRun<'_, '_>,
    call: impl FnOnce(*mut UmiPhysicsCx) -> UmiStatus,
) -> UmiStatus {
    let cx = core::ptr::from_mut(run).cast::<UmiPhysicsCx>();
    let _restore = Restore(ACTIVE.replace(cx.cast()));
    call(cx)
}

pub(super) extern "C" fn host_register_physics(
    me: UmiModHandle,
    mode: UmiServiceMode,
    step: Option<UmiPhysicsFn>,
    user: *mut c_void,
) -> UmiStatus {
    guarded(me, |context| {
        context.register_physics(mode, step, UserPtr::new(user))
    })
}

pub(super) extern "C" fn host_physics_next(cx: *mut UmiPhysicsCx) -> UmiStatus {
    catch_unwind(AssertUnwindSafe(|| {
        if cx.is_null() {
            return UmiStatus::INVALID_ARGUMENT;
        }
        if ACTIVE.get() != cx.cast() {
            return UmiStatus::DENIED;
        }
        // SAFETY: `cx` equals the address of the `LayerRun` that `with_physics_cx`
        // put in `ACTIVE` for this thread and removes when the layer's call ends, so
        // it is live. The host holds it only as the raw pointer given to the layer
        // (the `&mut` passed to `with_physics_cx` is not used until the call
        // returns), and `next` is the only reference made from it, ended on return.
        let run = unsafe { &mut *cx.cast::<LayerRun<'_, '_>>() };
        run.next()
    }))
    .unwrap_or(UmiStatus::PANICKED)
}

/// Raw-pointer helpers for the fake `extern "C"` layers of the tests, so those stay
/// free of `unsafe` (ADR-0087).
#[cfg(test)]
pub(crate) mod testing {
    use umicraft_modapi_sys::{UmiPhysicsInput, UmiPlayerPhysics};

    /// Sets the x velocity of the state a layer was given.
    pub(crate) fn set_velocity_x(state: *mut UmiPlayerPhysics, value: f64) {
        // SAFETY: the host passes a valid, exclusive state for the layer's call.
        unsafe { (*state).velocity[0] = value };
    }

    /// Multiplies the x velocity of the state a layer was given.
    pub(crate) fn scale_velocity_x(state: *mut UmiPlayerPhysics, factor: f64) {
        // SAFETY: as in `set_velocity_x`.
        unsafe { (*state).velocity[0] *= factor };
    }

    /// The yaw of the input a layer was given.
    pub(crate) fn input_yaw(input: *const UmiPhysicsInput) -> f32 {
        // SAFETY: the host passes a valid input for the layer's call.
        unsafe { (*input).yaw }
    }
}
