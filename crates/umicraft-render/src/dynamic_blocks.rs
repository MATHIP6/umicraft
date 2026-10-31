// SPDX-License-Identifier: MIT OR Apache-2.0

//! The blocks a piston is moving (P7d), drawn with the terrain pipelines.
//!
//! They change place every frame, so they cannot live in a chunk mesh. The
//! client rebuilds one small [`Mesh`] per frame and this pass rewrites two
//! growable buffers from it, one per terrain pass: the opaque faces share the
//! chunks' cutout pipeline, glass and ice the blended one.

use umicraft_mesh::Mesh;

use crate::items::DynamicMesh;

/// Buffers of the moving blocks, one per terrain pass.
pub(crate) struct MovingBlocksPass {
    opaque: DynamicMesh,
    transparent: DynamicMesh,
}

impl MovingBlocksPass {
    /// Creates empty buffers; nothing is drawn until [`MovingBlocksPass::update`].
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        Self {
            opaque: DynamicMesh::new(device, "moving blocks"),
            transparent: DynamicMesh::new(device, "moving blocks transparent"),
        }
    }

    /// Replaces the blocks in flight; an empty mesh hides them.
    pub(crate) fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, mesh: &Mesh) {
        self.opaque
            .update_geometry(device, queue, &mesh.vertices, &mesh.indices);
        self.transparent.update_geometry(
            device,
            queue,
            &mesh.transparent_vertices,
            &mesh.transparent_indices,
        );
    }

    /// Draws the opaque faces; the pass must have the opaque terrain pipeline.
    pub(crate) fn draw_opaque(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.opaque.draw(pass);
    }

    /// Draws the blended faces; the pass must have the transparent terrain
    /// pipeline.
    pub(crate) fn draw_transparent(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.transparent.draw(pass);
    }
}
