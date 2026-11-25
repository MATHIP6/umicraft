// SPDX-License-Identifier: MIT OR Apache-2.0

//! The entity mask of the mods' post passes (API 1.4, ADR-0097).
//!
//! The entities a mod marked with `world_highlight` are drawn once more, after
//! the world pass, into the mask texture the post chain owns: their models (the
//! skin's transparent texels dropped, as in the entity pass) in the flat colour
//! the mod chose. Marks with the depth test are tested against the world's depth
//! without writing it, so only their visible parts land in the mask; the others
//! are drawn whole. Post passes read the result as `umi_entity_mask` and turn it
//! into an outline, a glow or a blur.
//!
//! The vertices are [`EntityVertex`]es, so the batches reuse the entity buffers:
//! [`mask_vertex`] keeps the position, the texture coordinates and the alpha
//! cutoff, and stores the colour in the normal and the opacity in the light word.

use crate::batch_buffer::BatchBuffer;
use crate::entity_pass::{draw_batches, EntityVertex, ENTITY_ATTRIBUTES};
use crate::post_chain::MASK_FORMAT;
use crate::renderer::DEPTH_FORMAT;

/// The bits of the light word that hold the alpha cutoff of a layer.
const CUTOFF_BITS: u32 = 0xF800_0000;

/// The mask vertex of `vertex` for a mark of straight-alpha `color`.
#[must_use]
pub fn mask_vertex(vertex: EntityVertex, color: [f32; 4]) -> EntityVertex {
    let alpha = (color[3].clamp(0.0, 1.0) * 255.0).round() as u32;
    EntityVertex {
        normal: [color[0], color[1], color[2]],
        light: (vertex.light & CUTOFF_BITS) | alpha,
        ..vertex
    }
}

/// The marked entities and the two pipelines that draw them into the mask.
pub(crate) struct EntityMaskRenderer {
    /// Depth-tested, then drawn through the world.
    pipelines: [wgpu::RenderPipeline; 2],
    batches: [BatchBuffer; 2],
}

impl EntityMaskRenderer {
    /// Creates the pipelines; `texture_layout` is the entity textures' layout.
    pub(crate) fn new(
        device: &wgpu::Device,
        camera_layout: &wgpu::BindGroupLayout,
        texture_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("entity mask shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("entity_mask.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("entity mask layout"),
            bind_group_layouts: &[Some(camera_layout), Some(texture_layout)],
            immediate_size: 0,
        });
        let pipeline = |compare| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("entity mask pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<EntityVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &ENTITY_ATTRIBUTES,
                    })],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    // A later mark replaces an earlier one where they overlap.
                    targets: &[Some(wgpu::ColorTargetState {
                        format: MASK_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                // Both sides, as the entity pass draws some layers (capes, wings).
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(compare),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            pipelines: [
                pipeline(wgpu::CompareFunction::LessEqual),
                pipeline(wgpu::CompareFunction::Always),
            ],
            batches: [BatchBuffer::default(), BatchBuffer::default()],
        }
    }

    /// Replaces the marked entities: `tested` are drawn where the world shows
    /// them, `through` whole.
    pub(crate) fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        tested: &[(&[EntityVertex], &wgpu::BindGroup)],
        through: &[(&[EntityVertex], &wgpu::BindGroup)],
    ) {
        self.batches[0].set(device, queue, tested);
        self.batches[1].set(device, queue, through);
    }

    /// Clears `target` and draws the marked entities into it, tested against
    /// `depth` (the world's, kept intact).
    pub(crate) fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        camera: &wgpu::BindGroup,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("entity mask pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        for (pipeline, batches) in self.pipelines.iter().zip(&self.batches) {
            if !batches.is_empty() {
                draw_batches(&mut pass, pipeline, camera, batches.iter());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mask_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(include_str!("entity_mask.wgsl"))
            .expect("the shader parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("the shader validates");
    }

    #[test]
    fn a_mask_vertex_carries_the_colour_and_keeps_the_cutoff() {
        let vertex = EntityVertex {
            position: [1.0, 2.0, 3.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.25, 0.5],
            light: (7 << 27) | (1 << 8) | 0xF0,
        };
        let mask = mask_vertex(vertex, [1.0, 0.5, 0.0, 0.5]);
        assert_eq!((mask.position, mask.uv), (vertex.position, vertex.uv));
        assert_eq!(mask.normal, [1.0, 0.5, 0.0]);
        assert_eq!(mask.light >> 27, 7, "the cutoff stays");
        assert_eq!(mask.light & 0x07FF_FF00, 0, "the hurt and light bits go");
        assert_eq!(mask.light & 0xFF, 128);
        assert_eq!(mask_vertex(vertex, [0.0, 0.0, 0.0, 3.0]).light & 0xFF, 255);
    }
}
