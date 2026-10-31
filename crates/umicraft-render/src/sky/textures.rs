// SPDX-License-Identifier: MIT OR Apache-2.0

//! The images of the sky and their original fallbacks.
//!
//! The sun, the moon phases and the End sky come from the user's jar at
//! runtime (`textures/environment/`); nothing of them is stored in this
//! repository (spec §3). Until the client supplies them, or when an image is
//! missing or malformed, the pass draws the fallbacks generated here: a soft
//! white disc, eight lit discs and a dark violet noise.

/// A decoded RGBA8 image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyImage {
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// Tightly packed RGBA8 pixels, `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

impl SkyImage {
    /// Whether the pixel buffer matches the declared size (and is not empty).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let expected = u64::from(self.width) * u64::from(self.height) * 4;
        self.width > 0 && self.height > 0 && self.rgba.len() as u64 == expected
    }
}

/// The images of the sky and the weather; `None` keeps the fallback.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkyTextures {
    /// `textures/environment/sun.png`.
    pub sun: Option<SkyImage>,
    /// `textures/environment/moon_phases.png`, 4 x 2 phases.
    pub moon_phases: Option<SkyImage>,
    /// `textures/environment/end_sky.png`.
    pub end_sky: Option<SkyImage>,
    /// `textures/environment/clouds.png`, 256 x 256 cloud cells.
    pub clouds: Option<SkyImage>,
    /// `textures/environment/rain.png`, drawn by the weather pass.
    pub rain: Option<SkyImage>,
    /// `textures/environment/snow.png`, drawn by the weather pass.
    pub snow: Option<SkyImage>,
}

/// Edge of the fallback sun in texels.
const SUN_SIZE: u32 = 64;
/// Edge of one fallback moon phase in texels (the sheet is 4 x 2 of these).
const MOON_CELL: u32 = 32;
/// Edge of the fallback End sky tile in texels.
const END_SIZE: u32 = 64;

/// A soft white disc: solid core, smooth halo, transparent corners.
pub(super) fn fallback_sun() -> SkyImage {
    let mut rgba = Vec::with_capacity((SUN_SIZE * SUN_SIZE * 4) as usize);
    for y in 0..SUN_SIZE {
        for x in 0..SUN_SIZE {
            let radius = centre_distance(x, y, SUN_SIZE);
            let alpha = 1.0 - smoothstep(0.28, 0.5, radius);
            rgba.extend_from_slice(&[255, 250, 225, (alpha * 255.0) as u8]);
        }
    }
    SkyImage {
        width: SUN_SIZE,
        height: SUN_SIZE,
        rgba,
    }
}

/// Eight discs in the sheet layout of the reference, phase 0 (full) first,
/// then waning to new (4) and waxing back (5..7).
pub(super) fn fallback_moon_phases() -> SkyImage {
    let (width, height) = (MOON_CELL * 4, MOON_CELL * 2);
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for phase in 0..8u32 {
        let (column, row) = (phase % 4, phase / 4);
        for y in 0..MOON_CELL {
            for x in 0..MOON_CELL {
                let pixel = moon_pixel(phase, x, y);
                let at = (((row * MOON_CELL + y) * width + column * MOON_CELL + x) * 4) as usize;
                rgba[at..at + 4].copy_from_slice(&pixel);
            }
        }
    }
    SkyImage {
        width,
        height,
        rgba,
    }
}

/// One texel of a moon phase: lit surface, faint dark side, or empty sky.
fn moon_pixel(phase: u32, x: u32, y: u32) -> [u8; 4] {
    const DISC_RADIUS: f32 = 0.42;
    let (dx, dy) = (
        (x as f32 + 0.5) / MOON_CELL as f32 - 0.5,
        (y as f32 + 0.5) / MOON_CELL as f32 - 0.5,
    );
    let (nx, ny) = (dx / DISC_RADIUS, dy / DISC_RADIUS);
    let rest = 1.0 - nx * nx - ny * ny;
    if rest < 0.0 {
        return [0, 0, 0, 0];
    }
    // Light direction turns from the viewer (full) through the left side
    // (phase 2) to behind the moon (new).
    let angle = phase as f32 / 8.0 * std::f32::consts::TAU;
    let lit = -nx * angle.sin() + rest.sqrt() * angle.cos();
    if lit > 0.0 {
        [232, 232, 216, 255]
    } else {
        [18, 20, 28, 255]
    }
}

