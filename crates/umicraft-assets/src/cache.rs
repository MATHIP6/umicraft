// SPDX-License-Identifier: MIT OR Apache-2.0

//! The on-disk asset cache (spec AST-3).
//!
//! The cache lives in the platform data directory — **never inside the
//! repository**: Mojang assets may not be committed (spec §3 rule 3), so the
//! only place the transport layer is allowed to write them is the user's data
//! directory (spec §9).
//!
//! The layout mirrors the official launcher so the paths stay familiar:
//!
//! ```text
//! <data>/assets/objects/<hash[0..2]>/<hash>   the asset objects
//! <data>/assets/indexes/<sha1>.json           the asset index documents
//! <data>/assets/versions/<id>/<id>.jar        the client jars
//! ```
//!
//! Every object that is read back is re-validated by its SHA-1 (AST-3), so a
//! truncated or tampered cache entry is detected instead of being used.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{AssetsError, AssetsResult};
use crate::hash;
use crate::objects::ObjectStore;

/// The on-disk cache of downloaded assets.
#[derive(Debug, Clone)]
pub struct AssetCache {
    root: PathBuf,
}

impl AssetCache {
    /// A cache rooted at `root`.
    ///
    /// The root is created lazily on the first write.
    ///
    /// # Examples
    ///
    /// ```
    /// use umicraft_assets::cache::AssetCache;
    ///
    /// let cache = AssetCache::new("/tmp/umicraft-assets");
    /// assert!(cache.root().ends_with("umicraft-assets"));
    /// ```
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The cache inside the platform data directory.
    ///
    /// # Where the data directory comes from
    ///
    /// Spec §9 asks for the platform data directory "via `directories`
    /// crate", but this crate resolves the three well-known locations
    /// itself instead of taking the dependency:
    ///
    /// * the `directories` crate pulls in `dirs-sys`, which depends on
    ///   `option-ext` — an **MPL-2.0** crate, and the license allow list of
    ///   `deny.toml` (spec §3 rule 7) does not accept MPL-2.0;
    /// * the three locations are a handful of lines and stable for decades
    ///   (XDG base directory specification, macOS `Application Support`,
    ///   Windows roaming `APPDATA`).
    ///
    /// The result is the same layout the `directories` crate produces for an
    /// application named `umicraft`: `~/.local/share/umicraft` (Linux),
    /// `~/Library/Application Support/umicraft` (macOS) and
    /// `%APPDATA%\umicraft` (Windows).
    ///
    /// # Errors
    ///
    /// [`AssetsError::NoDataDir`] when the platform data directory cannot be
    /// determined.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use umicraft_assets::cache::AssetCache;
    ///
    /// let cache = AssetCache::in_data_dir()?;
    /// println!("assets live in {}", cache.root().display());
    /// # Ok::<(), umicraft_assets::AssetsError>(())
    /// ```
    pub fn in_data_dir() -> AssetsResult<Self> {
        let base = platform_data_dir().ok_or(AssetsError::NoDataDir)?;
        Ok(Self::new(base.join("umicraft").join("assets")))
    }

    /// The root directory of the cache.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The directory holding the asset objects.
    #[must_use]
    pub fn objects_dir(&self) -> PathBuf {
        self.root.join("objects")
    }

    /// The directory holding the asset index documents.
    #[must_use]
    pub fn indexes_dir(&self) -> PathBuf {
        self.root.join("indexes")
    }

    /// The directory holding the downloaded client jars.
    #[must_use]
    pub fn versions_dir(&self) -> PathBuf {
        self.root.join("versions")
    }

    /// The path an asset object with the given SHA-1 is stored at.
    ///
    /// The first two hex characters of the hash select the sub-directory,
    /// exactly like the official object store.
    ///
    /// # Examples
    ///
    /// ```
    /// use umicraft_assets::cache::AssetCache;
    ///
    /// let cache = AssetCache::new("/tmp/umicraft-assets");
    /// let path = cache.object_path("f6ad102bcaa53b1a58358f16e376d548d44933ec");
    /// assert!(path.ends_with("objects/f6/f6ad102bcaa53b1a58358f16e376d548d44933ec"));
    /// ```
    #[must_use]
    pub fn object_path(&self, hash: &str) -> PathBuf {
        let prefix = hash.get(..2).unwrap_or(hash);
        self.objects_dir().join(prefix).join(hash)
    }

