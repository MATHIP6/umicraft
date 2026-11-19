//! The trust list: which library hash the player accepted for each mod.
//!
//! Native mods run with the client's full rights, so a library loads only if its
//! SHA-256 was accepted before. A changed hash for a known id is untrusted again, so
//! a swapped library cannot ride on an earlier approval.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, BufReader, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::TrustError;
use crate::manifest::ModId;

/// Mod id to lowercase hex SHA-256, stored as `trusted.toml`:
///
/// ```toml
/// [trusted]
/// example_hud = "ba7816bf..."
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustList {
    trusted: BTreeMap<String, String>,
}

impl TrustList {
    /// Reads the list at `path`; a missing file is an empty list (first run).
    ///
    /// # Errors
    ///
    /// [`TrustError::Io`] for a read failure other than "not found", or
    /// [`TrustError::Parse`] for malformed content.
    pub fn load(path: &Path) -> Result<Self, TrustError> {
        match fs::read_to_string(path) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Writes the list to `path` through a temporary file and a rename, so a crash
    /// while saving never leaves a truncated list.
    ///
    /// # Errors
    ///
    /// [`TrustError::Io`] or [`TrustError::Serialize`].
    pub fn save(&self, path: &Path) -> Result<(), TrustError> {
        let text = toml::to_string(self)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("toml.tmp");
        fs::write(&temp, text)?;
        fs::rename(&temp, path)?;
        Ok(())
    }

    /// Whether `sha256` (lowercase hex) is the hash recorded for `id`.
    pub fn is_trusted(&self, id: &ModId, sha256: &str) -> bool {
        self.trusted.get(id.as_str()).is_some_and(|h| h == sha256)
    }

    /// Records `sha256` for `id`, replacing any earlier hash.
    pub fn trust(&mut self, id: &ModId, sha256: &str) {
        self.trusted.insert(id.to_string(), sha256.to_owned());
    }
}

/// Streams the file at `path` through SHA-256 and returns the lowercase hex digest.
///
/// Streaming keeps memory flat for large libraries.
///
/// # Errors
///
/// The I/O error if the file cannot be opened or read.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            return Ok(to_hex(&hasher.finalize()));
        }
        hasher.update(&buffer[..n]);
    }
}

/// Lowercase hex without a dependency.
fn to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            // Writing to a String cannot fail.
            let _ = write!(out, "{b:02x}");
            out
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("umi-trust-{}-{name}", std::process::id()))
    }

    fn id(text: &str) -> ModId {
        text.to_owned().try_into().unwrap()
    }

    #[test]
    fn sha256_of_abc_matches_the_fips_vector() {
        let path = scratch("abc");
        fs::write(&path, "abc").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn missing_file_cannot_be_hashed() {
        assert!(sha256_file(&scratch("absent")).is_err());
    }

    #[test]
    fn round_trips_through_toml() {
        let path = scratch("list.toml");
        let mut list = TrustList::default();
        list.trust(&id("hud"), "aa");
        list.save(&path).unwrap();
        assert_eq!(TrustList::load(&path).unwrap(), list);
        assert!(fs::read_to_string(&path).unwrap().contains("[trusted]"));
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn missing_list_is_empty_and_garbage_is_an_error() {
        assert_eq!(
            TrustList::load(&scratch("none.toml")).unwrap(),
            TrustList::default()
        );
        let path = scratch("bad.toml");
        fs::write(&path, "trusted = 3").unwrap();
        assert!(matches!(TrustList::load(&path), Err(TrustError::Parse(_))));
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_changed_hash_is_untrusted() {
        let mut list = TrustList::default();
        list.trust(&id("hud"), "aa");
        assert!(list.is_trusted(&id("hud"), "aa"));
        assert!(!list.is_trusted(&id("hud"), "bb"));
        assert!(!list.is_trusted(&id("other"), "aa"));
    }
}
