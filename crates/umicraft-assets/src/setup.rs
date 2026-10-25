// SPDX-License-Identifier: MIT OR Apache-2.0

//! Orchestration of the first-run asset setup (spec AST-1, AST-2, AST-3).
//!
//! When the client runs on a machine without an existing Minecraft 1.8.8 installation,
//! this module orchestrates the official download flow:
//!
//! 1. Fetching the official version manifest (`version_manifest_v2.json`);
//! 2. Resolving the 1.8.8 version JSON;
//! 3. Downloading and verifying the 1.8.8 client jar;
//! 4. Downloading the asset index;
//! 5. Concurrently downloading missing asset objects with SHA-1 validation and resume support;
//! 6. Caching everything into the user's data directory.

use std::sync::Arc;

use crate::cache::AssetCache;
use crate::client_jar::ClientJar;
use crate::error::AssetsResult;
use crate::http::{Endpoints, HttpClient};
use crate::install::OfficialInstall;
use crate::manifest::VersionManifest;
use crate::objects::{AssetIndex, Downloader, FetchMode, FetchReport};
use crate::progress::Progress;

/// Target version required by Umicraft.
pub const TARGET_VERSION: &str = "1.8.8";

/// The distinct phases of the first-run asset setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupStep {
    /// Inspecting local official installation (`.minecraft`).
    CheckingLocalInstall,
    /// Inspecting the local Umicraft cache.
    CheckingCache,
    /// Fetching and parsing the official version manifest.
    ResolvingManifest,
    /// Downloading and validating the client jar.
    DownloadingClientJar,
    /// Fetching the asset index.
    FetchingAssetIndex,
    /// Concurrently downloading missing asset objects.
    DownloadingObjects,
    /// Setup completed successfully.
    Completed,
}

/// Discovers an already available, verified 1.8.8 client jar.
///
/// Checks in order:
/// 1. The official local installation in the user's home directory (if present);
/// 2. The local Umicraft data cache.
///
/// Returns `Ok(Some(jar))` if a verified jar is found, or `Ok(None)` if a first-run
/// download is needed.
pub fn find_existing_jar(cache: &AssetCache) -> Option<ClientJar> {
    if let Some(install) = OfficialInstall::from_env() {
        if let Some(jar_path) = install.client_jar_path(TARGET_VERSION) {
            let version_dir = install.versions_dir().join(TARGET_VERSION);
            let json_path = version_dir.join(format!("{TARGET_VERSION}.json"));
            if let Ok(json_bytes) = std::fs::read(&json_path) {
                if let Ok(version_json) = crate::manifest::VersionJson::parse(&json_bytes) {
                    if let Ok(descriptor) = version_json.require_client_jar() {
                        if ClientJar::open(jar_path, &descriptor.sha1, descriptor.size).is_ok() {
                            return ClientJar::open(
                                install.client_jar_path(TARGET_VERSION).unwrap_or_default(),
                                &descriptor.sha1,
                                descriptor.size,
                            )
                            .ok();
                        }
                    }
                }
            }
        }
    }

    let cache_jar = cache.client_jar_path(TARGET_VERSION);
    if cache_jar.is_file() {
        // When cached without local install descriptor, verify with the known official 1.8.8 hash
        const OFFICIAL_1_8_8_JAR_SHA1: &str = "0a279f6916551b94c3ce7d045d656093d50b4ec2";
        const OFFICIAL_1_8_8_JAR_SIZE: u64 = 8_637_730;
        if ClientJar::open(&cache_jar, OFFICIAL_1_8_8_JAR_SHA1, OFFICIAL_1_8_8_JAR_SIZE).is_ok() {
            return ClientJar::open(&cache_jar, OFFICIAL_1_8_8_JAR_SHA1, OFFICIAL_1_8_8_JAR_SIZE)
                .ok();
        }
    }

    None
}

/// Orchestrates the complete asset download flow to `cache`.
///
/// Progress updates are reported via `progress`, and stage changes are reported
/// via `on_step`.
///
/// # Errors
///
/// [`crate::AssetsError`] on network, hash, size, JSON, or filesystem failure.
pub async fn run_setup<F>(
    endpoints: Endpoints,
    cache: Arc<AssetCache>,
    concurrency: usize,
    progress: &Progress,
    mut on_step: F,
) -> AssetsResult<(ClientJar, FetchReport)>
where
    F: FnMut(SetupStep),
{
    on_step(SetupStep::CheckingCache);
    cache.ensure_dirs()?;

    let http = HttpClient::new(endpoints)?;
    let downloader = Downloader::new(http.clone(), concurrency);

    on_step(SetupStep::ResolvingManifest);
    let manifest_bytes = http.get_bytes(http.endpoints().manifest_url()).await?;
    let manifest = VersionManifest::parse(&manifest_bytes)?;
    let version = manifest.resolve(&http, TARGET_VERSION).await?;

    let jar_desc = version.require_client_jar()?;
    on_step(SetupStep::DownloadingClientJar);
    let jar = ClientJar::fetch(&cache, TARGET_VERSION, jar_desc, &downloader).await?;

    let index_desc = version.require_asset_index()?;
    on_step(SetupStep::FetchingAssetIndex);
    let index = AssetIndex::fetch(&http, index_desc).await?;

    on_step(SetupStep::DownloadingObjects);
    let report = downloader
        .fetch(&index, cache.clone(), FetchMode::Download, progress)
        .await?;

    on_step(SetupStep::Completed);
    Ok((jar, report))
}
