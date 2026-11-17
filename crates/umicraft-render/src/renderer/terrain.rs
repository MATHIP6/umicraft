// SPDX-License-Identifier: MIT OR Apache-2.0

//! The chunk meshes: upload, culling (frustum and caves) and the draw lists
//! of the opaque and transparent passes.

use std::collections::hash_map::Entry;

use glam::{DVec3, Vec3};
use umicraft_mesh::visibility::Connectivity;
use umicraft_mesh::{Mesh, VertexSpan};
use umicraft_world::ChunkPos;

use super::Renderer;
use crate::camera::{Camera, Frustum};
use crate::chunk_pass::{create_chunk_pass, ChunkPass, Group, SECTIONS};

/// GPU buffers for one chunk column mesh.
///
/// The two passes keep separate buffers so the transparent pass can be drawn
/// after every opaque chunk without touching vertex data. A pass with no faces
/// is absent rather than being a zero-sized buffer (which wgpu rejects).
pub(super) struct GpuMesh {
    pub(super) opaque: Option<ChunkPass>,
    pub(super) transparent: Option<ChunkPass>,
    /// Which faces of each section see each other, for cave culling; all open
    /// for a mesh that came without sections.
    pub(super) connectivity: [Connectivity; SECTIONS],
}

/// One chunk queued for a sorted pass: opaque ones go near to far (less
/// overdraw), transparent ones far to near.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ChunkDraw {
    pub(super) pos: ChunkPos,
    pub(super) distance_sq: f32,
    /// The sections to draw (bit `s` = section `s`): in the frustum and not
    /// hidden by the cave graph.
    pub(super) sections: u16,
    pub(super) has_solid: bool,
    pub(super) has_cutout: bool,
}

/// Up to 4 contiguous index ranges per chunk draw, stack-inline without heap allocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChunkRuns {
    pub ranges: [std::ops::Range<u32>; 4],
    pub count: u8,
}

impl ChunkRuns {
    #[inline(always)]
    pub fn empty() -> Self {
        Self {
            ranges: [0..0, 0..0, 0..0, 0..0],
            count: 0,
        }
    }

    #[inline(always)]
    pub fn push(&mut self, range: std::ops::Range<u32>) {
        if range.is_empty() {
            return;
        }
        let c = self.count as usize;
        if c > 0 && self.ranges[c - 1].end == range.start {
            self.ranges[c - 1].end = range.end;
            return;
        }
        if c < self.ranges.len() {
            self.ranges[c] = range;
            self.count += 1;
        } else if c > 0 {
            self.ranges[c - 1].end = range.end;
        }
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[std::ops::Range<u32>] {
        &self.ranges[..self.count as usize]
    }
}

/// A pre-resolved draw command for a chunk pass: direct mesh slot and pre-collected index ranges.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct PreparedChunkDraw {
    pub(super) slot: u32,
    pub(super) runs: ChunkRuns,
}

pub(super) struct OpaqueCandidate {
    pub(super) slot: u32,
    pub(super) pos: ChunkPos,
    pub(super) distance_sq: f32,
    pub(super) sections: u16,
    pub(super) has_cutout: bool,
}

pub(super) struct TransparentCandidate {
    pub(super) slot: u32,
    pub(super) pos: ChunkPos,
    pub(super) distance_sq: f32,
    pub(super) sections: u16,
}

pub(super) struct MeshSlot {
    pub(super) pos: ChunkPos,
    pub(super) mesh: GpuMesh,
}

/// The chunk centre used for transparent sorting: the middle of the 16×16
/// column at the cloud/terrain reference height of y = 128.
#[cfg_attr(not(test), allow(dead_code))]
#[inline]
pub(super) fn chunk_centre(pos: ChunkPos) -> DVec3 {
    DVec3::new(
        f64::from(pos.x) * 16.0 + 8.0,
        128.0,
        f64::from(pos.z) * 16.0 + 8.0,
    )
}

/// True when the column is within `reach` chunks of the eye (a square, like
/// the reference's `renderDistanceChunks`) and its bounding box touches the
/// frustum. `frustum` and the box are relative to `origin`.
#[allow(dead_code)]
fn chunk_is_visible(
    pos: ChunkPos,
    eye: ChunkPos,
    reach: i32,
    frustum: &Frustum,
    origin: DVec3,
) -> bool {
    if (pos.x - eye.x).abs() > reach || (pos.z - eye.z).abs() > reach {
        return false;
    }
    let min = DVec3::new(f64::from(pos.x) * 16.0, 0.0, f64::from(pos.z) * 16.0) - origin;
    let min = min.as_vec3();
    frustum.intersects_aabb(min, min + Vec3::new(16.0, 256.0, 16.0))
}

