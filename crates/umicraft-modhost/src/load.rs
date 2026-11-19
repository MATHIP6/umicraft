// SPDX-License-Identifier: MIT OR Apache-2.0

//! The checks that run before a mod's library is opened.

use std::path::{Path, PathBuf};

use crate::error::LoadError;
use crate::manifest::{library_file_name, platform_dir, Arch, Manifest, Os};
use crate::trust::{sha256_file, TrustList};

/// `mods_dir/<id>/bin/<os>-<arch>/<library file>` for this platform.
///
/// # Errors
///
/// [`LoadError::UnsupportedPlatform`] if the OS or CPU is not one mods target, or
/// [`LoadError::LibraryMissing`] if the file does not exist.
pub fn locate_library(manifest: &Manifest, mods_dir: &Path) -> Result<PathBuf, LoadError> {
    let (Some(os), Some(arch)) = (Os::current(), Arch::current()) else {
        return Err(LoadError::UnsupportedPlatform);
    };
    let path = mods_dir
        .join(manifest.id.as_str())
        .join("bin")
        .join(platform_dir(os, arch))
        .join(library_file_name(&manifest.library, os));
    if path.is_file() {
        Ok(path)
    } else {
        Err(LoadError::LibraryMissing { path })
    }
}

/// Accepts the library at `path` if its hash is trusted, or if `confirm` accepts it
/// (the hash is then added to `trust`).
///
/// The file is hashed and later opened by path, so it could be swapped in between;
/// closing that window needs the library opened from the hashed descriptor, which
/// is not portable. The mods folder is as trusted as the client's own binaries.
///
/// # Errors
///
/// [`LoadError::Hash`] if the file cannot be read, [`LoadError::Untrusted`] if the
/// hash is unknown and refused.
pub fn authorize(
    manifest: &Manifest,
    path: &Path,
    trust: &mut TrustList,
    confirm: &mut dyn FnMut(&Manifest, &str) -> bool,
) -> Result<(), LoadError> {
    let sha256 = sha256_file(path).map_err(LoadError::Hash)?;
    if trust.is_trusted(&manifest.id, &sha256) {
        return Ok(());
    }
    if confirm(manifest, &sha256) {
        trust.trust(&manifest.id, &sha256);
        Ok(())
    } else {
        Err(LoadError::Untrusted { sha256 })
    }
}
