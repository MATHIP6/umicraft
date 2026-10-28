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
//!
//! Every atlas layer is normalised to the largest frame edge found in the jar.
//! In 1.8.8 that is 32 pixels because the flowing water and lava strips have
//! 32×32 frames, so all 16×16 textures are upscaled with nearest-neighbour
//! sampling rather than cropping the larger animation frames.

use std::collections::BTreeMap;

use umicraft_model::{AtlasLayout, ModelError, ModelResult, ResourceSource};

use crate::animation::{AnimationMeta, TextureAnimation};
use crate::atlas::BlockAtlas;
use crate::client_jar::ClientJar;
use crate::colormap::{BiomeColormaps, Colormap, COLORMAP_PREFIX, FOLIAGE_PATH, GRASS_PATH};
use crate::error::image_error;
use crate::gui_atlas::{pack_gui_atlas, GuiAtlas, GuiImage};
use crate::AssetsResult;

/// Prefix of the 1.8 blockstate documents inside the client jar.
const BLOCKSTATES_PREFIX: &str = "assets/minecraft/blockstates/";
/// Prefix of the 1.8 block and item model documents inside the client jar.
const MODELS_PREFIX: &str = "assets/minecraft/models/";
/// Prefix of the 1.8 block textures inside the client jar.
const BLOCK_TEXTURES_PREFIX: &str = "assets/minecraft/textures/blocks/";
/// Prefix of the 1.8 item textures inside the client jar.
const ITEM_TEXTURES_PREFIX: &str = "assets/minecraft/textures/items/";
/// Prefix of the 1.8 GUI textures (backgrounds, widgets, icons).
const GUI_TEXTURES_PREFIX: &str = "assets/minecraft/textures/gui/";
/// The enchantment glint texture, drawn additively over enchanted items.
const GLINT_PATH: &str = "assets/minecraft/textures/misc/enchanted_item_glint.png";
/// Prefix shared by every texture key.
const TEXTURES_ROOT: &str = "assets/minecraft/textures/";

/// Edge length of an atlas with no textures at all.
///
/// A normal source derives the edge from its largest frame (32 for 1.8.8).
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

    /// The edge of one square frame in this image.
    ///
    /// A square PNG is a single frame. A tall PNG is an animation strip made of
    /// vertically stacked square frames, so its height must be an exact
    /// multiple of its width. Anything else is not a 1.8 texture and is
    /// rejected instead of being silently misread.
    fn frame_edge(&self, path: &str) -> AssetsResult<u32> {
        if self.width == self.height
            || (self.height > self.width && self.height.is_multiple_of(self.width))
        {
            return Ok(self.width);
        }
        Err(image_error(
            path,
            format!(
                "expected a square image or a vertical animation strip, got {}x{}",
                self.width, self.height
            ),
        ))
    }

    /// How many square frames the image holds (1 unless it is an animation strip).
    fn frame_count(&self, path: &str) -> AssetsResult<u32> {
        Ok(self.height / self.frame_edge(path)?)
    }

    /// Returns frame number `index` scaled to `edge`×`edge` with
    /// nearest-neighbour sampling.
    ///
    /// A square image has the single frame 0. An animation strip stacks
    /// `width`×`width` frames, so a 16×512 strip yields 32 frames of 16×16 and a
    /// 32×1024 strip yields 32 full 32×32 frames. The frame is then replicated
    /// or sampled to the atlas edge; integer scale factors preserve every
    /// source pixel and its alpha exactly.
    fn frame_at(&self, index: u32, edge: u32, path: &str) -> AssetsResult<Vec<u8>> {
        let frame_edge = self.frame_edge(path)?;
        let source_edge = usize::try_from(frame_edge)
            .map_err(|_| image_error(path, "the frame size does not fit into memory"))?;
        let atlas_edge = usize::try_from(edge)
            .map_err(|_| image_error(path, "the atlas size does not fit into memory"))?;
        if atlas_edge == 0 {
            return Err(image_error(path, "the atlas frame is empty"));
        }

        let source_stride = pixel_bytes(self.width, 1)
            .ok_or_else(|| image_error(path, "the image width overflows"))?;
        let frame_row_bytes = pixel_bytes(frame_edge, 1)
            .ok_or_else(|| image_error(path, "the frame width overflows"))?;
        let frame_len = pixel_bytes(frame_edge, frame_edge)
            .ok_or_else(|| image_error(path, "the frame size overflows"))?;
        let mut frame = Vec::with_capacity(frame_len);
        let first_row = index
            .checked_mul(frame_edge)
            .ok_or_else(|| image_error(path, "the frame row overflows"))?;
        for row in first_row..first_row.saturating_add(frame_edge) {
            let start = source_stride
                .checked_mul(
                    usize::try_from(row)
                        .map_err(|_| image_error(path, "the frame row overflows"))?,
                )
                .ok_or_else(|| image_error(path, "the frame offset overflows"))?;
            let end = start
                .checked_add(frame_row_bytes)
                .ok_or_else(|| image_error(path, "the frame offset overflows"))?;
            let row = self
                .pixels
                .get(start..end)
                .ok_or_else(|| image_error(path, "the animation frame is truncated"))?;
            frame.extend_from_slice(row);
        }

        if frame_edge == edge {
            return Ok(frame);
        }

        let scaled_len = pixel_bytes(edge, edge)
            .ok_or_else(|| image_error(path, "the scaled frame size overflows"))?;
        let mut scaled = Vec::with_capacity(scaled_len);
        for y in 0..atlas_edge {
            let source_y = y
                .checked_mul(source_edge)
                .ok_or_else(|| image_error(path, "the source row overflows"))?
                / atlas_edge;
            for x in 0..atlas_edge {
                let source_x = x
                    .checked_mul(source_edge)
                    .ok_or_else(|| image_error(path, "the source column overflows"))?
                    / atlas_edge;
                let index = (source_y * source_edge + source_x) * 4;
                let pixel = frame
                    .get(index..index + 4)
                    .ok_or_else(|| image_error(path, "the scaled frame is truncated"))?;
                scaled.extend_from_slice(pixel);
            }
        }
        Ok(scaled)
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

/// The decoded enchantment glint texture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlintTexture {
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Tightly packed RGBA8 pixels, `width * height * 4` bytes.
    pub pixels: Vec<u8>,
}

