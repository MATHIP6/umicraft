// SPDX-License-Identifier: MIT OR Apache-2.0

//! The resource-pack files that [`ClientResources`] does not bake.
//!
//! [`ClientResources::apply_packs`] keeps only the path classes it turns into
//! atlases and models (block and item textures, colormaps, GUI, models,
//! blockstates). Other consumers need more:
//! - the sky, weather and overlay pictures (`textures/environment`,
//!   `textures/misc`, `textures/particle`);
//! - the entity and armour textures (`textures/entity`,
//!   `textures/models/armor`);
//! - the map pictures (`textures/map`);
//! - the font pictures (`textures/font`);
//! - the pack features of milestone F6 (`optifine/`, `mcpatcher/`; ADR-0089).
//!
//! [`PackFiles`] keeps exactly those files of the active packs. It keeps them
//! in priority order and answers top-down lookups. It takes the packs by value
//! and filters them, so no byte is copied and the other files are freed with
//! the packs.
//!
//! The jar is not a layer here: the client puts it below the packs.
//!
//! [`ClientResources`]: crate::ClientResources
//! [`ClientResources::apply_packs`]: crate::ClientResources::apply_packs

use std::collections::BTreeMap;

use crate::resource_pack::ResourcePack;

/// The path classes kept, relative to `assets/minecraft/`.
pub const KEPT_PREFIXES: [&str; 9] = [
    "optifine/",
    "mcpatcher/",
    "textures/environment/",
    "textures/entity/",
    "textures/misc/",
    "textures/particle/",
    "textures/map/",
    "textures/models/armor/",
    "textures/font/",
];

/// Folders searched for a pack-feature file, in the order they are tried
/// within one pack. `optifine/` is the current name and `mcpatcher/` the
/// legacy one. A pack carrying both is read from `optifine/`.
const FEATURE_FOLDERS: [&str; 2] = ["assets/minecraft/optifine/", "assets/minecraft/mcpatcher/"];

/// The kept files of every active pack, lowest priority first.
///
/// # Example
///
/// ```
/// use std::collections::BTreeMap;
/// use umicraft_assets::{PackFiles, ResourcePack};
///
/// let mut files = BTreeMap::new();
/// files.insert("assets/minecraft/mcpatcher/sky/world0/sky1.properties".to_string(), b"blend=add".to_vec());
/// let pack = ResourcePack::from_parts("sky", None, None, files);
///
/// let files = PackFiles::from_packs(vec![pack]);
/// let found = files.feature("sky/world0/sky1.properties").expect("present");
/// assert_eq!(found.path, "assets/minecraft/mcpatcher/sky/world0/sky1.properties");
/// assert_eq!(found.bytes, b"blend=add");
/// ```
#[derive(Debug, Clone, Default)]
pub struct PackFiles {
    /// One map per pack, in ascending priority.
    layers: Vec<BTreeMap<String, Vec<u8>>>,
}

/// A pack-feature file found by [`PackFiles::feature`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureFile<'a> {
    /// The full virtual path (`assets/minecraft/optifine/...`). Features
    /// resolve the relative paths inside the file against it.
    pub path: &'a str,
    /// The file's content.
    pub bytes: &'a [u8],
}

impl PackFiles {
    /// Keeps the files of `packs` under [`KEPT_PREFIXES`]. `packs` are in
    /// ascending priority, as in [`ResourceStack`](crate::ResourceStack).
    #[must_use]
    pub fn from_packs(packs: impl IntoIterator<Item = ResourcePack>) -> Self {
        let layers = packs
            .into_iter()
            .map(|pack| {
                pack.into_files()
                    .into_iter()
                    .filter(|(path, _)| is_kept(path))
                    .collect()
            })
            .collect();
        Self { layers }
    }

    /// Whether no active pack has a kept file.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.layers.iter().all(BTreeMap::is_empty)
    }

    /// The file at the full virtual `path` from the highest pack that has it.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.layers
            .iter()
            .rev()
            .find_map(|layer| layer.get(path))
            .map(Vec::as_slice)
    }

    /// The pack-feature file at `relative` (for example
    /// `sky/world0/sky1.properties`). Packs are searched from the highest
    /// priority down. Within a pack, `optifine/` is tried before
    /// `mcpatcher/`, so a higher pack wins whichever folder it uses.
    #[must_use]
    pub fn feature(&self, relative: &str) -> Option<FeatureFile<'_>> {
        self.layers.iter().rev().find_map(|layer| {
            FEATURE_FOLDERS.iter().find_map(|folder| {
                layer
                    .get_key_value(&format!("{folder}{relative}"))
                    .map(|(path, bytes)| FeatureFile { path, bytes })
            })
        })
    }
}

