// SPDX-License-Identifier: MIT OR Apache-2.0

//! Listing the resource packs of the `resourcepacks` folder, for the
//! resource packs screen.
//!
//! This is the counterpart of `ResourcePackRepository.updateRepositoryEntriesAll`:
//! a pack is a `.zip` file or a directory that has a `pack.mcmeta` (or, for a
//! pack that lost it, an `assets` folder). Vanilla only looks at the folder
//! itself; here a directory that is not a pack is a *container*, such as a
//! folder of Faithful packs or of the variants of one pack, and the search goes
//! down into it, [`MAX_DEPTH`] levels at most. Only the metadata and the icon
//! are read, not the textures, so listing a folder of large packs stays cheap
//! on the screen thread.

use std::path::Path;

use crate::error::AssetsResult;
use crate::resource_pack::PackMetadata;
use crate::zip::ZipArchive;

const PACK_MCMETA: &str = "pack.mcmeta";
const PACK_PNG: &str = "pack.png";
const PACK_ASSETS: &str = "assets";
/// How many folders below the packs folder the search goes: a pack in
/// `a/b/c/d/pack.zip` is found, one in `a/b/c/d/e/pack.zip` is not.
pub const MAX_DEPTH: usize = 4;

/// One pack found in the packs folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPack {
    /// The path of the file or folder below the packs folder, with `/`
    /// between the folders (`Faithful/32x.zip`); the key the options store.
    pub name: String,
    /// The parsed `pack.mcmeta`; `None` when it is missing or malformed
    /// (vanilla lists such a pack with a "Missing pack.mcmeta" description).
    pub metadata: Option<PackMetadata>,
    /// The bytes of `pack.png`, when the pack has one.
    pub icon: Option<Vec<u8>>,
}

/// Lists the packs in `folder` and in the folders below it, sorted by path.
///
/// A missing folder or an entry that is neither a pack directory, a container
/// directory nor a readable `.zip` file gives no pack; nothing here fails the
/// screen. A hidden folder (a name that starts with a dot, such as `.git`) is
/// not searched.
#[must_use]
pub fn discover_packs(folder: &Path) -> Vec<DiscoveredPack> {
    let mut packs = Vec::new();
    scan(folder, "", 0, &mut packs);
    packs.sort_by(|a, b| a.name.cmp(&b.name));
    packs
}

/// Adds the packs of `directory` to `found`; `prefix` is its path below the
/// packs folder (empty for the folder itself).
fn scan(directory: &Path, prefix: &str, depth: usize, found: &mut Vec<DiscoveredPack>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_name) = entry.file_name().into_string() else {
            continue;
        };
        let name = if prefix.is_empty() {
            file_name.clone()
        } else {
            format!("{prefix}/{file_name}")
        };
        let path = entry.path();
        if path.is_dir() {
            if is_pack_directory(&path) {
                let (metadata, icon) = read_directory_header(&path);
                found.push(DiscoveredPack {
                    name,
                    metadata,
                    icon,
                });
            } else if depth < MAX_DEPTH && !file_name.starts_with('.') {
                scan(&path, &name, depth + 1, found);
            }
        } else if is_zip_name(&file_name) {
            if let Ok((metadata, icon)) = read_zip_header(&path) {
                found.push(DiscoveredPack {
                    name,
                    metadata,
                    icon,
                });
            }
        }
    }
}

/// Whether a directory is a pack rather than a folder of packs: it has a
/// `pack.mcmeta`, or an `assets` folder (a pack that lost its metadata is
/// still listed, with its "Missing pack.mcmeta" description).
fn is_pack_directory(path: &Path) -> bool {
    path.join(PACK_MCMETA).is_file() || path.join(PACK_ASSETS).is_dir()
}

fn is_zip_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
}

type Header = (Option<PackMetadata>, Option<Vec<u8>>);

fn read_directory_header(root: &Path) -> Header {
    let metadata = std::fs::read(root.join(PACK_MCMETA))
        .ok()
        .and_then(|bytes| PackMetadata::parse(&bytes).ok());
    (metadata, std::fs::read(root.join(PACK_PNG)).ok())
}

