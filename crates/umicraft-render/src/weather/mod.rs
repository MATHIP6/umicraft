// SPDX-License-Identifier: MIT OR Apache-2.0

//! Rain and snow, mirroring `EntityRenderer.renderRainSnow` of the 1.8
//! reference (P7f).
//!
//! | Module | Owns |
//! |---|---|
//! | `field` | [`PrecipitationField`]: the columns the game thread samples |
//! | `geometry` | the quads of one frame, built like the reference |
//! | `texture` | the original stand-ins for `rain.png` and `snow.png` |
//!
//! # Draw order
//!
//! The reference draws the weather after the entities and the particles and
//! before the translucent terrain, with the depth test, no depth write, alpha
//! blending and the fog. The renderer calls [`WeatherRenderer::draw`] at that
//! point, with the camera bind group of the terrain (the fog, the sun
//! brightness and the gamma are the terrain's).
//!
//! # Frame cost
//!
//! At most 441 quads (fancy) are rebuilt on the CPU each frame from the grid
//! the game thread published: no world access, no allocation after the first
//! frame, and nothing at all while it does not rain.

mod field;
mod geometry;
mod texture;

use std::sync::Arc;

use crate::sky::{self, SkyImage};
use crate::{Camera, RenderEnvironment};
pub use field::{PrecipitationCell, PrecipitationField, FIELD_RADIUS};
use geometry::{FrameInput, Precipitation, WeatherMesh, WeatherVertex, MAX_VERTICES};

/// The shader with the vanilla lightmap in front.
const SHADER: &str = concat!(
    include_str!("../lightmap.wgsl"),
    include_str!("weather.wgsl")
);

/// GPU resources of the weather pass.
pub(crate) struct WeatherRenderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    rain: wgpu::BindGroup,
    snow: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    mesh: WeatherMesh,
    field: Arc<PrecipitationField>,
    fancy: bool,
}

impl WeatherRenderer {
    /// Creates the pipeline and the fallback pictures. `camera_layout` is the
    /// layout of the shared camera uniform (group 0).
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let layout = picture_layout(device);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("weather shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("weather pipeline layout"),
            bind_group_layouts: &[Some(camera_layout), Some(&layout)],
            immediate_size: 0,
        });
        let sampler = sky::sampler(device, wgpu::AddressMode::Repeat);
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("weather vertices"),
            size: (MAX_VERTICES * std::mem::size_of::<WeatherVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline: pipeline(device, &pipeline_layout, &shader, format),
            rain: picture(device, queue, &layout, &sampler, &texture::fallback_rain()),
            snow: picture(device, queue, &layout, &sampler, &texture::fallback_snow()),
            layout,
            sampler,
            vertex_buffer,
            mesh: WeatherMesh::default(),
            field: Arc::default(),
            fancy: true,
        }
    }

    /// Replaces the pictures; `None` or a malformed image restores the
    /// original fallback.
    pub(crate) fn set_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rain: Option<&SkyImage>,
        snow: Option<&SkyImage>,
    ) {
        let usable = |image: Option<&SkyImage>, fallback: fn() -> SkyImage| match image {
            Some(image) if image.is_valid() => image.clone(),
            _ => fallback(),
        };
        let rain = usable(rain, texture::fallback_rain);
        let snow = usable(snow, texture::fallback_snow);
        self.rain = picture(device, queue, &self.layout, &self.sampler, &rain);
        self.snow = picture(device, queue, &self.layout, &self.sampler, &snow);
    }

    /// Stores the columns to draw and the detail level (`fancyGraphics`).
    pub(crate) fn set_field(&mut self, field: Arc<PrecipitationField>, fancy: bool) {
        self.field = field;
        self.fancy = fancy;
    }

    /// Builds and uploads the quads of the frame. `ticks` is the client tick
    /// clock with its fraction (`rendererUpdateCount + partialTicks`).
    pub(crate) fn update(
        &mut self,
        queue: &wgpu::Queue,
        camera: &Camera,
        environment: RenderEnvironment,
        ticks: f64,
    ) {
        let input = FrameInput {
            eye: camera.position.as_dvec3(),
            strength: environment.rain_strength,
            fancy: self.fancy,
            ticks,
        };
        geometry::build(&self.field, &input, &mut self.mesh);
        if !self.mesh.vertices.is_empty() {
            queue.write_buffer(
                &self.vertex_buffer,
                0,
                bytemuck::cast_slice(&self.mesh.vertices),
            );
        }
    }

    /// Draws the frame's runs with the terrain's camera bind group.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, camera: &wgpu::BindGroup) {
        if self.mesh.runs.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        for run in &self.mesh.runs {
            let picture = match run.kind {
                Precipitation::Rain => &self.rain,
                Precipitation::Snow => &self.snow,
            };
            pass.set_bind_group(1, picture, &[]);
            pass.draw(run.vertices.clone(), 0..1);
        }
    }
}

/// The bind layout of a picture: texture and sampler.
fn picture_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("weather picture layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn picture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    image: &SkyImage,
) -> wgpu::BindGroup {
    let view = sky::upload(device, queue, image);
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("weather picture"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// Alpha blending, depth test without depth write, no culling
/// (`disableCull`, `depthMask(false)`).
fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("weather pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<WeatherVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &WeatherVertex::ATTRIBUTES,
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..wgpu::PrimitiveState::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::renderer::DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::Zero,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shader is only compiled on a GPU device; this validates it without one.
    #[test]
    fn the_weather_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(SHADER).expect("the shader parses");
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
    fn the_vertex_is_eight_floats() {
        assert_eq!(std::mem::size_of::<WeatherVertex>(), 32);
    }
}
