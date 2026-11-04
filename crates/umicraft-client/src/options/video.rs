// SPDX-License-Identifier: MIT OR Apache-2.0

//! The video options of `GuiVideoSettings`: render distance, field of view,
//! frame-rate cap, graphics, clouds, particles, smooth lighting, GUI scale,
//! brightness, view bobbing, mipmaps, vsync, fullscreen, block alternatives
//! and entity shadows.
//!
//! Defaults and ranges follow MCP `GameSettings`. Which of them drive the
//! renderer and which are only stored is listed in `docs/parity.md`.

use serde::{Deserialize, Deserializer, Serialize};
use umicraft_render::CloudMode;

/// The render distance range, in chunks.
pub const RENDER_DISTANCE: std::ops::RangeInclusive<u8> = 2..=16;
/// The field-of-view range, in degrees.
pub const FOV: std::ops::RangeInclusive<f32> = 30.0..=110.0;
/// The frame-rate cap range; the top value means "unlimited".
pub const MAX_FPS: std::ops::RangeInclusive<u32> = 10..=260;
/// The frame-rate cap value that disables the limiter.
pub const UNLIMITED_FPS: u32 = 260;
/// The highest `particles` value (minimal).
pub const MAX_PARTICLE_SETTING: u8 = 2;
/// The highest `clouds` value (fancy); `0` is off and `1` fast.
pub const MAX_CLOUD_SETTING: u8 = 2;
/// The highest `smooth_lighting` value (maximum); `0` is off, `1` minimum.
pub const MAX_SMOOTH_LIGHTING: u8 = 2;
/// The highest `gui_scale` value (large); `0` is auto, `1` small, `2` normal.
pub const MAX_GUI_SCALE: u8 = 3;
/// The mipmap level range (`GameSettings.Options.MIPMAP_LEVELS`).
pub const MIPMAP_LEVELS: std::ops::RangeInclusive<u8> = 0..=4;
/// The render distance below which vanilla draws no clouds
/// (`GameSettings.func_181147_e`).
pub const MIN_CLOUD_DISTANCE: u8 = 4;

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
    /// Fancy graphics instead of fast (`GameSettings.fancyGraphics`): it
    /// selects the precipitation and the screen overlays.
    pub fancy_graphics: bool,
    /// `GameSettings.clouds`: `0` off, `1` fast, `2` fancy. A file written
    /// before the setting had three states holds a boolean, read as off or
    /// fancy.
    #[serde(deserialize_with = "cloud_setting")]
    pub clouds: u8,
    /// `GameSettings.particleSetting`: 0 all, 1 decreased, 2 minimal.
    pub particles: u8,
    /// `GameSettings.ambientOcclusion` ("Smooth Lighting"): 0 off, 1
    /// minimum, 2 maximum.
    pub smooth_lighting: u8,
    /// `GameSettings.guiScale`: 0 auto, 1 small, 2 normal, 3 large.
    pub gui_scale: u8,
    /// `GameSettings.gammaSetting` ("Brightness"), `0.0` moody to `1.0`
    /// bright.
    pub gamma: f32,
    /// `GameSettings.viewBobbing`.
    pub view_bobbing: bool,
    /// `GameSettings.mipmapLevels`, `0..=4`.
    pub mipmap_levels: u8,
    /// `GameSettings.enableVsync`.
    pub vsync: bool,
    /// `GameSettings.fullScreen`.
    pub fullscreen: bool,
    /// `GameSettings.allowBlockAlternatives`.
    pub block_alternatives: bool,
    /// `GameSettings.field_181151_V` ("Entity Shadows").
    pub entity_shadows: bool,
}

/// Reads the cloud setting as a number, or as the boolean of the files that
/// predate the fast/fancy/off cycle.
fn cloud_setting<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Flag(bool),
        Level(u8),
    }
    Ok(match Stored::deserialize(deserializer)? {
        Stored::Flag(false) => 0,
        Stored::Flag(true) => MAX_CLOUD_SETTING,
        Stored::Level(level) => level,
    })
}

impl VideoOptions {
    /// The particle setting these options select.
    #[must_use]
    pub fn particle_setting(&self) -> umicraft_game::particles::ParticleSetting {
        umicraft_game::particles::ParticleSetting::from_index(self.particles)
    }

    /// The cloud version these options select. Like vanilla, a render
    /// distance under [`MIN_CLOUD_DISTANCE`] draws none.
    #[must_use]
    pub fn cloud_mode(&self) -> CloudMode {
        if self.render_distance < MIN_CLOUD_DISTANCE {
            return CloudMode::Off;
        }
        match self.clouds {
            0 => CloudMode::Off,
            1 => CloudMode::Fast,
            _ => CloudMode::Fancy,
        }
    }

