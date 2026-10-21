// SPDX-License-Identifier: MIT OR Apache-2.0

//! The asset index and the object store (spec AST-1/AST-3).
//!
//! An **asset index** is a JSON document that maps a logical asset path
//! (`minecraft/sounds/step/grass1.ogg`) to the SHA-1 and the size of the
//! object that carries it. The objects themselves live on Mojang's resource
//! host at `<base>/<hash[0..2]>/<hash>`.
//!
//! The 1.8 asset index holds 734 objects (sounds, language files, launcher
//! icons). The block textures, blockstates and models are *not* in the index:
//! in 1.8 they ship inside the client jar (spec AST-1, implementation note).
//!
//! [`Downloader`] fetches the objects that are missing from an
//! [`ObjectStore`], verifies the SHA-1 and the size of every byte it
//! receives, and skips the objects that are already present — which is what
//! makes an interrupted first-run setup resumable.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use serde::Deserialize;
use tokio::task::JoinSet;

use crate::error::{AssetsError, AssetsResult};
use crate::hash;
use crate::http::HttpClient;
use crate::manifest::AssetIndexDescriptor;
use crate::progress::Progress;

/// Builds the URL of the asset object with the given SHA-1.
///
/// The official object store groups the objects by the first two hex
/// characters of their hash.
///
/// # Examples
///
/// ```
/// use umicraft_assets::objects::object_url;
///
/// let url = object_url("https://resources.download.minecraft.net", "f6ad102bcaa53b1a58358f16e376d548d44933ec");
/// assert_eq!(
///     url,
///     "https://resources.download.minecraft.net/f6/f6ad102bcaa53b1a58358f16e376d548d44933ec"
/// );
/// ```
#[must_use]
pub fn object_url(base: &str, hash: &str) -> String {
    let base = base.trim_end_matches('/');
    let prefix = hash.get(..2).unwrap_or(hash);
    format!("{base}/{prefix}/{hash}")
}

/// One entry of an asset index.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AssetObject {
    /// The SHA-1 of the object.
    pub hash: String,
    /// The size of the object in bytes.
    pub size: u64,
}

/// The parsed asset index.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AssetIndex {
    /// The objects, keyed by their logical asset path.
    ///
    /// The field is deliberately **not** optional: an asset index without an
    /// `objects` member is a document this crate does not understand, and
    /// silently treating it as an empty index would look like a successful
    /// download of zero assets.
    objects: BTreeMap<String, AssetObject>,
}

impl AssetIndex {
    /// Parses an asset index document.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Json`] when the document is not valid JSON, and
    /// [`AssetsError::InvalidHash`] when an object carries a hash that is not
    /// a 40 character hex SHA-1.
    pub fn parse(bytes: &[u8]) -> AssetsResult<Self> {
        let index: AssetIndex =
            serde_json::from_slice(bytes).map_err(|source| AssetsError::Json {
                what: "asset index".to_string(),
                source,
            })?;
        for (path, object) in &index.objects {
            if !is_sha1_hex(&object.hash) {
                return Err(AssetsError::InvalidHash {
                    path: path.clone(),
                    hash: object.hash.clone(),
                });
            }
        }
        Ok(index)
    }

    /// Downloads the asset index described by `descriptor` and parses it.
    ///
    /// The digest and the size the version JSON promises are verified before
    /// the document is parsed.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Http`], [`AssetsError::HttpStatus`],
    /// [`AssetsError::HashMismatch`], [`AssetsError::SizeMismatch`], or
    /// [`AssetsError::Json`].
    pub async fn fetch(
        client: &HttpClient,
        descriptor: &AssetIndexDescriptor,
    ) -> AssetsResult<Self> {
        let bytes = client.get_bytes(&descriptor.url).await?;
        if bytes.len() as u64 != descriptor.size {
            return Err(AssetsError::SizeMismatch {
                what: descriptor.sha1.clone(),
                expected: descriptor.size,
                actual: bytes.len() as u64,
            });
        }
        hash::verify_sha1(&bytes, &descriptor.sha1)?;
        Self::parse(&bytes)
    }

    /// The descriptor of the object at `path`, if the index has one.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&AssetObject> {
        self.objects.get(path)
    }

    /// Every object of the index, ordered by path.
    #[must_use]
    pub fn entries(&self) -> Vec<(String, AssetObject)> {
        self.objects
            .iter()
            .map(|(path, object)| (path.clone(), object.clone()))
            .collect()
    }

    /// The number of objects in the index.
    #[must_use]
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Whether the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// The combined size of every object in the index.
    #[must_use]
    pub fn total_size(&self) -> u64 {
        self.objects.values().map(|object| object.size).sum()
    }
}

