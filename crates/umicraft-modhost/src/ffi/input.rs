// SPDX-License-Identifier: MIT OR Apache-2.0

//! The C entry points of the input functions; no pointer is read, the policy is in
//! [`crate::input`] (API 1.5).

use umicraft_modapi_sys::{UmiModHandle, UmiStatus};

use super::guarded;

pub(super) extern "C" fn host_input_press(me: UmiModHandle, action: u32, count: u32) -> UmiStatus {
    guarded(me, |cx| cx.input_press(action, count))
}

pub(super) extern "C" fn host_input_clear_press(me: UmiModHandle, action: u32) -> UmiStatus {
    guarded(me, |cx| cx.input_clear_press(action))
}

pub(super) extern "C" fn host_input_add_look(me: UmiModHandle, yaw: f32, pitch: f32) -> UmiStatus {
    guarded(me, |cx| cx.input_add_look(yaw, pitch))
}