/// The sections of column `pos` whose 16³ box touches the frustum (bit `s` =
/// section `s`). Boxes are relative to the render `origin`.
///
/// Uses $O(1)$ plane-column interval projection without evaluating each section box individually.
fn sections_in_frustum(pos: ChunkPos, frustum: &Frustum, origin: DVec3) -> u16 {
    let base = DVec3::new(f64::from(pos.x) * 16.0, 0.0, f64::from(pos.z) * 16.0) - origin;
    frustum.sections_in_column(base.x as f32, base.y as f32, base.z as f32)
}

/// Squared camera-to-chunk-centre distance; avoids a square root per chunk.
pub(super) fn squared_distance_to_chunk(pos: ChunkPos, eye: DVec3) -> f32 {
    let dx = f64::from(pos.x) * 16.0 + 8.0 - eye.x;
    let dy = 128.0 - eye.y;
    let dz = f64::from(pos.z) * 16.0 + 8.0 - eye.z;
    (dx * dx + dy * dy + dz * dz) as f32
}

/// Orders opaque chunks nearest first, so what stands in front is drawn
/// before what hides behind it and fewer fragments are shaded for nothing
/// (F2-2b); `ChunkPos` is the deterministic tie-break.
#[cfg(test)]
#[allow(dead_code)]
fn sort_nearest_first(chunks: &mut [ChunkDraw]) {
    chunks.sort_unstable_by(|a, b| {
        a.distance_sq
            .total_cmp(&b.distance_sq)
            .then_with(|| a.pos.cmp(&b.pos))
    });
}

/// Orders transparent chunks farthest first, with `ChunkPos` as the
/// deterministic tie-break (ADR-0008 §3).
#[cfg(test)]
pub(super) fn sort_transparent_farthest_first(chunks: &mut [ChunkDraw]) {
    chunks.sort_unstable_by(|a, b| {
        b.distance_sq
            .total_cmp(&a.distance_sq)
            .then_with(|| a.pos.cmp(&b.pos))
    });
}

impl Renderer {
    #[inline(always)]
    fn collect_chunk_runs(pass: &ChunkPass, mask: u16, group: Group) -> ChunkRuns {
        let mut runs = ChunkRuns::empty();
        pass.visible_runs(mask, group, |r| runs.push(r));
        runs
    }

