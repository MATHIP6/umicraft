// SPDX-License-Identifier: MIT OR Apache-2.0

//! Reading assets from an existing official installation (spec AST-2).
//!
//! A player who already owns the game can point Umicraft at their
//! `.minecraft` directory instead of downloading everything a second time.
//! Umicraft **only reads** that directory: nothing is copied into the
//! Umicraft cache and nothing is written back into the installation.
//!
//! The layout of an official installation is the same as the one the
//! [`crate::cache::AssetCache`] uses:
//!
//! ```text
//! <install>/assets/objects/<hash[0..2]>/<hash>
//! <install>/assets/indexes/<sha1>.json
//! <install>/versions/<id>/<id>.jar
//! ```

use std::path::{Path, PathBuf};

use crate::error::{AssetsError, AssetsResult};
use crate::hash;
use crate::objects::ObjectStore;

/// An existing official installation (a `.minecraft` directory).
#[derive(Debug, Clone)]
pub struct OfficialInstall {
    root: PathBuf,
}

impl OfficialInstall {
    /// An installation rooted at `root`.
    ///
    /// # Examples
    ///
    /// ```
    /// use umicraft_assets::install::OfficialInstall;
    ///
    /// let install = OfficialInstall::new("/home/player/.minecraft");
    /// assert!(install.root().ends_with(".minecraft"));
    /// ```
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The installation in the user's home directory, if it exists.
    ///
    /// On Unix that is `$HOME/.minecraft`, on Windows `%APPDATA%\.minecraft`.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let home = if cfg!(windows) {
            std::env::var_os("APPDATA")?
        } else {
            std::env::var_os("HOME")?
        };
        let install = Self::new(PathBuf::from(home).join(".minecraft"));
        install.root.is_dir().then_some(install)
    }

    /// The root directory of the installation.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The `assets` directory of the installation.
    #[must_use]
    pub fn assets_dir(&self) -> PathBuf {
        self.root.join("assets")
    }

    /// The directory holding the asset objects.
    #[must_use]
    pub fn objects_dir(&self) -> PathBuf {
        self.assets_dir().join("objects")
    }

    /// The directory holding the asset index documents.
    #[must_use]
    pub fn indexes_dir(&self) -> PathBuf {
        self.assets_dir().join("indexes")
    }

    /// The directory holding the installed versions.
    #[must_use]
    pub fn versions_dir(&self) -> PathBuf {
        self.root.join("versions")
    }

    /// The path of the asset object with the given SHA-1, when it is present.
    #[must_use]
    pub fn object_path(&self, hash: &str) -> Option<PathBuf> {
        let prefix = hash.get(..2).unwrap_or(hash);
        let path = self.objects_dir().join(prefix).join(hash);
        path.is_file().then_some(path)
    }

    /// The path of the asset index document with the given SHA-1, when it is
    /// present.
    #[must_use]
    pub fn index_path(&self, sha1: &str) -> Option<PathBuf> {
        let path = self.indexes_dir().join(format!("{sha1}.json"));
        path.is_file().then_some(path)
    }

    /// The path of the client jar of `version_id`, when it is present.
    #[must_use]
    pub fn client_jar_path(&self, version_id: &str) -> Option<PathBuf> {
        let path = self
            .versions_dir()
            .join(version_id)
            .join(format!("{version_id}.jar"));
        path.is_file().then_some(path)
    }

    /// Reads an asset index document from the installation.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Missing`] when the document is not there,
    /// [`AssetsError::Io`] when it cannot be read, and
    /// [`AssetsError::HashMismatch`] when it is corrupted.
    pub fn read_index(&self, sha1: &str) -> AssetsResult<Vec<u8>> {
        let path = self.index_path(sha1).ok_or_else(|| AssetsError::Missing {
            what: format!("asset index {sha1}"),
            path: self
                .indexes_dir()
                .join(format!("{sha1}.json"))
                .display()
                .to_string(),
        })?;
        let bytes = std::fs::read(&path).map_err(|source| AssetsError::Io {
            path: path.display().to_string(),
            source,
        })?;
        hash::verify_sha1(&bytes, sha1)?;
        Ok(bytes)
    }
}

impl ObjectStore for OfficialInstall {
    fn object_path(&self, hash: &str) -> Option<PathBuf> {
        OfficialInstall::object_path(self, hash)
    }

    fn read_object(&self, hash: &str) -> AssetsResult<Vec<u8>> {
        let path =
            OfficialInstall::object_path(self, hash).ok_or_else(|| AssetsError::Missing {
                what: format!("asset object {hash}"),
                path: self.objects_dir().join(hash).display().to_string(),
            })?;
        let bytes = std::fs::read(&path).map_err(|source| AssetsError::Io {
            path: path.display().to_string(),
            source,
        })?;
        hash::verify_sha1(&bytes, hash)?;
        Ok(bytes)
    }

    fn store_object(&self, _hash: &str, _bytes: &[u8]) -> AssetsResult<PathBuf> {
        // AST-2: an existing installation is only ever read.
        Err(AssetsError::ReadOnly(self.root.display().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "umicraft-assets-install-{name}-{}",
            std::process::id()
        ))
    }

    /// Builds a fake `.minecraft` directory with one object and one index.
    fn fake_install(name: &str) -> (OfficialInstall, String, Vec<u8>) {
        let root = install_root(name);
        let _ = std::fs::remove_dir_all(&root);
        let install = OfficialInstall::new(&root);
        let bytes = b"sound data".to_vec();
        let hash = hash::sha1_hex(&bytes);
        let object_dir = install.objects_dir().join(&hash[..2]);
        std::fs::create_dir_all(&object_dir).expect("object dir is created");
        std::fs::write(object_dir.join(&hash), &bytes).expect("object is written");
        (install, hash, bytes)
    }

    #[test]
    fn an_object_is_found_without_being_copied() {
        let (install, hash, bytes) = fake_install("object");
        let path = install.object_path(&hash).expect("object is present");
        assert!(path.is_file());

        let read = install.read_object(&hash).expect("object is readable");
        assert_eq!(read, bytes);

        // Nothing was written into the installation besides the fixture.
        let files = count_files(install.root());
        assert_eq!(files, 1);

        let _ = std::fs::remove_dir_all(install.root());
    }

    #[test]
    fn a_missing_object_is_a_typed_error() {
        let (install, _hash, _bytes) = fake_install("missing");
        let err = install
            .read_object("0000000000000000000000000000000000000000")
            .expect_err("a missing object must fail");
        assert!(matches!(err, AssetsError::Missing { .. }));
        let _ = std::fs::remove_dir_all(install.root());
    }

    #[test]
    fn the_installation_is_never_written_to() {
        let (install, _hash, _bytes) = fake_install("readonly");
        let err = install
            .store_object("0000000000000000000000000000000000000000", b"x")
            .expect_err("the installation must stay read-only");
        assert!(matches!(err, AssetsError::ReadOnly(_)));
        let _ = std::fs::remove_dir_all(install.root());
    }

    /// Counts the files below `root`.
    fn count_files(root: &Path) -> usize {
        let Ok(entries) = std::fs::read_dir(root) else {
            return 0;
        };
        let mut count = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_files(&path);
            } else {
                count += 1;
            }
        }
        count
    }
}
