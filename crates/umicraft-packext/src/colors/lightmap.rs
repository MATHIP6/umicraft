// SPDX-License-Identifier: MIT OR Apache-2.0

//! Custom lightmaps (`lightmap/world<dimension>.png`).
//!
//! A lightmap picture is at least 16 pixels wide and has 16 rows of sky light
//! on top of 16 rows of block light (and, for night vision, the same again
//! below: height 64). Sun brightness picks a column of the sky rows, the torch
//! flicker picks a column of the block rows, and the two are added per sky and
//! block level. Behaviour from OptiFine's `LightMap` and `LightMapPack`.

use thiserror::Error;

/// Levels per axis.
const LEVELS: usize = 16;

/// The 256 light colours, `[sky * 16 + block]`, channels in `0..=1`.
pub type Colors = [[f32; 3]; LEVELS * LEVELS];

/// Why a lightmap picture was dropped.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LightmapError {
    /// The picture is narrower than 16 pixels or not 16 rows high.
    #[error("invalid lightmap size {width}x{height}")]
    InvalidSize {
        /// The width in pixels.
        width: u32,
        /// The height in pixels.
        height: u32,
    },
    /// The pixel buffer does not fit the size.
    #[error("lightmap pixels do not match {width}x{height}")]
    PixelCount {
        /// The width in pixels.
        width: u32,
        /// The height in pixels.
        height: u32,
    },
}

/// The world state one frame's lightmap depends on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightmapInput {
    /// `World.getSunBrightness`, `0.2..=1`.
    pub sun_brightness: f32,
    /// A lightning flash is on (`getLastLightningBolt() > 0`).
    pub lightning: bool,
    /// `EntityRenderer.torchFlickerX`.
    pub torch_flicker: f32,
    /// The brightness option, `0..=1`.
    pub gamma: f32,
    /// Night Vision is active: the lower half of a tall picture is used.
    pub night_vision: bool,
}

/// One decoded lightmap picture.
#[derive(Debug, Clone, PartialEq)]
pub struct Lightmap {
    width: usize,
    height: usize,
    pixels: Vec<[f32; 3]>,
}

impl Lightmap {
    /// Builds a lightmap from a decoded RGBA8 picture.
    ///
    /// # Errors
    ///
    /// [`LightmapError`] when the picture is narrower than 16 pixels, has
    /// fewer than 32 rows, or `rgba` does not fit.
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Self, LightmapError> {
        if width < LEVELS as u32 || height < 2 * LEVELS as u32 {
            return Err(LightmapError::InvalidSize { width, height });
        }
        let (width, height) = (width as usize, height as usize);
        if width.checked_mul(height).and_then(|n| n.checked_mul(4)) != Some(rgba.len()) {
            return Err(LightmapError::PixelCount {
                width: width as u32,
                height: height as u32,
            });
        }
        let pixels = rgba
            .chunks_exact(4)
            .map(|texel| [texel[0], texel[1], texel[2]].map(|c| f32::from(c) / 255.0))
            .collect();
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// The colours for `input`, or `None` when the picture cannot serve it
    /// (night vision needs a picture of 64 rows).
    #[must_use]
    pub fn colors(&self, input: &LightmapInput) -> Option<Colors> {
        if input.night_vision && self.height < 4 * LEVELS {
            return None;
        }
        let night = if input.night_vision {
            self.width * LEVELS * 2
        } else {
            0
        };
        let mut sun = 1.166_666_6 * (input.sun_brightness - 0.2);
        if input.lightning {
            sun = 1.0;
        }
        let last = (self.width - 1) as f32;
        let sun_x = sun.clamp(0.0, 1.0) * last;
        let torch_x = (input.torch_flicker + 0.5).clamp(0.0, 1.0) * last;
        let sky = self.column(sun_x, night);
        let block = self.column(torch_x, night + LEVELS * self.width);
        let gamma = input.gamma.clamp(0.0, 1.0);
        let lift = gamma > 1.0e-4;
        let mut colors = [[0.0; 3]; LEVELS * LEVELS];
        for (s, sky_row) in sky.iter().enumerate() {
            for (b, block_row) in block.iter().enumerate() {
                colors[s * LEVELS + b] = std::array::from_fn(|channel| {
                    let sum = (sky_row[channel] + block_row[channel]).min(1.0);
                    if lift {
                        let inverse = 1.0 - sum;
                        let lifted = 1.0 - inverse * inverse * inverse * inverse;
                        gamma * lifted + (1.0 - gamma) * sum
                    } else {
                        sum
                    }
                });
            }
        }
        Some(colors)
    }

    /// The 16 colours of the column at fractional position `x`, interpolated
    /// between the two pixel columns around it (`getLightMapColumn`).
    fn column(&self, x: f32, offset: usize) -> [[f32; 3]; LEVELS] {
        let low = x.floor() as usize;
        let high = (x.ceil() as usize).min(self.width - 1);
        let weight_low = if low == high {
            1.0
        } else {
            1.0 - (x - low as f32)
        };
        let weight_high = if low == high {
            0.0
        } else {
            1.0 - (high as f32 - x)
        };
        std::array::from_fn(|row| {
            let base = offset + row * self.width;
            let a = self.pixels[base + low];
            let b = self.pixels[base + high];
            std::array::from_fn(|c| a[c] * weight_low + b[c] * weight_high)
        })
    }
}

/// The lightmaps of one dimension: dry weather, and optionally rain and
/// thunder variants that are blended in by the weather strengths.
#[derive(Debug, Clone, PartialEq)]
pub struct LightmapPack {
    normal: Lightmap,
    rain: Option<Lightmap>,
    thunder: Option<Lightmap>,
}

/// Weather weights below this count as zero (`LightMapPack`).
const WEATHER_EPSILON: f32 = 1.0e-4;

impl LightmapPack {
    /// A pack. A missing rain picture stands in for a missing thunder one,
    /// and the normal picture for a missing rain one, but only when at least
    /// one of the two exists.
    #[must_use]
    pub fn new(normal: Lightmap, rain: Option<Lightmap>, thunder: Option<Lightmap>) -> Self {
        let weather = rain.is_some() || thunder.is_some();
        let rain = rain.or_else(|| weather.then(|| normal.clone()));
        let thunder = thunder.or_else(|| rain.clone());
        Self {
            normal,
            rain,
            thunder,
        }
    }

