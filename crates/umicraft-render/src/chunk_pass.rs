// SPDX-License-Identifier: MIT OR Apache-2.0

//! The GPU side of a chunk mesh pass: vertices only, solid quads first, drawn
//! with one shared quad index buffer (F2-2a, F2-2d).
//!
//! Every quad the mesher emits has the same index pattern
//! `[0, 1, 2, 0, 2, 3] + 4 * quad`, so one static index buffer serves every
//! chunk and a mesh uploads vertices only. The opaque pass is also split in two
//! ranges: quads whose texture has no texel the cutout test could discard
//! (SOLID) come first and are drawn by a pipeline without `discard`, which
//! keeps early depth rejection on; the rest (CUTOUT) follow.

use std::ops::Range;

use umicraft_mesh::Vertex;
use wgpu::util::DeviceExt;

/// Indices per quad.
const INDICES_PER_QUAD: u32 = 6;
/// Vertices per quad.
const VERTICES_PER_QUAD: usize = 4;
/// The alpha below which the terrain shader discards a texel (0.5 in 0..=255).
const CUTOUT_ALPHA: u8 = 128;
/// Quads the shared index buffer starts with; it doubles when a mesh needs more.
const MIN_QUADS: u32 = 4096;

/// One uploaded pass of a chunk mesh.
pub(crate) struct ChunkPass {
    pub(crate) vertex_buffer: wgpu::Buffer,
    /// Quads at the start of the buffer that need no alpha test.
    pub(crate) solid_quads: u32,
    /// All quads of the buffer.
    pub(crate) quads: u32,
    /// The cutout flags this pass was sorted with: [`Renderer`](crate::Renderer)
    /// bumps its epoch when the block array changes, which makes the sort
    /// stale until the mesh is uploaded again.
    pub(crate) epoch: u32,
}

impl ChunkPass {
    /// The indices of the SOLID quads.
    pub(crate) fn solid_indices(&self) -> Range<u32> {
        0..self.solid_quads * INDICES_PER_QUAD
    }

    /// The indices of the CUTOUT quads.
    pub(crate) fn cutout_indices(&self) -> Range<u32> {
        self.solid_quads * INDICES_PER_QUAD..self.quads * INDICES_PER_QUAD
    }

    /// The indices of every quad.
    pub(crate) fn all_indices(&self) -> Range<u32> {
        0..self.quads * INDICES_PER_QUAD
    }
}

/// Uploads one pass, or returns `None` when it has no whole quad (wgpu rejects
/// a zero-sized buffer).
///
/// With `cutout` (one flag per texture layer, and the epoch of the flags) the quads are reordered, solid
/// first, through `scratch`, which keeps its capacity across calls. Without
/// it all quads count as solid (the blended pass, which is not split).
pub(crate) fn create_chunk_pass(
    device: &wgpu::Device,
    label: &str,
    vertices: &[Vertex],
    cutout: Option<(&[bool], u32)>,
    scratch: &mut Vec<Vertex>,
) -> Option<ChunkPass> {
    let quads = (vertices.len() / VERTICES_PER_QUAD) as u32;
    if quads == 0 {
        return None;
    }
    let vertices = &vertices[..quads as usize * VERTICES_PER_QUAD];
    let (contents, solid_quads, epoch) = match cutout {
        Some((flags, epoch)) => {
            let solid = split_solid_cutout(vertices, flags, scratch);
            (scratch.as_slice(), solid, epoch)
        }
        None => (vertices, quads, 0),
    };
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(contents),
        usage: wgpu::BufferUsages::VERTEX,
    });
    Some(ChunkPass {
        vertex_buffer,
        solid_quads,
        quads,
        epoch,
    })
}

/// Copies the quads of `vertices` into `out`, solid ones first (in their
/// order), and returns how many are solid.
///
/// A quad is cutout when the layer of its first vertex is flagged, or is
/// unknown: the safe side is the pipeline that discards.
pub(crate) fn split_solid_cutout(
    vertices: &[Vertex],
    cutout: &[bool],
    out: &mut Vec<Vertex>,
) -> u32 {
    let is_cutout = |quad: &[Vertex]| cutout.get(quad[0].layer as usize).copied().unwrap_or(true);
    out.clear();
    out.reserve(vertices.len());
    for quad in vertices.chunks_exact(VERTICES_PER_QUAD) {
        if !is_cutout(quad) {
            out.extend_from_slice(quad);
        }
    }
    let solid = (out.len() / VERTICES_PER_QUAD) as u32;
    for quad in vertices.chunks_exact(VERTICES_PER_QUAD) {
        if is_cutout(quad) {
            out.extend_from_slice(quad);
        }
    }
    solid
}

