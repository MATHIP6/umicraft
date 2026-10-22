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
//!
//! # Celestial convention
//!
//! `celestial_angle` follows `WorldProvider.calculateCelestialAngle`: it is
//! `0` at noon, `0.5` at midnight and roughly `0.85`/`0.15` at sunrise and
//! sunset. The sun travels on the celestial circle
//! `(-sin(2π·angle), cos(2π·angle), 0)`: it rises in the east (`+X`), passes
//! overhead at noon and sets in the west (`−X`). The moon is the opposite
//! point of the same circle. The `Z` component stays zero, matching the 1.8
//! `RenderGlobal.renderSky` rotation of the sun and moon quads.

use bytemuck::{Pod, Zeroable};

use crate::{Camera, RenderEnvironment};

/// Length of one day/night cycle in ticks (1.8 `WorldProvider`).
const DAY_TICKS: i64 = 24_000;
/// The 1.8 cloud layer height in blocks (`WorldProvider.getCloudHeight`).
const CLOUD_HEIGHT: f32 = 128.0;
/// Horizontal cloud drift per tick, in blocks (`RenderGlobal.renderClouds`).
const CLOUD_DRIFT_PER_TICK: f64 = 0.03;
/// Period of the cloud drift, in blocks.
///
/// The reference implementation wraps the drift at 2048 blocks so the texture
/// coordinates stay small; the same wrap keeps the `f32` hash input precise
/// after a long session.
const CLOUD_DRIFT_PERIOD: f64 = 2048.0;
/// Vanilla `cloudColour` is white; the day factor darkens it toward night.
const CLOUD_ALPHA: f32 = 0.8;
/// Temperate biome sky colour (`0x78A7FF`), an original default tint.
const TEMPERATE_SKY: [f32; 3] = [120.0 / 255.0, 167.0 / 255.0, 1.0];
/// Overworld fog colour base (`WorldProvider.getFogColor`).
const FOG_BASE: [f32; 3] = [0.752_941_2, 0.847_058_83, 1.0];
/// Zenith is darker than the horizon by this factor.
const ZENITH_SCALE: f32 = 0.55;

/// Colours and celestial positions for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyState {
    /// Celestial angle in `0.0..1.0`; `0` is noon, `0.5` is midnight and
    /// roughly `0.85`/`0.15` at sunrise/sunset (see the module convention).
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
    /// The overworld uses the 1.8 curves (`calculateCelestialAngle`,
    /// `getSunBrightness`, `getSkyColor`, `getFogColor`, `getCloudColour` and
    /// `getStarBrightness`) with rain and thunder fixed to zero. The Nether
    /// and the End get a dark dimension-specific sky with no sun, stars or
    /// clouds so they never show an overworld noon.
    ///
    /// The time of day is interpreted with Euclidean modulo, so negative
    /// (frozen-cycle) times are well defined and never panic.
    #[must_use]
    pub fn at(environment: RenderEnvironment) -> Self {
        let celestial_angle = celestial_angle(environment.world_time, environment.partial_tick);
        let moon_phase = moon_phase(environment.world_time);
        let (sun_direction, moon_direction) = celestial_directions(celestial_angle);

        if environment.dimension != 0 {
            let (sky_color, fog_color) = dimension_colors(environment.dimension);
            return Self {
                celestial_angle,
                sun_direction,
                moon_direction,
                sky_color,
                fog_color,
                // Zero alpha keeps the procedural cloud layer hidden.
                cloud_color: [0.0; 4],
                star_alpha: 0.0,
                sun_brightness: 0.0,
                moon_phase,
            };
        }

        let sky_factor = day_factor(celestial_angle);
        let fog_factor = sky_factor;
        let star_factor = (1.0 - (cos_cycle(celestial_angle) * 2.0 + 0.25)).clamp(0.0, 1.0);

        Self {
            celestial_angle,
            sun_direction,
            moon_direction,
            sky_color: [
                TEMPERATE_SKY[0] * sky_factor,
                TEMPERATE_SKY[1] * sky_factor,
                TEMPERATE_SKY[2] * sky_factor,
            ],
            fog_color: [
                FOG_BASE[0] * (fog_factor * 0.94 + 0.06),
                FOG_BASE[1] * (fog_factor * 0.94 + 0.06),
                FOG_BASE[2] * (fog_factor * 0.91 + 0.09),
            ],
            cloud_color: [
                sky_factor * 0.9 + 0.1,
                sky_factor * 0.9 + 0.1,
                sky_factor * 0.85 + 0.15,
                CLOUD_ALPHA,
            ],
            star_alpha: star_factor * star_factor * 0.5,
            sun_brightness: sun_brightness(celestial_angle),
            moon_phase,
        }
    }
}