    /// Puts the numbers of this section back in their ranges; a value that
    /// is not a number returns to its default.
    pub(super) fn clamp(&mut self) {
        let defaults = Self::default();
        self.render_distance = self
            .render_distance
            .clamp(*RENDER_DISTANCE.start(), *RENDER_DISTANCE.end());
        self.max_fps = self.max_fps.clamp(*MAX_FPS.start(), *MAX_FPS.end());
        self.particles = self.particles.min(MAX_PARTICLE_SETTING);
        self.clouds = self.clouds.min(MAX_CLOUD_SETTING);
        self.smooth_lighting = self.smooth_lighting.min(MAX_SMOOTH_LIGHTING);
        self.gui_scale = self.gui_scale.min(MAX_GUI_SCALE);
        self.mipmap_levels = self
            .mipmap_levels
            .clamp(*MIPMAP_LEVELS.start(), *MIPMAP_LEVELS.end());
        self.fov = if self.fov.is_finite() {
            self.fov.clamp(*FOV.start(), *FOV.end())
        } else {
            defaults.fov
        };
        self.gamma = if self.gamma.is_finite() {
            self.gamma.clamp(0.0, 1.0)
        } else {
            defaults.gamma
        };
    }
}

impl Default for VideoOptions {
    fn default() -> Self {
        Self {
            render_distance: 8,
            fov: 70.0,
            max_fps: 120,
            fancy_graphics: true,
            clouds: MAX_CLOUD_SETTING,
            particles: 0,
            smooth_lighting: MAX_SMOOTH_LIGHTING,
            gui_scale: 0,
            gamma: 0.0,
            view_bobbing: true,
            mipmap_levels: *MIPMAP_LEVELS.end(),
            vsync: true,
            fullscreen: false,
            block_alternatives: true,
            entity_shadows: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::Options;

    #[test]
    fn the_defaults_match_game_settings() {
        let video = VideoOptions::default();
        assert_eq!(video.smooth_lighting, 2);
        assert_eq!(video.gui_scale, 0);
        assert_eq!(video.gamma, 0.0);
        assert!(video.view_bobbing && video.vsync && !video.fullscreen);
        assert_eq!(video.mipmap_levels, 4);
        assert!(video.block_alternatives && video.entity_shadows);
    }

    #[test]
    fn the_new_numbers_are_clamped_into_their_ranges() {
        let mut options = Options::default();
        options.video.smooth_lighting = 9;
        options.video.gui_scale = 7;
        options.video.mipmap_levels = 200;
        options.video.clouds = 5;
        options.video.gamma = 3.0;
        options.clamp();
        assert_eq!(options.video.smooth_lighting, MAX_SMOOTH_LIGHTING);
        assert_eq!(options.video.gui_scale, MAX_GUI_SCALE);
        assert_eq!(options.video.mipmap_levels, 4);
        assert_eq!(options.video.clouds, MAX_CLOUD_SETTING);
        assert_eq!(options.video.gamma, 1.0);
        options.video.gamma = f32::NAN;
        options.clamp();
        assert_eq!(options.video.gamma, 0.0);
    }

    #[test]
    fn the_particle_setting_defaults_to_all_and_is_clamped() {
        let mut options = Options::default();
        assert_eq!(
            options.video.particle_setting().lang_key(),
            "options.particles.all"
        );
        options.video.particles = 9;
        options.clamp();
        assert_eq!(options.video.particles, MAX_PARTICLE_SETTING);
        assert_eq!(
            options.video.particle_setting().lang_key(),
            "options.particles.minimal"
        );
    }

    #[test]
    fn the_clouds_default_to_fancy() {
        assert_eq!(VideoOptions::default().cloud_mode(), CloudMode::Fancy);
    }

    #[test]
    fn the_cloud_mode_follows_the_setting_and_needs_a_render_distance() {
        let mode = |clouds, render_distance| {
            VideoOptions {
                clouds,
                render_distance,
                ..VideoOptions::default()
            }
            .cloud_mode()
        };
        assert_eq!(mode(0, 8), CloudMode::Off);
        assert_eq!(mode(1, 8), CloudMode::Fast);
        assert_eq!(mode(2, 8), CloudMode::Fancy);
        assert_eq!(mode(2, 3), CloudMode::Off);
        assert_eq!(mode(2, 4), CloudMode::Fancy);
    }

    #[test]
    fn a_boolean_cloud_setting_of_an_older_file_is_still_read() {
        let on: VideoOptions = toml::from_str("clouds = true").unwrap();
        let off: VideoOptions = toml::from_str("clouds = false").unwrap();
        let fast: VideoOptions = toml::from_str("clouds = 1").unwrap();
        assert_eq!((on.clouds, off.clouds, fast.clouds), (2, 0, 1));
    }
}
