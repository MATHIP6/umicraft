// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bulk read of the blockstate, model and texture files of a verified client
//! jar.
//!
//! The jar is read once into a virtual resource map; the model parser and the
//! atlas builder then work purely in memory. This avoids re-reading and
//! re-inflating the 8 MB jar for every one of the ~2,000 entries.
//!
//! The map keeps the complete virtual path (`assets/minecraft/...`) because
//! that is what [`ResourceSource`] expects. The atlas uses the shorter texture
//! key relative to `textures/` (`blocks/stone`, `items/apple`) because that is
//! what the baked block models reference.

use std::collections::BTreeMap;

use umicraft_model::{AtlasLayout, ModelError, ModelResult, ResourceSource};

use crate::atlas::BlockAtlas;
use crate::client_jar::ClientJar;
use crate::{AssetsError, AssetsResult};

/// Prefix of the 1.8 blockstate documents inside the client jar.
const BLOCKSTATES_PREFIX: &str = "assets/minecraft/blockstates/";
/// Prefix of the 1.8 block model documents inside the client jar.
const MODELS_PREFIX: &str = "assets/minecraft/models/block/";
/// Prefix of the 1.8 block textures inside the client jar.
const BLOCK_TEXTURES_PREFIX: &str = "assets/minecraft/textures/blocks/";
/// Prefix of the 1.8 item textures inside the client jar.
const ITEM_TEXTURES_PREFIX: &str = "assets/minecraft/textures/items/";
/// Prefix shared by every texture key.
const TEXTURES_ROOT: &str = "assets/minecraft/textures/";

/// Edge length of a normal (non-animated) 1.8 texture.
///
/// Used as the atlas frame size when a synthetic source contains only animated
/// strips and therefore has no square image to infer a size from.
const DEFAULT_FRAME_SIZE: u32 = 16;

/// One decoded PNG before it is cropped and copied into the atlas.
#[derive(Debug)]
struct DecodedPng {
    /// Pixel width of the full image.
    width: u32,
    /// Pixel height of the full image (larger than `width` for animation strips).
    height: u32,
    /// Tightly packed RGBA8 pixels of the full image.
    pixels: Vec<u8>,
}

impl DecodedPng {
    /// Decodes `bytes` to RGBA8, expanding palette, grayscale and 16-bit PNGs.
    fn decode(path: &str, bytes: &[u8]) -> AssetsResult<Self> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        // EXPAND turns palette and low-bit-depth images into 8-bit samples;
        // STRIP_16 narrows 16-bit samples; ALPHA adds an opaque alpha channel
        // when the PNG has none. Together they guarantee RGBA or gray+alpha.
        decoder.set_transformations(
            png::Transformations::EXPAND
                | png::Transformations::STRIP_16
                | png::Transformations::ALPHA,
        );
        let mut reader = decoder
            .read_info()
            .map_err(|error| image_error(path, format!("invalid PNG header: {error}")))?;
        let mut buffer = vec![0_u8; reader.output_buffer_size()];
        let info = reader
            .next_frame(&mut buffer)
            .map_err(|error| image_error(path, format!("invalid PNG data: {error}")))?;

        if info.width == 0 || info.height == 0 {
            return Err(image_error(path, "the image has a zero dimension"));
        }
        if info.bit_depth != png::BitDepth::Eight {
            return Err(image_error(
                path,
                format!("unsupported bit depth {:?}", info.bit_depth),
            ));
        }

