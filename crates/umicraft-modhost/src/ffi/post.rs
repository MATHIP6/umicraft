// SPDX-License-Identifier: MIT OR Apache-2.0

//! The unsafe edge of the F3.7 post-process functions: strings become owned,
//! validated Rust values here; the decisions are in [`crate::post_pass`].
#![allow(unsafe_code)]

use umicraft_modapi_sys::{UmiModHandle, UmiStatus, UmiStr};

use super::{guarded, text_of};
use crate::post::MAX_SOURCE_BYTES;
use crate::ui::MAX_LABEL_BYTES;

/// Longest key, in bytes (the grammar's own limit).
const MAX_KEY_BYTES: usize = 64;

pub(super) extern "C" fn host_register_post_pass(
    me: UmiModHandle,
    key: UmiStr,
    label: UmiStr,
    wgsl: UmiStr,
) -> UmiStatus {
    guarded(me, |cx| {
        let result = (|| {
            let key = text_of(key, "post pass key", MAX_KEY_BYTES)?;
            let label = text_of(label, "post pass label", MAX_LABEL_BYTES)?;
            let wgsl = text_of(wgsl, "post pass shader", MAX_SOURCE_BYTES)?;
            cx.register_post_pass(key, label, wgsl)
        })();
        cx.report(result)
    })
}

pub(super) extern "C" fn host_post_set_param(
    me: UmiModHandle,
    key: UmiStr,
    index: u32,
    x: f32,
    y: f32,
    z: f32,
    w: f32,
) -> UmiStatus {
    guarded(me, |cx| {
        match text_of(key, "post pass key", MAX_KEY_BYTES) {
            Ok(key) => cx.post_set_param(key, index, [x, y, z, w]),
            Err(_) => UmiStatus::INVALID_ARGUMENT,
        }
    })
}