/// Every blockstate, block model and block/item texture of one client jar,
/// plus the optional biome colormaps and the GUI texture atlas.
#[derive(Debug, Clone)]
pub struct ClientResources {
    files: BTreeMap<String, Vec<u8>>,
    raw_colormaps: BTreeMap<String, Vec<u8>>,
    raw_gui: BTreeMap<String, Vec<u8>>,
    raw_glint: Option<Vec<u8>>,
    atlas: BlockAtlas,
    colormaps: Option<BiomeColormaps>,
    gui_atlas: GuiAtlas,
    glint: Option<GlintTexture>,
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
    /// The two 256×256 biome colormaps (`colormap/grass.png`,
    /// `colormap/foliage.png`) are decoded into [`Self::colormaps`] and are
    /// **not** added to the block atlas: the renderer multiplies them in the
    /// shader, it never samples them as a texture layer.
    ///
    /// # Errors
    ///
    /// [`crate::AssetsError::Io`] or [`crate::AssetsError::Zip`] when the jar cannot be read,
    /// and [`crate::AssetsError::Image`] when a texture or a present colormap is not a
    /// decodable PNG, or when the colormap is not 256×256.
    pub fn load(jar: &ClientJar) -> AssetsResult<Self> {
        let entries = jar.read_prefixes(&[
            BLOCKSTATES_PREFIX,
            MODELS_PREFIX,
            BLOCK_TEXTURES_PREFIX,
            ITEM_TEXTURES_PREFIX,
            COLORMAP_PREFIX,
            GUI_TEXTURES_PREFIX,
            GLINT_PATH,
        ])?;

        let mut files = BTreeMap::new();
        let mut raw_colormaps = BTreeMap::new();
        let mut raw_gui = BTreeMap::new();
        let mut raw_glint = None;
        for (path, bytes) in entries {
            if path == GLINT_PATH {
                raw_glint = Some(bytes);
            } else if path == GRASS_PATH || path == FOLIAGE_PATH {
                raw_colormaps.insert(path, bytes);
            } else if is_gui_texture(&path) {
                raw_gui.insert(path, bytes);
            } else if is_json_resource(&path)
                || is_texture_metadata(&path)
                || texture_key(&path).is_some()
            {
                files.insert(path, bytes);
            }
        }

        Self::build_internal(files, raw_colormaps, raw_gui, raw_glint)
    }

