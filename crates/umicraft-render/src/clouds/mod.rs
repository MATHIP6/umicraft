// SPDX-License-Identifier: MIT OR Apache-2.0

//! The vanilla clouds, mirroring `RenderGlobal.renderClouds` and
//! `renderCloudsFancy` of the 1.8 reference.
//!
//! | Module | Owns |
//! |---|---|
//! | `drift` | the per-frame shift and texture scroll, with the 2048 wrap |
//! | `geometry` | the static fast and fancy meshes |
//! | `texture` | the original stand-in for `clouds.png` |
//! | `gpu` | uniform, bind layout and the two pipelines |
//!
//! # Draw order
//!
//! The reference draws clouds in `EntityRenderer.renderWorldPass`, not in the
//! sky pass: before the translucent terrain while the eye is below `y = 128`,
//! after it above. They use the depth test, so terrain hides them and they
//! hide water behind them. The renderer therefore calls
//! [`CloudRenderer::draw_below`] after the opaque passes and before the
//! transparent terrain, and [`CloudRenderer::draw_above`] after it.
//!
//! # Approximation
//!
//! Like the reference, the meshes are texture driven (the alpha of the cloud
//! picture decides which texels exist), so the geometry is static and built
//! once. The frame writes one uniform; nothing is rebuilt or allocated.
//! The overworld only (`isSurfaceWorld`).

mod drift;
mod geometry;
mod gpu;
mod texture;

use std::ops::Range;

use crate::sky::{self, SkyImage, SkyKind, SkyState};
use crate::{Camera, RenderEnvironment};
use geometry::FancyRanges;
use gpu::{CloudUniform, Stage};

/// Which version of the clouds to draw (`GameSettings.clouds`, `fancyGraphics`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CloudMode {
    /// No clouds.
    Off,
    /// One flat textured layer.
    Fast,
    /// Boxes with shaded sides.
    #[default]
    Fancy,
}

/// Opacity of a cloud texel (`color(r, g, b, 0.8F)`).
const OPACITY: f32 = 0.8;

/// The cloud shader with the shared fog falloff in front.
const CLOUDS_WGSL: &str = concat!(include_str!("../fog.wgsl"), include_str!("clouds.wgsl"));

/// What the current frame draws.
struct FramePlan {
    prepass: bool,
    above: bool,
    ranges: [Range<u32>; 3],
}

/// GPU resources of the cloud pass.
pub(crate) struct CloudRenderer {
    prepass: wgpu::RenderPipeline,
    colour: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    fast: Range<u32>,
    fancy: FancyRanges,
    mode: CloudMode,
    plan: Option<FramePlan>,
}

