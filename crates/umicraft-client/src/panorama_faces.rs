// SPDX-License-Identifier: MIT OR Apache-2.0

//! The six title-screen panorama pictures of the user's client jar (P10).
//!
//! `panorama_0.png` to `panorama_5.png` are read at runtime from the verified
//! jar and never stored in the repository (spec §3, owner-approved use). When
//! the jar is absent or a face does not decode, the title screen keeps its
//! dirt background.

use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::panorama::PanoramaImages;
use umicraft_render::SkyImage;

/// Reads the six faces of `jar`; `None` when there is no jar or a face is
/// missing or malformed (logged at debug level).
#[must_use]
pub fn load_panorama_faces(jar: Option<&ClientJar>) -> Option<[SkyImage; 6]> {
    let images = match PanoramaImages::load(jar?) {
        Ok(images) => images,
        Err(error) => {
            tracing::debug!(%error, "panorama faces unavailable, using the dirt background");
            return None;
        }
    };
    Some(images.faces.map(|face| SkyImage {
        width: face.size,
        height: face.size,
        rgba: face.pixels,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_jar_there_are_no_faces() {
        assert!(load_panorama_faces(None).is_none());
    }
}
