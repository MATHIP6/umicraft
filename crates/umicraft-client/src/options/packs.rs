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

/// Whether `name` is a plain file or folder name.
#[must_use]
pub fn is_pack_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
}

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
            enabled: ["b.zip", "../evil.zip", "", "a", "b.zip", "x/y", "..", "c\\d"]
                .map(String::from)
                .to_vec(),
        };
        packs.clamp();
        assert_eq!(packs.enabled, ["b.zip", "a"]);
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
