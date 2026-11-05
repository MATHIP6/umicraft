// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sound files of the user's own assets.
//!
//! Nothing is bundled (spec §3): `sounds.json` and the `.ogg` files are objects
//! of the asset index the user downloaded with `--setup-assets` (or that an
//! official installation already has). [`open`] finds them; without them the
//! caller runs silent.

use std::io;

use thiserror::Error;
use umicraft_assets::{AssetsError, SoundAssets};
use umicraft_audio::engine::SoundLoader;
use umicraft_audio::{AudioError, SoundIndex};

/// Why the sounds could not be set up.
#[derive(Debug, Error)]
pub enum SoundSetupError {
    /// Neither the Umicraft cache nor an official installation has the asset index.
    #[error("no downloaded assets found (run `umicraft-client --setup-assets`)")]
    NoAssets,
    /// `sounds.json` is missing from the store or corrupted.
    #[error("sounds.json: {0}")]
    Assets(#[from] AssetsError),
    /// `sounds.json` is not valid UTF-8.
    #[error("sounds.json is not UTF-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    /// `sounds.json` does not parse.
    #[error("sounds.json: {0}")]
    Index(#[from] AudioError),
}

/// Reads sound files for the audio thread from the user's asset store.
pub struct AssetSoundLoader {
    assets: SoundAssets,
}

impl SoundLoader for AssetSoundLoader {
    fn load(&mut self, resource_path: &str) -> io::Result<Vec<u8>> {
        self.assets
            .sound(resource_path)
            .map_err(|error| match error {
                AssetsError::Missing { .. } => io::Error::new(io::ErrorKind::NotFound, error),
                other => io::Error::other(other),
            })
    }
}

/// Parses the `sounds.json` of `assets` and pairs it with the loader of the
/// `.ogg` files.
///
/// # Errors
///
/// [`SoundSetupError`] when `sounds.json` is missing, corrupt or invalid.
pub fn from_assets(assets: SoundAssets) -> Result<(AssetSoundLoader, SoundIndex), SoundSetupError> {
    let json = String::from_utf8(assets.sounds_json()?)?;
    let index = SoundIndex::parse(&json)?;
    Ok((AssetSoundLoader { assets }, index))
}

/// Finds the user's sounds on this machine.
///
/// # Errors
///
/// [`SoundSetupError::NoAssets`] when no asset index exists, otherwise as
/// [`from_assets`].
pub fn open() -> Result<(AssetSoundLoader, SoundIndex), SoundSetupError> {
    from_assets(SoundAssets::open().ok_or(SoundSetupError::NoAssets)?)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use umicraft_assets::objects::{AssetIndex, ObjectStore};
    use umicraft_assets::{hash, AssetsResult};

    use super::*;

    /// An in-memory object store.
    struct Memory(Vec<Vec<u8>>);

    impl ObjectStore for Memory {
        fn object_path(&self, _hash: &str) -> Option<PathBuf> {
            None
        }

        fn read_object(&self, wanted: &str) -> AssetsResult<Vec<u8>> {
            self.0
                .iter()
                .find(|bytes| hash::sha1_hex(bytes) == wanted)
                .cloned()
                .ok_or_else(|| AssetsError::Missing {
                    what: wanted.to_owned(),
                    path: "memory".to_owned(),
                })
        }

        fn store_object(&self, _hash: &str, _bytes: &[u8]) -> AssetsResult<PathBuf> {
            Err(AssetsError::ReadOnly("memory".to_owned()))
        }
    }

    const SOUNDS_JSON: &str = r#"{"dig.stone": {"category": "block", "sounds": ["dig/stone1"]}}"#;

    fn assets(objects: &[(&str, &[u8])]) -> SoundAssets {
        let entries: Vec<String> = objects
            .iter()
            .map(|(path, bytes)| {
                format!(
                    r#""{path}": {{"hash": "{}", "size": {}}}"#,
                    hash::sha1_hex(bytes),
                    bytes.len()
                )
            })
            .collect();
        let index = format!(r#"{{"objects": {{{}}}}}"#, entries.join(","));
        SoundAssets::new(
            AssetIndex::parse(index.as_bytes()).expect("valid index"),
            Box::new(Memory(objects.iter().map(|(_, b)| b.to_vec()).collect())),
        )
    }

    #[test]
    fn the_index_and_the_files_come_from_the_asset_objects() {
        let ogg: &[u8] = b"OggS-bytes";
        let assets = assets(&[
            ("minecraft/sounds.json", SOUNDS_JSON.as_bytes()),
            ("minecraft/sounds/dig/stone1.ogg", ogg),
        ]);
        let (mut loader, index) = from_assets(assets).expect("sounds are set up");
        assert!(index.event("dig.stone").is_some());
        assert_eq!(loader.load("sounds/dig/stone1.ogg").expect("file"), ogg);
    }

    #[test]
    fn a_missing_file_is_not_found_not_a_crash() {
        let assets = assets(&[("minecraft/sounds.json", SOUNDS_JSON.as_bytes())]);
        let (mut loader, _) = from_assets(assets).expect("index only");
        let error = loader.load("sounds/dig/stone1.ogg").expect_err("absent");
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn missing_or_broken_sounds_json_is_a_typed_error() {
        assert!(matches!(
            from_assets(assets(&[])),
            Err(SoundSetupError::Assets(_))
        ));
        let broken = assets(&[("minecraft/sounds.json", b"{ nope")]);
        assert!(matches!(
            from_assets(broken),
            Err(SoundSetupError::Index(_))
        ));
    }
}
