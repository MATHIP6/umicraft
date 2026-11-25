// SPDX-License-Identifier: MIT OR Apache-2.0

//! The world geometry of mods (API 1.4, ADR-0097): lines and filled triangles in
//! world space, each either hidden by the world or drawn through it.
//!
//! Drawn after the translucent terrain and before the hand: the hand stays in
//! front, and a shape behind water or glass is tinted like the selection box. The
//! lines go through the line renderer of the outline (widened quads), the shapes
//! through a small pipeline of their own. Positions are relative to the render
//! origin, like the entities, so they stay precise far from the world origin.

use bytemuck::{Pod, Zeroable};

use crate::outline::{Edge, LineDepth, LineRenderer};
use crate::renderer::DEPTH_FORMAT;

/// Lines the renderer holds per depth mode; more are dropped.
pub const MAX_MOD_LINES: usize = 16_384;
/// Triangles the renderer holds per depth mode; more are dropped.
pub const MAX_MOD_TRIANGLES: usize = 16_384;

/// A line of a mod, ready to draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModWorldLine {
    /// One end, relative to the render origin.
    pub from: [f32; 3],
    /// The other end, relative to the render origin.
    pub to: [f32; 3],
    /// Straight-alpha RGBA.
    pub color: [f32; 4],
    /// Width in physical pixels (at least one; a thinner line comes fainter).
    pub width: f32,
    /// Drawn over the world instead of hidden by it.
    pub through_walls: bool,
}

/// A filled triangle of a mod, ready to draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModWorldTriangle {
    /// The corners, relative to the render origin.
    pub points: [[f32; 3]; 3],
    /// Straight-alpha RGBA.
    pub color: [f32; 4],
    /// Drawn over the world instead of hidden by it.
    pub through_walls: bool,
}

/// A vertex of a filled shape.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
struct FillVertex {
    position: [f32; 3],
    color: [f32; 4],
}

/// Filled triangles under one depth mode.
struct FillRenderer {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    vertex_count: u32,
}

impl FillRenderer {
    fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::PipelineLayout,
        format: wgpu::TextureFormat,
        depth: LineDepth,
    ) -> Self {
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mod world fill pipeline"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<FillVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            // Both sides: a mod's polygon has no reliable winding.
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(match depth {
                    LineDepth::Tested => wgpu::CompareFunction::LessEqual,
                    LineDepth::Ignored => wgpu::CompareFunction::Always,
                }),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mod world fill vertices"),
            size: (MAX_MOD_TRIANGLES * 3 * std::mem::size_of::<FillVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            buffer,
            vertex_count: 0,
        }
    }

    fn set(&mut self, queue: &wgpu::Queue, vertices: &[FillVertex]) {
        let vertices = &vertices[..vertices.len().min(MAX_MOD_TRIANGLES * 3)];
        if !vertices.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(vertices));
        }
        self.vertex_count = vertices.len() as u32;
    }

    fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, camera: &'a wgpu::BindGroup) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera, &[]);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

/// The world geometry of the mods, in its four pipelines.
pub(crate) struct ModWorldRenderer {
    lines: [LineRenderer; 2],
    fills: [FillRenderer; 2],
    /// Scratch buffers, reused every frame.
    edges: [Vec<Edge>; 2],
    vertices: [Vec<FillVertex>; 2],
}

/// The index of a depth mode in the renderer's pairs: hidden ones first.
fn slot(through_walls: bool) -> usize {
    usize::from(through_walls)
}