    /// The path an asset index document with the given SHA-1 is stored at.
    #[must_use]
    pub fn index_path(&self, sha1: &str) -> PathBuf {
        self.indexes_dir().join(format!("{sha1}.json"))
    }

    /// The path the client jar of `version_id` is stored at.
    #[must_use]
    pub fn client_jar_path(&self, version_id: &str) -> PathBuf {
        self.versions_dir()
            .join(version_id)
            .join(format!("{version_id}.jar"))
    }

    /// Whether the object with the given SHA-1 is present in the cache.
    #[must_use]
    pub fn has_object(&self, hash: &str) -> bool {
        self.object_path(hash).is_file()
    }

    /// Creates the cache directories if they do not exist yet.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Io`] when a directory cannot be created.
    pub fn ensure_dirs(&self) -> AssetsResult<()> {
        for dir in [self.objects_dir(), self.indexes_dir(), self.versions_dir()] {
            std::fs::create_dir_all(&dir).map_err(|source| AssetsError::Io {
                path: dir.display().to_string(),
                source,
            })?;
        }
        Ok(())
    }

    /// Stores an asset object under its SHA-1 and returns the path.
    ///
    /// The bytes are written to a temporary sibling file and renamed into
    /// place, so an interrupted write never leaves a truncated object behind.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Io`] when the file cannot be written.
    pub fn store_object(&self, hash: &str, bytes: &[u8]) -> AssetsResult<PathBuf> {
        let path = self.object_path(hash);
        write_atomic(&path, bytes)?;
        Ok(path)
    }

    /// Reads an asset object and re-validates it by its SHA-1 (AST-3).
    ///
    /// # Errors
    ///
    /// [`AssetsError::Missing`] when the object is not cached,
    /// [`AssetsError::Io`] when it cannot be read, and
    /// [`AssetsError::HashMismatch`] when the cached bytes are corrupted.
    pub fn read_object(&self, hash: &str) -> AssetsResult<Vec<u8>> {
        let path = self.object_path(hash);
        let bytes = std::fs::read(&path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                AssetsError::Missing {
                    what: format!("asset object {hash}"),
                    path: path.display().to_string(),
                }
            } else {
                AssetsError::Io {
                    path: path.display().to_string(),
                    source,
                }
            }
        })?;
        hash::verify_sha1(&bytes, hash)?;
        Ok(bytes)
    }

    /// Stores an asset index document under its SHA-1 and returns the path.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Io`] when the file cannot be written.
    pub fn store_index(&self, sha1: &str, bytes: &[u8]) -> AssetsResult<PathBuf> {
        let path = self.index_path(sha1);
        write_atomic(&path, bytes)?;
        Ok(path)
    }

    /// Reads a cached asset index document and re-validates it by its SHA-1.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Missing`], [`AssetsError::Io`], or
    /// [`AssetsError::HashMismatch`].
    pub fn read_index(&self, sha1: &str) -> AssetsResult<Vec<u8>> {
        let path = self.index_path(sha1);
        let bytes = std::fs::read(&path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                AssetsError::Missing {
                    what: format!("asset index {sha1}"),
                    path: path.display().to_string(),
                }
            } else {
                AssetsError::Io {
                    path: path.display().to_string(),
                    source,
                }
            }
        })?;
        hash::verify_sha1(&bytes, sha1)?;
        Ok(bytes)
    }
}

impl ObjectStore for AssetCache {
    fn object_path(&self, hash: &str) -> Option<PathBuf> {
        let path = AssetCache::object_path(self, hash);
        path.is_file().then_some(path)
    }

    fn read_object(&self, hash: &str) -> AssetsResult<Vec<u8>> {
        AssetCache::read_object(self, hash)
    }

    fn store_object(&self, hash: &str, bytes: &[u8]) -> AssetsResult<PathBuf> {
        AssetCache::store_object(self, hash, bytes)
    }
}