        let data = buffer
            .get(..info.buffer_size())
            .ok_or_else(|| image_error(path, "the decoder returned a short buffer"))?;
        let pixels = match info.color_type {
            png::ColorType::Rgba => data.to_vec(),
            png::ColorType::GrayscaleAlpha => gray_alpha_to_rgba(data),
            png::ColorType::Rgb => rgb_to_rgba(data),
            png::ColorType::Grayscale => gray_to_rgba(data),
            png::ColorType::Indexed => {
                return Err(image_error(path, "the indexed image was not expanded"))
            }
        };
        let expected = pixel_bytes(info.width, info.height)
            .ok_or_else(|| image_error(path, "the image size overflows"))?;
        if pixels.len() != expected {
            return Err(image_error(
                path,
                format!(
                    "the decoder returned {} bytes for a {}x{} RGBA image",
                    pixels.len(),
                    info.width,
                    info.height
                ),
            ));
        }
        Ok(Self {
            width: info.width,
            height: info.height,
            pixels,
        })
    }

    /// True when the image is a single square frame.
    fn is_square(&self) -> bool {
        self.width == self.height
    }

    /// Returns the first `frame_width`×`frame_height` frame as RGBA8.
    ///
    /// 1.8 animation strips are vertically stacked square frames; M5 uses the
    /// static first frame. Every other non-frame-sized image is rejected so a
    /// mixed-resolution source fails loudly instead of producing a corrupt
    /// atlas (see ADR-0007).
    fn first_frame(
        &self,
        frame_width: u32,
        frame_height: u32,
        path: &str,
    ) -> AssetsResult<Vec<u8>> {
        if self.width == frame_width && self.height == frame_height {
            return Ok(self.pixels.clone());
        }

        let is_animation_strip = self.height > self.width
            && self.width >= frame_width
            && self.height >= frame_height
            && self.width.is_multiple_of(frame_width)
            && self.height.is_multiple_of(frame_height)
            && self.height.is_multiple_of(self.width);
        if !is_animation_strip {
            return Err(image_error(
                path,
                format!(
                    "expected a {frame_width}x{frame_height} frame or an animation strip, \
                     got {}x{}",
                    self.width, self.height
                ),
            ));
        }

        let row_bytes = pixel_bytes(frame_width, 1)
            .ok_or_else(|| image_error(path, "the frame width overflows"))?;
        let source_stride = pixel_bytes(self.width, 1)
            .ok_or_else(|| image_error(path, "the image width overflows"))?;
        let frame_len = row_bytes
            .checked_mul(usize::try_from(frame_height).unwrap_or(usize::MAX))
            .ok_or_else(|| image_error(path, "the frame size overflows"))?;
        let mut frame = Vec::with_capacity(frame_len);
        for row in 0..frame_height {
            let start = source_stride
                .checked_mul(usize::try_from(row).unwrap_or(usize::MAX))
                .ok_or_else(|| image_error(path, "the animation frame offset overflows"))?;
            let end = start
                .checked_add(row_bytes)
                .ok_or_else(|| image_error(path, "the animation frame offset overflows"))?;
            let row = self
                .pixels
                .get(start..end)
                .ok_or_else(|| image_error(path, "the animation frame is truncated"))?;
            frame.extend_from_slice(row);
        }
        Ok(frame)
    }
}

/// One texture key and its decoded pixels, in jar path order.
#[derive(Debug)]
struct TextureLayer {
    /// Texture key relative to `textures/`, e.g. `blocks/stone`.
    key: String,
    /// Virtual jar path, used in error messages.
    path: String,
    /// Decoded full image.
    image: DecodedPng,
}

/// Every blockstate, block model and block/item texture of one client jar.
#[derive(Debug, Clone)]
pub struct ClientResources {
    files: BTreeMap<String, Vec<u8>>,
    atlas: BlockAtlas,
}

impl ClientResources {
    /// Reads the M5 subset of `jar` and builds its texture array.
    ///
    /// The bulk read opens and reads the archive exactly once. The resulting
    /// map contains every blockstate and block model JSON, every block and item
    /// PNG, and the `.mcmeta` animation metadata (the metadata is exposed as a
    /// resource but does not add an atlas layer). Entries outside those
    /// prefixes or extensions are ignored.
    ///
    /// # Errors
    ///
    /// [`AssetsError::Io`] or [`AssetsError::Zip`] when the jar cannot be read,
    /// and [`AssetsError::Image`] when a texture is not a decodable PNG or does
    /// not fit the atlas frame size.
    pub fn load(jar: &ClientJar) -> AssetsResult<Self> {
        let entries = jar.read_prefixes(&[
            BLOCKSTATES_PREFIX,
            MODELS_PREFIX,
            BLOCK_TEXTURES_PREFIX,
            ITEM_TEXTURES_PREFIX,
        ])?;

        let mut files = BTreeMap::new();
        let mut textures = Vec::new();
        for (path, bytes) in entries {
            if is_json_resource(&path) {
                files.insert(path, bytes);
            } else if is_texture_metadata(&path) {
                // Kept for the future animation pass; it is not a layer.
                files.insert(path, bytes);
            } else if let Some(key) = texture_key(&path) {
                let image = DecodedPng::decode(&path, &bytes)?;
                files.insert(path.clone(), bytes);
                textures.push(TextureLayer { key, path, image });
            }
        }

        let atlas = build_atlas(textures)?;
        Ok(Self { files, atlas })
    }

