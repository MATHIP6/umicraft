// SPDX-License-Identifier: MIT OR Apache-2.0

//! The CPU mesh of one chunk column, split into an opaque and a blended pass.

use crate::Vertex;

/// CPU mesh for one chunk column.
///
/// Geometry is split into two passes so the renderer can draw all opaque
/// faces first and blend the transparent faces afterwards, back to front.
/// [`Mesh::vertices`]/[`Mesh::indices`] hold the opaque pass.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    /// Opaque interleaved vertices, four per face.
    pub vertices: Vec<Vertex>,
    /// Opaque triangle indices, six per face.
    pub indices: Vec<u32>,
    /// Transparent interleaved vertices (water, glass, leaves), four per face.
    pub transparent_vertices: Vec<Vertex>,
    /// Transparent triangle indices, six per face.
    pub transparent_indices: Vec<u32>,
}

impl Mesh {
    /// True when neither pass has faces.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty() && self.transparent_indices.is_empty()
    }

    /// Number of triangles across both passes.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        (self.indices.len() + self.transparent_indices.len()) / 3
    }

    /// Number of quads (faces) across both passes.
    #[must_use]
    pub fn face_count(&self) -> usize {
        (self.vertices.len() + self.transparent_vertices.len()) / 4
    }

    /// Number of opaque quads.
    #[must_use]
    pub fn opaque_face_count(&self) -> usize {
        self.vertices.len() / 4
    }

    /// Number of transparent quads.
    #[must_use]
    pub fn transparent_face_count(&self) -> usize {
        self.transparent_vertices.len() / 4
    }

    /// Appends one quad to the selected pass.
    ///
    /// The four vertices already carry the corner light/AO payload; this only
    /// fixes up the index base, so the mesher never allocates a temporary
    /// vertex buffer.
    pub(crate) fn push_quad(&mut self, transparent: bool, vertices: [Vertex; 4]) {
        let (target_vertices, target_indices) = if transparent {
            (
                &mut self.transparent_vertices,
                &mut self.transparent_indices,
            )
        } else {
            (&mut self.vertices, &mut self.indices)
        };
        let base = target_vertices.len() as u32;
        target_vertices.extend_from_slice(&vertices);
        target_indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{tint, VertexLight};

    #[test]
    fn mesh_counts_include_both_passes() {
        let vertex = Vertex {
            position: [0.0, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
            layer: 0,
            light: VertexLight::DAY.pack(),
            tint: tint::WHITE,
        };
        let mut mesh = Mesh::default();
        assert!(mesh.is_empty());
        mesh.vertices.extend([vertex; 4]);
        mesh.indices.extend([0, 1, 2, 0, 2, 3]);
        mesh.transparent_vertices.extend([vertex; 8]);
        mesh.transparent_indices
            .extend([0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]);
        assert!(!mesh.is_empty());
        assert_eq!(mesh.opaque_face_count(), 1);
        assert_eq!(mesh.transparent_face_count(), 2);
        assert_eq!(mesh.face_count(), 3);
        assert_eq!(mesh.triangle_count(), 6);
    }
}