    /// The colours for `input`. `rain` and `thunder` are the interpolated
    /// strengths, or `None` in the Nether and the End, which have no weather.
    /// `None` is returned when no picture can serve the frame.
    #[must_use]
    pub fn colors(&self, input: &LightmapInput, weather: Option<(f32, f32)>) -> Option<Colors> {
        let (Some(rain_map), Some(thunder_map)) = (&self.rain, &self.thunder) else {
            return self.normal.colors(input);
        };
        let Some((rain, thunder)) = weather else {
            return self.normal.colors(input);
        };
        if rain <= WEATHER_EPSILON && thunder <= WEATHER_EPSILON {
            return self.normal.colors(input);
        }
        let thunder_share = if rain > 0.0 { thunder / rain } else { thunder };
        let weights = [1.0 - rain, rain - thunder_share, thunder_share];
        let maps = [&self.normal, rain_map, thunder_map];
        let mut blended = [[0.0_f32; 3]; LEVELS * LEVELS];
        let mut any = false;
        for (map, weight) in maps.into_iter().zip(weights) {
            if weight <= WEATHER_EPSILON {
                continue;
            }
            let Some(colors) = map.colors(input) else {
                continue;
            };
            any = true;
            for (out, color) in blended.iter_mut().zip(colors) {
                for channel in 0..3 {
                    out[channel] += color[channel] * weight;
                }
            }
        }
        any.then_some(blended)
    }
}

/// A set of packs indexed by dimension id (`lightmap/world-1.png`,
/// `world0.png`, `world1.png`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DimensionLightmaps {
    packs: Vec<(i32, LightmapPack)>,
}

impl DimensionLightmaps {
    /// Adds the pack of `dimension`, replacing an earlier one.
    pub fn insert(&mut self, dimension: i32, pack: LightmapPack) {
        self.packs.retain(|(id, _)| *id != dimension);
        self.packs.push((dimension, pack));
    }

    /// The pack of `dimension`.
    #[must_use]
    pub fn get(&self, dimension: i32) -> Option<&LightmapPack> {
        self.packs
            .iter()
            .find(|(id, _)| *id == dimension)
            .map(|(_, pack)| pack)
    }

    /// Whether no dimension has a pack.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.packs.is_empty()
    }
}

/// The dimension id in a lightmap file name `world<N>.png` (the path relative
/// to `lightmap/`), `None` for any other name (including `_rain` and
/// `_thunder` variants).
#[must_use]
pub fn dimension_of(file: &str) -> Option<i32> {
    file.strip_prefix("world")?
        .strip_suffix(".png")?
        .parse()
        .ok()
}
