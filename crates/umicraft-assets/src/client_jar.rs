// SPDX-License-Identifier: MIT OR Apache-2.0

//! The client jar of a game version (spec AST-1, second hop).
//!
//! In 1.8 the block textures, the blockstates and the block models are **not**
//! part of the asset index: they ship inside the client jar, which the version
//! JSON points at with a URL, a SHA-1 and a size.
//!
//! This module locates that jar, verifies it against the digest the version
//! JSON promises, and exposes a minimal read of one entry out of it. The full
//! extraction and the model/atlas work belongs to M5.

use std::path::{Path, PathBuf};

use crate::cache::AssetCache;
use crate::error::{AssetsError, AssetsResult};
use crate::hash;
use crate::manifest::ClientJarDescriptor;
use crate::objects::Downloader;
use crate::zip::ZipArchive;

/// A verified client jar on disk.
#[derive(Debug, Clone)]
pub struct ClientJar {
    path: PathBuf,
    sha1: String,
    size: u64,
}

impl ClientJar {
    /// Makes the client jar of `version_id` available in `cache`.
    ///
    /// A jar that is already cached and still matches its digest is reused
    /// (resume); otherwise it is downloaded and verified before it is renamed
    /// into place.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Http`], [`AssetsError::HttpStatus`],
    /// [`AssetsError::HashMismatch`], [`AssetsError::SizeMismatch`], or
    /// [`AssetsError::Io`].
    pub async fn fetch(
        cache: &AssetCache,
        version_id: &str,
        descriptor: &ClientJarDescriptor,
        downloader: &Downloader,
    ) -> AssetsResult<Self> {
        let path = cache.client_jar_path(version_id);
        if path.is_file() && Self::verify(&path, &descriptor.sha1, descriptor.size).is_ok() {
            return Self::open(path, &descriptor.sha1, descriptor.size);
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| AssetsError::Io {
                path: parent.display().to_string(),
                source,
            })?;
        }
        downloader
            .client()
            .download_to_file(
                &descriptor.url,
                &path,
                &descriptor.sha1,
                descriptor.size,
                None,
            )
            .await?;
        Self::open(path, &descriptor.sha1, descriptor.size)
    }

    /// Opens an existing client jar and verifies its digest and its size.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Missing`] when the file is not there,
    /// [`AssetsError::Io`] when it cannot be read,
    /// [`AssetsError::HashMismatch`] or [`AssetsError::SizeMismatch`] when it
    /// does not match the descriptor.
    pub fn open(path: impl Into<PathBuf>, sha1: &str, size: u64) -> AssetsResult<Self> {
        let path = path.into();
        Self::verify(&path, sha1, size)?;
        Ok(Self {
            path,
            sha1: sha1.to_string(),
            size,
        })
    }

    /// The path of the jar.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The SHA-1 of the jar.
    #[must_use]
    pub fn sha1(&self) -> &str {
        &self.sha1
    }

    /// The size of the jar in bytes.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// The names of every entry in the jar.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Io`] when the jar cannot be read and
    /// [`AssetsError::Zip`] when it is not a readable zip container.
    pub fn entry_names(&self) -> AssetsResult<Vec<String>> {
        Ok(self.archive()?.names())
    }

    /// Reads one entry out of the jar.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Missing`] when the jar has no such entry,
    /// [`AssetsError::Io`] when the jar cannot be read, and
    /// [`AssetsError::Zip`] when the container is malformed.
    pub fn read_entry(&self, name: &str) -> AssetsResult<Vec<u8>> {
        let archive = self.archive()?;
        let entry = archive.entry(name).ok_or_else(|| AssetsError::Missing {
            what: format!("jar entry {name}"),
            path: self.path.display().to_string(),
        })?;
        archive.read(entry)
    }

    /// Parses the jar as a zip container.
    fn archive(&self) -> AssetsResult<ZipArchive> {
        let data = std::fs::read(&self.path).map_err(|source| AssetsError::Io {
            path: self.path.display().to_string(),
            source,
        })?;
        ZipArchive::open(data)
    }

    /// Verifies that the file at `path` has the expected digest and size.
    fn verify(path: &Path, sha1: &str, size: u64) -> AssetsResult<()> {
        let bytes = std::fs::read(path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                AssetsError::Missing {
                    what: format!("client jar {sha1}"),
                    path: path.display().to_string(),
                }
            } else {
                AssetsError::Io {
                    path: path.display().to_string(),
                    source,
                }
            }
        })?;
        if bytes.len() as u64 != size {
            return Err(AssetsError::SizeMismatch {
                what: sha1.to_string(),
                expected: size,
                actual: bytes.len() as u64,
            });
        }
        hash::verify_sha1(&bytes, sha1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a zip archive with one *stored* entry.
    fn stored_zip(name: &str, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let name_bytes = name.as_bytes();

        out.extend_from_slice(&0x0403_4b50_u32.to_le_bytes()); // local header
        out.extend_from_slice(&10_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes()); // stored
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u32.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(payload);

        let central_offset = out.len() as u32;
        out.extend_from_slice(&0x0201_4b50_u32.to_le_bytes()); // central header
        out.extend_from_slice(&20_u16.to_le_bytes());
        out.extend_from_slice(&10_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u32.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u32.to_le_bytes());
        out.extend_from_slice(&0_u32.to_le_bytes());
        out.extend_from_slice(name_bytes);

        let central_size = out.len() as u32 - central_offset;
        out.extend_from_slice(&0x0605_4b50_u32.to_le_bytes()); // eocd
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&1_u16.to_le_bytes());
        out.extend_from_slice(&1_u16.to_le_bytes());
        out.extend_from_slice(&central_size.to_le_bytes());
        out.extend_from_slice(&central_offset.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out
    }

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("umicraft-assets-jar-{name}-{}", std::process::id()))
    }

    #[test]
    fn an_entry_is_read_out_of_the_jar() {
        let root = temp_root("entry");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp dir is created");

        let bytes = stored_zip(
            "assets/minecraft/blockstates/stone.json",
            b"{\"variants\":{}}",
        );
        let sha1 = hash::sha1_hex(&bytes);
        let path = root.join("client.jar");
        std::fs::write(&path, &bytes).expect("jar is written");

        let jar = ClientJar::open(&path, &sha1, bytes.len() as u64).expect("jar opens");
        assert_eq!(jar.size(), bytes.len() as u64);
        assert_eq!(
            jar.read_entry("assets/minecraft/blockstates/stone.json")
                .expect("entry is read"),
            b"{\"variants\":{}}"
        );
        assert_eq!(jar.entry_names().expect("names are listed").len(), 1);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_jar_with_the_wrong_digest_is_rejected() {
        let root = temp_root("digest");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp dir is created");
        let path = root.join("client.jar");
        let payload = b"not the jar you are looking for";
        std::fs::write(&path, payload).expect("jar is written");

        let err = ClientJar::open(&path, &"0".repeat(40), payload.len() as u64)
            .expect_err("a bad digest must fail");
        assert!(matches!(err, AssetsError::HashMismatch { .. }));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_jar_with_the_wrong_size_is_rejected() {
        let root = temp_root("size");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp dir is created");
        let path = root.join("client.jar");
        std::fs::write(&path, b"short").expect("jar is written");
        let sha1 = hash::sha1_hex(b"short");

        let err = ClientJar::open(&path, &sha1, 999).expect_err("a bad size must fail");
        assert!(matches!(err, AssetsError::SizeMismatch { .. }));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_jar_is_a_typed_error() {
        let err = ClientJar::open("/tmp/umicraft-assets-not-there.jar", &"0".repeat(40), 1)
            .expect_err("a missing jar must fail");
        assert!(matches!(err, AssetsError::Missing { .. }));
    }
}
