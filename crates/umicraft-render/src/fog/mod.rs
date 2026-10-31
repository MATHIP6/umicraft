// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fog parity (P7g): the colour and the falloff every pass fades by.
//!
//! One pure model, ported from MCP `EntityRenderer.updateFogColor` (colour)
//! and `EntityRenderer.setupFog` (mode and range), feeds the terrain, entity,
//! cloud and sky shaders through the same three numbers `(start, end,
//! density)`; `fog.wgsl` turns them into a blend amount. The medium at the eye
//! (`ActiveRenderInfo.getBlockAtEntityViewpoint`) and the potion effects come
//! from the game thread as a [`FogContext`].
//!
//! Not modelled: Respiration and Aqua Affinity (the client does not read the
//! helmet enchantments), `cloudFog` (the eye inside a cloud), the boss-bar
//! colour modifier and anaglyph.

use crate::RenderEnvironment;

/// Longest fog distance in blocks: the far plane of the camera is 512.
const MAX_FAR_PLANE: f32 = 500.0;
/// `WorldProvider.getVoidFogYFactor` for a non-flat world: fog turns black
/// as the eye sinks toward `y = 0`, fully dark at `y = 1 / 0.03125 = 32`.
const VOID_FOG_Y_FACTOR: f32 = 0.031_25;
/// Exponential density in water (`setupFog`, no Respiration).
const WATER_DENSITY: f32 = 0.1;
/// Exponential density in water while water breathing.
const WATER_BREATHING_DENSITY: f32 = 0.01;
/// Exponential density in lava.
const LAVA_DENSITY: f32 = 2.0;
/// Ticks of blindness left from which the fog opens up again.
const BLINDNESS_FADE_TICKS: u32 = 20;
/// Fog distance of full blindness in blocks.
const BLINDNESS_DISTANCE: f32 = 5.0;
/// Light brightness table floor in the Nether (`WorldProviderHell`).
const NETHER_AMBIENT: f32 = 0.1;

/// What the eye is inside of (`ActiveRenderInfo.getBlockAtEntityViewpoint`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FogMedium {
    /// Air, or any non-liquid block.
    #[default]
    Air,
    /// Water (`Material.water`).
    Water,
    /// Lava (`Material.lava`).
    Lava,
}

/// The camera-side inputs of the fog that the world and the potion effects
/// decide, published by the game thread each tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FogContext {
    /// The medium at the eye.
    pub medium: FogMedium,
    /// Ticks of Blindness left, `None` without the effect.
    pub blindness_ticks: Option<u32>,
    /// Ticks of Night Vision left, `None` without the effect.
    pub night_vision_ticks: Option<u32>,
    /// Water Breathing is active (clears the underwater fog).
    pub water_breathing: bool,
    /// Eye height in blocks, for the darkening toward the void.
    pub eye_y: f32,
    /// `World.getLightBrightness` at the eye in `0.0..=1.0`, smoothed over
    /// ticks by the caller (the `fogColor1` of the reference). Caves get a
    /// darker fog.
    pub light_brightness: f32,
}

impl FogContext {
    /// An eye in open air at full light, with no effects.
    pub const DEFAULT: Self = Self {
        medium: FogMedium::Air,
        blindness_ticks: None,
        night_vision_ticks: None,
        water_breathing: false,
        eye_y: 64.0,
        light_brightness: 1.0,
    };
}

impl Default for FogContext {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// How the fog amount grows with the distance from the eye.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FogMode {
    /// `GL_LINEAR`: none up to `start`, complete at `end` (blocks).
    Linear {
        /// Distance where the fog begins.
        start: f32,
        /// Distance where the fog is complete.
        end: f32,
    },
    /// `GL_EXP`: `1 - exp(-density * distance)`.
    Exp {
        /// Falloff per block.
        density: f32,
    },
}

impl FogMode {
    /// The `(start, end, density)` triple the shaders read; a density above
    /// zero selects the exponential branch of `fog_amount` in `fog.wgsl`.
    #[must_use]
    pub fn uniform(self) -> [f32; 3] {
        match self {
            Self::Linear { start, end } => [start, end, 0.0],
            Self::Exp { density } => [0.0, 1.0, density],
        }
    }
}

/// The fog of one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FogParams {
    /// Fog colour, which is also the clear colour.
    pub color: [f32; 3],
    /// The falloff of the world passes (`setupFog(0)`).
    pub world: FogMode,
    /// The falloff of the sky dome (`setupFog(-1)`), which reaches the
    /// horizon sooner.
    pub sky: FogMode,
}

impl FogParams {
    /// The fog for `environment`, starting from `base_color`: the provider
    /// fog after the sunrise tint, the sky mix and the weather dimming
    /// (`SkyState`), which the medium, the light and the effects then adjust.
    #[must_use]
    pub fn new(base_color: [f32; 3], environment: &RenderEnvironment) -> Self {
        Self {
            color: final_color(base_color, environment),
            world: mode(environment, false),
            sky: mode(environment, true),
        }
    }
}

/// The farthest distance fog reaches in blocks (`EntityRenderer.farPlaneDistance`),
/// capped inside the camera far plane.
#[must_use]
pub fn far_plane(view_distance: u8) -> f32 {
    (f32::from(view_distance) * 16.0).clamp(1.0, MAX_FAR_PLANE)
}

