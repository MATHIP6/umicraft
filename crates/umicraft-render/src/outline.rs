// SPDX-License-Identifier: MIT OR Apache-2.0

//! Wireframe lines: the dark outline around the block the player looks at and
//! the coloured debug lines (F3 + B hitboxes, F3 + G chunk borders).
//!
//! The game thread sends plain world-space boxes ([`OutlineBox`]) or lines
//! ([`DebugLine`]); this module turns each box into 12 edges and draws the
//! edges with a pipeline that tests depth but does not write it, as vanilla
//! does.
//!
//! Vanilla sets `glLineWidth(2.0)`, but wgpu only draws one-pixel lines. Each
//! edge is therefore one instance of a quad that the vertex shader widens in
//! screen space to [`LINE_WIDTH_PX`] pixels.

use crate::renderer::DEPTH_FORMAT;

/// Growth of each box on every side, in blocks.
///
/// MCP `RenderGlobal.java:1879,1889` (`drawSelectionBox`): the box is
/// expanded by `0.002` so the lines do not z-fight with the block faces.
pub const OUTLINE_GROWTH: f32 = 0.002;

/// Vertices of one box: 12 edges of two vertices each.
pub const VERTICES_PER_BOX: usize = 24;

/// The most boxes of the selection outline; a 1.8 block shape has at most two.
const MAX_SELECTION_BOXES: usize = 4;

/// Edges the selection renderer holds: 12 per box.
pub(crate) const SELECTION_EDGES: usize = MAX_SELECTION_BOXES * VERTICES_PER_BOX / 2;

/// The most lines of the debug view; hitboxes and chunk borders beyond it
/// are dropped rather than growing the buffer.
pub const MAX_DEBUG_LINES: usize = 16_384;

/// The colour of the selection outline: MCP `RenderGlobal.drawSelectionBox`
/// uses `(0, 0, 0, 0.4)`.
const SELECTION_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.4];

/// The vertices of the quad that draws one edge.
const QUAD_VERTICES: u32 = 6;

/// The line width in pixels: vanilla draws 2 px lines at any window size
/// (MCP `RenderGlobal.drawSelectionBox`: `glLineWidth(2.0F)`).
pub const LINE_WIDTH_PX: f32 = 2.0;

/// One coloured line in world space, as plain data for the renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebugLine {
    /// The first end point.
    pub from: [f32; 3],
    /// The second end point.
    pub to: [f32; 3],
    /// The straight (non-premultiplied) RGBA colour.
    pub color: [f32; 4],
}

/// One edge as the vertex shader reads it: end points and colour.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Edge {
    from: [f32; 3],
    to: [f32; 3],
    color: [f32; 4],
}

/// An axis-aligned box in world space, as plain data for the renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutlineBox {
    /// The corner with the smallest coordinates.
    pub min: [f32; 3],
    /// The corner with the largest coordinates.
    pub max: [f32; 3],
}

/// The 24 vertices (12 edges, two end points each) of `outline`, grown by
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

/// A pipeline that draws a bounded number of coloured edges.
pub(crate) struct LineRenderer {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    /// The viewport size and line width read by the vertex shader.
    screen: wgpu::Buffer,
    screen_group: wgpu::BindGroup,
    capacity: usize,
    edge_count: u32,
}

impl LineRenderer {
    /// Creates the pipeline for at most `capacity` edges; `camera_layout` is
    /// the shared camera uniform.
    pub(crate) fn new(
        device: &wgpu::Device,
        camera_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        capacity: usize,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("outline shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("outline.wgsl").into()),
        });
        let screen_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("outline screen layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("outline pipeline layout"),
            bind_group_layouts: &[Some(camera_layout), Some(&screen_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("outline pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                // One instance per edge: its two end points and its colour.
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Edge>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3, 1 => Float32x3, 2 => Float32x4
                    ],
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
                topology: wgpu::PrimitiveTopology::TriangleList,
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
            size: (capacity * std::mem::size_of::<Edge>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let screen = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("outline screen"),
            size: std::mem::size_of::<[f32; 4]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let screen_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("outline screen group"),
            layout: &screen_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen.as_entire_binding(),
            }],
        });
        Self {
            pipeline,
            buffer,
            screen,
            screen_group,
            capacity,
            edge_count: 0,
        }
    }

    /// Replaces the edges to draw; an empty slice hides them. Edges beyond
    /// the capacity are ignored.
    fn set_edges(&mut self, queue: &wgpu::Queue, edges: &[Edge], viewport: (u32, u32)) {
        let edges = &edges[..edges.len().min(self.capacity)];
        if !edges.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(edges));
        }
        self.edge_count = edges.len() as u32;
        let screen = [viewport.0 as f32, viewport.1 as f32, LINE_WIDTH_PX, 0.0];
        queue.write_buffer(&self.screen, 0, bytemuck::cast_slice(&screen));
    }

    /// Replaces the boxes to draw in the selection colour; an empty slice
    /// hides the outline. Boxes beyond [`MAX_SELECTION_BOXES`] are ignored.
    pub(crate) fn set_boxes(
        &mut self,
        queue: &wgpu::Queue,
        boxes: &[OutlineBox],
        viewport: (u32, u32),
    ) {
        let edges: Vec<Edge> = boxes
            .iter()
            .take(MAX_SELECTION_BOXES)
            .flat_map(|outline| {
                edge_vertices(outline)
                    .chunks_exact(2)
                    .map(|pair| Edge {
                        from: pair[0],
                        to: pair[1],
                        color: SELECTION_COLOR,
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        self.set_edges(queue, &edges, viewport);
    }

    /// Replaces the coloured lines to draw; an empty slice hides them. Lines
    /// beyond [`MAX_DEBUG_LINES`] are ignored.
    pub(crate) fn set_lines(
        &mut self,
        queue: &wgpu::Queue,
        lines: &[DebugLine],
        viewport: (u32, u32),
    ) {
        let edges: Vec<Edge> = lines
            .iter()
            .map(|line| Edge {
                from: line.from,
                to: line.to,
                color: line.color,
            })
            .collect();
        self.set_edges(queue, &edges, viewport);
    }

    /// Draws the edges with the camera bind group at slot 0.
    pub(crate) fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera_bind_group: &'a wgpu::BindGroup,
    ) {
        if self.edge_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera_bind_group, &[]);
        pass.set_bind_group(1, &self.screen_group, &[]);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..QUAD_VERTICES, 0..self.edge_count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT: OutlineBox = OutlineBox {
        min: [0.0; 3],
        max: [1.0; 3],
    };

    /// The shader is only compiled on a GPU device; this parses and
    /// validates it without one, so a typo does not wait for a window.
    #[test]
    fn the_outline_shader_is_valid_wgsl() {
        let module =
            naga::front::wgsl::parse_str(include_str!("outline.wgsl")).expect("the shader parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("the shader validates");
    }

    #[test]
    fn the_line_is_two_pixels_wide_like_vanilla() {
        assert_eq!(LINE_WIDTH_PX, 2.0);
    }

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
