// SPDX-License-Identifier: MIT OR Apache-2.0

//! The enchantment glint over world and hand items.
//!
//! Like MCP `RenderItem.renderEffect`, the item geometry is drawn a second
//! time with the glint texture, additively blended, on the surface that is
//! already visible: depth is tested `LessEqual` and never written, and the
//! item texel alpha masks the glow (`item_glint.wgsl`). The transforms come
//! from the same [`GlintUniform`](super::glint::GlintUniform) buffer as the
//! GUI glint, so the two stay in step. The pipeline uses the terrain bind
//! group at slot 0 (the caller picks the world or hand camera) and its own
//! glint bindings at slot 1.

use umicraft_mesh::item::ItemMesh;
use umicraft_mesh::Vertex;

use super::DynamicMesh;
use crate::renderer::{DEPTH_FORMAT, TERRAIN_ATTRIBUTES};

/// The glint fragment stage appended to the terrain vertex stage and the
/// shared glint maths.
const WORLD_GLINT_WGSL: &str = concat!(
    include_str!("../glint_common.wgsl"),
    include_str!("../terrain.wgsl"),
    include_str!("../item_glint.wgsl"),
);

/// Pulls the glint towards the camera so it wins the equal-depth comparison
/// against the surface it was copied from despite rounding in the vertex
/// transform.
const DEPTH_BIAS: wgpu::DepthBiasState = wgpu::DepthBiasState {
    constant: -2,
    slope_scale: -1.0,
    clamp: 0.0,
};

/// The glint pipeline, the world and hand glint meshes, and the glint
/// bindings once a texture exists.
pub(crate) struct WorldGlint {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    /// `None` until [`WorldGlint::set_texture`]; nothing is drawn then.
    bind_group: Option<wgpu::BindGroup>,
    world: DynamicMesh,
    hand: DynamicMesh,
}

impl WorldGlint {
    /// Builds the pipeline for a render target of `format`; `terrain_layout`
    /// is the layout of the camera, texture array and sampler at slot 0.
    pub(crate) fn new(
        device: &wgpu::Device,
        terrain_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("item glint shader"),
            source: wgpu::ShaderSource::Wgsl(WORLD_GLINT_WGSL.into()),
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("item glint bind group layout"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(
                    2,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });
        Self {
            pipeline: create_pipeline(device, &shader, terrain_layout, &layout, format),
            layout,
            bind_group: None,
            world: DynamicMesh::new(device, "world glint"),
            hand: DynamicMesh::new(device, "hand glint"),
        }
    }

    /// Binds the glint texture, its sampler and the shared time uniform.
    pub(crate) fn set_texture(
        &mut self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        uniform: &wgpu::Buffer,
    ) {
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("item glint bind group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        }));
    }

    /// Replaces the world-space geometry that glints; an empty mesh hides it.
    pub(crate) fn update_world(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &ItemMesh,
    ) {
        self.world.update(device, queue, mesh);
    }

    /// Replaces the first-person hand geometry that glints.
    pub(crate) fn update_hand(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &ItemMesh,
    ) {
        self.hand.update(device, queue, mesh);
    }

    /// Draws the world glint with the terrain bind group `camera`.
    pub(crate) fn draw_world(&self, pass: &mut wgpu::RenderPass<'_>, camera: &wgpu::BindGroup) {
        self.draw(pass, camera, &self.world);
    }

    /// Draws the hand glint with the hand bind group `camera`.
    pub(crate) fn draw_hand(&self, pass: &mut wgpu::RenderPass<'_>, camera: &wgpu::BindGroup) {
        self.draw(pass, camera, &self.hand);
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>, camera: &wgpu::BindGroup, mesh: &DynamicMesh) {
        let Some(glint) = &self.bind_group else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, glint, &[]);
        mesh.draw(pass);
    }
}

fn create_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    terrain_layout: &wgpu::BindGroupLayout,
    glint_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("item glint pipeline layout"),
        bind_group_layouts: &[Some(terrain_layout), Some(glint_layout)],
        immediate_size: 0,
    });
    // Colour adds on top of the item; alpha is left as it is.
    let additive = wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("item glint pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &TERRAIN_ATTRIBUTES,
            })],
        },
        primitive: wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: DEPTH_BIAS,
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_glint"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(additive),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::WORLD_GLINT_WGSL;

    #[test]
    fn the_world_glint_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(WORLD_GLINT_WGSL).expect("the shader parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("the shader validates");
        for entry in ["vs_main", "fs_glint"] {
            assert!(
                module.entry_points.iter().any(|point| point.name == entry),
                "missing {entry}"
            );
        }
    }
}