/// A dark violet noise that tiles because every texel is hashed on its own.
pub(super) fn fallback_end_sky() -> SkyImage {
    let mut rgba = Vec::with_capacity((END_SIZE * END_SIZE * 4) as usize);
    for y in 0..END_SIZE {
        for x in 0..END_SIZE {
            let noise = hash(x, y);
            let speck = if noise > 0xF6 { 90 } else { 0 };
            let shade = i32::from(noise >> 4) * 3;
            rgba.extend_from_slice(&[
                (60 + shade + speck).min(255) as u8,
                (30 + shade / 2 + speck).min(255) as u8,
                (95 + shade + speck).min(255) as u8,
                255,
            ]);
        }
    }
    SkyImage {
        width: END_SIZE,
        height: END_SIZE,
        rgba,
    }
}

/// Distance of the texel centre from the image centre, in image widths.
fn centre_distance(x: u32, y: u32, size: u32) -> f32 {
    let dx = (x as f32 + 0.5) / size as f32 - 0.5;
    let dy = (y as f32 + 0.5) / size as f32 - 0.5;
    dx.hypot(dy)
}

/// Hermite step from 0 at `low` to 1 at `high`.
fn smoothstep(low: f32, high: f32, value: f32) -> f32 {
    let t = ((value - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A byte of deterministic noise for a texel.
pub(crate) fn hash(x: u32, y: u32) -> u8 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFF) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(image: &SkyImage, x: u32, y: u32) -> u8 {
        image.rgba[((y * image.width + x) * 4 + 3) as usize]
    }

    #[test]
    fn the_fallbacks_are_valid_images() {
        for image in [fallback_sun(), fallback_moon_phases(), fallback_end_sky()] {
            assert!(image.is_valid());
        }
        assert_eq!(
            fallback_moon_phases().width / fallback_moon_phases().height,
            2
        );
    }

    #[test]
    fn a_malformed_image_is_invalid() {
        let short = SkyImage {
            width: 2,
            height: 2,
            rgba: vec![0; 15],
        };
        let empty = SkyImage {
            width: 0,
            height: 4,
            rgba: Vec::new(),
        };
        assert!(!short.is_valid() && !empty.is_valid());
    }

    #[test]
    fn the_sun_is_solid_in_the_middle_and_clear_in_the_corner() {
        let sun = fallback_sun();
        assert_eq!(alpha(&sun, 32, 32), 255);
        assert_eq!(alpha(&sun, 0, 0), 0);
    }

    #[test]
    fn the_moon_phases_light_the_expected_part_of_the_disc() {
        let sheet = fallback_moon_phases();
        let lit = |phase: u32, x: u32, y: u32| {
            let (column, row) = (phase % 4, phase / 4);
            let at = (((row * MOON_CELL + y) * sheet.width + column * MOON_CELL + x) * 4) as usize;
            sheet.rgba[at] > 100
        };
        let centre = MOON_CELL / 2;
        // Full: centre lit. New (4): centre dark. Quarter (2): left lit only.
        assert!(lit(0, centre, centre));
        assert!(!lit(4, centre, centre));
        assert!(lit(2, centre - 8, centre) && !lit(2, centre + 8, centre));
        assert!(lit(6, centre + 8, centre) && !lit(6, centre - 8, centre));
        // Outside the disc the sheet is transparent.
        assert_eq!(sheet.rgba[3], 0);
    }

    #[test]
    fn the_end_sky_is_dark_violet_and_deterministic() {
        let sky = fallback_end_sky();
        assert_eq!(sky, fallback_end_sky());
        let blue: u32 = sky.rgba.chunks_exact(4).map(|p| u32::from(p[2])).sum();
        let green: u32 = sky.rgba.chunks_exact(4).map(|p| u32::from(p[1])).sum();
        assert!(blue > green, "violet leans blue over green");
        assert!(sky.rgba.chunks_exact(4).all(|p| p[3] == 255));
    }
}
