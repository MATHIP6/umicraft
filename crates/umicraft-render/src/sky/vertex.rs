// SPDX-License-Identifier: MIT OR Apache-2.0

//! The one vertex format of the sky pass and the quad helper that turns the
//! reference's `GL_QUADS` into triangles.

use bytemuck::{Pod, Zeroable};

/// A sky vertex: position relative to the eye, texture coordinate and colour.
///
/// The colour multiplies the pass tint, so untinted geometry uses white and
/// the End cube bakes its `0x282828` (`color(40, 40, 40, 255)`) here.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(super) struct SkyVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl SkyVertex {
    /// Vertex attributes for the vertex buffer layout (locations 0..3).
    pub(super) const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4];

    /// A white vertex without texture coordinates.
    pub(super) const fn plain(position: [f32; 3]) -> Self {
        Self::textured(position, [0.0; 2])
    }

    /// A white vertex with texture coordinates.
    pub(super) const fn textured(position: [f32; 3], uv: [f32; 2]) -> Self {
        Self::coloured(position, uv, [1.0; 4])
    }

    /// A vertex with every attribute.
    pub(super) const fn coloured(position: [f32; 3], uv: [f32; 2], color: [f32; 4]) -> Self {
        Self {
            position,
            uv,
            color,
        }
    }
}

/// The two triangles `0 1 2` and `0 2 3` of one `GL_QUADS` quad.
pub(super) fn quad(corners: [SkyVertex; 4]) -> [SkyVertex; 6] {
    [
        corners[0], corners[1], corners[2], corners[0], corners[2], corners[3],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vertex_is_nine_floats_without_padding() {
        assert_eq!(std::mem::size_of::<SkyVertex>(), 36);
    }

    #[test]
    fn a_quad_splits_along_the_first_diagonal() {
        let corner = |x: f32| SkyVertex::plain([x, 0.0, 0.0]);
        let triangles = quad([corner(0.0), corner(1.0), corner(2.0), corner(3.0)]);
        let xs: Vec<f32> = triangles.iter().map(|v| v.position[0]).collect();
        assert_eq!(xs, [0.0, 1.0, 2.0, 0.0, 2.0, 3.0]);
    }
}
