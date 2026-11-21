// SPDX-License-Identifier: MIT OR Apache-2.0

//! Colormap palettes (`CustomColormap`).

use thiserror::Error;

use crate::ctm::{block_id_from_name, parse_match_blocks, BlockMatch};
use crate::Properties;

use super::properties::{parse_color, Rgb};
use super::resolve_path;

/// Why a palette was dropped.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PaletteError {
    /// The `format` value is none of `vanilla`, `grid`, `fixed`.
    #[error("unknown palette format {0:?}")]
    UnknownFormat(String),
    /// The picture has no pixel, or its pixel buffer does not fit its size.
    #[error("invalid palette picture {width}x{height}")]
    InvalidPicture {
        /// The width in pixels.
        width: u32,
        /// The height in pixels.
        height: u32,
    },
}

/// How a palette is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Temperature across, rainfall down, like `grass.png`.
    Vanilla,
    /// Biome id across, height down.
    Grid,
    /// One colour for every block.
    Fixed,
}

impl Format {
    /// The format named `text`.
    ///
    /// # Errors
    ///
    /// [`PaletteError::UnknownFormat`] for any other text.
    pub fn parse(text: &str) -> Result<Self, PaletteError> {
        match text.trim() {
            "vanilla" => Ok(Self::Vanilla),
            "grid" => Ok(Self::Grid),
            "fixed" => Ok(Self::Fixed),
            other => Err(PaletteError::UnknownFormat(other.to_string())),
        }
    }
}

/// What a colormap's `.properties` file (or `palette.block.*` link) says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteSpec {
    /// The lookup.
    pub format: Format,
    /// The blocks that use the palette (`blocks`). `None` when the file names
    /// none and its own name is no block either.
    pub blocks: Option<Vec<BlockMatch>>,
    /// The picture, without extension, relative to the feature root. `None`
    /// for a `fixed` palette.
    pub source: Option<String>,
    /// The colour of a `fixed` palette (`color`); white when absent.
    pub color: Option<Rgb>,
    /// `yVariance`: a position-dependent jitter of the `grid` row, in blocks.
    pub y_variance: i32,
    /// `yOffset`: subtracted from the height in the `grid` format.
    pub y_offset: i32,
}

impl PaletteSpec {
    /// Reads a colormap properties file at the feature-relative `path`.
    ///
    /// `default_format` is `palette.format`. `block_id` names a block
    /// (`minecraft:grass`).
    ///
    /// # Errors
    ///
    /// [`PaletteError::UnknownFormat`] when `format` is not a format name.
    pub fn parse(
        props: &Properties,
        path: &str,
        default_format: Format,
        block_id: &dyn Fn(&str) -> Option<u16>,
    ) -> Result<Self, PaletteError> {
        let format = match props.get("format") {
            Some(text) => Format::parse(text)?,
            None => default_format,
        };
        let (base, file) = path.rsplit_once('/').unwrap_or(("", path));
        let name = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
        let blocks = match props.get("blocks") {
            Some(text) => parse_match_blocks(text, block_id).ok(),
            None => detect_blocks(name, block_id),
        };
        let source = match props.get("source") {
            Some(text) => resolve_path(text.strip_suffix(".png").unwrap_or(text), base),
            None => format!("{base}/{name}").trim_start_matches('/').to_string(),
        };
        Ok(Self {
            format,
            blocks,
            source: (format != Format::Fixed).then_some(source),
            color: props.get("color").and_then(parse_color),
            y_variance: props
                .get("yVariance")
                .and_then(|text| text.parse().ok())
                .unwrap_or(0),
            y_offset: props
                .get("yOffset")
                .and_then(|text| text.parse().ok())
                .unwrap_or(0),
        })
    }

    /// A spec for a bare picture with no properties file.
    #[must_use]
    pub fn bare(source: &str, format: Format) -> Self {
        Self {
            format,
            blocks: None,
            source: Some(source.trim_end_matches(".png").to_string()),
            color: None,
            y_variance: 0,
            y_offset: 0,
        }
    }
}

/// The blocks a colormap file is named after (`detectMatchBlocks`): a block
/// name (`grass`), `block<ID>`, or a `matchBlocks`-style entry.
fn detect_blocks(name: &str, block_id: &dyn Fn(&str) -> Option<u16>) -> Option<Vec<BlockMatch>> {
    if let Some(id) = block_id(&format!("minecraft:{name}")) {
        return Some(vec![BlockMatch { id, metadata: None }]);
    }
    block_id_from_name(name).or_else(|| parse_match_blocks(name, block_id).ok())
}

/// A decoded colormap with its lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    format: Format,
    width: u32,
    height: u32,
    pixels: Vec<Rgb>,
    color: Rgb,
    y_variance: i32,
    y_offset: i32,
}

