// SPDX-License-Identifier: MIT OR Apache-2.0

//! Host side of the Umicraft modding API (ADR-0086).
//!
//! F3.1 covers the part that needs no native code: reading a mod's `mod.toml`
//! ([`Manifest`]) and deciding which of the installed mods load, and in which order
//! ([`resolve`]). Every mod that cannot load comes back with a [`RejectReason`] whose
//! `Display` is written for the player, so the launcher can show why instead of
//! failing silently.
//!
//! F3.2 adds the native side: [`discover`] finds installed mods, [`TrustList`] gates
//! libraries by SHA-256, [`CrashSentinel`] detects an unclean exit, and [`ModHost`]
//! loads libraries and runs their lifecycle. All `unsafe` is confined to `ffi`.

mod context;
mod discover;
pub mod error;
mod ffi;
mod host;
mod load;
pub mod manifest;
pub mod resolve;
mod sentinel;
mod trust;

pub use context::{ModContext, Subscription};
pub use discover::{discover, Discovered};
pub use error::{DiscoverError, IdError, LoadError, LoadFailure, ManifestError, TrustError};
pub use ffi::UserPtr;
pub use host::ModHost;
pub use manifest::{
    library_file_name, platform_dir, ApiVersion, ApiVersionError, Arch, Load, Manifest, ModId, Os,
    Permissions, ServiceId,
};
pub use resolve::{resolve, Candidate, RejectReason, Rejection, Resolution};
pub use sentinel::{CrashSentinel, SentinelGuard};
pub use trust::{sha256_file, TrustList};
