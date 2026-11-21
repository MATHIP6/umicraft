// SPDX-License-Identifier: MIT OR Apache-2.0

//! The files read at runtime: the active resource packs over the client jar.
//!
//! The sky pictures, the entity and armour textures and the map pictures used
//! to be read from the jar alone, so a pack could not replace them. Every such
//! read now goes through [`GameFiles`], which asks the packs first
//! ([`PackFiles`], highest priority down) and the verified jar last. That is
//! the order of vanilla's `FallbackResourceManager`: the default pack sits at
//! the bottom of the stack.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::skin::decode_png_bytes;
use umicraft_assets::{PackFiles, ResourcePack};

/// The active packs over the verified jar. Cheap to clone: the pack files are
/// shared and the jar is a path.
#[derive(Debug, Clone, Default)]
pub struct GameFiles {
    packs: Arc<PackFiles>,
    jar: Option<ClientJar>,
}

impl GameFiles {
    /// The `packs` (kept files only) over `jar`.
    #[must_use]
    pub fn new(packs: PackFiles, jar: Option<ClientJar>) -> Self {
        Self {
            packs: Arc::new(packs),
            jar,
        }
    }

    /// The verified jar alone, for demos, tests and the first frames before
    /// the packs are loaded.
    #[must_use]
    pub fn jar_only() -> Self {
        Self::new(
            PackFiles::default(),
            super::official::find_verified_jar().ok(),
        )
    }

    /// The file at the full virtual `path` (`assets/minecraft/...`): from the
    /// highest pack that has it, else from the jar. `None` when neither has it.
    #[must_use]
    pub fn read(&self, path: &str) -> Option<Cow<'_, [u8]>> {
        if let Some(bytes) = self.packs.get(path) {
            return Some(Cow::Borrowed(bytes));
        }
        self.jar.as_ref()?.read_entry(path).ok().map(Cow::Owned)
    }

    /// The PNG at `path` decoded to `(rgba, width, height)`. A pack picture
    /// that does not decode gives way to the jar's, as a bad pack texture
    /// gives way to the one beneath it in `ClientResources::apply_packs`.
    #[must_use]
    pub fn read_png(&self, path: &str) -> Option<(Vec<u8>, u32, u32)> {
        let from_pack = self.packs.get(path).and_then(|bytes| {
            decode_png_bytes(bytes)
                .map_err(|error| tracing::warn!(%path, %error, "pack picture does not decode"))
                .ok()
        });
        from_pack.or_else(|| {
            let bytes = self.jar.as_ref()?.read_entry(path).ok()?;
            decode_png_bytes(&bytes).ok()
        })
    }

    /// Whether the verified jar is below the packs.
    #[must_use]
    pub fn has_jar(&self) -> bool {
        self.jar.is_some()
    }

    /// The pack files, for the pack features (ADR-0089).
    #[must_use]
    pub fn packs(&self) -> &PackFiles {
        &self.packs
    }
}

/// Reads every pack in `paths` (ascending priority). A pack that cannot be
/// read (deleted, unreadable folder, broken zip) is skipped with a warning:
/// one bad pack must not cost the others.
#[must_use]
pub fn load_packs(paths: &[PathBuf]) -> Vec<ResourcePack> {
    paths
        .iter()
        .filter_map(|path| match ResourcePack::from_path(path) {
            Ok(pack) => Some(pack),
            Err(error) => {
                tracing::warn!(pack = %path.display(), %error, "skipping a resource pack that could not be loaded");
                None
            }
        })
        .collect()
}

/// Builders for tests that need pack files without a pack on disk.
#[cfg(test)]
pub(crate) mod test_support {
    use std::collections::BTreeMap;

    use umicraft_assets::{PackFiles, ResourcePack};

    use super::GameFiles;

    /// An opaque white RGBA PNG of `width` × `height`.
    pub(crate) fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");
        writer
            .write_image_data(&vec![255; (width * height * 4) as usize])
            .expect("pixels");
        drop(writer);
        bytes
    }

    /// One pack holding `bytes` at the full virtual `path`, and no jar.
    pub(crate) fn files_with(path: &str, bytes: Vec<u8>) -> GameFiles {
        let pack = ResourcePack::from_parts(
            "test",
            None,
            None,
            BTreeMap::from([(path.to_string(), bytes)]),
        );
        GameFiles::new(PackFiles::from_packs([pack]), None)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{files_with, png};
    use super::*;

    const SUN: &str = "assets/minecraft/textures/environment/sun.png";

    #[test]
    fn a_pack_picture_is_read_without_a_jar() {
        let files = files_with(SUN, png(4, 2));
        assert_eq!(files.read_png(SUN).map(|(_, w, h)| (w, h)), Some((4, 2)));
        assert!(files.read(SUN).is_some());
    }

    #[test]
    fn a_broken_pack_picture_falls_through() {
        let files = files_with(SUN, b"not a png".to_vec());
        assert!(files.read_png(SUN).is_none());
    }

    #[test]
    fn nothing_without_packs_or_jar() {
        let files = GameFiles::default();
        assert!(files.read(SUN).is_none());
        assert!(files.packs().is_empty());
    }

    #[test]
    fn unreadable_packs_are_skipped() {
        let packs = load_packs(&[PathBuf::from("/nonexistent/umicraft-pack.zip")]);
        assert!(packs.is_empty());
    }
}