/// The `umicraft` folder of the platform data directory: where the options
/// file lives next to the asset cache (`%APPDATA%\umicraft` on Windows).
///
/// `None` when the platform data directory cannot be determined.
#[must_use]
pub fn data_dir() -> Option<PathBuf> {
    platform_data_dir().map(|base| base.join("umicraft"))
}

/// The platform data directory, without the application folder.
///
/// * Linux and the other Unix systems: `$XDG_DATA_HOME`, falling back to
///   `~/.local/share` (XDG base directory specification);
/// * macOS: `~/Library/Application Support`;
/// * Windows: `%APPDATA%` (the roaming application data folder).
fn platform_data_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        return std::env::var_os("APPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
    }

    let home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    if cfg!(target_os = "macos") {
        return home.map(|home| home.join("Library").join("Application Support"));
    }

    // The XDG specification says a relative `XDG_DATA_HOME` must be ignored.
    match std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        Some(dir) if dir.is_absolute() => Some(dir),
        _ => home.map(|home| home.join(".local").join("share")),
    }
}

/// Writes `bytes` to `path` through a temporary sibling file.
///
/// The temporary name is unique per call, so two workers that store the
/// same object at the same time cannot clobber each other's partial file.
fn write_atomic(path: &Path, bytes: &[u8]) -> AssetsResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| AssetsError::Io {
            path: parent.display().to_string(),
            source,
        })?;
    }
    let temp = temp_sibling(path);

    std::fs::write(&temp, bytes).map_err(|source| AssetsError::Io {
        path: temp.display().to_string(),
        source,
    })?;
    if let Err(err) = std::fs::rename(&temp, path) {
        // Do not leave the temporary file behind when the rename fails.
        let _ = std::fs::remove_file(&temp);
        return Err(AssetsError::Io {
            path: path.display().to_string(),
            source: err,
        });
    }
    Ok(())
}

/// A unique temporary sibling path of `path`.
fn temp_sibling(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".part{serial}"));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "umicraft-assets-cache-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn object_paths_follow_the_official_layout() {
        let cache = AssetCache::new("/tmp/root");
        let path = cache.object_path("0983f08be6a4e624f5d85689d1aca869ed99c738");
        assert_eq!(
            path,
            PathBuf::from("/tmp/root/objects/09/0983f08be6a4e624f5d85689d1aca869ed99c738")
        );
    }

    #[test]
    fn a_stored_object_is_read_back_and_verified() {
        let root = temp_root("roundtrip");
        let _ = std::fs::remove_dir_all(&root);
        let cache = AssetCache::new(&root);
        let bytes = b"an asset object";
        let hash = hash::sha1_hex(bytes);

        let path = cache.store_object(&hash, bytes).expect("object is stored");
        assert!(path.is_file());
        assert!(cache.has_object(&hash));

        let read = cache.read_object(&hash).expect("object is read back");
        assert_eq!(read, bytes);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_corrupted_cache_entry_is_detected() {
        let root = temp_root("corrupt");
        let _ = std::fs::remove_dir_all(&root);
        let cache = AssetCache::new(&root);
        let hash = hash::sha1_hex(b"original");
        cache
            .store_object(&hash, b"original")
            .expect("object is stored");

        // Simulate a corrupted cache entry.
        std::fs::write(cache.object_path(&hash), b"tampered").expect("cache entry is corrupted");

        let err = cache
            .read_object(&hash)
            .expect_err("corruption must be detected");
        assert!(matches!(err, AssetsError::HashMismatch { .. }));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_object_is_a_typed_error() {
        let cache = AssetCache::new("/tmp/umicraft-assets-missing");
        let err = cache
            .read_object("0000000000000000000000000000000000000000")
            .expect_err("a missing object must fail");
        assert!(matches!(err, AssetsError::Missing { .. }));
    }

    #[test]
    fn the_platform_data_directory_is_resolved() {
        // The test environment always has a home directory, so the data
        // directory can be resolved; only the layout is checked here.
        let Some(base) = platform_data_dir() else {
            return;
        };
        assert!(base.is_absolute());
        if cfg!(target_os = "macos") {
            assert!(base.ends_with("Library/Application Support"));
        } else if cfg!(target_os = "windows") {
            assert!(base.is_absolute());
        }
    }
}