impl PackFiles {
    /// Every pack-feature file under the folder `relative_dir` (for example
    /// `cem/` or `ctm/`), at any depth, as [`FeatureFile`]s. A relative path
    /// present in several packs or in both feature folders is listed once, with
    /// the file [`PackFiles::feature`] would pick. Sorted by relative path.
    #[must_use]
    pub fn list_feature(&self, relative_dir: &str) -> Vec<(String, FeatureFile<'_>)> {
        let mut relatives: Vec<String> = self
            .layers
            .iter()
            .flat_map(|layer| layer.keys())
            .filter_map(|path| {
                FEATURE_FOLDERS
                    .iter()
                    .find_map(|folder| path.strip_prefix(folder))
                    .filter(|rest| rest.starts_with(relative_dir))
                    .map(str::to_string)
            })
            .collect();
        relatives.sort();
        relatives.dedup();
        relatives
            .into_iter()
            .filter_map(|relative| {
                let file = self.feature(&relative)?;
                Some((relative, file))
            })
            .collect()
    }
}

/// Whether the full virtual `path` falls in one of the [`KEPT_PREFIXES`].
fn is_kept(path: &str) -> bool {
    path.strip_prefix("assets/minecraft/")
        .is_some_and(|rest| KEPT_PREFIXES.iter().any(|prefix| rest.starts_with(prefix)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(id: &str, files: &[(&str, &[u8])]) -> ResourcePack {
        let files = files
            .iter()
            .map(|(path, bytes)| (format!("assets/minecraft/{path}"), bytes.to_vec()))
            .collect();
        ResourcePack::from_parts(id, None, None, files)
    }

    #[test]
    fn only_the_kept_path_classes_survive() {
        let files = PackFiles::from_packs([pack(
            "a",
            &[
                ("textures/environment/sun.png", b"sun"),
                ("textures/entity/creeper/creeper.png", b"creeper"),
                ("textures/blocks/stone.png", b"stone"),
                ("models/block/stone.json", b"{}"),
            ],
        )]);
        assert_eq!(
            files.get("assets/minecraft/textures/environment/sun.png"),
            Some(&b"sun"[..])
        );
        assert!(files
            .get("assets/minecraft/textures/entity/creeper/creeper.png")
            .is_some());
        assert!(files
            .get("assets/minecraft/textures/blocks/stone.png")
            .is_none());
        assert!(files
            .get("assets/minecraft/models/block/stone.json")
            .is_none());
    }

    #[test]
    fn the_font_pictures_are_kept() {
        let files = PackFiles::from_packs([pack(
            "font",
            &[
                ("textures/font/ascii.png", b"grid"),
                ("textures/font/unicode_page_00.png", b"page"),
            ],
        )]);
        assert!(files
            .get("assets/minecraft/textures/font/ascii.png")
            .is_some());
        assert!(files
            .get("assets/minecraft/textures/font/unicode_page_00.png")
            .is_some());
    }

    #[test]
    fn the_highest_pack_wins() {
        let files = PackFiles::from_packs([
            pack("low", &[("textures/environment/sun.png", b"low")]),
            pack("high", &[("textures/environment/sun.png", b"high")]),
        ]);
        assert_eq!(
            files.get("assets/minecraft/textures/environment/sun.png"),
            Some(&b"high"[..])
        );
    }

    #[test]
    fn a_feature_prefers_optifine_within_one_pack() {
        let files = PackFiles::from_packs([pack(
            "both",
            &[
                ("mcpatcher/sky/world0/sky1.properties", b"legacy"),
                ("optifine/sky/world0/sky1.properties", b"current"),
            ],
        )]);
        let found = files.feature("sky/world0/sky1.properties");
        assert_eq!(
            found.map(|file| (file.path, file.bytes)),
            Some((
                "assets/minecraft/optifine/sky/world0/sky1.properties",
                &b"current"[..]
            ))
        );
    }

    #[test]
    fn a_higher_pack_wins_over_the_folder_order() {
        let files = PackFiles::from_packs([
            pack("low", &[("optifine/sky/world0/sky1.properties", b"low")]),
            pack("high", &[("mcpatcher/sky/world0/sky1.properties", b"high")]),
        ]);
        assert_eq!(
            files
                .feature("sky/world0/sky1.properties")
                .map(|file| file.bytes),
            Some(&b"high"[..])
        );
    }

    #[test]
    fn listing_a_feature_folder_merges_packs_and_folders() {
        let files = PackFiles::from_packs([
            pack(
                "low",
                &[
                    ("mcpatcher/ctm/glass/glass.properties", b"low"),
                    ("mcpatcher/ctm/stone.properties", b"stone"),
                ],
            ),
            pack("high", &[("optifine/ctm/glass/glass.properties", b"high")]),
        ]);
        let listed: Vec<_> = files
            .list_feature("ctm/")
            .into_iter()
            .map(|(relative, file)| (relative, file.bytes))
            .collect();
        assert_eq!(
            listed,
            [
                ("ctm/glass/glass.properties".to_string(), &b"high"[..]),
                ("ctm/stone.properties".to_string(), &b"stone"[..]),
            ]
        );
        assert!(files.list_feature("cem/").is_empty());
    }

    #[test]
    fn no_pack_means_empty_and_no_file() {
        let files = PackFiles::default();
        assert!(files.is_empty());
        assert!(files.feature("sky/world0/sky1.properties").is_none());
        let only_blocks = PackFiles::from_packs([pack("b", &[("textures/blocks/a.png", b"a")])]);
        assert!(only_blocks.is_empty());
    }
}
