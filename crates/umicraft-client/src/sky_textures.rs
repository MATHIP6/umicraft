// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sky pictures of the user's client jar (P7e).
//!
//! `sun.png`, `moon_phases.png`, `end_sky.png`, `clouds.png`, `rain.png`, `snow.png` and the particle sheet are read at runtime from the
//! verified jar and never stored in the repository (spec §3). A picture the jar
//! lacks, or that does not decode, stays `None` and the renderer draws its
//! original stand-in.

use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::skin::decode_png_bytes;
use umicraft_render::{SkyImage, SkyTextures};

/// Jar path of the sun.
const SUN: &str = "assets/minecraft/textures/environment/sun.png";
/// Jar path of the eight phases of the moon.
const MOON_PHASES: &str = "assets/minecraft/textures/environment/moon_phases.png";
/// Jar path of the End sky.
const END_SKY: &str = "assets/minecraft/textures/environment/end_sky.png";

/// Jar path of the clouds.
const CLOUDS: &str = "assets/minecraft/textures/environment/clouds.png";

/// Jar path of the world border's force field.
const FORCEFIELD: &str = "assets/minecraft/textures/misc/forcefield.png";

/// Reads and decodes one picture of `jar`; `None` when absent or malformed.
fn read_image(jar: Option<&ClientJar>, path: &str) -> Option<SkyImage> {
    let bytes = jar?.read_entry(path).ok()?;
    let (rgba, width, height) = decode_png_bytes(&bytes).ok()?;
    Some(SkyImage {
        width,
        height,
        rgba,
    })
}

/// Reads the world border picture of `jar`; `None` keeps the renderer's
/// original fallback.
#[must_use]
pub fn load_border_texture(jar: Option<&ClientJar>) -> Option<SkyImage> {
    read_image(jar, FORCEFIELD)
}

/// Jar path of the rain streaks.
const RAIN: &str = "assets/minecraft/textures/environment/rain.png";
/// Jar path of the snowflakes.
const SNOW: &str = "assets/minecraft/textures/environment/snow.png";

/// Jar path of the particle sheet.
const PARTICLES: &str = "assets/minecraft/textures/particle/particles.png";

/// Reads the sky pictures of `jar`; every missing or undecodable one is `None`.
#[must_use]
pub fn load_sky_textures(jar: Option<&ClientJar>) -> SkyTextures {
    let read = |path: &str| read_image(jar, path);
    SkyTextures {
        sun: read(SUN),
        moon_phases: read(MOON_PHASES),
        end_sky: read(END_SKY),
        clouds: read(CLOUDS),
        rain: read(RAIN),
        snow: read(SNOW),
        particles: read(PARTICLES),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_jar_every_picture_is_left_to_the_fallback() {
        let textures = load_sky_textures(None);
        assert!(textures.sun.is_none());
        assert!(textures.moon_phases.is_none());
        assert!(textures.end_sky.is_none());
        assert!(textures.clouds.is_none());
        assert!(textures.rain.is_none());
        assert!(textures.snow.is_none());
        assert!(textures.particles.is_none());
        assert!(load_border_texture(None).is_none());
    }
}
