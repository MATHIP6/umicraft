// SPDX-License-Identifier: MIT OR Apache-2.0

//! The six title-screen panorama images (`GuiMainMenu.titlePanoramaPaths`).
//!
//! They are `assets/minecraft/textures/gui/title/background/panorama_0.png`
//! to `panorama_5.png` inside the client jar the user downloaded. They are
//! Mojang assets (owner-approved use, ADR-0042): this module only decodes
//! them at runtime from the verified cache and never writes them anywhere;
//! they must never be committed or shipped (spec §3, `docs/sources.md`).

use crate::client_jar::ClientJar;
use crate::client_resources::DecodedPng;
use crate::error::{image_error, AssetsResult};

/// Number of faces of the panorama cube.
pub const FACE_COUNT: usize = 6;

/// The jar path of face `index`.
#[must_use]
pub fn face_path(index: usize) -> String {
    format!("assets/minecraft/textures/gui/title/background/panorama_{index}.png")
}

/// One decoded face.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanoramaFace {
    /// Edge length in pixels (the faces are square).
    pub size: u32,
    /// Tightly packed RGBA8 pixels, row-major, top to bottom.
    pub pixels: Vec<u8>,
}

/// The six faces in file order (`panorama_0` first).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanoramaImages {
    /// The faces; all share one size.
    pub faces: [PanoramaFace; FACE_COUNT],
}

impl PanoramaImages {
    /// Reads and decodes the six faces from `jar`.
    ///
    /// # Errors
    ///
    /// [`crate::AssetsError::Missing`] when a face is not in the jar,
    /// [`crate::AssetsError::Io`] / [`crate::AssetsError::Zip`] when the jar
    /// cannot be read and [`crate::AssetsError::Image`] when a face is not a
    /// decodable square PNG or the faces differ in size.
    pub fn load(jar: &ClientJar) -> AssetsResult<Self> {
        Self::from_reader(|path| jar.read_entry(path))
    }

    /// Decodes the six faces served by `read`, which maps a jar path to the
    /// bytes of that entry. [`Self::load`] is this with a jar.
    ///
    /// # Errors
    ///
    /// The error of `read`, or [`crate::AssetsError::Image`] as in
    /// [`Self::load`].
    pub fn from_reader(mut read: impl FnMut(&str) -> AssetsResult<Vec<u8>>) -> AssetsResult<Self> {
        let mut faces = Vec::with_capacity(FACE_COUNT);
        for index in 0..FACE_COUNT {
            let path = face_path(index);
            let png = DecodedPng::decode(&path, &read(&path)?)?;
            if png.width != png.height {
                return Err(image_error(&path, "a panorama face must be square"));
            }
            faces.push(PanoramaFace {
                size: png.width,
                pixels: png.pixels,
            });
        }
        let size = faces[0].size;
        if let Some(index) = faces.iter().position(|face| face.size != size) {
            return Err(image_error(
                &face_path(index),
                format!("the face is not {size} pixels wide like panorama_0"),
            ));
        }
        let faces = faces
            .try_into()
            .map_err(|_| image_error("panorama", "the panorama needs six faces"))?;
        Ok(Self { faces })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AssetsError;

    fn png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");
        let pixels: Vec<u8> = rgba.repeat((width * height) as usize);
        writer.write_image_data(&pixels).expect("data");
        writer.finish().expect("finish");
        out
    }

    #[test]
    fn six_square_faces_decode_in_order() {
        let images = PanoramaImages::from_reader(|path| {
            let index = path.as_bytes()[path.len() - 5] - b'0';
            Ok(png(4, 4, [index, 0, 0, 255]))
        })
        .expect("decodes");
        for (index, face) in images.faces.iter().enumerate() {
            assert_eq!(face.size, 4);
            assert_eq!(face.pixels[0] as usize, index);
            assert_eq!(face.pixels.len(), 4 * 4 * 4);
        }
    }

    #[test]
    fn the_paths_are_the_vanilla_ones() {
        assert_eq!(
            face_path(5),
            "assets/minecraft/textures/gui/title/background/panorama_5.png"
        );
    }

    #[test]
    fn a_missing_face_is_reported() {
        let error = PanoramaImages::from_reader(|path| {
            Err(AssetsError::Missing {
                what: path.to_string(),
                path: "jar".to_string(),
            })
        })
        .expect_err("missing");
        assert!(matches!(error, AssetsError::Missing { .. }));
    }

    #[test]
    fn a_non_square_or_mixed_size_face_is_rejected() {
        let wide = PanoramaImages::from_reader(|_| Ok(png(4, 2, [0; 4])));
        assert!(matches!(wide, Err(AssetsError::Image { .. })));
        let mixed = PanoramaImages::from_reader(|path| {
            let size = if path.ends_with("3.png") { 8 } else { 4 };
            Ok(png(size, size, [0; 4]))
        });
        assert!(
            matches!(mixed, Err(AssetsError::Image { path, .. }) if path.ends_with("panorama_3.png"))
        );
    }

    #[test]
    fn garbage_is_an_image_error() {
        let result = PanoramaImages::from_reader(|_| Ok(vec![1, 2, 3]));
        assert!(matches!(result, Err(AssetsError::Image { .. })));
    }
}