    /// Builds a new [`ClientResources`] instance by layering resource packs
    /// over this base instance.
    ///
    /// Files present in packs override matching files from this instance.
    /// The atlas and colormaps are re-generated accordingly.
    ///
    /// # Errors
    ///
    /// [`crate::AssetsError::Image`] if an overridden texture or colormap is invalid.
    pub fn apply_packs(&self, packs: &[crate::resource_pack::ResourcePack]) -> AssetsResult<Self> {
        let mut files = self.files.clone();
        let mut raw_colormaps = self.raw_colormaps.clone();
        let mut raw_gui = self.raw_gui.clone();
        let mut raw_glint = self.raw_glint.clone();
        for pack in packs {
            for (path, bytes) in pack.files() {
                if path == GLINT_PATH {
                    raw_glint = Some(bytes.clone());
                } else if path == GRASS_PATH || path == FOLIAGE_PATH {
                    raw_colormaps.insert(path.clone(), bytes.clone());
                } else if is_gui_texture(path) {
                    raw_gui.insert(path.clone(), bytes.clone());
                } else if is_json_resource(path)
                    || is_texture_metadata(path)
                    || texture_key(path).is_some()
                {
                    files.insert(path.clone(), bytes.clone());
                }
            }
        }
        Self::build_internal(files, raw_colormaps, raw_gui, raw_glint)
    }

