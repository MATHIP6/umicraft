//! Errors of manifest parsing.

use thiserror::Error;

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
