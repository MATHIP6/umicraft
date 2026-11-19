//! Host side of the Umicraft modding API (ADR-0086).
//!
//! F3.1b covers the part that needs no native code: reading a mod's `mod.toml`
//! ([`Manifest`]) and deciding which of the installed mods load, and in which order
//! ([`resolve`]). Every mod that cannot load comes back with a [`RejectReason`] whose
//! `Display` is written for the player, so the launcher can show why instead of
//! failing silently.

pub mod error;
pub mod manifest;
pub mod resolve;

pub use error::{IdError, ManifestError};
pub use manifest::{
    library_file_name, platform_dir, ApiVersion, ApiVersionError, Arch, Load, Manifest, ModId, Os,
    Permissions, ServiceId,
};
pub use resolve::{resolve, Candidate, RejectReason, Rejection, Resolution};