    /// Reads one virtual resource path.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }

    /// Number of loaded files.
    ///
    /// This includes the texture `.mcmeta` entries, which are counted as
    /// texture entries but are not atlas layers.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// The decoded texture array.
    #[must_use]
    pub fn atlas(&self) -> &BlockAtlas {
        &self.atlas
    }
}

impl ResourceSource for ClientResources {
    fn read(&self, path: &str) -> ModelResult<Vec<u8>> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| ModelError::Resource {
                path: path.to_string(),
                message: "the verified client jar does not contain this entry".to_string(),
            })
    }

    fn list(&self, prefix: &str) -> Vec<String> {
        self.files
            .keys()
            .filter(|path| path.starts_with(prefix))
            .cloned()
            .collect()
    }
}

/// True for a blockstate or block-model JSON file.
fn is_json_resource(path: &str) -> bool {
    path.ends_with(".json")
        && (path.starts_with(BLOCKSTATES_PREFIX) || path.starts_with(MODELS_PREFIX))
}

/// True for a texture animation metadata file.
fn is_texture_metadata(path: &str) -> bool {
    path.ends_with(".mcmeta")
        && (path.starts_with(BLOCK_TEXTURES_PREFIX) || path.starts_with(ITEM_TEXTURES_PREFIX))
}

/// The atlas key of a block or item PNG, or `None` for any other path.
fn texture_key(path: &str) -> Option<String> {
    let relative = path.strip_prefix(TEXTURES_ROOT)?.strip_suffix(".png")?;
    if relative.is_empty() {
        return None;
    }
    Some(relative.to_string())
}

/// Builds the deterministic atlas from decoded textures.
fn build_atlas(textures: Vec<TextureLayer>) -> AssetsResult<BlockAtlas> {
    let (frame_width, frame_height) = frame_size(&textures);
    let mut layers = BTreeMap::new();
    for texture in textures {
        let frame = texture
            .image
            .first_frame(frame_width, frame_height, &texture.path)?;
        layers.insert(texture.key, frame);
    }

    let layout = AtlasLayout::from_keys(layers.keys().cloned());
    let layer_len = pixel_bytes(frame_width, frame_height)
        .ok_or_else(|| image_error("atlas", "the atlas frame size overflows"))?;
    let mut pixels = Vec::with_capacity(
        layer_len
            .checked_mul(usize::try_from(layout.len()).unwrap_or(usize::MAX))
            .ok_or_else(|| image_error("atlas", "the atlas pixel count overflows"))?,
    );
    pixels.extend_from_slice(&missing_checker(frame_width, frame_height));
    for key in layout.keys().iter().skip(1) {
        let frame = layers
            .remove(key)
            .ok_or_else(|| image_error(key, "the texture is missing from the decoded set"))?;
        pixels.extend_from_slice(&frame);
    }

    BlockAtlas::new(frame_width, frame_height, pixels, layout)
}

/// Picks the atlas frame size from the most common square texture, preferring
/// the smaller size on a tie. Tall animation strips do not vote because their
/// first frame is cropped to this size.
fn frame_size(textures: &[TextureLayer]) -> (u32, u32) {
    let mut counts: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for texture in textures {
        if texture.image.is_square() {
            *counts
                .entry((texture.image.width, texture.image.height))
                .or_default() += 1;
        }
    }

    let mut best: Option<((u32, u32), usize)> = None;
    for (size, count) in counts {
        let replace = match best {
            None => true,
            Some((best_size, best_count)) => {
                count > best_count || (count == best_count && size.0 < best_size.0)
            }
        };
        if replace {
            best = Some((size, count));
        }
    }
    if let Some((size, _)) = best {
        return size;
    }

    textures
        .iter()
        .find(|texture| texture.image.width > 0)
        .map_or((DEFAULT_FRAME_SIZE, DEFAULT_FRAME_SIZE), |texture| {
            (texture.image.width, texture.image.width)
        })
}

