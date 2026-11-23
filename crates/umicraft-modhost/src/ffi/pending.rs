// SPDX-License-Identifier: MIT OR Apache-2.0

//! Host functions of API 1.1 whose phase has not landed yet (ADR-0094): each
//! answers `UNSUPPORTED`, which a mod already handles for a missing feature.
//! Every phase of `docs/tasks/api-1.1` removes its stubs from here.

use core::ffi::c_void;

use umicraft_modapi_sys::{
    UmiCameraCx, UmiCameraFn, UmiEntity, UmiModHandle, UmiServiceMode, UmiStatus,
};

pub(super) extern "C" fn host_register_camera(
    _me: UmiModHandle,
    _mode: UmiServiceMode,
    _step: Option<UmiCameraFn>,
    _user: *mut c_void,
) -> UmiStatus {
    UmiStatus::UNSUPPORTED
}

pub(super) extern "C" fn host_camera_next(_cx: *mut UmiCameraCx) -> UmiStatus {
    UmiStatus::UNSUPPORTED
}

pub(super) extern "C" fn host_world_entities_snapshot(
    _me: UmiModHandle,
    _out: *mut UmiEntity,
    _capacity: usize,
    _count: *mut usize,
) -> UmiStatus {
    UmiStatus::UNSUPPORTED
}
