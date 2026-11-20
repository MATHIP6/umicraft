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
    /// The manifest wraps or replaces a service the host does not offer. Checked
    /// before the library is opened, so nothing of the mod runs.
    #[error("the manifest names an unknown service `{service}` in `wraps` or `replaces`")]
    UnknownService {
        /// The service as written in `mod.toml`.
        service: ServiceId,
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

/// Why a registration made during `umi_mod_init` (setting, widget, key binding or
/// command) was refused. The host function reports it to the mod as a
/// [`UmiStatus`] ([`DeclareError::status`]) and logs the text for the mod's author.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeclareError {
    /// The call came after `umi_mod_init` returned.
    #[error("registrations are only accepted during `umi_mod_init`")]
    Late,
    /// A key or name breaks its grammar.
    #[error("invalid {what} `{value}`: expected {grammar}")]
    InvalidKey {
        /// What is being named (`setting key`, `command name`, ...).
        what: &'static str,
        /// The rejected text.
        value: String,
        /// The accepted grammar, as a regular expression.
        grammar: &'static str,
    },
    /// The same key or name was declared twice (by this mod or, for commands, by
    /// another one).
    #[error("{what} `{value}` is already registered")]
    Duplicate {
        /// What is being named.
        what: &'static str,
        /// The repeated text.
        value: String,
    },
    /// A text is empty, too long or not UTF-8.
    #[error("{what} must be valid UTF-8 of 1 to {max} bytes")]
    Text {
        /// Which text.
        what: &'static str,
        /// Its length limit, in bytes.
        max: usize,
    },
    /// A definition is inconsistent (a slider with `min >= max`, an unknown
    /// anchor, a null function...).
    #[error("{what}: {why}")]
    Invalid {
        /// What is being declared.
        what: &'static str,
        /// What is wrong with it.
        why: &'static str,
    },
    /// The mod declared more of something than the host accepts.
    #[error("at most {max} {what} may be registered")]
    TooMany {
        /// What is being declared.
        what: &'static str,
        /// The limit.
        max: usize,
    },
}

impl DeclareError {
    /// The status a mod receives: `DENIED` for a late call or too many items,
    /// `CONFLICT` for a name already taken, `INVALID_ARGUMENT` otherwise.
    pub fn status(&self) -> UmiStatus {
        match self {
            Self::Late | Self::TooMany { .. } => UmiStatus::DENIED,
            Self::Duplicate { .. } => UmiStatus::CONFLICT,
            _ => UmiStatus::INVALID_ARGUMENT,
        }
    }
}

/// Why the player's enable switch could not be changed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EnableError {
    /// No loaded mod has this id.
    #[error("no loaded mod `{0}`")]
    UnknownMod(ModId),
    /// The mod failed (panic, error status, over budget, or failed to initialise)
    /// and stays off until the client restarts.
    #[error("mod `{0}` failed and stays disabled until restart")]
    Failed(ModId),
}

/// Why a mod's setting, widget placement or key binding could not be changed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConfigureError {
    /// No loaded mod has this id.
    #[error("no loaded mod `{0}`")]
    UnknownMod(ModId),
    /// The mod declared no such setting, widget or key binding.
    #[error("the mod declares no {kind} `{key}`")]
    UnknownKey {
        /// `setting`, `widget` or `key binding`.
        kind: &'static str,
        /// The key that was asked for.
        key: String,
    },
}

/// Why a mod's configuration file could not be read or written.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The file could not be read or written.
    #[error("mod config I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// The file is not valid TOML for a mod config.
    #[error("mod config is malformed: {0}")]
    Parse(#[from] toml::de::Error),
    /// The config could not be serialised (a non-finite number, for instance).
    #[error("mod config cannot be serialised: {0}")]
    Serialize(#[from] toml::ser::Error),
}
