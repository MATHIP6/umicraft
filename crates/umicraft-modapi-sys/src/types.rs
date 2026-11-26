// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core value types shared by every part of the ABI.

/// A borrowed UTF-8 string, valid for the duration of the call only.
///
/// `ptr` may be null when `len` is 0. The type has no reading helper because
/// that needs `unsafe`; the `ffi` modules of the host and of `umicraft-modapi`
/// do the conversion. It is not `Send`/`Sync` because of the raw pointer.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiStr {
    /// First byte of the UTF-8 data, or null when `len` is 0.
    pub ptr: *const u8,
    /// Length in bytes (not characters), without any terminator.
    pub len: usize,
}

impl UmiStr {
    /// The empty string (null pointer, zero length).
    pub const EMPTY: Self = Self {
        ptr: core::ptr::null(),
        len: 0,
    };

    /// Borrows a `'static` string; being static guarantees the "valid for the
    /// call" rule without `unsafe`.
    pub const fn from_static(s: &'static str) -> Self {
        Self {
            ptr: s.as_ptr(),
            len: s.len(),
        }
    }
}

/// Borrowed bytes of any content, valid for the duration of the call only.
///
/// `ptr` may be null when `len` is 0. Added in API 0.4 for packet bodies.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiBytes {
    /// First byte, or null when `len` is 0.
    pub ptr: *const u8,
    /// Length in bytes.
    pub len: usize,
}

impl UmiBytes {
    /// No bytes (null pointer, zero length).
    pub const EMPTY: Self = Self {
        ptr: core::ptr::null(),
        len: 0,
    };

    /// Borrows a `'static` byte slice without `unsafe`.
    pub const fn from_static(bytes: &'static [u8]) -> Self {
        Self {
            ptr: bytes.as_ptr(),
            len: bytes.len(),
        }
    }
}

/// The API version a host implements or a mod was built for.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiVersion {
    /// Major version; a different major is never compatible.
    pub major: u16,
    /// Minor version; see [`UmiVersion::accepts`] for how it is compared.
    pub minor: u16,
}

/// The API version of this crate: unstable `0.x` until the `1.0` freeze.
///
/// 0.2 added the chat, connection and screen events, `UMI_EVENT_MUTABLE` and
/// `UmiHost::event_set_text` (F3.3); 0.3 added settings, HUD widgets, key
/// bindings, client commands and `KEY_PRESSED` (F3.4); 0.4 added the packet
/// events, `UmiBytes`, `UmiStatus::BUSY` and `UmiHost::packet_send` (F3.5); 0.5
/// added the `player_physics` service and the world queries (F3.6); 0.6 added
/// the post-process passes (F3.7); 1.0 added `FRAME` and froze the ABI (F3.9):
/// from now on fields and functions are only appended, with a minor bump, and
/// nothing is removed or reordered within major 1 (ADR-0087); 1.1 added the
/// `render_camera` service, `PRE_RENDER`, the camera of `FRAME`, the entity
/// snapshot and metadata, writable flight and game mode, two HUD primitives and
/// the screen anchor (ADR-0094); 1.2 added categories, modules and buttons in the
/// mods menu (ADR-0095); 1.3 added `camera_capture`, for a free camera
/// (ADR-0096); 1.4 added the combat facts, world layers and the entity mask
/// (ADR-0097); 1.5 added the `INPUT` event and the input host functions
/// (ADR-0098).
pub const UMI_API_VERSION: UmiVersion = UmiVersion { major: 1, minor: 5 };

impl UmiVersion {
    /// Whether a host of version `self` can load a mod built for `mod_version`.
    ///
    /// Major 0 is unstable, so it needs an exact major and minor match. From
    /// major 1 on, the majors must match and the host minor must be at least
    /// the mod minor (structs only grow by appending fields).
    pub const fn accepts(self, mod_version: UmiVersion) -> bool {
        if self.major != mod_version.major {
            return false;
        }
        if self.major == 0 {
            self.minor == mod_version.minor
        } else {
            self.minor >= mod_version.minor
        }
    }
}

/// Result code of a call across the boundary.
///
/// A transparent newtype instead of a Rust enum: a C mod may return any
/// `i32`, and an out-of-range enum discriminant would be undefined behavior.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiStatus(pub i32);

impl UmiStatus {
    /// Success.
    pub const OK: Self = Self(0);
    /// Unspecified failure.
    pub const ERROR: Self = Self(1);
    /// The callee panicked; the panic was caught at the boundary.
    pub const PANICKED: Self = Self(2);
    /// The host or mod does not support the request.
    pub const UNSUPPORTED: Self = Self(3);
    /// An argument was invalid (null, out of range, unknown id).
    pub const INVALID_ARGUMENT: Self = Self(4);
    /// The mod lacks the permission for the request.
    pub const DENIED: Self = Self(5);
    /// The request conflicts with existing state (e.g. duplicate id).
    pub const CONFLICT: Self = Self(6);
    /// A bounded queue is full; the request may succeed later. Added in API 0.4.
    pub const BUSY: Self = Self(7);
    /// What was asked is not available here: a world query outside a
    /// game-thread callback, or a block of a chunk that is not loaded. Added in
    /// API 0.5.
    pub const UNAVAILABLE: Self = Self(8);

    /// Whether the status is [`UmiStatus::OK`].
    pub const fn is_ok(self) -> bool {
        self.0 == Self::OK.0
    }
}

/// The host's per-mod state; opaque to mods, which only hold pointers to it.
///
/// A zero-sized `repr(C)` struct with a private field: it cannot be built or
/// read outside the host, and cbindgen emits it as an incomplete C type.
#[repr(C)]
pub struct UmiModContext {
    _private: [u8; 0],
}

/// The handle a mod passes back to every host function.
///
/// A pointer rather than an index so that host functions reach their state
/// without a process-global registry: several hosts (tests run in parallel)
/// never share one. The host keeps the pointee alive and at a fixed address
/// until the client exits (ADR-0086: unload means disable).
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiModHandle(pub *mut UmiModContext);
