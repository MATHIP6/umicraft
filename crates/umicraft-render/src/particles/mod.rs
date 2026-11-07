// SPDX-License-Identifier: MIT OR Apache-2.0

//! The particle pass (P8-a), the drawing half of `EffectRenderer`.
//!
//! | Module | Owns |
//! |---|---|
//! | `billboard` | [`ParticleSprite`], the screen axes and the quads |
//! | `texture` | the original stand-in for `particles.png` |
//!
//! # Draw order
//!
//! The reference draws the particles after the entities and before the
//! weather and the translucent terrain, with alpha blending, the fog and an
//! alpha test. Opaque particles also write depth there; this pass never does,
//! so two particles overlap by submission order. That is invisible for the
//! mostly translucent, small sprites and avoids a second pipeline.
//!
//! # Frame cost
//!
//! The client resolves each particle into a [`ParticleSprite`]; the pass turns
//! them into at most [`MAX_SPRITES`] quads in one reused vertex list and one
//! draw call, with no allocation after the first frame.

mod billboard;
mod texture;

use bytemuck::cast_slice;

use crate::sky::{self, SkyImage};
use crate::Camera;
pub use billboard::ParticleSprite;
use billboard::{quad, screen_axes, ParticleVertex};

/// The most particles a frame draws: the 4000 of each of the 8 buckets of the
/// engine.
pub const MAX_SPRITES: usize = 32_000;
/// Vertices per sprite (two triangles).
const VERTICES_PER_SPRITE: usize = 6;

/// GPU resources of the particle pass.
pub(crate) struct ParticleRenderer {
    pipeline: wgpu::RenderPipeline,
    sheet_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    sheet: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    vertices: Vec<ParticleVertex>,
}

impl ParticleRenderer {
    /// Creates the pipeline and the fallback sheet. `terrain_layout` is the
    /// layout of the terrain bind group (camera, block atlas, sampler), used
    /// as group 0 so block sprites need no second atlas.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        terrain_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let sheet_layout = sheet_layout(device);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particle pipeline layout"),
            bind_group_layouts: &[Some(terrain_layout), Some(&sheet_layout)],
            immediate_size: 0,
        });
        let sampler = sky::sampler(device, wgpu::AddressMode::ClampToEdge);
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle vertices"),
            size: (MAX_SPRITES * VERTICES_PER_SPRITE * std::mem::size_of::<ParticleVertex>())
                as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline: pipeline(device, &pipeline_layout, &shader, format),
            sheet: sheet(
                device,
                queue,
                &sheet_layout,
                &sampler,
                &texture::fallback_sheet(),
            ),
            sheet_layout,
            sampler,
            vertex_buffer,
            vertices: Vec::new(),
        }
    }

    /// Replaces `particles.png`; `None` or a malformed image restores the
    /// original fallback.
    pub(crate) fn set_sheet(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: Option<&SkyImage>,
    ) {
        let image = match image {
            Some(image) if image.is_valid() => image.clone(),
            _ => texture::fallback_sheet(),
        };
        self.sheet = sheet(device, queue, &self.sheet_layout, &self.sampler, &image);
    }

    /// Builds and uploads the quads of the frame; anything past
    /// [`MAX_SPRITES`] is dropped.
    pub(crate) fn update(
        &mut self,
        queue: &wgpu::Queue,
        camera: &Camera,
        sprites: &[ParticleSprite],
    ) {
        self.vertices.clear();
        let (right, up) = screen_axes(camera.forward());
        for sprite in sprites.iter().take(MAX_SPRITES) {
            self.vertices.extend_from_slice(&quad(sprite, right, up));
        }
        if !self.vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, cast_slice(&self.vertices));
        }
    }

    /// Draws the frame's quads with the terrain's bind group.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, terrain: &wgpu::BindGroup) {
        if self.vertices.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, terrain, &[]);
        pass.set_bind_group(1, &self.sheet, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertices.len() as u32, 0..1);
    }
}

/// The shader with the shared fog falloff in front.
const SHADER: &str = concat!(
    include_str!("../fog.wgsl"),
    include_str!("../lightmap.wgsl"),
    include_str!("particles.wgsl")
);

/// The bind layout of the sheet: texture and sampler.
fn sheet_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("particle sheet layout"),
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

fn sheet(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    image: &SkyImage,
) -> wgpu::BindGroup {
    let view = sky::upload(device, queue, image);
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("particle sheet"),
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

/// Alpha blending, depth test without depth write, no culling.
fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("particle pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<ParticleVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &ParticleVertex::ATTRIBUTES,
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
    fn the_particle_shader_is_valid_wgsl() {
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
    fn the_vertex_is_twelve_floats() {
        assert_eq!(std::mem::size_of::<ParticleVertex>(), 48);
    }

    #[test]
    fn the_buffer_holds_the_engines_whole_capacity() {
        const { assert!(MAX_SPRITES >= 8 * 4000) };
    }
}
