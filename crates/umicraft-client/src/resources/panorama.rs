// SPDX-License-Identifier: MIT OR Apache-2.0

//! The six title-screen panorama pictures (P10, ADR-0056).
//!
//! The client pack embeds Umicraft's own panorama, so the title screen has a
//! background without any download. The jar's `panorama_0.png` to
//! `panorama_5.png` (read at runtime, never stored in the repository, spec §3)
//! are only the fallback for a pack that fails to decode.

use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::client_pack;
use umicraft_assets::panorama::PanoramaImages;
use umicraft_assets::AssetsResult;
use umicraft_render::SkyImage;

/// The six faces of the client pack, or of `jar` when the pack does not
/// decode; `None` when neither works (logged at debug level), which keeps the
/// dirt background.
#[must_use]
pub fn load_panorama_faces(jar: Option<&ClientJar>) -> Option<[SkyImage; 6]> {
    let pack = client_pack::panorama();
    let images = pack.or_else(|error| {
        tracing::debug!(%error, "client pack panorama unavailable, trying the jar");
        jar_panorama(jar)
    });
    match images {
        Ok(images) => Some(sky_faces(images)),
        Err(error) => {
            tracing::debug!(%error, "panorama faces unavailable, using the dirt background");
            None
        }
    }
}

fn jar_panorama(jar: Option<&ClientJar>) -> AssetsResult<PanoramaImages> {
    let jar = jar.ok_or_else(|| umicraft_assets::AssetsError::Missing {
        what: "client jar".to_string(),
        path: String::new(),
    })?;
    PanoramaImages::load(jar)
}

fn sky_faces(images: PanoramaImages) -> [SkyImage; 6] {
    images.faces.map(|face| SkyImage {
        width: face.size,
        height: face.size,
        rgba: face.pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_pack_gives_square_faces_without_a_jar() {
        let faces = load_panorama_faces(None).expect("the pack is embedded");
        assert!(faces
            .iter()
            .all(|face| face.width == face.height && face.width == faces[0].width));
    }
}
