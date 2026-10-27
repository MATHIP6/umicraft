// SPDX-License-Identifier: MIT OR Apache-2.0

//! Block selection outline: the dark wireframe around the block the player
//! looks at.
//!
//! The game thread sends plain world-space boxes ([`OutlineBox`]); this module
//! turns each into 12 line segments and draws them with a `LineList`
//! pipeline that tests depth but does not write it, as vanilla does.
//!
//! Vanilla sets `glLineWidth(2.0)`; wgpu has no line width other than one
//! pixel, so the outline is thinner than the reference. Widening it would
//! need screen-space quad expansion and is not worth a second pipeline yet.

use crate::renderer::DEPTH_FORMAT;

/// Growth of each box on every side, in blocks.
///
/// MCP `RenderGlobal.java:1879,1889` (`drawSelectionBox`): the box is
/// expanded by `0.002` so the lines do not z-fight with the block faces.
pub const OUTLINE_GROWTH: f32 = 0.002;

/// Vertices of one box: 12 edges of two vertices each.
pub const VERTICES_PER_BOX: usize = 24;

/// The most boxes drawn at once; a 1.8 block shape has at most two.
const MAX_BOXES: usize = 4;

const BUFFER_BYTES: u64 = (MAX_BOXES * VERTICES_PER_BOX * std::mem::size_of::<[f32; 3]>()) as u64;

/// An axis-aligned box in world space, as plain data for the renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutlineBox {
    /// The corner with the smallest coordinates.
    pub min: [f32; 3],
    /// The corner with the largest coordinates.
    pub max: [f32; 3],
}

/// The 24 line-list vertices (12 edges) of `outline`, grown by
/// [`OUTLINE_GROWTH`] on every side.
///
/// Corner `i` has `x = max` when bit 0 is set, `y = max` for bit 1 and
/// `z = max` for bit 2; an edge joins two corners that differ in one bit.
#[must_use]
pub fn edge_vertices(outline: &OutlineBox) -> [[f32; 3]; VERTICES_PER_BOX] {
    let corner = |index: usize| {
        std::array::from_fn(|axis| {
            if (index >> axis) & 1 == 1 {
                outline.max[axis] + OUTLINE_GROWTH
            } else {
                outline.min[axis] - OUTLINE_GROWTH
            }
        })
    };
    let mut vertices = [[0.0; 3]; VERTICES_PER_BOX];
    let mut next = 0;
    for from in 0..8usize {
        for axis in 0..3 {
            if (from >> axis) & 1 == 0 {
                vertices[next] = corner(from);
                vertices[next + 1] = corner(from | (1 << axis));
                next += 2;
            }
        }
    }
    vertices
}

/// The selection outline pipeline and its small vertex buffer.
pub(crate) struct OutlineRenderer {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    vertex_count: u32,
}

impl OutlineRenderer {
    /// Creates the pipeline; `camera_layout` is the shared camera uniform.
    pub(crate) fn new(
        device: &wgpu::Device,
        camera_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("outline shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("outline.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("outline pipeline layout"),
            bind_group_layouts: &[Some(camera_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("outline pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 3]>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("outline vertices"),
            size: BUFFER_BYTES,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            buffer,
            vertex_count: 0,
        }
    }

    /// Replaces the boxes to draw; an empty slice hides the outline. Boxes
    /// beyond [`MAX_BOXES`] are ignored.
    pub(crate) fn set_boxes(&mut self, queue: &wgpu::Queue, boxes: &[OutlineBox]) {
        let mut vertices = Vec::with_capacity(boxes.len().min(MAX_BOXES) * VERTICES_PER_BOX);
        for outline in boxes.iter().take(MAX_BOXES) {
            vertices.extend_from_slice(&edge_vertices(outline));
        }
        if !vertices.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&vertices));
        }
        self.vertex_count = vertices.len() as u32;
    }

    /// Draws the outline with the camera bind group at slot 0.
    pub(crate) fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera_bind_group: &'a wgpu::BindGroup,
    ) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT: OutlineBox = OutlineBox {
        min: [0.0; 3],
        max: [1.0; 3],
    };

    #[test]
    fn a_box_has_twelve_edges() {
        let vertices = edge_vertices(&UNIT);
        assert_eq!(vertices.len(), 24);
        for edge in vertices.chunks(2) {
            let differing = (0..3).filter(|&a| edge[0][a] != edge[1][a]).count();
            assert_eq!(differing, 1, "an edge runs along exactly one axis");
        }
    }

    #[test]
    fn edges_are_distinct() {
        let vertices = edge_vertices(&UNIT);
        let mut keys: Vec<_> = vertices
            .chunks(2)
            .map(|e| {
                let (a, b) = (e[0].map(f32::to_bits), e[1].map(f32::to_bits));
                if a < b {
                    (a, b)
                } else {
                    (b, a)
                }
            })
            .collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), 12);
    }

    #[test]
    fn the_box_grows_by_two_thousandths_on_every_side() {
        let vertices = edge_vertices(&UNIT);
        for axis in 0..3 {
            let low = vertices.iter().map(|v| v[axis]).fold(f32::MAX, f32::min);
            let high = vertices.iter().map(|v| v[axis]).fold(f32::MIN, f32::max);
            assert!((low + 0.002).abs() < 1e-6);
            assert!((high - 1.002).abs() < 1e-6);
        }
    }
}
