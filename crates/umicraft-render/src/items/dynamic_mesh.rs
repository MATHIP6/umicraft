// SPDX-License-Identifier: MIT OR Apache-2.0

//! A per-frame item mesh with buffers that are reused between frames.
//!
//! Dropped and held items change every frame, so uploading them through
//! `create_buffer_init` would allocate GPU buffers each time. The buffers here
//! only grow (to the next power of two) and are rewritten with
//! `Queue::write_buffer`.

use umicraft_mesh::item::ItemMesh;

use crate::renderer::{draw_gpu_pass_buffers, MIN_DYNAMIC_BYTES};

/// Vertex and index buffers rewritten each frame.
pub(crate) struct DynamicMesh {
    label: &'static str,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
}

impl DynamicMesh {
    /// Creates empty buffers; nothing is drawn until [`DynamicMesh::update`].
    pub(crate) fn new(device: &wgpu::Device, label: &'static str) -> Self {
        Self {
            label,
            vertices: create_buffer(device, label, MIN_DYNAMIC_BYTES, wgpu::BufferUsages::VERTEX),
            indices: create_buffer(device, label, MIN_DYNAMIC_BYTES, wgpu::BufferUsages::INDEX),
            index_count: 0,
        }
    }

    /// Replaces the geometry; an empty mesh hides it.
    pub(crate) fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, mesh: &ItemMesh) {
        self.index_count = if mesh.vertices.is_empty() {
            0
        } else {
            mesh.indices.len() as u32
        };
        if self.index_count == 0 {
            return;
        }
        let vertex_bytes: &[u8] = bytemuck::cast_slice(&mesh.vertices);
        let index_bytes: &[u8] = bytemuck::cast_slice(&mesh.indices);
        grow(
            device,
            &mut self.vertices,
            self.label,
            vertex_bytes.len(),
            wgpu::BufferUsages::VERTEX,
        );
        grow(
            device,
            &mut self.indices,
            self.label,
            index_bytes.len(),
            wgpu::BufferUsages::INDEX,
        );
        queue.write_buffer(&self.vertices, 0, vertex_bytes);
        queue.write_buffer(&self.indices, 0, index_bytes);
    }

    /// Draws the mesh with whatever pipeline and bind group the pass has set.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.index_count != 0 {
            draw_gpu_pass_buffers(pass, &self.vertices, &self.indices, self.index_count);
        }
    }
}

fn create_buffer(
    device: &wgpu::Device,
    label: &str,
    size: u64,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Reallocates `buffer` when `needed` bytes do not fit.
fn grow(
    device: &wgpu::Device,
    buffer: &mut wgpu::Buffer,
    label: &str,
    needed: usize,
    usage: wgpu::BufferUsages,
) {
    if needed as u64 > buffer.size() {
        let size = (needed as u64).next_power_of_two().max(MIN_DYNAMIC_BYTES);
        *buffer = create_buffer(device, label, size, usage);
    }
}
