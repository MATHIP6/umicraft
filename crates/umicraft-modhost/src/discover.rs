// SPDX-License-Identifier: MIT OR Apache-2.0

//! Finding installed mods: every sub-directory of the mods folder with a `mod.toml`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::DiscoverError;
use crate::manifest::Manifest;

/// File name of a mod's manifest.
const MANIFEST_FILE: &str = "mod.toml";

/// One installed mod found on disk.
#[derive(Debug)]
pub struct Discovered {
    /// The mod's directory.
    pub dir: PathBuf,
    /// Its manifest, or why it is unusable (shown to the player, never fatal).
    pub manifest: Result<Manifest, DiscoverError>,
}

/// Lists the mods under `mods_dir`, sorted by directory name for a deterministic order.
///
/// Plain files and directories without a `mod.toml` are skipped; a manifest that
/// cannot be read or parsed, or a folder not named after its mod id, yields an `Err` entry so the problem can be shown. A
/// missing or unreadable `mods_dir` yields no mods (a fresh install has none).
pub fn discover(mods_dir: &Path) -> Vec<Discovered> {
    let Ok(entries) = fs::read_dir(mods_dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join(MANIFEST_FILE).is_file())
        .collect();
    dirs.sort();
    dirs.into_iter()
        .map(|dir| Discovered {
            manifest: read_manifest(&dir),
            dir,
        })
        .collect()
}

fn read_manifest(dir: &Path) -> Result<Manifest, DiscoverError> {
    let text = fs::read_to_string(dir.join(MANIFEST_FILE))?;
    let manifest = Manifest::parse(&text)?;
    let folder = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    match folder {
        Some(folder) if folder == manifest.id.as_str() => Ok(manifest),
        folder => Err(DiscoverError::FolderName {
            folder: folder.unwrap_or_default(),
            id: manifest.id.to_string(),
        }),
    }
}
