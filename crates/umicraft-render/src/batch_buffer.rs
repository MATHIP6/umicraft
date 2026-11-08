// SPDX-License-Identifier: MIT OR Apache-2.0

//! One persistent vertex buffer for the batches of an entity pass (F2-5a).
//!
//! The entity passes (entities, shadows, glow, ghosts, glint, the hand) used to
//! create one GPU buffer per texture batch every frame. A [`BatchBuffer`] keeps
//! one buffer per pass, rewrites it each frame with the batches laid end to end,
//! and remembers `(offset, vertices, texture)` for each. The buffer only grows
//! (to the next power of two), so a steady scene allocates nothing.

use std::ops::Range;

use crate::entity_pass::EntityVertex;

/// Bytes of one vertex.
const VERTEX_BYTES: u64 = std::mem::size_of::<EntityVertex>() as u64;
/// The buffer a pass starts with, in vertices.
const MIN_VERTICES: u64 = 1 << 12;

/// Where one batch lies in the buffer.
struct Placed {
    bytes: Range<u64>,
    vertices: u32,
    bind_group: wgpu::BindGroup,
}

/// The batches of one pass and the buffer that holds them.
#[derive(Default)]
pub(crate) struct BatchBuffer {
    buffer: Option<wgpu::Buffer>,
    /// The size of `buffer` in bytes.
    capacity: u64,
    batches: Vec<Placed>,
}

impl BatchBuffer {
    /// Replaces the batches with `batches` (a vertex list and the texture it
    /// samples each); empty lists are skipped.
    pub(crate) fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batches: &[(&[EntityVertex], &wgpu::BindGroup)],
    ) {
        self.batches.clear();
        let vertices: u64 = batches.iter().map(|(list, _)| list.len() as u64).sum();
        if vertices == 0 {
            return;
        }
        let needed = vertices * VERTEX_BYTES;
        let buffer = match &self.buffer {
            Some(buffer) if needed <= self.capacity => buffer,
            _ => {
                self.capacity = needed.next_power_of_two().max(MIN_VERTICES * VERTEX_BYTES);
                self.buffer
                    .insert(device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("entity batch vertices"),
                        size: self.capacity,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }))
            }
        };
        let Some(size) = wgpu::BufferSize::new(needed) else {
            return;
        };
        // Written straight into the staging memory: no intermediate copy.
        let Some(mut view) = queue.write_buffer_with(buffer, 0, size) else {
            return;
        };
        let mut offset = 0_u64;
        for (list, bind_group) in batches.iter().filter(|(list, _)| !list.is_empty()) {
            let bytes = bytemuck::cast_slice::<EntityVertex, u8>(list);
            let end = offset + bytes.len() as u64;
            view.slice(offset as usize..end as usize)
                .copy_from_slice(bytes);
            self.batches.push(Placed {
                bytes: offset..end,
                vertices: list.len() as u32,
                bind_group: (*bind_group).clone(),
            });
            offset = end;
        }
    }

    /// Forgets the batches; the buffer stays for the next frame.
    pub(crate) fn clear(&mut self) {
        self.batches.clear();
    }

    /// Whether there is nothing to draw.
    pub(crate) fn is_empty(&self) -> bool {
        self.batches.is_empty()
    }

    /// The batches as the entity passes draw them: the slice of the buffer, the
    /// vertex count and the texture.
    pub(crate) fn iter(
        &self,
    ) -> impl Iterator<Item = (wgpu::BufferSlice<'_>, u32, &wgpu::BindGroup)> {
        self.buffer.iter().flat_map(|buffer| {
            self.batches.iter().map(move |placed| {
                (
                    buffer.slice(placed.bytes.clone()),
                    placed.vertices,
                    &placed.bind_group,
                )
            })
        })
    }
}

/// A vertex buffer that is rewritten every frame and only ever grows.
#[derive(Default)]
pub(crate) struct GrowBuffer {
    buffer: Option<wgpu::Buffer>,
    capacity: u64,
}

impl GrowBuffer {
    /// Writes `bytes` at the start of the buffer, making it larger first (to
    /// the next power of two) when they do not fit. Returns the buffer, or
    /// `None` for no bytes (a zero-sized binding is invalid).
    pub(crate) fn write(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &'static str,
        bytes: &[u8],
    ) -> Option<&wgpu::Buffer> {
        if bytes.is_empty() {
            return None;
        }
        let needed = bytes.len() as u64;
        if needed > self.capacity || self.buffer.is_none() {
            self.capacity = needed.next_power_of_two().max(256);
            self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: self.capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let buffer = self.buffer.as_ref()?;
        queue.write_buffer(buffer, 0, bytes);
        Some(buffer)
    }

    /// The buffer written last, if any.
    pub(crate) fn get(&self) -> Option<&wgpu::Buffer> {
        self.buffer.as_ref()
    }
}
