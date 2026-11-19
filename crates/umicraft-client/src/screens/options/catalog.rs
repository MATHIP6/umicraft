// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the language and resource pack pages choose from.
//!
//! The screen reads no file itself: the window builds a [`Catalog`] when the
//! options open ([`Catalog::discover`]) and hands it over, and the tests
//! build one by hand ([`Catalog::new`]).

use umicraft_assets::cache::{data_dir, AssetCache};
use umicraft_assets::languages::{available_languages, LanguageInfo};
use umicraft_assets::pack_discovery::{discover_packs, DiscoveredPack};
use umicraft_core::lang::Lang;

use crate::options::PACKS_FOLDER;

/// The languages and resource packs the player can pick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    languages: Vec<LanguageInfo>,
    packs: Vec<DiscoveredPack>,
}

impl Default for Catalog {
    /// Only the always-available English, and no pack.
    fn default() -> Self {
        Self::new(vec![english()], Vec::new())
    }
}

impl Catalog {
    /// A catalog of the given languages and packs.
    #[must_use]
    pub fn new(languages: Vec<LanguageInfo>, packs: Vec<DiscoveredPack>) -> Self {
        Self { languages, packs }
    }

    /// Reads the languages of the downloaded assets and the packs of the
    /// `resourcepacks` folder. A missing asset index leaves English alone
    /// and a missing folder leaves no pack, so the pages always open.
    #[must_use]
    pub fn discover() -> Self {
        let languages = AssetCache::in_data_dir()
            .ok()
            .and_then(|cache| available_languages(&cache).ok())
            .filter(|languages| !languages.is_empty())
            .unwrap_or_else(|| vec![english()]);
        let packs = data_dir()
            .map(|folder| discover_packs(&folder.join(PACKS_FOLDER)))
            .unwrap_or_default();
        Self::new(languages, packs)
    }

    /// The selectable languages.
    #[must_use]
    pub fn languages(&self) -> &[LanguageInfo] {
        &self.languages
    }

    /// The packs found in the packs folder.
    #[must_use]
    pub fn packs(&self) -> &[DiscoveredPack] {
        &self.packs
    }
}

fn english() -> LanguageInfo {
    LanguageInfo {
        code: Lang::DEFAULT_CODE.to_string(),
        name: "English".to_string(),
        region: "US".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_catalog_offers_english_and_no_pack() {
        let catalog = Catalog::default();
        assert_eq!(catalog.languages().len(), 1);
        assert_eq!(catalog.languages()[0].code, "en_US");
        assert!(catalog.packs().is_empty());
    }
}
