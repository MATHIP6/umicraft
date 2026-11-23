// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `register_physics` / `physics_next` host functions of the `player_physics`
//! service (ADR-0087); the pointer checks they rely on are in `ffi::service`.
#![allow(unsafe_code)]

use core::ffi::c_void;
use std::cell::Cell;

use umicraft_modapi_sys::{UmiModHandle, UmiPhysicsCx, UmiPhysicsFn, UmiServiceMode, UmiStatus};

use super::service::service_next;
use super::{guarded, UserPtr};
use crate::physics::PlayerPhysics;

thread_local! {
    /// The `LayerRun` of the `player_physics` layer running on this thread, or null.
    pub(crate) static PHYSICS_ACTIVE: Cell<*mut c_void> = const { Cell::new(core::ptr::null_mut()) };
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
    service_next::<PlayerPhysics>(cx.cast())
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
