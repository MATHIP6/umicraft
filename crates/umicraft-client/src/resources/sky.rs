// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sky pictures of the active packs and the user's client jar (P7e).
//!
//! `sun.png`, `moon_phases.png`, `end_sky.png`, `clouds.png`, `rain.png`, `snow.png`, the particle sheet
//! and the overlay pictures (`underwater.png`, `pumpkinblur.png`, `vignette.png`) are read at runtime
//! through [`GameFiles`] (packs first, then the verified jar) and never stored in the repository
//! (spec §3). A picture that neither has, or that does not decode, stays `None` and the renderer
//! draws its original stand-in.

use umicraft_render::{SkyImage, SkyTextures};

use super::game_files::GameFiles;

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

/// Reads and decodes one picture; `None` when absent or malformed.
fn read_image(files: &GameFiles, path: &str) -> Option<SkyImage> {
    let (rgba, width, height) = files.read_png(path)?;
    Some(SkyImage {
        width,
        height,
        rgba,
    })
}

/// Reads the world border picture; `None` keeps the renderer's original
/// fallback.
#[must_use]
pub fn load_border_texture(files: &GameFiles) -> Option<SkyImage> {
    read_image(files, FORCEFIELD)
}

/// Jar path of the rain streaks.
const RAIN: &str = "assets/minecraft/textures/environment/rain.png";
/// Jar path of the snowflakes.
const SNOW: &str = "assets/minecraft/textures/environment/snow.png";

/// Jar path of the particle sheet.
const PARTICLES: &str = "assets/minecraft/textures/particle/particles.png";

/// Jar path of the veil seen under water.
const UNDERWATER: &str = "assets/minecraft/textures/misc/underwater.png";
/// Jar path of the pumpkin worn on the head.
const PUMPKIN_BLUR: &str = "assets/minecraft/textures/misc/pumpkinblur.png";
/// Jar path of the shade of the screen edges.
const VIGNETTE: &str = "assets/minecraft/textures/misc/vignette.png";

/// Reads the sky pictures; every missing or undecodable one is `None`.
#[must_use]
pub fn load_sky_textures(files: &GameFiles) -> SkyTextures {
    let read = |path: &str| read_image(files, path);
    SkyTextures {
        sun: read(SUN),
        moon_phases: read(MOON_PHASES),
        end_sky: read(END_SKY),
        clouds: read(CLOUDS),
        rain: read(RAIN),
        snow: read(SNOW),
        particles: read(PARTICLES),
        underwater: read(UNDERWATER),
        pumpkin_blur: read(PUMPKIN_BLUR),
        vignette: read(VIGNETTE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_jar_every_picture_is_left_to_the_fallback() {
        let textures = load_sky_textures(&GameFiles::default());
        assert!(textures.sun.is_none());
        assert!(textures.moon_phases.is_none());
        assert!(textures.end_sky.is_none());
        assert!(textures.clouds.is_none());
        assert!(textures.rain.is_none());
        assert!(textures.snow.is_none());
        assert!(textures.particles.is_none());
        assert!(textures.underwater.is_none());
        assert!(textures.pumpkin_blur.is_none());
        assert!(textures.vignette.is_none());
        assert!(load_border_texture(&GameFiles::default()).is_none());
    }

    /// Regression: the sky pictures were read from the jar alone, so a pack's
    /// `sun.png` was ignored.
    #[test]
    fn a_pack_sun_is_used() {
        let files = crate::resources::game_files::test_support::files_with(
            SUN,
            crate::resources::game_files::test_support::png(32, 32),
        );
        let textures = load_sky_textures(&files);
        assert_eq!(
            textures.sun.map(|sun| (sun.width, sun.height)),
            Some((32, 32))
        );
        assert!(textures.moon_phases.is_none());
    }
}
