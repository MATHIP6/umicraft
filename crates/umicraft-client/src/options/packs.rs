// SPDX-License-Identifier: MIT OR Apache-2.0

//! The enabled resource packs (`GameSettings.resourcePacks`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The folder of the data directory that holds the packs, like the
/// `resourcepacks` folder of the vanilla game directory.
pub const PACKS_FOLDER: &str = "resourcepacks";

/// The enabled resource packs, in the order they are applied.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ResourcePackOptions {
    /// File names (or folder names) inside the packs folder, lowest priority
    /// first: a later pack overrides an earlier one, as in
    /// `ResourcePackRepository.setRepositories`.
    pub enabled: Vec<String>,
}

impl ResourcePackOptions {
    /// Drops what cannot name a pack: an empty name, a name with a path
    /// separator or `..` (the file must lie inside the packs folder), and a
    /// repeated name.
    pub(super) fn clamp(&mut self) {
        let mut kept: Vec<String> = Vec::with_capacity(self.enabled.len());
        for name in self.enabled.drain(..) {
            if is_pack_name(&name) && !kept.contains(&name) {
                kept.push(name);
            }
        }
        self.enabled = kept;
    }

    /// The paths of the enabled packs inside `folder`, in application order.
    #[must_use]
    pub fn paths_in(&self, folder: &Path) -> Vec<PathBuf> {
        self.enabled.iter().map(|name| folder.join(name)).collect()
    }
}

/// Whether `name` names a pack that lies inside the packs folder: a file or
/// folder name, or the path of one below other folders (`Faithful/32x.zip`,
/// with `/` between them, at most [`MAX_PACK_DEPTH`] folders deep).
///
/// Nothing that could leave the folder is a pack name: no empty, `.` or `..`
/// part, no backslash, no NUL, and no `:` (a drive such as `C:` replaces the
/// whole path on Windows when it is joined).
#[must_use]
pub fn is_pack_name(name: &str) -> bool {
    let mut parts = 0;
    for part in name.split('/') {
        parts += 1;
        let plain = !part.is_empty() && part != "." && part != "..";
        if !plain || part.contains(['\\', '\0', ':']) {
            return false;
        }
    }
    parts <= MAX_PACK_DEPTH + 1
}

/// How many folders deep a pack may sit, as the search of
/// [`umicraft_assets::pack_discovery::discover_packs`] goes.
pub const MAX_PACK_DEPTH: usize = umicraft_assets::pack_discovery::MAX_DEPTH;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_pack_is_enabled_by_default() {
        assert!(ResourcePackOptions::default().enabled.is_empty());
    }

    #[test]
    fn names_that_escape_the_folder_or_repeat_are_dropped_in_order() {
        let mut packs = ResourcePackOptions {
            enabled: [
                "b.zip",
                "../evil.zip",
                "",
                "a",
                "b.zip",
                "x/../../y",
                "..",
                "c\\d",
                "/abs.zip",
                "x//y",
                "C:evil.zip",
                "a/b.zip:stream",
                "nul\0.zip",
            ]
            .map(String::from)
            .to_vec(),
        };
        packs.clamp();
        assert_eq!(packs.enabled, ["b.zip", "a"]);
    }

    #[test]
    fn a_pack_inside_a_folder_is_a_valid_name_down_to_the_search_depth() {
        assert!(is_pack_name("Faithful/32x.zip"));
        assert!(is_pack_name("a/b/c/d/pack.zip"), "four folders deep");
        assert!(!is_pack_name("a/b/c/d/e/pack.zip"), "five folders is out");
        let mut packs = ResourcePackOptions {
            enabled: ["one.zip", "Folder/two.zip", "Folder/two.zip", "x/./y"]
                .map(String::from)
                .to_vec(),
        };
        packs.clamp();
        assert_eq!(packs.enabled, ["one.zip", "Folder/two.zip"]);
    }

    #[test]
    fn the_path_of_a_nested_pack_stays_inside_the_folder() {
        let packs = ResourcePackOptions {
            enabled: vec!["Faithful/32x.zip".to_string()],
        };
        let folder = Path::new("/data/resourcepacks");
        assert_eq!(
            packs.paths_in(folder),
            [folder.join("Faithful").join("32x.zip")]
        );
    }

    #[test]
    fn the_paths_keep_the_order_inside_the_folder() {
        let packs = ResourcePackOptions {
            enabled: vec!["low.zip".to_string(), "high".to_string()],
        };
        let folder = Path::new("/data/resourcepacks");
        assert_eq!(
            packs.paths_in(folder),
            [folder.join("low.zip"), folder.join("high")]
        );
    }
}
