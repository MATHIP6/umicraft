// SPDX-License-Identifier: MIT OR Apache-2.0

//! The video options: render distance, field of view, frame-rate cap,
//! clouds and particles.

use serde::{Deserialize, Serialize};
use umicraft_render::CloudMode;

/// The render distance range, in chunks.
pub const RENDER_DISTANCE: std::ops::RangeInclusive<u8> = 2..=16;
/// The field-of-view range, in degrees.
pub const FOV: std::ops::RangeInclusive<f32> = 30.0..=110.0;
/// The frame-rate cap range; the top value means "unlimited".
pub const MAX_FPS: std::ops::RangeInclusive<u32> = 10..=260;
/// The frame-rate cap value that disables the limiter.
pub const UNLIMITED_FPS: u32 = 260;

/// The video options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoOptions {
    /// How many chunks around the player are requested and drawn.
    pub render_distance: u8,
    /// The vertical field of view, in degrees.
    pub fov: f32,
    /// The frame-rate cap; [`UNLIMITED_FPS`] means no cap.
    pub max_fps: u32,
    /// Fancy graphics: boxed clouds instead of a flat layer
    /// (`GameSettings.fancyGraphics`).
    pub fancy_graphics: bool,
    /// Whether clouds are drawn at all (`GameSettings.clouds`).
    pub clouds: bool,
    /// `GameSettings.particleSetting`: 0 all, 1 decreased, 2 minimal.
    pub particles: u8,
}

/// The highest `particles` value (minimal).
pub const MAX_PARTICLE_SETTING: u8 = 2;

impl VideoOptions {
    /// The particle setting these options select.
    #[must_use]
    pub fn particle_setting(&self) -> umicraft_game::particles::ParticleSetting {
        umicraft_game::particles::ParticleSetting::from_index(self.particles)
    }

    /// The cloud version these options select.
    #[must_use]
    pub fn cloud_mode(&self) -> CloudMode {
        match (self.clouds, self.fancy_graphics) {
            (false, _) => CloudMode::Off,
            (true, false) => CloudMode::Fast,
            (true, true) => CloudMode::Fancy,
        }
    }
}

impl Default for VideoOptions {
    fn default() -> Self {
        Self {
            render_distance: 8,
            fov: 70.0,
            max_fps: 120,
            fancy_graphics: true,
            clouds: true,
            particles: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::Options;

    #[test]
    fn the_particle_setting_defaults_to_all_and_is_clamped() {
        let mut options = Options::default();
        assert_eq!(options.video.particle_setting().label(), "All");
        options.video.particles = 9;
        options.clamp();
        assert_eq!(options.video.particles, MAX_PARTICLE_SETTING);
        assert_eq!(options.video.particle_setting().label(), "Minimal");
    }

    #[test]
    fn the_cloud_options_default_to_fancy_clouds() {
        let video = VideoOptions::default();
        assert!(video.fancy_graphics && video.clouds);
        assert_eq!(video.cloud_mode(), CloudMode::Fancy);
    }

    #[test]
    fn the_cloud_mode_follows_the_two_switches() {
        let mode = |clouds, fancy_graphics| {
            VideoOptions {
                clouds,
                fancy_graphics,
                ..VideoOptions::default()
            }
            .cloud_mode()
        };
        assert_eq!(mode(false, true), CloudMode::Off);
        assert_eq!(mode(true, false), CloudMode::Fast);
        assert_eq!(mode(true, true), CloudMode::Fancy);
    }
}