    /// Rebuilds the reused per-frame draw lists from the chunks that are
    /// inside the render distance and the view frustum.
    ///
    /// The server may keep sending or keeping columns beyond the client's
    /// render distance, and the GPU would still shade every one of them, so
    /// the cull happens here. The transparent list is sorted farthest first;
    /// both lists keep their capacity across frames.
    pub(super) fn prepare_visible_chunks(&mut self, camera: &Camera) {
        umicraft_core::profile_scope!("prepare_visible_chunks");
        // Boxes are made relative to the render origin in f64, so the frustum
        // planes and the boxes stay precise far from the world origin (A5).
        let origin = camera.render_origin();
        let view_projection = camera.view_projection_from(origin);
        let eye = camera.eye();
        let reach = i32::from(self.environment.view_distance);
        let key = (
            view_projection,
            eye,
            self.chunks_version,
            reach,
            self.cave_culling,
        );
        // A still camera over unchanged chunks has the lists it had.
        if self.visible_key == Some(key) {
            return;
        }
        self.visible_key = Some(key);
        let frustum = Frustum::from_view_projection(view_projection);
        let eye_chunk = ChunkPos::new((eye.x / 16.0).floor() as i32, (eye.z / 16.0).floor() as i32);
        self.visible_solid.clear();
        self.visible_cutout.clear();
        self.visible_transparent.clear();
        self.opaque_candidates.clear();
        self.transparent_candidates.clear();

        if self.cave_culling {
            let slots = &self.mesh_slots;
            let chunk_map = &self.chunks;
            let cave_reach = reach.min(24);
            self.cave_graph.update(
                (eye.x, eye.y, eye.z),
                cave_reach,
                self.chunks_version,
                |pos| {
                    chunk_map.get(&pos).and_then(|&slot| {
                        slots
                            .get(slot as usize)
                            .and_then(Option::as_ref)
                            .map(|s| &s.mesh.connectivity)
                    })
                },
            );
        }
        let cave = self.cave_culling.then_some(&self.cave_graph);
        let cave_reach = reach.min(24);
        for (slot_idx, slot_entry) in self.mesh_slots.iter().enumerate() {
            let Some(entry) = slot_entry.as_ref() else {
                continue;
            };
            let pos = entry.pos;
            let mesh = &entry.mesh;
            let dx = (pos.x - eye_chunk.x).abs();
            let dz = (pos.z - eye_chunk.z).abs();
            if dx > reach || dz > reach {
                continue;
            }
            let cave_mask = match cave {
                Some(c) if dx <= cave_reach && dz <= cave_reach => {
                    let m = c.mask(pos);
                    if m == 0 {
                        continue;
                    }
                    m
                }
                _ => 0xFFFF,
            };
            let sections = sections_in_frustum(pos, &frustum, origin) & cave_mask;
            if sections == 0 {
                continue;
            }
            let distance_sq = squared_distance_to_chunk(pos, eye);
            if let Some(opaque) = &mesh.opaque {
                let is_split = opaque.epoch == self.cutout_epoch;
                let has_solid = is_split && opaque.solid_quads > 0;
                let has_cutout = !is_split || opaque.solid_quads < opaque.quads;
                if has_solid || has_cutout {
                    self.opaque_candidates.push(OpaqueCandidate {
                        slot: slot_idx as u32,
                        pos,
                        distance_sq,
                        sections,
                        has_cutout,
                    });
                }
            }
            if mesh.transparent.is_some() {
                self.transparent_candidates.push(TransparentCandidate {
                    slot: slot_idx as u32,
                    pos,
                    distance_sq,
                    sections,
                });
            }
        }

        self.opaque_candidates.sort_unstable_by(|a, b| {
            a.distance_sq
                .total_cmp(&b.distance_sq)
                .then_with(|| a.pos.cmp(&b.pos))
        });
        for c in &self.opaque_candidates {
            let Some(slot) = self
                .mesh_slots
                .get(c.slot as usize)
                .and_then(Option::as_ref)
            else {
                continue;
            };
            let Some(opaque) = &slot.mesh.opaque else {
                continue;
            };
            if opaque.epoch == self.cutout_epoch {
                if !c.has_cutout {
                    let runs = if c.sections == 0xFFFF {
                        let mut r = ChunkRuns::empty();
                        r.push(0..opaque.quads * 6);
                        r
                    } else {
                        Self::collect_chunk_runs(opaque, c.sections, Group::Solid)
                    };
                    if runs.count > 0 {
                        self.visible_solid
                            .push(PreparedChunkDraw { slot: c.slot, runs });
                    }
                } else {
                    let mut runs = ChunkRuns::empty();
                    if c.sections == 0xFFFF {
                        runs.push(0..opaque.quads * 6);
                    } else {
                        let solid_runs = Self::collect_chunk_runs(opaque, c.sections, Group::Solid);
                        let cutout_runs =
                            Self::collect_chunk_runs(opaque, c.sections, Group::Cutout);
                        for r in solid_runs.as_slice() {
                            runs.push(r.clone());
                        }
                        for r in cutout_runs.as_slice() {
                            runs.push(r.clone());
                        }
                    }
                    if runs.count > 0 {
                        self.visible_cutout
                            .push(PreparedChunkDraw { slot: c.slot, runs });
                    }
                }
            } else {
                let mut runs = ChunkRuns::empty();
                if c.sections == 0xFFFF {
                    runs.push(0..opaque.quads * 6);
                } else {
                    let solid_runs = Self::collect_chunk_runs(opaque, c.sections, Group::Solid);
                    let cutout_runs = Self::collect_chunk_runs(opaque, c.sections, Group::Cutout);
                    for r in solid_runs.as_slice() {
                        runs.push(r.clone());
                    }
                    for r in cutout_runs.as_slice() {
                        runs.push(r.clone());
                    }
                }
                if runs.count > 0 {
                    self.visible_cutout
                        .push(PreparedChunkDraw { slot: c.slot, runs });
                }
            }
        }

        self.transparent_candidates.sort_unstable_by(|a, b| {
            b.distance_sq
                .total_cmp(&a.distance_sq)
                .then_with(|| a.pos.cmp(&b.pos))
        });
        for c in &self.transparent_candidates {
            let Some(slot) = self
                .mesh_slots
                .get(c.slot as usize)
                .and_then(Option::as_ref)
            else {
                continue;
            };
            let Some(transparent) = &slot.mesh.transparent else {
                continue;
            };
            let runs = if c.sections == 0xFFFF {
                let mut r = ChunkRuns::empty();
                r.push(0..transparent.quads * 6);
                r
            } else {
                Self::collect_chunk_runs(transparent, c.sections, Group::Solid)
            };
            if runs.count > 0 {
                self.visible_transparent
                    .push(PreparedChunkDraw { slot: c.slot, runs });
            }
        }
    }

