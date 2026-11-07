// SPDX-License-Identifier: MIT OR Apache-2.0

//! Umicraft's own assets, embedded at build time (ADR-0056).
//!
//! The files live under `client-pack/umicraft/` and are the only assets the
//! repository ships (spec §3); this module is the single place that names
//! them. Each file is listed with its provenance in
//! `client-pack/umicraft/SOURCES.md`.

use crate::client_resources::DecodedPng;
use crate::error::{AssetsError, AssetsResult};
use crate::gui_atlas::DynamicImage;
use crate::panorama::{self, PanoramaImages, FACE_COUNT};

/// The embedded `.lang` files, by language code.
const LANGS: [(&str, &str); 2] = [
    (
        "en_US",
        include_str!("../client-pack/umicraft/lang/en_US.lang"),
    ),
    (
        "fr_FR",
        include_str!("../client-pack/umicraft/lang/fr_FR.lang"),
    ),
];

const LOGO: &[u8] = include_bytes!("../client-pack/umicraft/textures/ui/logo.png");
const SETTINGS: &[u8] = include_bytes!("../client-pack/umicraft/textures/ui/settings.png");
const BRAND_FONT: &[u8] = include_bytes!("../client-pack/umicraft/font/Inter-SemiBold.ttf");

/// The panorama faces in file order.
const PANORAMA: [&[u8]; FACE_COUNT] = [
    include_bytes!("../client-pack/umicraft/textures/ui/panorama/panorama_0.png"),
    include_bytes!("../client-pack/umicraft/textures/ui/panorama/panorama_1.png"),
    include_bytes!("../client-pack/umicraft/textures/ui/panorama/panorama_2.png"),
    include_bytes!("../client-pack/umicraft/textures/ui/panorama/panorama_3.png"),
    include_bytes!("../client-pack/umicraft/textures/ui/panorama/panorama_4.png"),
    include_bytes!("../client-pack/umicraft/textures/ui/panorama/panorama_5.png"),
];

/// The text of Umicraft's `.lang` file for `code` (`umicraft.*` keys), or
/// `None` when the pack has no translation for that language.
#[must_use]
pub fn lang(code: &str) -> Option<&'static str> {
    LANGS
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(_, text)| *text)
}

/// The Umicraft logo, a white PNG on a transparent background.
#[must_use]
pub fn logo_png() -> &'static [u8] {
    LOGO
}

/// The options icon of the modern title screen (64x64 PNG).
#[must_use]
pub fn settings_png() -> &'static [u8] {
    SETTINGS
}

/// The PNG of panorama face `face` (`0..6`), `None` past the last face.
#[must_use]
pub fn panorama_png(face: usize) -> Option<&'static [u8]> {
    PANORAMA.get(face).copied()
}

/// The brand font (Inter SemiBold, SIL OFL 1.1) as TrueType data.
#[must_use]
pub fn brand_font_ttf() -> &'static [u8] {
    BRAND_FONT
}

/// Decodes the embedded panorama, the same way the jar one is decoded.
///
/// # Errors
///
/// [`AssetsError::Image`] when a face is not a square PNG or the faces differ
/// in size; this only happens if the embedded files are replaced by bad ones.
pub fn panorama() -> AssetsResult<PanoramaImages> {
    PanoramaImages::from_reader(|path| {
        (0..FACE_COUNT)
            .find(|face| panorama::face_path(*face) == path)
            .and_then(panorama_png)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| AssetsError::Missing {
                what: "panorama face".to_string(),
                path: path.into(),
            })
    })
}

/// The brand images as GUI atlas images named `umicraft/logo` and
/// `umicraft/settings`, ready for
/// [`crate::GuiAtlas::with_dynamic`] (its regions get the
/// [`crate::DYNAMIC_PREFIX`]).
///
/// # Errors
///
/// [`AssetsError::Image`] when an embedded PNG does not decode.
pub fn brand_images() -> AssetsResult<Vec<DynamicImage>> {
    [("umicraft/logo", LOGO), ("umicraft/settings", SETTINGS)]
        .into_iter()
        .map(|(name, bytes)| {
            let png = DecodedPng::decode(name, bytes)?;
            Ok(DynamicImage {
                name: name.to_string(),
                width: png.width,
                height: png.height,
                pixels: png.pixels,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_png_decodes() {
        for (name, bytes) in [("logo", LOGO), ("settings", SETTINGS)] {
            DecodedPng::decode(name, bytes).expect("embedded PNG decodes");
        }
        assert!(panorama_png(FACE_COUNT).is_none());
    }

    #[test]
    fn the_panorama_faces_are_square_and_equal() {
        let images = panorama().expect("embedded panorama decodes");
        let size = images.faces[0].size;
        assert!(images.faces.iter().all(|face| face.size == size));
        assert!(size > 0);
    }

    #[test]
    fn brand_images_have_their_pixels() {
        let images = brand_images().expect("brand images decode");
        let names: Vec<_> = images.iter().map(|image| image.name.as_str()).collect();
        assert_eq!(names, ["umicraft/logo", "umicraft/settings"]);
        assert!(images
            .iter()
            .all(|image| image.pixels.len() == image.width as usize * image.height as usize * 4));
    }

    #[test]
    fn languages_are_found_by_code() {
        assert!(lang("en_US").is_some_and(|text| text.contains("umicraft.title.play=")));
        assert!(lang("fr_FR").is_some());
        assert_eq!(lang("xx_XX"), None);
    }

    #[test]
    fn the_brand_font_is_truetype() {
        assert_eq!(&brand_font_ttf()[..4], &[0, 1, 0, 0]);
    }

    #[test]
    fn every_translation_has_the_keys_of_english() {
        let keys = |text: &str| -> Vec<String> {
            let mut keys: Vec<String> = text
                .lines()
                .filter_map(|line| line.split_once('='))
                .map(|(key, _)| key.to_string())
                .collect();
            keys.sort();
            keys
        };
        let english = keys(lang("en_US").expect("en_US"));
        assert_eq!(keys(lang("fr_FR").expect("fr_FR")), english);
    }
}
