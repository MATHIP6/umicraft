// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sounds of the user's own downloaded assets (spec §3: nothing bundled).
//!
//! In 1.8 the `sounds.json` index and every `.ogg` file are objects of the
//! asset index, under the logical paths `minecraft/sounds.json` and
//! `minecraft/sounds/<name>.ogg`. [`SoundAssets`] pairs the asset index with an
//! [`ObjectStore`] (the Umicraft cache or an official installation) and reads
//! those objects by the resource paths the audio engine uses
//! (`sounds/dig/stone1.ogg`).

use crate::cache::AssetCache;
use crate::error::{AssetsError, AssetsResult};
use crate::install::OfficialInstall;
use crate::objects::{AssetIndex, ObjectStore};

/// The logical path of the sound index in the asset index.
const SOUNDS_JSON: &str = "minecraft/sounds.json";

/// The SHA-1 an official installation names the 1.8 asset index by.
const OFFICIAL_INDEX_SHA1: &str = "f6ad102bcaa53b1a58358f16e376d548d44933ec";

/// The asset objects that carry the game sounds.
pub struct SoundAssets {
    index: AssetIndex,
    store: Box<dyn ObjectStore>,
}

impl SoundAssets {
    /// Sounds read from `store` as the asset `index` describes them.
    #[must_use]
    pub fn new(index: AssetIndex, store: Box<dyn ObjectStore>) -> Self {
        Self { index, store }
    }

    /// Finds the sounds on this machine: the Umicraft cache first, then an
    /// official installation. `None` when neither has the asset index.
    #[must_use]
    pub fn open() -> Option<Self> {
        Self::from_cache().or_else(Self::from_official_install)
    }

    fn from_cache() -> Option<Self> {
        let cache = AssetCache::in_data_dir().ok()?;
        let index = AssetIndex::load_cached(&cache)?;
        Some(Self::new(index, Box::new(cache)))
    }

    fn from_official_install() -> Option<Self> {
        let install = OfficialInstall::from_env()?;
        let index = AssetIndex::parse(&install.read_index(OFFICIAL_INDEX_SHA1).ok()?).ok()?;
        Some(Self::new(index, Box::new(install)))
    }

    /// The bytes of `sounds.json`.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Missing`] when the index or the object is absent, and
    /// [`AssetsError::HashMismatch`] when the stored bytes are corrupted.
    pub fn sounds_json(&self) -> AssetsResult<Vec<u8>> {
        self.read(SOUNDS_JSON)
    }

    /// The bytes of a sound file by resource path (`sounds/dig/stone1.ogg`).
    ///
    /// # Errors
    ///
    /// As [`SoundAssets::sounds_json`].
    pub fn sound(&self, resource_path: &str) -> AssetsResult<Vec<u8>> {
        self.read(&logical_path(resource_path))
    }

    fn read(&self, logical: &str) -> AssetsResult<Vec<u8>> {
        let object = self
            .index
            .get(logical)
            .ok_or_else(|| AssetsError::Missing {
                what: format!("asset {logical}"),
                path: "the asset index".to_string(),
            })?;
        self.store.read_object(&object.hash)
    }
}

/// The asset-index path of a resource path: the `minecraft` domain in front.
fn logical_path(resource_path: &str) -> String {
    format!("minecraft/{resource_path}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::sha1_hex;
    use std::path::PathBuf;

    /// An in-memory store keyed by hash.
    struct MemoryStore(Vec<(String, Vec<u8>)>);

    impl ObjectStore for MemoryStore {
        fn object_path(&self, _hash: &str) -> Option<PathBuf> {
            None
        }

        fn read_object(&self, hash: &str) -> AssetsResult<Vec<u8>> {
            self.0
                .iter()
                .find(|(key, _)| key == hash)
                .map(|(_, bytes)| bytes.clone())
                .ok_or_else(|| AssetsError::Missing {
                    what: hash.to_string(),
                    path: "memory".to_string(),
                })
        }

        fn store_object(&self, _hash: &str, _bytes: &[u8]) -> AssetsResult<PathBuf> {
            Err(AssetsError::ReadOnly("memory".to_string()))
        }
    }

    fn assets() -> SoundAssets {
        let json = b"{}".to_vec();
        let ogg = b"OggS".to_vec();
        let (json_hash, ogg_hash) = (sha1_hex(&json), sha1_hex(&ogg));
        let index = format!(
            r#"{{"objects": {{
                "minecraft/sounds.json": {{"hash": "{json_hash}", "size": 2}},
                "minecraft/sounds/dig/stone1.ogg": {{"hash": "{ogg_hash}", "size": 4}},
                "minecraft/sounds/lost.ogg": {{"hash": "{}", "size": 1}}
            }}}}"#,
            "0".repeat(40)
        );
        SoundAssets::new(
            AssetIndex::parse(index.as_bytes()).expect("valid index"),
            Box::new(MemoryStore(vec![(json_hash, json), (ogg_hash, ogg)])),
        )
    }

    #[test]
    fn resource_paths_resolve_under_the_minecraft_domain() {
        assert_eq!(
            logical_path("sounds/dig/stone1.ogg"),
            "minecraft/sounds/dig/stone1.ogg"
        );
    }

    #[test]
    fn reads_the_index_and_a_sound() {
        let assets = assets();
        assert_eq!(assets.sounds_json().expect("present"), b"{}");
        assert_eq!(
            assets.sound("sounds/dig/stone1.ogg").expect("present"),
            b"OggS"
        );
    }

    #[test]
    fn a_sound_missing_from_the_index_or_the_store_is_a_typed_error() {
        let assets = assets();
        assert!(matches!(
            assets.sound("sounds/nope.ogg"),
            Err(AssetsError::Missing { .. })
        ));
        assert!(matches!(
            assets.sound("sounds/lost.ogg"),
            Err(AssetsError::Missing { .. })
        ));
    }
}