/// `setupFog`: the falloff of the medium, the effects or the dimension.
/// `sky` selects the `setupFog(-1)` variant used for the dome.
fn mode(environment: &RenderEnvironment, sky: bool) -> FogMode {
    let context = &environment.fog;
    let far = far_plane(environment.view_distance);
    if let Some(ticks) = context.blindness_ticks {
        let reach = if ticks < BLINDNESS_FADE_TICKS {
            let progress = 1.0 - ticks as f32 / BLINDNESS_FADE_TICKS as f32;
            BLINDNESS_DISTANCE + (far - BLINDNESS_DISTANCE) * progress
        } else {
            BLINDNESS_DISTANCE
        };
        return if sky {
            FogMode::Linear {
                start: 0.0,
                end: reach * 0.8,
            }
        } else {
            FogMode::Linear {
                start: reach * 0.25,
                end: reach,
            }
        };
    }
    match context.medium {
        FogMedium::Water => FogMode::Exp {
            density: if context.water_breathing {
                WATER_BREATHING_DENSITY
            } else {
                WATER_DENSITY
            },
        },
        FogMedium::Lava => FogMode::Exp {
            density: LAVA_DENSITY,
        },
        FogMedium::Air if matches!(environment.dimension, -1 | 1) => {
            // `doesXZShowFog`: the Nether and the End close in.
            FogMode::Linear {
                start: far * 0.05,
                end: far.min(192.0) * 0.5,
            }
        }
        FogMedium::Air if sky => FogMode::Linear {
            start: 0.0,
            end: far,
        },
        FogMedium::Air => FogMode::Linear {
            start: far * 0.75,
            end: far,
        },
    }
}

/// The part of `updateFogColor` after the weather dimming: the liquid colour,
/// the eye light, the void, Blindness and Night Vision.
fn final_color(base: [f32; 3], environment: &RenderEnvironment) -> [f32; 3] {
    let context = &environment.fog;
    let mut color = match context.medium {
        FogMedium::Air => base,
        FogMedium::Water => {
            // Water Breathing lightens the water: `f12 * 0.3 + 0.6` with no
            // Respiration.
            let lift = if context.water_breathing { 0.6 } else { 0.0 };
            [0.02 + lift, 0.02 + lift, 0.2 + lift]
        }
        FogMedium::Lava => [0.6, 0.1, 0.0],
    };

    let view = f32::from(environment.view_distance) / 32.0;
    let light = context.light_brightness * (1.0 - view) + view;
    color = color.map(|channel| channel * light);

    let void = void_factor(context);
    if void < 1.0 {
        color = color.map(|channel| channel * void * void);
    }
    if let Some(ticks) = context.night_vision_ticks {
        color = night_vision(
            color,
            night_vision_brightness(ticks, environment.partial_tick),
        );
    }
    color
}

/// The `d1` of `updateFogColor`: the eye height over 32, forced to `0` by
/// Blindness until its last second, clamped at zero.
fn void_factor(context: &FogContext) -> f32 {
    let mut factor = context.eye_y * VOID_FOG_Y_FACTOR;
    if let Some(ticks) = context.blindness_ticks {
        factor = if ticks < BLINDNESS_FADE_TICKS {
            factor * (1.0 - ticks as f32 / BLINDNESS_FADE_TICKS as f32)
        } else {
            0.0
        };
    }
    factor.max(0.0)
}

/// `EntityRenderer.getNightVisionBrightness`: steady, then flashing for the
/// last ten seconds.
#[must_use]
pub fn night_vision_brightness(ticks: u32, partial_tick: f32) -> f32 {
    if ticks > 200 {
        1.0
    } else {
        0.7 + ((ticks as f32 - partial_tick) * std::f32::consts::PI * 0.2).sin() * 0.3
    }
}

/// Scales the fog toward its brightest colour: the strongest channel reaches
/// `1.0` at full `brightness`. A black channel stays black (the reference
/// divides by zero there and ends with `NaN`).
fn night_vision(color: [f32; 3], brightness: f32) -> [f32; 3] {
    let scale = color
        .iter()
        .filter(|channel| **channel > 0.0)
        .map(|channel| 1.0 / channel)
        .fold(f32::INFINITY, f32::min);
    if !scale.is_finite() {
        return color;
    }
    color.map(|channel| channel * (1.0 - brightness) + channel * scale * brightness)
}

/// `World.getLightBrightness` at the eye from the block and sky light stored
/// at its block, as `World.getLight(pos, true)` reads them: the sky light
/// lowered by the time of day, then the brighter channel through the
/// `lightBrightnessTable`. Rain is ignored (the game thread does not know the
/// weather), which makes a rainy day look slightly brighter than vanilla.
#[must_use]
pub fn light_brightness(block_light: u8, sky_light: u8, dimension: i8, world_time: i64) -> f32 {
    let subtracted = crate::sky::skylight_subtracted(world_time);
    let level = sky_light
        .saturating_sub(subtracted)
        .max(block_light)
        .min(15);
    let ambient = if dimension == -1 { NETHER_AMBIENT } else { 0.0 };
    let dark = 1.0 - f32::from(level) / 15.0;
    (1.0 - dark) / (dark * 3.0 + 1.0) * (1.0 - ambient) + ambient
}

#[cfg(test)]
mod tests;