/// Cosine of the celestial circle at `angle`.
fn cos_cycle(angle: f32) -> f32 {
    (angle * std::f32::consts::TAU).cos()
}

/// `WorldProvider.calculateCelestialAngle` with Euclidean time handling.
fn celestial_angle(world_time: i64, partial_tick: f32) -> f32 {
    let day = world_time.rem_euclid(DAY_TICKS) as f32;
    let partial = partial_tick.clamp(0.0, 1.0);
    let phase = ((day + partial) / DAY_TICKS as f32 - 0.25).rem_euclid(1.0);
    1.0 - ((phase * std::f32::consts::PI).cos() + 1.0) * 0.5
}

/// `WorldProvider.getMoonPhase` in `0..=7`, 0 being the full moon.
fn moon_phase(world_time: i64) -> u8 {
    let phase = world_time.div_euclid(DAY_TICKS).rem_euclid(8);
    // `rem_euclid(8)` is in `0..=7`, so the narrowing conversion is exact.
    phase as u8
}

/// Unit sun and moon directions on the celestial circle.
fn celestial_directions(angle: f32) -> ([f32; 3], [f32; 3]) {
    let theta = angle * std::f32::consts::TAU;
    let sin = theta.sin();
    let cos = theta.cos();
    ([-sin, cos, 0.0], [sin, -cos, 0.0])
}

/// `World.getSkyColor`/`WorldProvider.getFogColor` day factor.
fn day_factor(angle: f32) -> f32 {
    (cos_cycle(angle) * 2.0 + 0.5).clamp(0.0, 1.0)
}

/// `World.getSunBrightness` with rain and thunder zero.
///
/// The reference formula clamps `1 - (cos·2 + 0.2)`, inverts the result again
/// (`f1 = 1 - f1`) and only then applies `* 0.8 + 0.2`; the two inversions
/// simplify to the clamped cosine ramp below.
fn sun_brightness(angle: f32) -> f32 {
    (cos_cycle(angle) * 2.0 + 0.2).clamp(0.0, 1.0) * 0.8 + 0.2
}

/// Dark sky colours for the non-overworld dimensions.
///
/// The Nether is a dull ember red, the End a violet twilight and any other
/// dimension a neutral near-black. None of them is the overworld noon.
fn dimension_colors(dimension: i8) -> ([f32; 3], [f32; 3]) {
    match dimension {
        -1 => ([0.16, 0.035, 0.02], [0.13, 0.025, 0.015]),
        1 => ([0.06, 0.03, 0.10], [0.045, 0.02, 0.08]),
        _ => ([0.02, 0.02, 0.03], [0.015, 0.015, 0.025]),
    }
}

/// Per-frame uniform shared with `sky.wgsl`.
///
/// The layout is `std140`-compatible: every member is a `vec4` or a matrix of
/// `vec4`s, so the WGSL struct in `sky.wgsl` has the same offsets.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SkyUniform {
    inverse_view_projection: [[f32; 4]; 4],
    camera_position: [f32; 4],
    /// `xyz` sun direction, `w` celestial visibility (`1` only in the
    /// overworld).
    sun_direction: [f32; 4],
    /// `xyz` moon direction, `w` moon phase in `0..=7`.
    moon_direction: [f32; 4],
    /// `rgb` horizon colour, `a` star alpha.
    sky_horizon: [f32; 4],
    /// `rgb` zenith colour, `a` sun brightness.
    sky_zenith: [f32; 4],
    /// `rgb` fog colour.
    fog_color: [f32; 4],
    /// `rgba` cloud tint.
    cloud_color: [f32; 4],
    /// `xy` cloud drift, `z` cloud height, `w` unused.
    cloud_offset: [f32; 4],
}

/// GPU resources for the sky, sun, moon and cloud pass.
///
/// The renderer calls [`SkyRenderer::update`] once per frame, then
/// [`SkyRenderer::draw`] inside the main render pass before the terrain.
pub(crate) struct SkyRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
}

