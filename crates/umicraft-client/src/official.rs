// SPDX-License-Identifier: MIT OR Apache-2.0

//! Locating and verifying the user's official 1.8.8 client jar.
//!
//! The font loader (M4) and the block-model loader (M5) both need the same
//! verified jar. This module keeps the version lookup, the version-JSON read
//! and the digest/size verification in one place so neither loader can drift
//! from the legal boundary of spec §3: the jar is only read, never copied into
//! the repository or a release.

use std::path::Path;

use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::install::OfficialInstall;
use umicraft_assets::manifest::VersionJson;
use umicraft_assets::AssetsError;

/// The only game version the client supports (protocol 47, spec §1.3).
pub(crate) const VERSION_ID: &str = "1.8.8";

/// Opens and verifies the 1.8.8 client jar of `install`.
///
/// # Errors
/// [`AssetsError::Missing`] when the version JSON or the jar does not exist,
/// [`AssetsError::Io`] when either cannot be read, and the manifest/hash/size
/// errors of [`VersionJson::parse`] and [`ClientJar::open`] when the files are
/// corrupt.
pub(crate) fn open_client_jar(install: &OfficialInstall) -> Result<ClientJar, AssetsError> {
    const OFFICIAL_1_8_8_JAR_SHA1: &str = "0983f08be6a4e624f5d85689d1aca869ed99c738";
    const OFFICIAL_1_8_8_JAR_SIZE: u64 = 8_465_313;

    let version_dir = install.versions_dir().join(VERSION_ID);
    let json_path = version_dir.join(format!("{VERSION_ID}.json"));
    if let Ok(json) = std::fs::read(&json_path) {
        if let Ok(version) = VersionJson::parse(&json) {
            if let Ok(descriptor) = version.require_client_jar() {
                let jar_path = client_jar_path(install, &version_dir)?;
                return ClientJar::open(jar_path, &descriptor.sha1, descriptor.size);
            }
        }
    }

    // Direct fallback if 1.8.8.jar exists without the version JSON
    let jar_path = client_jar_path(install, &version_dir)?;
    ClientJar::open(jar_path, OFFICIAL_1_8_8_JAR_SHA1, OFFICIAL_1_8_8_JAR_SIZE)
}

/// Discovers a verified client jar from an official installation or the Umicraft cache.
pub(crate) fn find_verified_jar() -> Result<ClientJar, AssetsError> {
    if let Some(install) = OfficialInstall::from_env() {
        if let Ok(jar) = open_client_jar(&install) {
            return Ok(jar);
        }
    }

    if let Ok(cache) = umicraft_assets::cache::AssetCache::in_data_dir() {
        if let Some(jar) = umicraft_assets::find_existing_jar(&cache) {
            return Ok(jar);
        }
    }

    Err(AssetsError::Missing {
        what: format!("client jar of version {VERSION_ID}"),
        path: "official installation or umicraft asset cache".to_string(),
    })
}

/// The client jar path of the installation, with a typed missing-file error.
fn client_jar_path(
    install: &OfficialInstall,
    version_dir: &Path,
) -> Result<std::path::PathBuf, AssetsError> {
    install
        .client_jar_path(VERSION_ID)
        .ok_or_else(|| AssetsError::Missing {
            what: format!("client jar of version {VERSION_ID}"),
            path: version_dir
                .join(format!("{VERSION_ID}.jar"))
                .display()
                .to_string(),
        })
}
