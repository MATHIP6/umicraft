// SPDX-License-Identifier: MIT OR Apache-2.0

//! Custom GUIs: container screens that packs retexture (F6 phase 7).
//!
//! The rules (`gui/container/**.properties`, format in
//! `umicraft_packext::guis`) name replacement pictures. The GUI atlas is
//! built from `textures/gui/**`, so [`CustomGuis::scan`] copies each picture a
//! rule uses into a small synthetic pack as `textures/gui/optifine_custom/<n>.png`;
//! the atlas then holds it as the region `optifine_custom/<n>`, and
//! [`CustomGuis::region_for`] tells which region replaces a vanilla one for a
//! screen.
//!
//! Replacement pictures must sit in a pack-feature folder (usually next to
//! the rule): pictures elsewhere in `assets/minecraft` are not kept for the
//! features (`PackFiles`), so such a rule is dropped with a warning.

use std::collections::BTreeMap;

use umicraft_assets::{PackFiles, ResourcePack};
use umicraft_packext::cem::PackPath;
use umicraft_packext::guis::{GuiContext, GuiSet};

use super::game_files::PackFeatures;

/// The region names start with this folder of `textures/gui/`.
const REGION_FOLDER: &str = "optifine_custom";

/// The container screen rules of the packs and the atlas regions of their
/// pictures. Empty when the option is off or no pack brings a rule.
#[derive(Debug, Clone, Default)]
pub struct CustomGuis {
    set: GuiSet,
    /// The region of each source picture, by [`source_key`].
    regions: BTreeMap<String, String>,
}

impl CustomGuis {
    /// Reads the rules of `files` and returns them with the pack that carries
    /// their pictures (`None` when there are none).
    #[must_use]
    pub fn scan(files: &PackFiles, features: PackFeatures) -> (Self, Option<ResourcePack>) {
        if !features.custom_guis {
            return (Self::default(), None);
        }
        let listed = files.list_feature("gui/container/");
        let set = GuiSet::parse(
            listed
                .iter()
                .map(|(relative, file)| (relative.as_str(), file.bytes)),
        );
        let mut regions = BTreeMap::new();
        let mut pictures = BTreeMap::new();
        for rule in set.rules() {
            for (_, source) in &rule.textures {
                let key = source_key(source);
                if regions.contains_key(&key) {
                    continue;
                }
                let Some(bytes) = source_bytes(files, source) else {
                    tracing::warn!(rule = %rule.path, ?source, "custom GUI picture not found");
                    continue;
                };
                let region = format!("{REGION_FOLDER}/{}", regions.len());
                pictures.insert(
                    format!("assets/minecraft/textures/gui/{region}.png"),
                    bytes.to_vec(),
                );
                regions.insert(key, region);
            }
        }
        let pack = (!pictures.is_empty())
            .then(|| ResourcePack::from_parts("optifine-custom-guis", None, None, pictures));
        (Self { set, regions }, pack)
    }

    /// Whether no rule was read.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// The atlas region that replaces the vanilla region `vanilla` for the
    /// screen of `context`, `None` to keep the vanilla one.
    #[must_use]
    pub fn region_for(&self, context: &GuiContext<'_>, vanilla: &str) -> Option<&str> {
        let source = self.set.texture(context, vanilla)?;
        self.regions.get(&source_key(source)).map(String::as_str)
    }
}

/// A unique key for a source picture.
fn source_key(source: &PackPath) -> String {
    match source {
        PackPath::Feature(path) => format!("feature:{path}"),
        PackPath::Resource(path) => format!("resource:{path}"),
    }
}

/// The bytes of a source picture, from the highest pack that has it.
fn source_bytes<'a>(files: &'a PackFiles, source: &PackPath) -> Option<&'a [u8]> {
    match source {
        PackPath::Feature(path) => files.feature(path).map(|file| file.bytes),
        PackPath::Resource(path) => files.get(&format!("assets/minecraft/{path}")),
    }
}

#[cfg(test)]
mod tests {
    use umicraft_packext::guis::Container;

    use super::*;
    use crate::resources::game_files::test_support::png;

    fn files(entries: &[(&str, Vec<u8>)]) -> PackFiles {
        let map = entries
            .iter()
            .map(|(path, bytes)| (format!("assets/minecraft/{path}"), bytes.clone()))
            .collect();
        PackFiles::from_packs([ResourcePack::from_parts("guis", None, None, map)])
    }

    const RULE: &str = "optifine/gui/container/chest.properties";
    const PICTURE: &str = "optifine/gui/container/blue.png";

    #[test]
    fn a_rule_gets_a_region_and_a_pack_with_its_picture() {
        let files = files(&[
            (
                RULE,
                b"container=chest\ntexture=blue\nlarge=true\n".to_vec(),
            ),
            (PICTURE, png(256, 256)),
        ]);
        let (guis, pack) = CustomGuis::scan(&files, PackFeatures::ALL);
        let large = GuiContext {
            large: Some(true),
            ..GuiContext::of(Container::Chest)
        };
        assert_eq!(
            guis.region_for(&large, "container/generic_54"),
            Some("optifine_custom/0")
        );
        assert_eq!(
            guis.region_for(&GuiContext::of(Container::Chest), "container/generic_54"),
            None
        );
        let pack = pack.expect("a pack with the picture");
        assert!(pack
            .get("assets/minecraft/textures/gui/optifine_custom/0.png")
            .is_some());
    }

    #[test]
    fn a_picture_shared_by_two_rules_is_copied_once() {
        let files = files(&[
            (RULE, b"container=chest\ntexture=blue\n".to_vec()),
            (
                "optifine/gui/container/furnace.properties",
                b"container=furnace\ntexture=./blue\n".to_vec(),
            ),
            (PICTURE, png(256, 256)),
        ]);
        let (guis, pack) = CustomGuis::scan(&files, PackFeatures::ALL);
        assert_eq!(pack.expect("pack").file_count(), 1);
        let region = |container, vanilla| guis.region_for(&GuiContext::of(container), vanilla);
        assert_eq!(
            region(Container::Chest, "container/generic_54"),
            Some("optifine_custom/0")
        );
        assert_eq!(
            region(Container::Furnace, "container/furnace"),
            Some("optifine_custom/0")
        );
    }

    #[test]
    fn a_missing_picture_drops_its_rule_and_the_option_off_reads_nothing() {
        let files = files(&[(RULE, b"container=chest\ntexture=gone\n".to_vec())]);
        let (guis, pack) = CustomGuis::scan(&files, PackFeatures::ALL);
        assert!(pack.is_none());
        assert_eq!(
            guis.region_for(&GuiContext::of(Container::Chest), "container/generic_54"),
            None
        );

        let with_picture = self::files(&[
            (RULE, b"container=chest\ntexture=blue\n".to_vec()),
            (PICTURE, png(256, 256)),
        ]);
        let off = PackFeatures {
            custom_guis: false,
            ..PackFeatures::ALL
        };
        let (guis, pack) = CustomGuis::scan(&with_picture, off);
        assert!(guis.is_empty() && pack.is_none());
    }
}