impl SkyRenderer {
    /// Creates the pipeline and uniform resources for `format`.
    ///
    /// The depth state uses [`crate::renderer::DEPTH_FORMAT`] so the pass can
    /// run inside the renderer's existing main render pass.
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sky.wgsl").into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky uniforms"),
            size: std::mem::size_of::<SkyUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::renderer::DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            bind_group,
            uniform_buffer,
        }
    }

    /// Writes the per-frame camera and [`SkyState`] uniforms.
    pub(crate) fn update(
        &self,
        queue: &wgpu::Queue,
        camera: &Camera,
        environment: RenderEnvironment,
    ) {
        let state = SkyState::at(environment);
        let visibility = if environment.dimension == 0 { 1.0 } else { 0.0 };
        let drift = cloud_drift(environment);
        let uniform = SkyUniform {
            inverse_view_projection: camera.view_projection().inverse().to_cols_array_2d(),
            camera_position: [camera.position.x, camera.position.y, camera.position.z, 0.0],
            sun_direction: [
                state.sun_direction[0],
                state.sun_direction[1],
                state.sun_direction[2],
                visibility,
            ],
            moon_direction: [
                state.moon_direction[0],
                state.moon_direction[1],
                state.moon_direction[2],
                f32::from(state.moon_phase),
            ],
            sky_horizon: [
                state.sky_color[0],
                state.sky_color[1],
                state.sky_color[2],
                state.star_alpha,
            ],
            sky_zenith: [
                state.sky_color[0] * ZENITH_SCALE,
                state.sky_color[1] * ZENITH_SCALE,
                state.sky_color[2] * ZENITH_SCALE,
                state.sun_brightness,
            ],
            fog_color: [
                state.fog_color[0],
                state.fog_color[1],
                state.fog_color[2],
                0.0,
            ],
            cloud_color: state.cloud_color,
            cloud_offset: [drift[0], drift[1], CLOUD_HEIGHT, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// Draws the sky behind the terrain.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// World-space cloud drift at the current time.
///
/// The drift is a pure function of the published world clock, so a frozen
/// (negative) server time keeps the clouds motionless between packets.
fn cloud_drift(environment: RenderEnvironment) -> [f32; 2] {
    let ticks = environment.world_time as f64 + f64::from(environment.partial_tick.clamp(0.0, 1.0));
    let drift = (ticks * CLOUD_DRIFT_PER_TICK).rem_euclid(CLOUD_DRIFT_PERIOD) as f32;
    [drift, 0.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sky state at `time` in the overworld.
    fn state(time: i64) -> SkyState {
        SkyState::at(RenderEnvironment::new(0, time, 0.0, 8))
    }

    /// Every colour and scalar of `state` must be finite.
    fn assert_finite(state: &SkyState) {
        let scalars = [
            state.celestial_angle,
            state.star_alpha,
            state.sun_brightness,
            state.sun_direction[0],
            state.sun_direction[1],
            state.sun_direction[2],
            state.moon_direction[0],
            state.moon_direction[1],
            state.moon_direction[2],
        ];
        assert!(scalars.iter().all(|value| value.is_finite()));
        for colour in [
            state.sky_color,
            state.fog_color,
            [
                state.cloud_color[0],
                state.cloud_color[1],
                state.cloud_color[2],
            ],
        ] {
            assert!(colour.iter().all(|channel| channel.is_finite()));
        }
        assert!(state.cloud_color[3].is_finite());
    }

    #[test]
    fn the_day_cycle_is_plausible() {
        let sunrise = state(0);
        let noon = state(6_000);
        let sunset = state(12_000);
        let midnight = state(18_000);
        for sample in [sunrise, noon, sunset, midnight] {
            assert_finite(&sample);
        }

        // The sun rises in the east, passes overhead at noon and sets in the
        // west; the moon is always its opposite.
        assert!(sunrise.sun_direction[0] > 0.5 && sunrise.sun_direction[1] > 0.5);
        assert!(noon.sun_direction[1] > 0.99);
        assert!(sunset.sun_direction[0] < -0.5 && sunset.sun_direction[1] > 0.5);
        for sample in [sunrise, noon, sunset, midnight] {
            for axis in 0..3 {
                assert!(
                    (sample.sun_direction[axis] + sample.moon_direction[axis]).abs() < 1e-6,
                    "the moon must be opposite the sun"
                );
            }
            let length = sample.sun_direction.iter().map(|v| v * v).sum::<f32>();
            assert!(
                (length - 1.0).abs() < 1e-5,
                "directions must be unit length"
            );
        }

        // Full daylight at noon and at sunrise/sunset, ambient at midnight.
        for sample in [sunrise, noon, sunset] {
            assert!(sample.sun_brightness > 0.95);
        }
        assert!(midnight.sun_brightness < 0.25);

        // The sky and the fog are full daylight at sunrise, noon and sunset
        // and darkest at midnight.
        let midnight_luminance: f32 = midnight.sky_color.iter().sum();
        let midnight_fog: f32 = midnight.fog_color.iter().sum();
        for sample in [sunrise, noon, sunset] {
            let luminance: f32 = sample.sky_color.iter().sum();
            let fog: f32 = sample.fog_color.iter().sum();
            assert!(luminance > midnight_luminance + 0.5);
            assert!(fog > midnight_fog + 0.5);
            // No stars while the sun is up.
            assert_eq!(sample.star_alpha, 0.0);
        }

        // The angle curve is fixed: noon 0, midnight 0.5, sunrise/sunset
        // roughly 0.85/0.15 (card §SkyState).
        assert!(noon.celestial_angle.abs() < 1e-6);
        assert!((sunrise.celestial_angle - 0.8536).abs() < 1e-3);
        assert!((sunset.celestial_angle - 0.1464).abs() < 1e-3);
        assert!((midnight.celestial_angle - 0.5).abs() < 1e-6);

        // Stars only at night, clouds always at the 1.8 alpha.
        assert!(midnight.star_alpha > 0.1);
        for sample in [sunrise, noon, sunset, midnight] {
            assert!((0.0..=0.5).contains(&sample.star_alpha));
            assert!((sample.cloud_color[3] - CLOUD_ALPHA).abs() < 1e-6);
        }

        // The moon phase counts whole days; all four samples are day 0.
        for sample in [sunrise, noon, sunset, midnight] {
            assert_eq!(sample.moon_phase, 0);
        }
        assert_eq!(state(24_000).moon_phase, 1);
        assert_eq!(state(24_000 * 7).moon_phase, 7);
        assert_eq!(state(24_000 * 8).moon_phase, 0);
    }

    #[test]
    fn negative_times_use_the_euclidean_clock() {
        let frozen = SkyState::at(RenderEnvironment::new(0, -6_000, 0.5, 8));
        let equivalent = SkyState::at(RenderEnvironment::new(0, 18_000, 0.5, 8));
        assert_finite(&frozen);
        assert!((frozen.celestial_angle - equivalent.celestial_angle).abs() < 1e-6);
        assert_eq!(frozen.sky_color, equivalent.sky_color);
        assert_eq!(frozen.fog_color, equivalent.fog_color);
        assert_eq!(frozen.sun_brightness, equivalent.sun_brightness);
        assert_eq!(frozen.star_alpha, equivalent.star_alpha);
        // -6000 ticks is the previous world day: phase 7, not phase 0.
        assert_eq!(frozen.moon_phase, 7);

        let extreme = SkyState::at(RenderEnvironment::new(0, i64::MIN, 0.0, 8));
        assert_finite(&extreme);
    }

    #[test]
    fn other_dimensions_have_a_dark_sky() {
        let overworld = state(6_000);
        let nether = SkyState::at(RenderEnvironment::new(-1, 6_000, 0.0, 8));
        let end = SkyState::at(RenderEnvironment::new(1, 6_000, 0.0, 8));

        for sample in [nether, end] {
            assert_finite(&sample);
            assert_eq!(sample.sun_brightness, 0.0);
            assert_eq!(sample.star_alpha, 0.0);
            assert_eq!(sample.cloud_color[3], 0.0);
            assert_ne!(sample.sky_color, overworld.sky_color);
            assert_ne!(sample.fog_color, overworld.fog_color);
            let luminance: f32 = sample.sky_color.iter().sum();
            let overworld_luminance: f32 = overworld.sky_color.iter().sum();
            assert!(
                luminance < overworld_luminance * 0.2,
                "the dimension sky must stay dark"
            );
            assert!(sample.sky_color.iter().all(|channel| *channel <= 0.2));
        }
        assert_ne!(nether.sky_color, end.sky_color);
    }

    #[test]
    fn the_cloud_drift_wraps_and_is_frozen_with_the_clock() {
        let start = cloud_drift(RenderEnvironment::new(0, 1_000, 0.0, 8));
        let later = cloud_drift(RenderEnvironment::new(0, 1_020, 0.0, 8));
        assert!(later[0] - start[0] > 0.5);
        assert_eq!(later[1], 0.0);

        let frozen = cloud_drift(RenderEnvironment::new(0, -6_000, 0.25, 8));
        let frozen_again = cloud_drift(RenderEnvironment::new(0, -6_000, 0.25, 8));
        assert_eq!(frozen, frozen_again);
        assert!((0.0..CLOUD_DRIFT_PERIOD as f32).contains(&frozen[0]));

        let wrapped = cloud_drift(RenderEnvironment::new(0, i64::MAX, 0.0, 8));
        assert!(wrapped[0].is_finite());
    }

    #[test]
    fn the_sky_uniform_matches_the_wgsl_layout() {
        // `sky.wgsl` declares a matrix plus eight `vec4`-aligned members; the
        // Rust side must keep the same offsets because the shader reads the
        // buffer directly.
        assert_eq!(std::mem::offset_of!(SkyUniform, camera_position), 64);
        assert_eq!(std::mem::offset_of!(SkyUniform, cloud_offset), 176);
        assert_eq!(std::mem::size_of::<SkyUniform>(), 12 * 16);
    }
}
