// SPDX-License-Identifier: MIT OR Apache-2.0

//! The unsafe edge of `packet_send`: the mod's byte view becomes a slice here; the
//! policy (permission, validation, queue) is in [`crate::packets`] (ADR-0087).
#![allow(unsafe_code)]

use umicraft_modapi_sys::{UmiBytes, UmiModHandle, UmiStatus};

use super::guarded;
use crate::packets::MAX_PACKET_BODY_BYTES;

/// Borrows a mod-provided packet body, `None` if it is longer than the limit.
fn body_of<'a>(bytes: UmiBytes) -> Option<&'a [u8]> {
    if bytes.len > MAX_PACKET_BODY_BYTES {
        return None;
    }
    if bytes.len == 0 {
        return Some(&[]);
    }
    // A null pointer with a length is a broken mod, not an empty body.
    if bytes.ptr.is_null() {
        return None;
    }
    // SAFETY: the ABI promises `ptr` points to `len` readable bytes for the duration
    // of the call; null was excluded above and `len` is bounded by the limit.
    Some(unsafe { core::slice::from_raw_parts(bytes.ptr, bytes.len) })
}

pub(super) extern "C" fn host_packet_send(me: UmiModHandle, id: u32, data: UmiBytes) -> UmiStatus {
    guarded(me, |cx| match body_of(data) {
        Some(body) => cx.packet_send(id, body),
        None => UmiStatus::INVALID_ARGUMENT,
    })
}
