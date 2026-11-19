// SPDX-License-Identifier: MIT OR Apache-2.0

//! Errors of manifest parsing.

use std::path::PathBuf;

use thiserror::Error;
use umicraft_modapi_sys::{UmiStatus, UmiVersion};

use crate::manifest::{ApiVersionError, ModId, ServiceId};

/// An identifier that does not match its grammar.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid {kind} `{value}`: expected {grammar}")]
pub struct IdError {
    /// What the identifier names (`mod id` or `service id`).
    pub kind: &'static str,
    /// The rejected text.
    pub value: String,
    /// The accepted grammar, as a regular expression.
    pub grammar: &'static str,
}

/// Why a `mod.toml` was refused.
///
/// The text is meant for the player or the mod author, so each variant says which
/// field is wrong.
#[derive(Debug, Error)]
pub enum ManifestError {
    /// Not valid TOML, a missing or unknown key, or a wrongly typed value.
    #[error("mod.toml is malformed: {0}")]
    Syntax(#[from] toml::de::Error),
    /// `mod.version` is not a semantic version.
    #[error("`mod.version` `{value}` is not a semantic version: {source}")]
    Version {
        /// The rejected text.
        value: String,
        /// The semver parser's complaint.
        source: semver::Error,
    },
    /// `mod.api_version` is not `major.minor`.
    #[error("`mod.api_version`: {0}")]
    ApiVersion(#[from] ApiVersionError),
    /// A dependency requirement is not a semver requirement.
    #[error("dependency `{dependency}` has the invalid requirement `{value}`: {source}")]
    Requirement {
        /// The dependency whose requirement is wrong.
        dependency: ModId,
        /// The rejected text.
        value: String,
        /// The semver parser's complaint.
        source: semver::Error,
    },
    /// `mod.name` is empty.
    #[error("`mod.name` must not be empty")]
    EmptyName,
    /// `mod.library` would not be a plain file stem.
    #[error("`mod.library` `{0}` must be non-empty and made of letters, digits, `_` and `-`")]
    InvalidLibrary(String),
    /// The mod lists itself under `[dependencies]`.
    #[error("mod `{0}` depends on itself")]
    SelfDependency(ModId),
    /// A service appears twice in one of the `[load]` lists.
    #[error("service `{service}` is listed twice in `load.{list}`")]
    DuplicateService {
        /// The list the service is repeated in (`replaces` or `wraps`).
        list: &'static str,
        /// The repeated service.
        service: ServiceId,
    },
    /// The same mod both replaces and wraps a service.
    #[error("service `{0}` is both replaced and wrapped by the same mod")]
    ReplacedAndWrapped(ServiceId),
}

/// Why a mod's native library was not loaded or initialised.
///
/// Every variant names what failed so the mods menu can show it to the player.
#[derive(Debug, Error)]
pub enum LoadError {
    /// This build targets an OS or CPU architecture mods are not built for.
    #[error("this platform is not supported by the mod loader")]
    UnsupportedPlatform,
    /// The mod ships no library for this platform.
    #[error("no library for this platform at {}", .path.display())]
    LibraryMissing {
        /// Where the library was expected.
        path: PathBuf,
    },
    /// The library's hash is not trusted and the player did not accept it.
    #[error("library with SHA-256 {sha256} is not trusted")]
    Untrusted {
        /// Lowercase hex SHA-256 of the library.
        sha256: String,
    },
    /// The library could not be read to compute its hash.
    #[error("cannot hash the library: {0}")]
    Hash(#[source] std::io::Error),
    /// The operating system refused to open the library.
    #[error("cannot open the library: {0}")]
    Open(#[source] libloading::Error),
    /// The library lacks one of the three mod exports.
    #[error("the library does not export `{symbol}`")]
    MissingSymbol {
        /// The missing symbol name.
        symbol: &'static str,
        /// The loader's complaint.
        #[source]
        source: libloading::Error,
    },
    /// The library was built for an API version this host cannot serve.
    #[error(
        "built for API {}.{}, host provides {}.{}",
        .module.major, .module.minor, .host.major, .host.minor
    )]
    ApiMismatch {
        /// The version the host implements.
        host: UmiVersion,
        /// The version the library reports.
        module: UmiVersion,
    },
    /// `umi_mod_init` returned a status other than `OK` (a Rust mod reports a
    /// caught panic as `PANICKED`).
    #[error("`umi_mod_init` failed with status {}", .0.0)]
    InitFailed(UmiStatus),
}

/// A mod that was resolved but could not be loaded.
#[derive(Debug, Error)]
#[error("mod `{id}` did not load: {reason}")]
pub struct LoadFailure {
    /// The mod that failed.
    pub id: ModId,
    /// Why it failed.
    #[source]
    pub reason: LoadError,
}

/// Why the trust list could not be read or written.
#[derive(Debug, Error)]
pub enum TrustError {
    /// The file could not be read or written.
    #[error("trust list I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// The file is not a valid trust list.
    #[error("trust list is malformed: {0}")]
    Parse(#[from] toml::de::Error),
    /// The list could not be serialised (never expected for plain strings).
    #[error("trust list cannot be serialised: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// Why one entry of the mods directory has no usable manifest.
#[derive(Debug, Error)]
pub enum DiscoverError {
    /// `mod.toml` exists but could not be read.
    #[error("cannot read mod.toml: {0}")]
    Read(#[from] std::io::Error),
    /// `mod.toml` was read but is invalid.
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    /// The directory is not named after the mod id; the loader finds a mod's
    /// library under `mods/<id>/`, so the mismatch is reported here, clearly,
    /// instead of later as a missing library.
    #[error("the folder `{folder}` must be named after the mod id `{id}`")]
    FolderName {
        /// The directory's actual name.
        folder: String,
        /// The id from `mod.toml`.
        id: String,
    },
}
