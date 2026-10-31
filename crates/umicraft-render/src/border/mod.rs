// SPDX-License-Identifier: MIT OR Apache-2.0

//! The world border, mirroring `RenderGlobal.renderWorldBorder` of the 1.8
//! reference.
//!
//! | Module | Owns |
//! |---|---|
//! | `view` | [`BorderView`], [`BorderStatus`] and the `WorldBorder` arithmetic |
//! | `plan` | which walls are in reach, their fade, scroll and geometry |
//! | `texture` | the original stand-in for `forcefield.png` |
//!
//! # Draw order
//!
//! The reference draws the border at the end of `renderWorldPass`, after the
//! translucent terrain and the weather and before the clouds seen from above.
//! The renderer calls [`BorderRenderer::draw`] at the same place.
//!
//! # Approximation
//!
//! `GlStateManager.doPolygonOffset(-3, -3)` becomes the same depth bias in
//! wgpu (its unit differs between APIs, so coplanar terrain may still
//! z-fight on some drivers). The reference's one-quad-per-block walls are one
//! stretched quad per wall (same texture coordinates). The border uses the
//! diameter of the last game tick, not an interpolation inside the tick.

mod plan;
mod texture;
mod view;

use glam::Mat4;

use crate::sky::{self, SkyImage, SkyVertex};
use crate::Camera;
use plan::MAX_VERTICES;

pub use view::{BorderStatus, BorderView};

/// The uniform block of `forcefield.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct BorderUniform {
    view_proj: [[f32; 4]; 4],
    tint: [f32; 4],
}

/// GPU resources of the border pass.
pub(crate) struct BorderRenderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    view: Option<BorderView>,
    clock: f64,
    vertex_count: u32,
}

impl BorderRenderer {
    /// Creates the pipeline and the fallback picture.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world border shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("forcefield.wgsl").into()),
        });
        let layout = bind_group_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world border pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("world border uniforms"),
            size: std::mem::size_of::<BorderUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("world border vertices"),
            size: (MAX_VERTICES * std::mem::size_of::<SkyVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = sky::sampler(device, wgpu::AddressMode::Repeat);
        let bind_group = make_bind_group(
            device,
            queue,
            &layout,
            &uniform_buffer,
            &sampler,
            &texture::fallback_forcefield(),
        );
        Self {
            pipeline: pipeline(device, &pipeline_layout, &shader, format),
            layout,
            sampler,
            uniform_buffer,
            vertex_buffer,
            bind_group,
            view: None,
            clock: 0.0,
            vertex_count: 0,
        }
    }

    /// Replaces the picture; `None` or a malformed image restores the
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
                fallback = texture::fallback_forcefield();
                &fallback
            }
        };
        self.bind_group = make_bind_group(
            device,
            queue,
            &self.layout,
            &self.uniform_buffer,
            &self.sampler,
            image,
        );
    }

    /// Stores the border of the next frame and the clock (seconds) that
    /// scrolls the texture; `None` hides the border.
    pub(crate) fn set_view(&mut self, view: Option<BorderView>, clock: f64) {
        self.view = view;
        self.clock = clock;
    }

    /// Plans the frame and uploads its vertices and tint. `reach` is the
    /// render distance in blocks.
    pub(crate) fn update(&mut self, queue: &wgpu::Queue, camera: &Camera, reach: f64) {
        self.vertex_count = 0;
        let Some(view) = self.view else { return };
        let eye = camera.position.as_dvec3().to_array();
        let Some(walls) = plan::plan(&view, eye, reach, plan::scroll(self.clock)) else {
            return;
        };
        let count = walls.count as usize;
        queue.write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(&walls.vertices[..count]),
        );
        let uniform = BorderUniform {
            view_proj: (camera.view_projection() * Mat4::from_translation(camera.position))
                .to_cols_array_2d(),
            tint: walls.colour,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
        self.vertex_count = walls.count;
    }

    /// Draws the walls planned by [`Self::update`].
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

/// The bind group layout: the uniform, the picture and its sampler.
fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty,
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("world border bind group layout"),
        entries: &[
            entry(
                0,
                wgpu::ShaderStages::VERTEX_FRAGMENT,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<BorderUniform>() as u64
                    ),
                },
            ),
            entry(
                1,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
            ),
            entry(
                2,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
        ],
    })
}

fn make_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    uniforms: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    image: &SkyImage,
) -> wgpu::BindGroup {
    let view = sky::upload(device, queue, image);
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("world border bind group"),
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

/// Additive blending (`tryBlendFuncSeparate(770, 1, 1, 0)`), depth test but
/// no depth write, no culling, and the reference's polygon offset as a bias.
fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("world border pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<SkyVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &SkyVertex::ATTRIBUTES,
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
            bias: wgpu::DepthBiasState {
                constant: -3,
                slope_scale: -3.0,
                clamp: 0.0,
            },
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
                        dst_factor: wgpu::BlendFactor::One,
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
    fn the_border_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(include_str!("forcefield.wgsl"))
            .expect("the shader parses");
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
        assert_eq!(std::mem::size_of::<BorderUniform>(), 64 + 16);
    }
}
