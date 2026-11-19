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

/// The API version a host implements or a mod was built for.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmiVersion {
    /// Major version; a different major is never compatible.
    pub major: u16,
    /// Minor version; see [`UmiVersion::accepts`] for how it is compared.
    pub minor: u16,
}

/// The API version of this crate: unstable `0.1` until the `1.0` freeze.
pub const UMI_API_VERSION: UmiVersion = UmiVersion { major: 0, minor: 1 };

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
