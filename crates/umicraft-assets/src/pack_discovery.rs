// SPDX-License-Identifier: MIT OR Apache-2.0

//! Listing the resource packs of the `resourcepacks` folder, for the
//! resource packs screen.
//!
//! This is the counterpart of `ResourcePackRepository.updateRepositoryEntriesAll`:
//! a pack is a directory or a `.zip` file that has a `pack.mcmeta`. Only the
//! metadata and the icon are read, not the textures, so listing a folder of
//! large packs stays cheap on the screen thread.

use std::path::Path;

use crate::error::AssetsResult;
use crate::resource_pack::PackMetadata;
use crate::zip::ZipArchive;

const PACK_MCMETA: &str = "pack.mcmeta";
const PACK_PNG: &str = "pack.png";

/// One pack found in the packs folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPack {
    /// The file or folder name inside the packs folder; the key the options
    /// store.
    pub name: String,
    /// The parsed `pack.mcmeta`; `None` when it is missing or malformed
    /// (vanilla lists such a pack with a "Missing pack.mcmeta" description).
    pub metadata: Option<PackMetadata>,
    /// The bytes of `pack.png`, when the pack has one.
    pub icon: Option<Vec<u8>>,
}

/// Lists the packs in `folder`, sorted by name.
///
/// A missing folder or an entry that is neither a directory nor a readable
/// `.zip` file gives no pack; nothing here fails the screen.
#[must_use]
pub fn discover_packs(folder: &Path) -> Vec<DiscoveredPack> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut packs: Vec<DiscoveredPack> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let path = entry.path();
            let (metadata, icon) = if path.is_dir() {
                read_directory_header(&path)
            } else if is_zip_name(&name) {
                read_zip_header(&path).ok()?
            } else {
                return None;
            };
            Some(DiscoveredPack {
                name,
                metadata,
                icon,
            })
        })
        .collect();
    packs.sort_by(|a, b| a.name.cmp(&b.name));
    packs
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

    #[test]
    fn directories_are_listed_sorted_with_their_metadata() {
        let folder = temp_folder("dirs");
        for (name, mcmeta) in [
            (
                "zeta",
                Some(r#"{"pack":{"pack_format":1,"description":"Last"}}"#),
            ),
            (
                "alpha",
                Some(r#"{"pack":{"pack_format":1,"description":"First"}}"#),
            ),
            ("broken", None),
        ] {
            let root = folder.join(name);
            std::fs::create_dir_all(&root).unwrap();
            if let Some(text) = mcmeta {
                std::fs::write(root.join(PACK_MCMETA), text).unwrap();
            }
        }
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
    fn the_zip_extension_is_matched_without_case() {
        assert!(is_zip_name("Pack.ZIP"));
        assert!(!is_zip_name(".zip"));
        assert!(!is_zip_name("pack.rar"));
    }
}
