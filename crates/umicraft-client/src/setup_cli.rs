// SPDX-License-Identifier: MIT OR Apache-2.0

//! First-run asset setup command-line runner (spec AST-1, AST-3).
//!
//! Downloads official 1.8.8 assets, validates SHA-1 digests, and caches them
//! into the Umicraft data directory with progress reporting.

use std::sync::Arc;

use umicraft_assets::cache::AssetCache;
use umicraft_assets::http::Endpoints;
use umicraft_assets::progress::Progress;
use umicraft_assets::setup::{run_setup, SetupStep};

use crate::error::ClientError;

/// Runs the asset setup from the command line, printing progress steps to standard output.
///
/// # Errors
///
/// [`ClientError`] if asset downloading, verification or caching fails.
pub fn run() -> Result<(), ClientError> {
    let cache = AssetCache::in_data_dir().map_err(|err| ClientError::Assets(err.to_string()))?;
    let cache = Arc::new(cache);
    println!("Umicraft Asset Setup");
    println!("Target cache: {}", cache.root().display());

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| ClientError::Network(format!("tokio runtime error: {err}")))?;

    let progress = Progress::new();
    let result = runtime.block_on(async {
        run_setup(
            Endpoints::official(),
            cache,
            8,
            &progress,
            |step| match step {
                SetupStep::CheckingLocalInstall => println!("[1/5] Checking local installation..."),
                SetupStep::CheckingCache => println!("[1/5] Checking asset cache..."),
                SetupStep::ResolvingManifest => {
                    println!("[2/5] Resolving official version manifest...")
                }
                SetupStep::DownloadingClientJar => {
                    println!("[3/5] Downloading and verifying 1.8.8 client jar...")
                }
                SetupStep::FetchingAssetIndex => println!("[4/5] Fetching asset index..."),
                SetupStep::DownloadingObjects => {
                    println!("[5/5] Downloading missing asset objects...")
                }
                SetupStep::Completed => println!("Asset setup complete!"),
            },
        )
        .await
    });

    let (_jar, report) = result.map_err(|err| ClientError::Assets(err.to_string()))?;
    println!(
        "Summary: {} objects total, {} downloaded, {} cached ({} MB downloaded)",
        report.objects_total,
        report.downloaded,
        report.cached,
        report.bytes_downloaded / (1024 * 1024)
    );
    Ok(())
}