/// Whether `hash` looks like a SHA-1 digest (40 lowercase hex characters).
fn is_sha1_hex(hash: &str) -> bool {
    hash.len() == 40 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// A place asset objects live.
///
/// The two implementations are the Umicraft cache
/// ([`crate::cache::AssetCache`], read/write) and an existing official
/// installation ([`crate::install::OfficialInstall`], read-only).
pub trait ObjectStore: Send + Sync {
    /// The local path of the object with the given SHA-1, when it is present.
    fn object_path(&self, hash: &str) -> Option<PathBuf>;

    /// Reads the object and re-validates it by its SHA-1.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Missing`] when the object is not there, and
    /// [`AssetsError::HashMismatch`] when the stored bytes are corrupted.
    fn read_object(&self, hash: &str) -> AssetsResult<Vec<u8>>;

    /// Stores the object under its SHA-1 and returns the path it was written
    /// to.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Io`] when the object cannot be written, and
    /// [`AssetsError::ReadOnly`] for stores that are read-only by design.
    fn store_object(&self, hash: &str, bytes: &[u8]) -> AssetsResult<PathBuf>;
}

/// What the downloader does with an object that is missing from the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchMode {
    /// Download the object and store it in the store (cache mode).
    Download,
    /// Report the object as missing; nothing is written anywhere.
    ///
    /// Used when the store is an existing official installation, which
    /// Umicraft only ever reads (spec AST-2).
    ReadOnly,
}

/// The result of a fetch run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FetchReport {
    /// The number of objects the index holds.
    pub objects_total: usize,
    /// The number of objects that were downloaded.
    pub downloaded: usize,
    /// The number of objects that were already present and valid.
    pub cached: usize,
    /// The number of bytes that were transferred.
    pub bytes_downloaded: u64,
}

/// Downloads asset objects and verifies them.
#[derive(Debug, Clone)]
pub struct Downloader {
    client: HttpClient,
    concurrency: usize,
}

impl Downloader {
    /// A downloader that talks to the endpoints of `client`.
    ///
    /// `concurrency` is the number of objects that are transferred at the
    /// same time; it is clamped to at least one.
    #[must_use]
    pub fn new(client: HttpClient, concurrency: usize) -> Self {
        Self {
            client,
            concurrency: concurrency.max(1),
        }
    }

    /// The HTTP client used for the transfers.
    #[must_use]
    pub fn client(&self) -> &HttpClient {
        &self.client
    }

    /// Downloads a single object and verifies its SHA-1 and its size.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Http`], [`AssetsError::HttpStatus`],
    /// [`AssetsError::HashMismatch`], or [`AssetsError::SizeMismatch`].
    pub async fn download_object(&self, object: &AssetObject) -> AssetsResult<Vec<u8>> {
        let url = self.client.endpoints().object_url(&object.hash);
        let bytes = self.client.get_bytes(&url).await?;
        if bytes.len() as u64 != object.size {
            return Err(AssetsError::SizeMismatch {
                what: object.hash.clone(),
                expected: object.size,
                actual: bytes.len() as u64,
            });
        }
        hash::verify_sha1(&bytes, &object.hash)?;
        Ok(bytes)
    }

    /// Makes sure every object of `index` is present in `store`.
    ///
    /// Objects that are already present are re-validated by their SHA-1 and
    /// skipped, which is what makes a failed first-run setup resumable
    /// (spec AST-1). Missing objects are downloaded and verified before they
    /// are written, so a corrupted transfer never reaches the cache.
    ///
    /// The transfer runs with a bounded number of workers; the queue of
    /// pending objects is the index itself, so no unbounded queue is built.
    ///
    /// The progress counters cover the whole index: an object that was
    /// already cached counts as done without transferring any byte.
    ///
    /// # Errors
    ///
    /// The first error of the run aborts it: a transport error, a hash or
    /// size mismatch, or — in [`FetchMode::ReadOnly`] — a missing object.
    pub async fn fetch(
        &self,
        index: &AssetIndex,
        store: Arc<dyn ObjectStore>,
        mode: FetchMode,
        progress: &Progress,
    ) -> AssetsResult<FetchReport> {
        let entries = Arc::new(index.entries());
        progress.set_objects_total(entries.len() as u64);
        progress.set_bytes_total(index.total_size());

        let next = Arc::new(AtomicUsize::new(0));
        let abort = Arc::new(AtomicBool::new(false));
        let workers = self.concurrency.min(entries.len().max(1));

        let mut tasks: JoinSet<AssetsResult<WorkerOutcome>> = JoinSet::new();
        for _ in 0..workers {
            let entries = Arc::clone(&entries);
            let next = Arc::clone(&next);
            let abort = Arc::clone(&abort);
            let store = Arc::clone(&store);
            let downloader = self.clone();
            let progress = progress.clone();
            tasks.spawn(async move {
                worker(
                    &downloader,
                    &entries,
                    &next,
                    &abort,
                    store.as_ref(),
                    mode,
                    &progress,
                )
                .await
            });
        }

        let mut report = FetchReport {
            objects_total: entries.len(),
            ..FetchReport::default()
        };
        while let Some(joined) = tasks.join_next().await {
            let outcome = joined.map_err(|err| AssetsError::TaskFailed(err.to_string()))??;
            report.downloaded += outcome.downloaded;
            report.cached += outcome.cached;
            report.bytes_downloaded += outcome.bytes;
        }
        progress.set_current(None);
        Ok(report)
    }
}

