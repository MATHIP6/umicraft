//! The C ABI of the Umicraft modding API (ADR-0087).
//!
//! Everything here is `#[repr(C)]` or `#[repr(transparent)]`, so C, C++, Zig
//! and Rust mods see the same layout. The crate is the single source of truth
//! for `include/umicraft.h` (generated with cbindgen, see `tools/gen-header.sh`).
//!
//! The crate contains no `unsafe`: it only declares types. Reading through the
//! raw pointers is the job of the `ffi` modules of `umicraft-modapi` and
//! `umicraft-modhost`. Structs holding raw pointers or `extern "C"` callbacks
//! are deliberately not `Send`/`Sync`; wrappers that move them across threads
//! must justify that themselves.

mod events;
mod exports;
mod host;
mod types;

pub use events::{
    UmiEventHeader, UmiEventId, UmiPostTick, UmiPreTick, UMI_EVENT_CANCELLABLE, UMI_EVENT_CANCELLED,
};
pub use exports::{
    UmiModApiVersionFn, UmiModInitFn, UmiModShutdownFn, UMI_MOD_API_VERSION_SYMBOL,
    UMI_MOD_INIT_SYMBOL, UMI_MOD_SHUTDOWN_SYMBOL,
};
pub use host::{UmiHandlerFn, UmiHost, UmiLevel, UmiMode};
pub use types::{UmiModContext, UmiModHandle, UmiStatus, UmiStr, UmiVersion, UMI_API_VERSION};
