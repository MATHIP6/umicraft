// SPDX-License-Identifier: MIT OR Apache-2.0

//! The function table the host hands to a mod.

use core::ffi::c_void;

use crate::{UmiEventHeader, UmiEventId, UmiModHandle, UmiStatus, UmiStr, UmiVersion};

/// How a handler takes part in an event.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiMode(pub u32);

impl UmiMode {
    /// The handler only watches the event.
    pub const OBSERVE: Self = Self(0);
    /// The handler may change or cancel the event.
    pub const INTERCEPT: Self = Self(1);
}

/// Severity of a log message.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiLevel(pub u32);

impl UmiLevel {
    /// Something failed.
    pub const ERROR: Self = Self(1);
    /// Something is suspicious.
    pub const WARN: Self = Self(2);
    /// Normal progress information.
    pub const INFO: Self = Self(3);
    /// Developer diagnostics.
    pub const DEBUG: Self = Self(4);
    /// Very verbose diagnostics.
    pub const TRACE: Self = Self(5);
}

/// Event callback: receives the payload and the `user` pointer given to
/// `subscribe`, which the host passes back untouched.
pub type UmiHandlerFn = extern "C" fn(event: *mut UmiEventHeader, user: *mut c_void) -> UmiStatus;

/// The host function table, append-only (ADR-0087).
///
/// A mod must check `size` before using a field added in a later minor.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UmiHost {
    /// Size in bytes of this struct as the host built it.
    pub size: usize,
    /// API version the host implements.
    pub version: UmiVersion,
    /// Logs `msg` on behalf of mod `me` at `level`.
    pub log: extern "C" fn(me: UmiModHandle, level: UmiLevel, msg: UmiStr),
    /// Registers `handler` for `event`; lower `priority` runs first. Returns
    /// `INVALID_ARGUMENT` for an unknown event or mode, or a null handler.
    ///
    /// `handler` is an `Option` because a C mod may pass `NULL`: a bare
    /// function pointer must never be null in Rust, and `Option` of it has
    /// the same ABI with null as `None`.
    pub subscribe: extern "C" fn(
        me: UmiModHandle,
        event: UmiEventId,
        mode: UmiMode,
        priority: i32,
        handler: Option<UmiHandlerFn>,
        user: *mut c_void,
    ) -> UmiStatus,
}