impl CloudRenderer {
    /// Creates the pipelines, the meshes and the fallback picture.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cloud shader"),
            source: wgpu::ShaderSource::Wgsl(CLOUDS_WGSL.into()),
        });
        let layout = gpu::bind_group_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cloud pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let make = |stage| gpu::pipeline(device, &pipeline_layout, &shader, format, stage);
        let meshes = geometry::build();
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cloud vertices"),
            size: std::mem::size_of_val(meshes.vertices.as_slice()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&meshes.vertices));
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cloud uniforms"),
            size: std::mem::size_of::<CloudUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = sky::sampler(device, wgpu::AddressMode::Repeat);
        let bind_group = Self::bind_group(
            device,
            queue,
            &layout,
            &uniform_buffer,
            &sampler,
            &texture::fallback_clouds(),
        );
        Self {
            prepass: make(Stage::DepthPrepass),
            colour: make(Stage::Colour),
            layout,
            sampler,
            uniform_buffer,
            vertex_buffer,
            bind_group,
            fast: meshes.fast,
            fancy: meshes.fancy,
            mode: CloudMode::default(),
            plan: None,
        }
    }

    fn bind_group(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        uniforms: &wgpu::Buffer,
        sampler: &wgpu::Sampler,
        image: &SkyImage,
    ) -> wgpu::BindGroup {
        let view = sky::upload(device, queue, image);
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cloud bind group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    /// Selects the version to draw.
    pub(crate) fn set_mode(&mut self, mode: CloudMode) {
        self.mode = mode;
    }

    /// Replaces the cloud picture; `None` or a malformed image restores the
    /// original fallback.
    pub(crate) fn set_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: Option<&SkyImage>,
    ) {
        let fallback;
        let image = match image {
            Some(image) if image.is_valid() => image,
            _ => {
                fallback = texture::fallback_clouds();
                &fallback
            }
        };
        self.bind_group = Self::bind_group(
            device,
            queue,
            &self.layout,
            &self.uniform_buffer,
            &self.sampler,
            image,
        );
    }

    /// Plans the frame and writes its uniform. `ticks` is the client tick
    /// clock including the sub-tick fraction (`cloudTickCounter + partial`).
    pub(crate) fn update(
        &mut self,
        queue: &wgpu::Queue,
        camera: &Camera,
        environment: RenderEnvironment,
        sky: &SkyState,
        ticks: f64,
    ) {
        self.plan = None;
        if SkyKind::of(environment.dimension) != SkyKind::Overworld {
            return;
        }
        let eye = camera.eye().to_array();
        let Some(placement) = drift::placement(self.mode, eye, ticks) else {
            return;
        };
        let [fog_start, fog_end, fog_density] = sky.fog.world.uniform();
        let uniform = CloudUniform {
            view_proj: camera.view_projection_around_eye().to_cols_array_2d(),
            shift: [
                placement.shift[0],
                placement.shift[1],
                placement.shift[2],
                0.0,
            ],
            scale: [placement.scale[0], 1.0, placement.scale[1], 0.0],
            uv_offset: [placement.uv_offset[0], placement.uv_offset[1], 0.0, 0.0],
            tint: [
                sky.cloud_color[0],
                sky.cloud_color[1],
                sky.cloud_color[2],
                OPACITY,
            ],
            fog_color: [sky.fog.color[0], sky.fog.color[1], sky.fog.color[2], 1.0],
            fog: [fog_start, fog_end, fog_density, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
        let fancy = self.mode == CloudMode::Fancy;
        self.plan = Some(FramePlan {
            prepass: fancy,
            // `entity.posY + eyeHeight < 128` draws before the translucent terrain.
            above: eye[1] >= drift::CLOUD_HEIGHT,
            ranges: if fancy {
                geometry::visible_ranges(&self.fancy, placement.shift[1])
            } else {
                [self.fast.clone(), 0..0, 0..0]
            },
        });
    }

    /// Draws the clouds when the eye is below them, before the translucent terrain.
    pub(crate) fn draw_below(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.draw(pass, false);
    }

    /// Draws the clouds when the eye is above them, after the translucent terrain.
    pub(crate) fn draw_above(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.draw(pass, true);
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>, above: bool) {
        let Some(plan) = self.plan.as_ref().filter(|plan| plan.above == above) else {
            return;
        };
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_bind_group(0, &self.bind_group, &[]);
        if plan.prepass {
            pass.set_pipeline(&self.prepass);
            for range in plan.ranges.iter().filter(|r| !r.is_empty()) {
                pass.draw(range.clone(), 0..1);
            }
        }
        pass.set_pipeline(&self.colour);
        for range in plan.ranges.iter().filter(|r| !r.is_empty()) {
            pass.draw(range.clone(), 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shader is only compiled on a GPU device; this validates it without one.
    #[test]
    fn the_cloud_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(CLOUDS_WGSL).expect("the shader parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("the shader validates");
        for entry in ["vs_main", "fs_main"] {
            assert!(module.entry_points.iter().any(|point| point.name == entry));
        }
    }

    #[test]
    fn the_uniform_matches_the_shader_block() {
        assert_eq!(std::mem::size_of::<CloudUniform>(), 64 + 6 * 16);
    }

    #[test]
    fn fancy_is_the_default() {
        assert_eq!(CloudMode::default(), CloudMode::Fancy);
    }
}
