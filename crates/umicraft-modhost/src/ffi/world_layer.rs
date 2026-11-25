// SPDX-License-Identifier: MIT OR Apache-2.0

//! The unsafe edge of the world layers (API 1.4, ADR-0097): `register_world_layer`
//! and the `world_*` drawing functions.
#![allow(unsafe_code)]

use core::ffi::c_void;

use umicraft_modapi_sys::{UmiModHandle, UmiStatus, UmiStr, UmiWorldCx, UmiWorldDrawFn};

pub(super) extern "C" fn host_register_world_layer(
    _me: UmiModHandle,
    _key: UmiStr,
    _draw: Option<UmiWorldDrawFn>,
    _user: *mut c_void,
) -> UmiStatus {
    UmiStatus::UNSUPPORTED
}

#[allow(clippy::too_many_arguments)] // the ABI signature of `UmiHost::world_line`
pub(super) extern "C" fn host_world_line(
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

#[allow(clippy::too_many_arguments)] // the ABI signature of `UmiHost::world_box`
pub(super) extern "C" fn host_world_box(
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

pub(super) extern "C" fn host_world_polygon(
    _world: *mut UmiWorldCx,
    _points: *const f64,
    _count: usize,
    _color: u32,
    _alpha: f32,
    _flags: u32,
) -> UmiStatus {
    UmiStatus::UNSUPPORTED
}

pub(super) extern "C" fn host_world_highlight(
    _world: *mut UmiWorldCx,
    _entity: i32,
    _color: u32,
    _alpha: f32,
    _flags: u32,
) -> UmiStatus {
    UmiStatus::UNSUPPORTED
}