/// What one download worker did.
#[derive(Debug, Default, Clone, Copy)]
struct WorkerOutcome {
    /// Objects that were transferred.
    downloaded: usize,
    /// Objects that were already present and valid.
    cached: usize,
    /// Bytes that were transferred.
    bytes: u64,
}

/// The body of one download worker.
async fn worker(
    downloader: &Downloader,
    entries: &[(String, AssetObject)],
    next: &AtomicUsize,
    abort: &AtomicBool,
    store: &dyn ObjectStore,
    mode: FetchMode,
    progress: &Progress,
) -> AssetsResult<WorkerOutcome> {
    let mut outcome = WorkerOutcome::default();

    loop {
        if abort.load(Ordering::Relaxed) {
            break;
        }
        let index = next.fetch_add(1, Ordering::Relaxed);
        let Some((path, object)) = entries.get(index) else {
            break;
        };

        // Resume: an object that is already stored is re-validated and
        // skipped. A corrupted entry is downloaded again.
        if store.object_path(&object.hash).is_some() {
            if store.read_object(&object.hash).is_ok() {
                outcome.cached += 1;
                progress.object_done();
                continue;
            }
            tracing::warn!(hash = %object.hash, "cached asset object is invalid, refetching");
        } else if mode == FetchMode::ReadOnly {
            // The store is read-only, so a missing object is an error and
            // nothing is transferred (spec AST-2).
            abort.store(true, Ordering::Relaxed);
            return Err(AssetsError::Missing {
                what: format!("asset object {path}"),
                path: object.hash.clone(),
            });
        }

        progress.set_current(Some(path.clone()));
        let payload = downloader.download_object(object).await?;
        progress.add_bytes(payload.len() as u64);

        match mode {
            FetchMode::Download => {
                store.store_object(&object.hash, &payload)?;
                outcome.downloaded += 1;
                outcome.bytes += payload.len() as u64;
            }
            FetchMode::ReadOnly => {
                abort.store(true, Ordering::Relaxed);
                return Err(AssetsError::Missing {
                    what: format!("asset object {path}"),
                    path: object.hash.clone(),
                });
            }
        }
        progress.object_done();
    }

    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = r#"{
        "objects": {
            "minecraft/lang/en_US.lang": {
                "hash": "b8b16a5b1a0c1a3d1e0f5c9b7d6e4a3f2c1b0a99",
                "size": 1234
            },
            "minecraft/sounds/step/grass1.ogg": {
                "hash": "92750c5f93c312ba9ab413d546f32190c56d6f1f",
                "size": 5362
            }
        }
    }"#;

    #[test]
    fn an_asset_index_is_parsed() {
        let index = AssetIndex::parse(INDEX.as_bytes()).expect("index parses");
        assert_eq!(index.len(), 2);
        assert!(!index.is_empty());
        assert_eq!(index.total_size(), 1234 + 5362);
        let object = index
            .get("minecraft/lang/en_US.lang")
            .expect("object present");
        assert_eq!(object.size, 1234);
        assert!(index.get("minecraft/missing").is_none());
    }

    #[test]
    fn a_broken_asset_index_is_a_typed_error() {
        let err = AssetIndex::parse(b"[]").expect_err("a list is not an index");
        assert!(matches!(err, AssetsError::Json { .. }));
    }

    #[test]
    fn an_object_with_a_malformed_hash_is_rejected() {
        let json = r#"{"objects":{"a/b":{"hash":"not-a-hash","size":1}}}"#;
        let err = AssetIndex::parse(json.as_bytes()).expect_err("a bad hash must be rejected");
        assert!(matches!(err, AssetsError::InvalidHash { .. }));
    }

    #[test]
    fn object_urls_use_the_hash_prefix() {
        assert_eq!(
            object_url("https://launcher.mojang.com/v1/objects/", "abcdef"),
            "https://launcher.mojang.com/v1/objects/ab/abcdef"
        );
    }
}