/// Which layers of a texture array have a texel the cutout test discards.
///
/// `pixels` is layer-major RGBA8, `layer_bytes` the size of one layer.
pub(crate) fn cutout_layers(pixels: &[u8], layer_bytes: usize) -> Vec<bool> {
    if layer_bytes == 0 {
        return Vec::new();
    }
    pixels
        .chunks_exact(layer_bytes)
        .map(|layer| layer.chunks_exact(4).any(|texel| texel[3] < CUTOUT_ALPHA))
        .collect()
}

/// The index pattern of `quads` quads.
fn quad_index_pattern(quads: u32) -> Vec<u32> {
    (0..quads)
        .flat_map(|quad| {
            let base = quad * VERTICES_PER_QUAD as u32;
            [base, base + 1, base + 2, base, base + 2, base + 3]
        })
        .collect()
}

/// The index buffer shared by every chunk pass.
pub(crate) struct QuadIndices {
    buffer: wgpu::Buffer,
    quads: u32,
}

impl QuadIndices {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        Self {
            buffer: Self::create(device, MIN_QUADS),
            quads: MIN_QUADS,
        }
    }

    fn create(device: &wgpu::Device, quads: u32) -> wgpu::Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shared quad indices"),
            contents: bytemuck::cast_slice(&quad_index_pattern(quads)),
            usage: wgpu::BufferUsages::INDEX,
        })
    }

    /// Makes sure the buffer covers a mesh of `quads` quads, doubling it as
    /// needed.
    pub(crate) fn ensure(&mut self, device: &wgpu::Device, quads: u32) {
        if quads > self.quads {
            self.quads = quads.next_power_of_two();
            self.buffer = Self::create(device, self.quads);
        }
    }

    /// Binds the buffer for the draws that follow.
    pub(crate) fn bind(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_index_buffer(self.buffer.slice(..), wgpu::IndexFormat::Uint32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(layer: u32, x: f32) -> [Vertex; 4] {
        let vertex = |corner: f32| Vertex {
            position: [x, corner, 0.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
            layer,
            light: 0,
            tint: 0,
        };
        [vertex(0.0), vertex(1.0), vertex(2.0), vertex(3.0)]
    }

    #[test]
    fn solid_quads_come_first_and_keep_their_order() {
        let flags = [false, true, false];
        // Layers: solid, cutout, solid, cutout, solid.
        let input: Vec<Vertex> = [(0, 0.0), (1, 1.0), (2, 2.0), (1, 3.0), (0, 4.0)]
            .iter()
            .flat_map(|&(layer, x)| quad(layer, x))
            .collect();
        let mut out = Vec::new();
        let solid = split_solid_cutout(&input, &flags, &mut out);
        assert_eq!(solid, 3);
        assert_eq!(out.len(), input.len());
        let xs: Vec<f32> = out.chunks(4).map(|q| q[0].position[0]).collect();
        assert_eq!(xs, [0.0, 2.0, 4.0, 1.0, 3.0]);
    }

    #[test]
    fn an_unknown_layer_is_cutout() {
        let input: Vec<Vertex> = quad(9, 0.0).to_vec();
        let mut out = Vec::new();
        assert_eq!(split_solid_cutout(&input, &[false], &mut out), 0);
    }

    #[test]
    fn a_layer_with_a_hole_is_cutout_and_a_full_one_is_solid() {
        let mut pixels = vec![255_u8; 2 * 16];
        // Layer 1 (second 16 bytes): one texel with alpha 127 < 128.
        pixels[16 + 7] = 127;
        // Layer 0: alpha exactly at the threshold stays solid.
        pixels[3] = 128;
        assert_eq!(cutout_layers(&pixels, 16), [false, true]);
        assert!(cutout_layers(&pixels, 0).is_empty());
    }

    #[test]
    fn the_pattern_is_six_indices_per_quad() {
        assert_eq!(quad_index_pattern(2), [0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]);
    }
}
