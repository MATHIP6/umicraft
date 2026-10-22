// SPDX-License-Identifier: MIT OR Apache-2.0

//! # umicraft-assets
//!
//! The **transport** half of the asset pipeline (spec AST-1, AST-2, AST-3):
//! it resolves a game version to the official endpoints, downloads the
//! assets, verifies every byte against the SHA-1 the official metadata
//! promises, and keeps the result in a local cache.
//!
//! ```text
//! version manifest ──▶ version json ──▶ asset index ──▶ asset objects
//!                                        └──────────▶ client jar
//! ```
//!
//! Legal boundary (spec §3): nothing this crate downloads may ever end up in
//! the repository or in a release. The crate writes **only** into the
//! platform data directory, and the CI job `assets-guard` fails the build if
//! a Mojang asset is committed. Umicraft never mirrors or re-hosts the
//! assets; the user downloads them from Mojang's official endpoints, or
//! points the client at an existing official installation, which is only ever
//! read (AST-2).
//!
//! What is **not** in this crate: the texture atlas, the 1.8 block models and
//! blockstates, the resource pack support, and the player skins. Those are
//! the visual half of the pipeline and belong to M5 to M11 (spec AST-5 to AST-9).
//!
//! # Example
//!
//! ```no_run
//! use umicraft_assets::cache::AssetCache;
//! use umicraft_assets::http::{Endpoints, HttpClient};
//! use umicraft_assets::manifest::VersionManifest;
//! use umicraft_assets::objects::{AssetIndex, Downloader, FetchMode};
//! use umicraft_assets::progress::Progress;
//! use std::sync::Arc;
//!
//! # async fn run() -> Result<(), umicraft_assets::AssetsError> {
//! let client = HttpClient::new(Endpoints::official())?;
//! let manifest = VersionManifest::parse(&client.get_bytes(client.endpoints().manifest_url()).await?)?;
//! let version = manifest.resolve(&client, "1.8.8").await?;
//!
//! let cache = AssetCache::in_data_dir()?;
//! let index = AssetIndex::fetch(&client, version.require_asset_index()?).await?;
//!
//! let progress = Progress::new();
//! let downloader = Downloader::new(client, 8);
//! let report = downloader
//!     .fetch(&index, Arc::new(cache), FetchMode::Download, &progress)
//!     .await?;
//! println!("{} objects, {} downloaded", report.objects_total, report.downloaded);
//! # Ok(())
//! # }
//! ```

pub mod cache;
pub mod client_jar;
pub mod error;
pub mod hash;
pub mod http;
pub mod install;
pub mod manifest;
pub mod objects;
pub mod progress;

mod zip;

pub use error::{AssetsError, AssetsResult};
pub use progress::{Progress, ProgressSnapshot};
