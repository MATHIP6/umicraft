// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `register_camera` / `camera_next` host functions of the `render_camera`
//! service (API 1.1, ADR-0094) and `camera_capture` (API 1.3, ADR-0096); the pointer
//! checks `camera_next` relies on are in `ffi::service`.

use core::ffi::c_void;
use std::cell::Cell;

use umicraft_modapi_sys::{UmiCameraCx, UmiCameraFn, UmiModHandle, UmiServiceMode, UmiStatus};

use super::service::service_next;
use super::{guarded, UserPtr};
use crate::camera::RenderCamera;

thread_local! {
    /// The `LayerRun` of the `render_camera` layer running on this thread, or null.
    pub(crate) static CAMERA_ACTIVE: Cell<*mut c_void> = const { Cell::new(core::ptr::null_mut()) };
    /// Whether a layer of the run in progress on this thread asked for the input.
    static CAPTURE: Cell<bool> = const { Cell::new(false) };
}

/// Runs `place` (one run of the camera chain) and returns whether one of its layers
/// called `camera_capture`.
pub(crate) fn capturing(place: impl FnOnce()) -> bool {
    CAPTURE.with(|capture| capture.set(false));
    place();
    CAPTURE.with(Cell::take)
}

pub(super) extern "C" fn host_register_camera(
    me: UmiModHandle,
    mode: UmiServiceMode,
    step: Option<UmiCameraFn>,
    user: *mut c_void,
) -> UmiStatus {
    guarded(me, |context| {
        context.register_camera(mode, step, UserPtr::new(user))
    })
}

pub(super) extern "C" fn host_camera_next(cx: *mut UmiCameraCx) -> UmiStatus {
    service_next::<RenderCamera>(cx.cast())
}

/// Only the running layer's own `cx` is accepted, as for `camera_next`: one kept
/// from an earlier frame, or used from another thread, is refused. Nothing is read
/// through the pointer, so no `unsafe` is needed.
pub(super) extern "C" fn host_camera_capture(cx: *mut UmiCameraCx) -> UmiStatus {
    let running = CAMERA_ACTIVE.with(Cell::get);
    if cx.is_null() || cx.cast::<c_void>() != running {
        return UmiStatus::DENIED;
    }
    CAPTURE.with(|capture| capture.set(true));
    UmiStatus::OK
}

/// Raw-pointer helpers for the fake `extern "C"` layers of the tests, so those stay
/// free of `unsafe` (ADR-0087).
#[cfg(test)]
pub(crate) mod testing {
    use umicraft_modapi_sys::UmiRenderCamera;

    /// Moves the eye of the camera a layer was given up by `dy` blocks.
    pub(crate) fn raise_eye(camera: *mut UmiRenderCamera, dy: f64) {
        // SAFETY: the host passes a valid, exclusive camera for the layer's call.
        unsafe { (*camera).eye[1] += dy };
    }

    /// Writes `value` into every angle and the field of view.
    pub(crate) fn set_angles(camera: *mut UmiRenderCamera, value: f32) {
        // SAFETY: as in `raise_eye`.
        unsafe {
            (*camera).yaw = value;
            (*camera).pitch = value;
            (*camera).fov_y = value;
            (*camera).aspect = value;
        }
    }
}