impl Palette {
    /// A `fixed` palette of one colour.
    #[must_use]
    pub fn fixed(color: Rgb) -> Self {
        Self {
            format: Format::Fixed,
            width: 1,
            height: 1,
            pixels: vec![color],
            color,
            y_variance: 0,
            y_offset: 0,
        }
    }

    /// Builds a palette from `spec` and a decoded RGBA8 picture. A `fixed`
    /// spec ignores the picture.
    ///
    /// # Errors
    ///
    /// [`PaletteError::InvalidPicture`] when the picture is empty or `rgba`
    /// is not `width * height * 4` bytes.
    pub fn from_rgba(
        spec: &PaletteSpec,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<Self, PaletteError> {
        if spec.format == Format::Fixed {
            return Ok(Self::fixed(spec.color.unwrap_or([0xFF; 3])));
        }
        let invalid = PaletteError::InvalidPicture { width, height };
        let texels = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| invalid.clone())?;
        if texels == 0 || texels.checked_mul(4) != Some(rgba.len()) {
            return Err(invalid);
        }
        let pixels: Vec<Rgb> = rgba
            .chunks_exact(4)
            .map(|texel| [texel[0], texel[1], texel[2]])
            .collect();
        let mut palette = Self {
            format: spec.format,
            width,
            height,
            pixels,
            color: [0xFF; 3],
            y_variance: spec.y_variance,
            y_offset: spec.y_offset,
        };
        // The default colour of a picture palette is its plains colour
        // (`CustomColormap.isValid`).
        palette.color = spec.color.unwrap_or_else(|| match spec.format {
            Format::Vanilla => palette.at(127, 127),
            _ => palette.grid(1, 64, [0, 0]),
        });
        Ok(palette)
    }

    /// The lookup of this palette.
    #[must_use]
    pub fn format(&self) -> Format {
        self.format
    }

    /// The width of the picture in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The height of the picture in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Whether every position has the same colour.
    #[must_use]
    pub fn is_constant(&self) -> bool {
        self.format == Format::Fixed
    }

    /// The pixel at `(x, y)`, each clamped into the picture
    /// (`getColor(int, int)`).
    #[must_use]
    pub fn at(&self, x: i64, y: i64) -> Rgb {
        let x = x.clamp(0, i64::from(self.width) - 1) as usize;
        let y = y.clamp(0, i64::from(self.height) - 1) as usize;
        self.pixels[y * self.width as usize + x]
    }

    /// The pixel at a linear index, clamped (`getColor(int)`): used by
    /// the 1-pixel-high palettes (redstone power, stem age).
    #[must_use]
    pub fn at_index(&self, index: usize) -> Rgb {
        self.pixels[index.min(self.pixels.len() - 1)]
    }

    /// The number of pixels, 1 for a fixed palette.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pixels.len()
    }

    /// Never empty: a palette needs at least one pixel.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        false
    }

    /// The colour for a biome with `temperature` and `rainfall`, at block
    /// height `y` and column `(x, z)`. The biome `id` is the column of a
    /// `grid` palette.
    #[must_use]
    pub fn sample(&self, climate: Climate, id: u8, y: i32, column: [i32; 2]) -> Rgb {
        match self.format {
            Format::Vanilla => self.vanilla(climate),
            Format::Grid => self.grid(id, y, column),
            Format::Fixed => self.color,
        }
    }

    /// `getColorVanilla`: temperature across, rainfall times temperature down.
    fn vanilla(&self, climate: Climate) -> Rgb {
        let temperature = f64::from(climate.temperature.clamp(0.0, 1.0));
        let rainfall = f64::from(climate.rainfall.clamp(0.0, 1.0)) * temperature;
        let x = ((1.0 - temperature) * f64::from(self.width - 1)) as i64;
        let y = ((1.0 - rainfall) * f64::from(self.height - 1)) as i64;
        self.at(x, y)
    }

    /// `getColorGrid`: the biome id across, the height down.
    fn grid(&self, id: u8, y: i32, [x, z]: [i32; 2]) -> Rgb {
        let mut row = y - self.y_offset;
        if self.y_variance > 0 {
            // Java shifts by `16 + z` and masks the count to five bits.
            let hash = int_hash(x.wrapping_shl((16 + z) as u32));
            let span = self.y_variance * 2 + 1;
            row += (hash & 0xFF) % span - self.y_variance;
        }
        self.at(i64::from(id), i64::from(row))
    }
}

/// The climate of a biome: `getFloatTemperature` and `getFloatRainfall`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Climate {
    /// Temperature, `0..=1` after clamping.
    pub temperature: f32,
    /// Rainfall, `0..=1` after clamping.
    pub rainfall: f32,
}

/// OptiFine's `Config.intHash`.
fn int_hash(x: i32) -> i32 {
    let mut x = x ^ 61 ^ (x >> 16);
    x = x.wrapping_add(x.wrapping_shl(3));
    x ^= x >> 4;
    x = x.wrapping_mul(668_265_261);
    x ^ (x >> 15)
}