/// The original Umicraft missing-texture layer: an opaque checker that is not
/// derived from any Mojang art.
fn missing_checker(width: u32, height: u32) -> Vec<u8> {
    const LIGHT: [u8; 4] = [232, 96, 160, 255];
    const DARK: [u8; 4] = [36, 28, 44, 255];

    let capacity = pixel_bytes(width, height).unwrap_or(0);
    let mut pixels = Vec::with_capacity(capacity);
    for y in 0..height {
        for x in 0..width {
            let color = if (x + y) % 2 == 0 { LIGHT } else { DARK };
            pixels.extend_from_slice(&color);
        }
    }
    pixels
}

/// Byte length of `width`×`height` RGBA8 pixels, or `None` on overflow.
fn pixel_bytes(width: u32, height: u32) -> Option<usize> {
    usize::try_from(
        u64::from(width)
            .checked_mul(u64::from(height))?
            .checked_mul(4)?,
    )
    .ok()
}

/// Expands two-channel gray+alpha pixels to RGBA8.
fn gray_alpha_to_rgba(pixels: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(pixels.len() * 2);
    for pixel in pixels.chunks_exact(2) {
        rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
    }
    rgba
}

/// Expands three-channel RGB pixels to opaque RGBA8.
fn rgb_to_rgba(pixels: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(pixels.len() / 3 * 4);
    for pixel in pixels.chunks_exact(3) {
        rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
    }
    rgba
}

/// Expands one-channel grayscale pixels to opaque RGBA8.
fn gray_to_rgba(pixels: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for &gray in pixels {
        rgba.extend_from_slice(&[gray, gray, gray, 255]);
    }
    rgba
}

/// Builds an [`AssetsError::Image`] with a consistent diagnostic.
fn image_error(path: &str, reason: impl Into<String>) -> AssetsError {
    AssetsError::Image {
        path: path.to_string(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_keys_are_relative_to_the_texture_root() {
        assert_eq!(
            texture_key("assets/minecraft/textures/blocks/stone.png").as_deref(),
            Some("blocks/stone")
        );
        assert_eq!(
            texture_key("assets/minecraft/textures/items/apple.png").as_deref(),
            Some("items/apple")
        );
        assert_eq!(
            texture_key("assets/minecraft/textures/blocks/stone.png.mcmeta"),
            None
        );
        assert_eq!(
            texture_key("assets/minecraft/models/block/stone.json"),
            None
        );
    }

    #[test]
    fn only_the_m5_document_prefixes_are_resources() {
        assert!(is_json_resource("assets/minecraft/blockstates/stone.json"));
        assert!(is_json_resource("assets/minecraft/models/block/stone.json"));
        assert!(!is_json_resource("assets/minecraft/models/item/stone.json"));
        assert!(is_texture_metadata(
            "assets/minecraft/textures/blocks/water_still.png.mcmeta"
        ));
        assert!(!is_texture_metadata(
            "assets/minecraft/textures/blocks/water_still.png"
        ));
    }

    #[test]
    fn the_missing_layer_is_an_opaque_checker() {
        let pixels = missing_checker(2, 2);
        assert_eq!(pixels.len(), 16);
        let texel = |x: usize, y: usize| &pixels[(y * 2 + x) * 4..(y * 2 + x + 1) * 4];
        assert_eq!(texel(0, 0)[3], 255);
        assert_eq!(texel(1, 0)[3], 255);
        assert_ne!(texel(0, 0), texel(1, 0));
        assert_eq!(texel(0, 0), texel(1, 1));
    }

    #[test]
    fn the_frame_size_prefers_the_most_common_square() {
        let texture = |width, height| TextureLayer {
            key: String::new(),
            path: String::new(),
            image: DecodedPng {
                width,
                height,
                pixels: Vec::new(),
            },
        };
        let textures = vec![
            texture(32, 32),
            texture(16, 512),
            texture(16, 16),
            texture(16, 16),
        ];
        assert_eq!(frame_size(&textures), (16, 16));
    }

    #[test]
    fn a_tied_frame_size_prefers_the_smaller_edge() {
        let texture = |width| TextureLayer {
            key: String::new(),
            path: String::new(),
            image: DecodedPng {
                width,
                height: width,
                pixels: Vec::new(),
            },
        };
        assert_eq!(frame_size(&[texture(4), texture(2)]), (2, 2));
    }
}