    /// Internal builder that constructs a complete [`ClientResources`].
    fn build_internal(
        files: BTreeMap<String, Vec<u8>>,
        raw_colormaps: BTreeMap<String, Vec<u8>>,
        raw_gui: BTreeMap<String, Vec<u8>>,
        raw_glint: Option<Vec<u8>>,
    ) -> AssetsResult<Self> {
        let mut textures = Vec::new();
        for (path, bytes) in &files {
            if let Some(key) = texture_key(path) {
                let image = DecodedPng::decode(path, bytes)?;
                textures.push(TextureLayer {
                    key,
                    path: path.clone(),
                    image,
                });
            }
        }

        let mut decoded_colormaps = BTreeMap::new();
        for (path, bytes) in &raw_colormaps {
            decoded_colormaps.insert(path.clone(), DecodedPng::decode(path, bytes)?);
        }

        let gui_images = decode_gui_textures(&raw_gui)?;
        let atlas = build_atlas(textures, &files)?;
        let colormaps = decode_colormaps(&mut decoded_colormaps)?;
        let gui_atlas = pack_gui_atlas(gui_images);
        let glint = raw_glint
            .as_deref()
            .map(|bytes| DecodedPng::decode(GLINT_PATH, bytes))
            .transpose()?
            .map(|png| GlintTexture {
                width: png.width,
                height: png.height,
                pixels: png.pixels,
            });
        Ok(Self {
            files,
            raw_colormaps,
            raw_gui,
            raw_glint,
            atlas,
            colormaps,
            gui_atlas,
            glint,
        })
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

    /// The GUI texture atlas built from `textures/gui/` entries in the jar.
    #[must_use]
    pub fn gui_atlas(&self) -> &GuiAtlas {
        &self.gui_atlas
    }

    /// The enchantment glint texture, when the jar or a pack provides it.
    ///
    /// `None` is not an error: enchanted items then simply draw without the
    /// glint overlay.
    #[must_use]
    pub fn glint_texture(&self) -> Option<&GlintTexture> {
        self.glint.as_ref()
    }

    /// The grass and foliage colormaps, when the jar contains both.
    ///
    /// `None` is not an error: the mesher then leaves every face untinted
    /// (white), which keeps custom or synthetic installations renderable.
    #[must_use]
    pub fn colormaps(&self) -> Option<&BiomeColormaps> {
        self.colormaps.as_ref()
    }
}

/// Builds the optional colormap pair from the decoded jar entries.
///
/// Both maps are required together: `BiomeTints` samples grass and foliage
/// from the same climate table, and partial tinting would silently diverge
/// from vanilla. A malformed map is an error; a missing one disables tinting.
fn decode_colormaps(
    decoded: &mut BTreeMap<String, DecodedPng>,
) -> AssetsResult<Option<BiomeColormaps>> {
    let (Some(grass), Some(foliage)) = (decoded.remove(GRASS_PATH), decoded.remove(FOLIAGE_PATH))
    else {
        return Ok(None);
    };
    let grass = Colormap::from_rgba8(GRASS_PATH, grass.width, grass.height, &grass.pixels)?;
    let foliage =
        Colormap::from_rgba8(FOLIAGE_PATH, foliage.width, foliage.height, &foliage.pixels)?;
    Ok(Some(BiomeColormaps { grass, foliage }))
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
    if !path.starts_with(BLOCK_TEXTURES_PREFIX) && !path.starts_with(ITEM_TEXTURES_PREFIX) {
        return None;
    }
    let relative = path.strip_prefix(TEXTURES_ROOT)?.strip_suffix(".png")?;
    if relative.is_empty() {
        return None;
    }
    Some(relative.to_string())
}

/// True for a GUI texture PNG (container backgrounds, widgets, icons).
fn is_gui_texture(path: &str) -> bool {
    path.starts_with(GUI_TEXTURES_PREFIX) && path.ends_with(".png")
}

/// The region name for a GUI texture path, e.g.
/// `"assets/minecraft/textures/gui/container/inventory.png"` → `"container/inventory"`.
fn gui_texture_name(path: &str) -> Option<String> {
    let relative = path
        .strip_prefix(GUI_TEXTURES_PREFIX)?
        .strip_suffix(".png")?;
    if relative.is_empty() {
        return None;
    }
    Some(relative.to_string())
}

/// Decodes all GUI PNGs into images ready for atlas packing.
fn decode_gui_textures(raw: &BTreeMap<String, Vec<u8>>) -> AssetsResult<Vec<GuiImage>> {
    let mut images = Vec::with_capacity(raw.len());
    for (path, bytes) in raw {
        let Some(name) = gui_texture_name(path) else {
            continue;
        };
        let decoded = DecodedPng::decode(path, bytes)?;
        images.push(GuiImage {
            name,
            width: decoded.width,
            height: decoded.height,
            pixels: decoded.pixels,
        });
    }
    Ok(images)
}

/// Builds the deterministic atlas from decoded textures.
///
/// A texture whose `.mcmeta` describes an animation keeps its first frame in
/// the layer and hands all its frames to a [`TextureAnimation`]; a strip
/// without metadata stays on its first frame, as in the reference.
fn build_atlas(
    textures: Vec<TextureLayer>,
    files: &BTreeMap<String, Vec<u8>>,
) -> AssetsResult<BlockAtlas> {
    let edge = atlas_frame_edge(&textures)?;
    let mut layers = BTreeMap::new();
    let mut strips = Vec::new();
    for texture in textures {
        let TextureLayer { key, path, image } = texture;
        let count = image.frame_count(&path)?;
        let meta = files
            .get(&format!("{path}.mcmeta"))
            .and_then(|bytes| AnimationMeta::parse(bytes));
        // Item strips are left alone: the clock and the compass carry a
        // `.mcmeta` but the reference drives them by the world time and the
        // heading (`TextureAtlasSpriteClock/Compass`), not by this timeline.
        let animated = key.starts_with("blocks/");
        if let (Some(meta), true, true) = (meta, count > 1, animated) {
            let frames = (0..count)
                .map(|index| image.frame_at(index, edge, &path))
                .collect::<AssetsResult<Vec<_>>>()?;
            layers.insert(key.clone(), frames[0].clone());
            strips.push((key, frames, meta));
        } else {
            layers.insert(key, image.frame_at(0, edge, &path)?);
        }
    }

    let layout = AtlasLayout::from_keys(layers.keys().cloned());
    let layer_len = pixel_bytes(edge, edge)
        .ok_or_else(|| image_error("atlas", "the atlas frame size overflows"))?;
    let mut pixels = Vec::with_capacity(
        layer_len
            .checked_mul(usize::try_from(layout.layer_count()).unwrap_or(usize::MAX))
            .ok_or_else(|| image_error("atlas", "the atlas pixel count overflows"))?,
    );
    pixels.extend_from_slice(&missing_checker(edge, edge));
    for key in layout.keys().iter().skip(1) {
        let frame = layers
            .remove(key)
            .ok_or_else(|| image_error(key, "the texture is missing from the decoded set"))?;
        pixels.extend_from_slice(&frame);
    }

    let animations = strips
        .into_iter()
        .filter_map(|(key, frames, meta)| TextureAnimation::new(layout.layer(&key)?, frames, &meta))
        .collect();
    Ok(BlockAtlas::new(edge, edge, pixels, layout)?.with_animations(animations))
}

/// The atlas edge is the largest frame edge of any texture.
///
/// 1.8.8 contains 32×32 frames (the flowing water and lava strips), so the
/// whole atlas is built at 32×32 and the 16×16 textures are upscaled. Using the
/// most common size instead would crop three quarters of those 32-wide frames.
fn atlas_frame_edge(textures: &[TextureLayer]) -> AssetsResult<u32> {
    let mut edge = 0;
    for texture in textures {
        edge = edge.max(texture.image.frame_edge(&texture.path)?);
    }
    if edge == 0 {
        return Ok(DEFAULT_FRAME_SIZE);
    }
    Ok(edge)
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

/// Builds an [`crate::AssetsError::Image`] with a consistent diagnostic.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::AssetsError;

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
            texture_key("assets/minecraft/textures/gui/container/inventory.png"),
            None
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

    fn png_bytes(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");
        writer.write_image_data(rgba).expect("data");
        writer.finish().expect("finish");
        out
    }

    #[test]
    fn the_glint_texture_is_decoded_when_present_and_optional_otherwise() {
        let bytes = png_bytes(2, 1, &[1, 2, 3, 255, 4, 5, 6, 255]);
        let resources = ClientResources::build_internal(
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            Some(bytes),
        )
        .expect("build");
        let glint = resources.glint_texture().expect("glint");
        assert_eq!((glint.width, glint.height), (2, 1));
        assert_eq!(glint.pixels, [1, 2, 3, 255, 4, 5, 6, 255]);

        let empty = ClientResources::build_internal(
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            None,
        )
        .expect("build");
        assert!(empty.glint_texture().is_none());
    }

    #[test]
    fn a_corrupt_glint_texture_is_an_image_error() {
        let result = ClientResources::build_internal(
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            Some(vec![1, 2, 3]),
        );
        assert!(matches!(result, Err(AssetsError::Image { .. })));
    }

    #[test]
    fn gui_textures_are_recognised_and_named() {
        assert!(is_gui_texture(
            "assets/minecraft/textures/gui/container/inventory.png"
        ));
        assert!(is_gui_texture("assets/minecraft/textures/gui/widgets.png"));
        assert!(!is_gui_texture(
            "assets/minecraft/textures/gui/container/inventory.png.mcmeta"
        ));
        assert!(!is_gui_texture(
            "assets/minecraft/textures/blocks/stone.png"
        ));

        assert_eq!(
            gui_texture_name("assets/minecraft/textures/gui/container/inventory.png").as_deref(),
            Some("container/inventory")
        );
        assert_eq!(
            gui_texture_name("assets/minecraft/textures/gui/widgets.png").as_deref(),
            Some("widgets")
        );
    }

    #[test]
    fn only_the_model_and_blockstate_prefixes_are_resources() {
        assert!(is_json_resource("assets/minecraft/blockstates/stone.json"));
        assert!(is_json_resource("assets/minecraft/models/block/stone.json"));
        assert!(is_json_resource("assets/minecraft/models/item/stone.json"));
        assert!(!is_json_resource("assets/minecraft/lang/en_US.json"));
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

    /// A texture layer with empty pixels, enough for size-only tests.
    fn texture(width: u32, height: u32) -> TextureLayer {
        TextureLayer {
            key: String::new(),
            path: String::new(),
            image: DecodedPng {
                width,
                height,
                pixels: Vec::new(),
            },
        }
    }

    #[test]
    fn the_atlas_edge_is_the_largest_frame_width() {
        let textures = vec![
            texture(32, 32),
            texture(16, 512),
            texture(16, 16),
            texture(16, 16),
        ];
        assert_eq!(
            atlas_frame_edge(&textures).expect("every texture is a valid frame"),
            32
        );
    }

    #[test]
    fn a_wide_or_irregular_image_is_not_a_frame() {
        for (width, height) in [(32, 16), (16, 24), (16, 20)] {
            let err = texture(width, height)
                .image
                .frame_edge("test")
                .expect_err("the image is not a square or an animation strip");
            assert!(matches!(err, AssetsError::Image { .. }));
        }
    }

    #[test]
    fn square_and_vertical_strip_frames_have_their_width_as_edge() {
        assert_eq!(
            texture(16, 16)
                .image
                .frame_edge("square")
                .expect("a square image is a frame"),
            16
        );
        assert_eq!(
            texture(16, 512)
                .image
                .frame_edge("animated")
                .expect("an animation strip is a frame"),
            16
        );
        assert_eq!(
            texture(32, 1024)
                .image
                .frame_edge("wide animation")
                .expect("a 32-wide animation strip is a frame"),
            32
        );
    }
}