fn read_zip_header(path: &Path) -> AssetsResult<Header> {
    let data = std::fs::read(path).map_err(|source| crate::error::AssetsError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let archive = ZipArchive::open(data)?;
    let read = |name: &str| {
        archive
            .entry(name)
            .and_then(|entry| archive.read(entry).ok())
    };
    let metadata = read(PACK_MCMETA).and_then(|bytes| PackMetadata::parse(&bytes).ok());
    Ok((metadata, read(PACK_PNG)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_folder(name: &str) -> std::path::PathBuf {
        let folder =
            std::env::temp_dir().join(format!("umicraft-packs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn a_missing_folder_has_no_packs() {
        assert!(discover_packs(&temp_folder("none").join("absent")).is_empty());
    }

    const MCMETA: &str = r#"{"pack":{"pack_format":1,"description":"A pack"}}"#;

    /// A pack directory with a `pack.mcmeta` at `folder/relative`.
    fn pack_dir(folder: &Path, relative: &str, description: &str) {
        let root = folder.join(relative);
        std::fs::create_dir_all(&root).unwrap();
        let text = format!(r#"{{"pack":{{"pack_format":1,"description":"{description}"}}}}"#);
        std::fs::write(root.join(PACK_MCMETA), text).unwrap();
    }

    #[test]
    fn directories_are_listed_sorted_with_their_metadata() {
        let folder = temp_folder("dirs");
        pack_dir(&folder, "zeta", "Last");
        pack_dir(&folder, "alpha", "First");
        // A pack that lost its `pack.mcmeta` still has its `assets` folder.
        std::fs::create_dir_all(folder.join("broken").join(PACK_ASSETS)).unwrap();
        std::fs::write(folder.join("notes.txt"), "not a pack").unwrap();
        std::fs::write(folder.join("bad.zip"), "not an archive").unwrap();

        let packs = discover_packs(&folder);
        let names: Vec<_> = packs.iter().map(|pack| pack.name.as_str()).collect();
        assert_eq!(names, ["alpha", "broken", "zeta"]);
        assert_eq!(packs[0].metadata.as_ref().unwrap().description, "First");
        assert!(packs[1].metadata.is_none());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_folder_of_packs_is_searched_and_its_packs_are_named_by_their_path() {
        let folder = temp_folder("nested");
        pack_dir(&folder, "Faithful/32x", "Thirty-two");
        pack_dir(&folder, "Faithful/64x", "Sixty-four");
        pack_dir(&folder, "Faithful/HD/128x", "Deeper");
        pack_dir(&folder, "Single", "Alone");
        // A folder of folders of packs, down to the depth limit.
        pack_dir(&folder, "a/b/c/d/deep", "Four levels");
        pack_dir(&folder, "a/b/c/d/e/too_deep", "Five levels");

        let names: Vec<String> = discover_packs(&folder)
            .into_iter()
            .map(|pack| pack.name)
            .collect();
        assert_eq!(
            names,
            [
                "Faithful/32x",
                "Faithful/64x",
                "Faithful/HD/128x",
                "Single",
                "a/b/c/d/deep",
            ],
            "the pack in the fifth folder is out of reach"
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_pack_is_not_searched_for_other_packs_and_empty_or_hidden_folders_give_none() {
        let folder = temp_folder("shapes");
        // A pack directory that holds a folder with a `pack.mcmeta` of its own:
        // the outer one is the pack, the inner one is part of its content.
        pack_dir(&folder, "outer", "Outer");
        pack_dir(&folder, "outer/inner", "Inner");
        std::fs::create_dir_all(folder.join("empty")).unwrap();
        pack_dir(&folder, ".hidden/pack", "In a hidden folder");
        std::fs::write(folder.join("Loose.txt"), MCMETA).unwrap();

        let names: Vec<String> = discover_packs(&folder)
            .into_iter()
            .map(|pack| pack.name)
            .collect();
        assert_eq!(names, ["outer"]);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_zip_extension_is_matched_without_case() {
        assert!(is_zip_name("Pack.ZIP"));
        assert!(!is_zip_name(".zip"));
        assert!(!is_zip_name("pack.rar"));
    }
}