impl ModWorldRenderer {
    /// Creates the pipelines; `camera_layout` is the shared camera uniform.
    pub(crate) fn new(
        device: &wgpu::Device,
        camera_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let line =
            |depth| LineRenderer::with_depth(device, camera_layout, format, MAX_MOD_LINES, depth);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mod world fill shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("world_fill.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mod world fill layout"),
            bind_group_layouts: &[Some(camera_layout)],
            immediate_size: 0,
        });
        let fill = |depth| FillRenderer::new(device, &shader, &layout, format, depth);
        Self {
            lines: [line(LineDepth::Tested), line(LineDepth::Ignored)],
            fills: [fill(LineDepth::Tested), fill(LineDepth::Ignored)],
            edges: [Vec::new(), Vec::new()],
            vertices: [Vec::new(), Vec::new()],
        }
    }

    /// Replaces what is drawn; empty slices hide it all.
    pub(crate) fn set(
        &mut self,
        queue: &wgpu::Queue,
        lines: &[ModWorldLine],
        triangles: &[ModWorldTriangle],
        viewport: (u32, u32),
    ) {
        split_lines(lines, &mut self.edges);
        split_triangles(triangles, &mut self.vertices);
        for (renderer, edges) in self.lines.iter_mut().zip(&self.edges) {
            renderer.set_edges(queue, edges, viewport);
        }
        for (renderer, vertices) in self.fills.iter_mut().zip(&self.vertices) {
            renderer.set(queue, vertices);
        }
    }

    /// Draws the shapes the world hides, then those drawn through it, each kind
    /// shapes first and lines over them.
    pub(crate) fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, camera: &'a wgpu::BindGroup) {
        for (fills, lines) in self.fills.iter().zip(&self.lines) {
            fills.draw(pass, camera);
            lines.draw(pass, camera);
        }
    }
}

/// Sorts `lines` into the edges of each depth mode. A line under one pixel is
/// drawn one pixel wide and as faint as the share of the pixel it covers.
fn split_lines(lines: &[ModWorldLine], edges: &mut [Vec<Edge>; 2]) {
    for list in edges.iter_mut() {
        list.clear();
    }
    for line in lines {
        let coverage = line.width.clamp(0.0, 1.0);
        let mut color = line.color;
        if line.width < 1.0 {
            color[3] *= coverage;
        }
        edges[slot(line.through_walls)].push(Edge {
            from: line.from,
            to: line.to,
            color,
            width: line.width.max(1.0),
        });
    }
}

/// Sorts `triangles` into the vertices of each depth mode.
fn split_triangles(triangles: &[ModWorldTriangle], vertices: &mut [Vec<FillVertex>; 2]) {
    for list in vertices.iter_mut() {
        list.clear();
    }
    for triangle in triangles {
        vertices[slot(triangle.through_walls)].extend(triangle.points.map(|position| FillVertex {
            position,
            color: triangle.color,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fill_shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(include_str!("world_fill.wgsl"))
            .expect("the shader parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("the shader validates");
    }

    fn line(width: f32, through_walls: bool) -> ModWorldLine {
        ModWorldLine {
            from: [0.0; 3],
            to: [1.0; 3],
            color: [1.0, 0.0, 0.0, 0.8],
            width,
            through_walls,
        }
    }

    #[test]
    fn lines_go_to_their_depth_mode_and_thin_ones_fade() {
        let mut edges = [Vec::new(), Vec::new()];
        split_lines(&[line(3.0, false), line(0.5, true)], &mut edges);
        assert_eq!(edges[0].len(), 1);
        assert_eq!(edges[0][0].width, 3.0);
        assert_eq!(edges[0][0].color[3], 0.8);
        assert_eq!(edges[1][0].width, 1.0);
        assert!((edges[1][0].color[3] - 0.4).abs() < 1e-6);
        split_lines(&[], &mut edges);
        assert!(edges.iter().all(Vec::is_empty), "each frame starts empty");
    }

    #[test]
    fn triangles_go_to_their_depth_mode() {
        let triangle = |through_walls| ModWorldTriangle {
            points: [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            color: [0.0, 1.0, 0.0, 0.5],
            through_walls,
        };
        let mut vertices = [Vec::new(), Vec::new()];
        split_triangles(
            &[triangle(true), triangle(true), triangle(false)],
            &mut vertices,
        );
        assert_eq!((vertices[0].len(), vertices[1].len()), (3, 6));
        assert_eq!(vertices[1][1].position, [1.0, 0.0, 0.0]);
        assert_eq!(vertices[1][1].color, [0.0, 1.0, 0.0, 0.5]);
    }
}