    /// Replaces the GPU mesh for `pos`, or removes it when both passes are
    /// empty.
    ///
    /// The opaque and transparent halves are uploaded into separate vertex and
    /// index buffers so the renderer can submit every opaque chunk before the
    /// sorted transparent pass. A half with no faces gets no buffer.
    pub fn upload_mesh(&mut self, pos: ChunkPos, mesh: &Mesh) {
        umicraft_core::profile_scope!("upload_mesh");
        self.chunks_version += 1;
        if mesh.is_empty() {
            if let Some(slot) = self.chunks.remove(&pos) {
                self.mesh_slots[slot as usize] = None;
                self.free_mesh_slots.push(slot);
            }
            return;
        }
        // The spans of each pass, when the mesher produced them.
        let has_sections = mesh.sections.len() == SECTIONS;
        let opaque_spans: [VertexSpan; SECTIONS] = std::array::from_fn(|i| {
            mesh.sections
                .get(i)
                .map_or_else(Default::default, |s| s.opaque)
        });
        let transparent_spans: [VertexSpan; SECTIONS] = std::array::from_fn(|i| {
            mesh.sections
                .get(i)
                .map_or_else(Default::default, |s| s.transparent)
        });
        let opaque = create_chunk_pass(
            &self.device,
            "chunk opaque",
            &mesh.vertices,
            has_sections.then_some(&opaque_spans[..]),
            Some((&self.cutout, self.cutout_epoch)),
            &mut self.chunk_scratch,
        );
        let transparent = create_chunk_pass(
            &self.device,
            "chunk transparent",
            &mesh.transparent_vertices,
            has_sections.then_some(&transparent_spans[..]),
            None,
            &mut self.chunk_scratch,
        );
        // A pass without a whole quad has nothing to draw. If neither half
        // produced a buffer, drop any previous mesh instead of keeping an
        // empty entry.
        if opaque.is_none() && transparent.is_none() {
            if let Some(slot) = self.chunks.remove(&pos) {
                self.mesh_slots[slot as usize] = None;
                self.free_mesh_slots.push(slot);
            }
            return;
        }
        let most_quads = [&opaque, &transparent]
            .into_iter()
            .flatten()
            .map(|pass| pass.quads)
            .max()
            .unwrap_or(0);
        self.quad_indices.ensure(&self.device, most_quads);
        let connectivity = std::array::from_fn(|i| {
            mesh.sections
                .get(i)
                .map_or(Connectivity::OPEN, |section| section.connectivity)
        });
        let mesh = GpuMesh {
            opaque,
            transparent,
            connectivity,
        };
        match self.chunks.entry(pos) {
            Entry::Occupied(entry) => {
                let slot = *entry.get();
                self.mesh_slots[slot as usize] = Some(MeshSlot { pos, mesh });
            }
            Entry::Vacant(entry) => {
                let slot = match self.free_mesh_slots.pop() {
                    Some(s) => {
                        self.mesh_slots[s as usize] = Some(MeshSlot { pos, mesh });
                        s
                    }
                    None => {
                        let s = self.mesh_slots.len() as u32;
                        self.mesh_slots.push(Some(MeshSlot { pos, mesh }));
                        s
                    }
                };
                entry.insert(slot);
            }
        }
    }

    /// Removes the GPU mesh for one chunk column.
    pub fn remove_chunk(&mut self, pos: ChunkPos) {
        self.chunks_version += 1;
        if let Some(slot) = self.chunks.remove(&pos) {
            self.mesh_slots[slot as usize] = None;
            self.free_mesh_slots.push(slot);
        }
    }

    /// Removes every uploaded chunk mesh (e.g. after a dimension change).
    pub fn clear_meshes(&mut self) {
        self.chunks_version += 1;
        self.chunks.clear();
        self.mesh_slots.clear();
        self.free_mesh_slots.clear();
        self.visible_solid.clear();
        self.visible_cutout.clear();
        self.visible_transparent.clear();
        self.visible_key = None;
    }

    /// Turns the cave culling of the terrain sections on or off (on by
    /// default). Off draws every section in the frustum.
    pub fn set_cave_culling(&mut self, enabled: bool) {
        self.cave_culling = enabled;
    }

    /// Number of chunk meshes currently resident on the GPU.
    #[must_use]
    pub fn mesh_count(&self) -> usize {
        self.chunks.len()
    }
}
