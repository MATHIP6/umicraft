// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `mod.toml` manifest (spec §8.2).
//!
//! The file is first deserialized into private raw structs (strict: unknown keys are
//! errors) and then converted, so that semver parsing and the cross-field rules live
//! in one place and a [`Manifest`] that exists is always valid.

mod api_version;
mod id;
mod platform;

use std::collections::{BTreeMap, BTreeSet};

use semver::{Version, VersionReq};
use serde::Deserialize;

pub use api_version::{ApiVersion, ApiVersionError};
pub(crate) use id::is_valid as is_identifier;
pub use id::{ModId, ServiceId};
pub use platform::{library_file_name, platform_dir, Arch, Os};

use crate::error::ManifestError;

/// The service that moves the local player once per tick (F3.6).
pub const PLAYER_PHYSICS: &str = "player_physics";

/// The service that places the camera of each frame (API 1.1, ADR-0094).
pub const RENDER_CAMERA: &str = "render_camera";

/// Every service a manifest may name in `wraps` or `replaces`; the loader refuses
/// anything else ([`LoadError::UnknownService`](crate::LoadError::UnknownService)) so a
/// typo is reported instead of silently doing nothing.
pub const KNOWN_SERVICES: &[&str] = &[PLAYER_PHYSICS, RENDER_CAMERA];

/// The `[load]` table: ordering hints and service hooks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Load {
    /// Lower loads earlier, among mods with no dependency between them.
    pub priority: i32,
    /// Services this mod replaces entirely; at most one mod may per service.
    pub replaces: Vec<ServiceId>,
    /// Services this mod wraps; any number of mods may, chained in load order.
    pub wraps: Vec<ServiceId>,
}

impl Load {
    /// The first service in `replaces` or `wraps` that the host does not offer.
    pub fn unknown_service(&self) -> Option<&ServiceId> {
        let named = self.replaces.iter().chain(&self.wraps);
        named
            .into_iter()
            .find(|s| !KNOWN_SERVICES.contains(&s.as_str()))
    }

    /// Whether `service` is in `wraps`.
    pub fn wraps_service(&self, service: &str) -> bool {
        self.wraps.iter().any(|s| s.as_str() == service)
    }

    /// Whether `service` is in `replaces`.
    pub fn replaces_service(&self, service: &str) -> bool {
        self.replaces.iter().any(|s| s.as_str() == service)
    }
}

/// The `[permissions]` table: what the mod asks for, shown to the user.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Permissions {
    /// May hook network packets.
    pub network_packets: bool,
    /// May alter player movement.
    pub movement: bool,
    /// May move the render camera (`render_camera`, API 1.1). Shown to the player
    /// only: it changes what is drawn, not what the server sees (ADR-0094).
    pub camera: bool,
    /// May read the attack and use bindings and press them, and turn the player's
    /// look (`INPUT`, `input_*`, API 1.5). Not held off by server-compat mode, by the
    /// owner's choice (ADR-0098); the mods menu shows it.
    pub input: bool,
    /// May access the filesystem beyond its own directory.
    pub filesystem: bool,
    /// May make HTTP requests.
    pub network_http: bool,
}

/// A validated `mod.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Unique id of the mod.
    pub id: ModId,
    /// Display name, never empty.
    pub name: String,
    /// The mod's own version.
    pub version: Version,
    /// Author names, as written.
    pub authors: Vec<String>,
    /// The API version the mod was built for.
    pub api_version: ApiVersion,
    /// SPDX license expression, if given.
    pub license: Option<String>,
    /// One-line description, if given.
    pub description: Option<String>,
    /// Library file stem under `bin/<os>-<arch>/`; the id unless overridden.
    pub library: String,
    /// Required mods and the versions accepted.
    pub dependencies: BTreeMap<ModId, VersionReq>,
    /// Ordering and service hooks.
    pub load: Load,
    /// Requested permissions.
    pub permissions: Permissions,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(rename = "mod")]
    module: RawMod,
    #[serde(default)]
    dependencies: BTreeMap<ModId, String>,
    #[serde(default)]
    load: Load,
    #[serde(default)]
    permissions: Permissions,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMod {
    id: ModId,
    name: String,
    version: String,
    authors: Vec<String>,
    api_version: ApiVersion,
    license: Option<String>,
    description: Option<String>,
    library: Option<String>,
}

impl Manifest {
    /// Parses and validates the text of a `mod.toml`.
    ///
    /// # Errors
    ///
    /// [`ManifestError`] for malformed TOML, unknown or missing keys, invalid ids or
    /// versions, an empty name, a self-dependency, or inconsistent `[load]` lists.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let raw: RawFile = toml::from_str(text)?;
        let RawMod {
            id,
            name,
            version,
            authors,
            api_version,
            license,
            description,
            library,
        } = raw.module;
        if name.trim().is_empty() {
            return Err(ManifestError::EmptyName);
        }
        let library = library.unwrap_or_else(|| id.to_string());
        if !is_file_stem(&library) {
            return Err(ManifestError::InvalidLibrary(library));
        }
        let version = Version::parse(&version).map_err(|source| ManifestError::Version {
            value: version,
            source,
        })?;
        let dependencies = parse_dependencies(&id, raw.dependencies)?;
        validate_load(&raw.load)?;
        Ok(Self {
            id,
            name,
            version,
            authors,
            api_version,
            license,
            description,
            library,
            dependencies,
            load: raw.load,
            permissions: raw.permissions,
        })
    }
}

/// A library stem must not be able to leave `bin/<os>-<arch>/`.
fn is_file_stem(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn parse_dependencies(
    owner: &ModId,
    raw: BTreeMap<ModId, String>,
) -> Result<BTreeMap<ModId, VersionReq>, ManifestError> {
    raw.into_iter()
        .map(|(dependency, value)| {
            if &dependency == owner {
                return Err(ManifestError::SelfDependency(dependency));
            }
            match VersionReq::parse(&value) {
                Ok(req) => Ok((dependency, req)),
                Err(source) => Err(ManifestError::Requirement {
                    dependency,
                    value,
                    source,
                }),
            }
        })
        .collect()
}

fn validate_load(load: &Load) -> Result<(), ManifestError> {
    ensure_unique("replaces", &load.replaces)?;
    ensure_unique("wraps", &load.wraps)?;
    match load.replaces.iter().find(|s| load.wraps.contains(s)) {
        Some(service) => Err(ManifestError::ReplacedAndWrapped(service.clone())),
        None => Ok(()),
    }
}

fn ensure_unique(list: &'static str, services: &[ServiceId]) -> Result<(), ManifestError> {
    let mut seen = BTreeSet::new();
    match services.iter().find(|s| !seen.insert(*s)) {
        Some(service) => Err(ManifestError::DuplicateService {
            list,
            service: service.clone(),
        }),
        None => Ok(()),
    }
}
