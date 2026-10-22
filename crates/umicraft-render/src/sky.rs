// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sky, sun/moon and cloud pass (M6).
//!
//! [`SkyState`] is the pure time-of-day model: it turns [`RenderEnvironment`]
//! into the celestial angle, colours and light factors used by the GPU pass
//! and the terrain shader. [`SkyRenderer`] owns the wgpu pipeline; it is
//! crate-private because [`crate::Renderer`] is the only caller.
//!
//! The visual assets are original: the sun and moon are procedural discs and
//! the clouds are generated from a hash in the shader. No Mojang texture or
//! shader is copied into the repository (spec §3).

use crate::{Camera, RenderEnvironment};

/// Colours and celestial positions for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyState {
    /// Celestial angle in `0.0..1.0`, where 0 is sunrise and 0.5 is sunset.
    pub celestial_angle: f32,
    /// Unit direction from the camera towards the sun.
    pub sun_direction: [f32; 3],
    /// Unit direction from the camera towards the moon.
    pub moon_direction: [f32; 3],
    /// Linear sky colour used for the clear and the fog target.
    pub sky_color: [f32; 3],
    /// Linear fog colour (usually close to `sky_color`).
    pub fog_color: [f32; 3],
    /// Linear RGBA cloud tint.
    pub cloud_color: [f32; 4],
    /// Star opacity in `0.0..=1.0`.
    pub star_alpha: f32,
    /// Sun-light factor in `0.0..=1.0`, applied to sky light in the terrain
    /// shader so a full sky nibble still reads dark at night.
    pub sun_brightness: f32,
    /// Moon phase index in `0..=7`; 0 is a full moon.
    pub moon_phase: u8,
}

impl SkyState {
    /// Computes the sky state for `environment`.
    ///
    /// This is a placeholder until the M6 sky workstream replaces it with the
    /// complete 1.8 celestial-angle and colour curves.
    #[must_use]
    pub fn at(environment: RenderEnvironment) -> Self {
        let _ = environment;
        Self {
            celestial_angle: 0.0,
            sun_direction: [0.0, 1.0, 0.0],
            moon_direction: [0.0, -1.0, 0.0],
            sky_color: [0.55, 0.72, 0.95],
            fog_color: [0.55, 0.72, 0.95],
            cloud_color: [1.0, 1.0, 1.0, 0.8],
            star_alpha: 0.0,
            sun_brightness: 1.0,
            moon_phase: 0,
        }
    }
}

/// GPU resources for the sky, sun, moon and cloud pass.
///
/// The renderer calls [`SkyRenderer::update`] once per frame, then
/// [`SkyRenderer::draw`] inside the main render pass before the terrain.
pub(crate) struct SkyRenderer;

impl SkyRenderer {
    /// Creates the pipeline and uniform resources for `format`.
    pub(crate) fn new(_device: &wgpu::Device, _format: wgpu::TextureFormat) -> Self {
        Self
    }

    /// Writes the per-frame camera and [`SkyState`] uniforms.
    pub(crate) fn update(
        &self,
        _queue: &wgpu::Queue,
        _camera: &Camera,
        _environment: RenderEnvironment,
    ) {
    }

    /// Draws the sky behind the terrain.
    pub(crate) fn draw(&self, _pass: &mut wgpu::RenderPass<'_>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_placeholder_is_daylight() {
        let state = SkyState::at(RenderEnvironment::default());
        assert_eq!(state.sky_color, [0.55, 0.72, 0.95]);
        assert_eq!(state.sun_brightness, 1.0);
        assert_eq!(state.star_alpha, 0.0);
    }
}
