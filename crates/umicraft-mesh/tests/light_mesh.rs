// SPDX-License-Identifier: MIT OR Apache-2.0

//! Integration tests for the M6 light sampling, corner ambient occlusion and
//! the transparent pass (ADR-0008 §1–2).
//!
//! The sources are synthetic: a few blocks plus explicit nibble values, so
//! the expected light and AO numbers are exact and no Mojang asset is needed
//! (spec §3). The corner mapping is the one described in the `umicraft-mesh`
//! module docs (`BlockModelRenderer.AmbientOcclusionFace`).

use std::sync::Arc;

use umicraft_mesh::{mesh_chunk, BlockSource, ChunkSnapshot, Mesh, Vertex, VertexLight};
use umicraft_world::{BlockState, ChunkColumn, ChunkPos, LightLevel};

/// A synthetic scene with explicit blocks and light; anything unspecified is
/// air with no light.
struct Scene {
    blocks: Vec<([i32; 3], BlockState)>,
    lights: Vec<([i32; 3], LightLevel)>,
}

impl Scene {
    /// Builds a dark scene from the given blocks.
    fn new(blocks: &[([i32; 3], BlockState)]) -> Self {
        Self {
            blocks: blocks.to_vec(),
            lights: Vec::new(),
        }
    }

    /// Sets one light sample; later calls for the same cell overwrite it.
    fn with_light(mut self, position: [i32; 3], light: LightLevel) -> Self {
        self.lights.retain(|(cell, _)| *cell != position);
        self.lights.push((position, light));
        self
    }

    /// Sets the same light on every cell of a slice.
    fn with_lights(mut self, positions: &[[i32; 3]], light: LightLevel) -> Self {
        for position in positions {
            self = self.with_light(*position, light);
        }
        self
    }
}

impl BlockSource for Scene {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        self.blocks
            .iter()
            .find(|(position, _)| *position == [x, y, z])
            .map_or(BlockState::AIR, |(_, state)| *state)
    }

    fn light(&self, x: i32, y: i32, z: i32) -> LightLevel {
        self.lights
            .iter()
            .find(|(position, _)| *position == [x, y, z])
            .map_or(LightLevel::DARK, |(_, light)| *light)
    }
}

/// A source with blocks only; the trait default keeps it full-bright.
struct Blocks(Vec<([i32; 3], BlockState)>);

impl BlockSource for Blocks {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        self.0
            .iter()
            .find(|(position, _)| *position == [x, y, z])
            .map_or(BlockState::AIR, |(_, state)| *state)
    }
}

/// Every vertex of both passes.
fn all_vertices(mesh: &Mesh) -> impl Iterator<Item = &Vertex> {
    mesh.vertices.iter().chain(mesh.transparent_vertices.iter())
}

/// The light payload of one vertex identified by position and normal.
///
/// Corner positions are exact sums of integers and `0.0`/`1.0`, so equality
/// is safe here.
fn vertex_light(mesh: &Mesh, position: [f32; 3], normal: [f32; 3]) -> VertexLight {
    let vertex = all_vertices(mesh)
        .find(|vertex| vertex.position == position && vertex.normal == normal)
        .expect("the scene must emit the requested vertex");
    VertexLight::unpack(vertex.light)
}

/// All four vertices of one quad share the coordinate on the normal axis.
fn assert_quad_is_planar(quad: &[Vertex]) {
    let normal = quad[0].normal;
    let axis = if normal[0] != 0.0 {
        0
    } else if normal[1] != 0.0 {
        1
    } else {
        2
    };
    let plane = quad[0].position[axis];
    assert!(quad.iter().all(|vertex| vertex.position[axis] == plane));
    assert!(quad.iter().all(|vertex| vertex.normal == normal));
}

#[test]
fn snapshot_light_reads_neighbours_and_defaults_edges_to_daylight() {
    let mut west = ChunkColumn::new(ChunkPos::new(-1, 0));
    west.set_sky_light(15, 0, 0, 4);
    let mut centre = ChunkColumn::new(ChunkPos::new(0, 0));
    centre.set_block_light(0, 0, 0, 9);
    let snapshot =
        ChunkSnapshot::from_columns(ChunkPos::new(0, 0), [Arc::new(west), Arc::new(centre)]);

    assert_eq!(snapshot.light(-1, 0, 0), LightLevel::new(0, 4));
    assert_eq!(snapshot.light(0, 0, 0), LightLevel::new(9, 0));
    // A missing neighbour column reads as daylight at the snapshot edge
    // (ADR-0008 §1; the card asks for this default to be documented).
    assert_eq!(snapshot.light(16, 0, 0), LightLevel::FULL_SKY);
    assert_eq!(snapshot.light(0, 256, 0), LightLevel::DARK);
}

#[test]
fn smooth_light_averages_the_four_outward_cells() {
    let stone = BlockState::new(1, 0);
    // The up face of the block at the origin samples the cells at y = 1.
    // The four cells around the (1, 1, 1) corner get 0/12/8/4, so the
    // average is 6 even though the cell directly outward is dark.
    let scene = Scene::new(&[([0, 0, 0], stone)])
        .with_light([1, 1, 0], LightLevel::new(12, 15))
        .with_light([0, 1, 1], LightLevel::new(8, 15))
        .with_light([1, 1, 1], LightLevel::new(4, 15));
    let mesh = mesh_chunk(ChunkPos::new(0, 0), &scene);

    let light = vertex_light(&mesh, [1.0, 1.0, 1.0], [0.0, 1.0, 0.0]);
    assert_eq!(light.block, 6, "(0 + 12 + 8 + 4) / 4 = 6");
    assert_eq!(light.sky, 11, "(0 + 15 + 15 + 15) / 4 rounds to 11");
    assert_ne!(light.block, 0, "the face centre alone must not be sampled");
}

#[test]
fn corner_occlusion_is_quantized_to_open_one_two_three() {
    let stone = BlockState::new(1, 0);
    let corner = [1.0, 1.0, 1.0];
    let up = [0.0, 1.0, 0.0];
    let mesh_for = |occluders: &[([i32; 3], BlockState)]| {
        let mut blocks = vec![([0, 0, 0], stone)];
        blocks.extend_from_slice(occluders);
        mesh_chunk(ChunkPos::new(0, 0), &Scene::new(&blocks))
    };

    // Open corner, no occluder in any of the three sampled cells.
    let mesh = mesh_for(&[]);
    assert_eq!(vertex_light(&mesh, corner, up).ao, 15);

    // One edge occluder.
    let mesh = mesh_for(&[([1, 1, 0], stone)]);
    assert_eq!(vertex_light(&mesh, corner, up).ao, 10);

    // One edge plus the diagonal.
    let mesh = mesh_for(&[([1, 1, 0], stone), ([1, 1, 1], stone)]);
    assert_eq!(vertex_light(&mesh, corner, up).ao, 5);

    // Both edges occlude: the diagonal is irrelevant, the corner is darkest.
    let mesh = mesh_for(&[([1, 1, 0], stone), ([0, 1, 1], stone)]);
    assert_eq!(vertex_light(&mesh, corner, up).ao, 0);
    let mesh = mesh_for(&[([1, 1, 0], stone), ([0, 1, 1], stone), ([1, 1, 1], stone)]);
    assert_eq!(vertex_light(&mesh, corner, up).ao, 0);
}

#[test]
fn sky_lit_and_cave_vertices_keep_separate_sky_light() {
    let stone = BlockState::new(1, 0);
    let outward = [[0, 1, 0], [1, 1, 0], [0, 1, 1], [1, 1, 1]];

    let sky = Scene::new(&[([0, 0, 0], stone)]).with_lights(&outward, LightLevel::new(0, 15));
    let cave = Scene::new(&[([0, 0, 0], stone)]).with_lights(&outward, LightLevel::new(5, 0));
    let up = [0.0, 1.0, 0.0];

    let day = vertex_light(&mesh_chunk(ChunkPos::new(0, 0), &sky), [1.0, 1.0, 1.0], up);
    let night = vertex_light(&mesh_chunk(ChunkPos::new(0, 0), &cave), [1.0, 1.0, 1.0], up);
    assert_eq!(day.sky, 15);
    assert_eq!(day.block, 0);
    assert_eq!(night.sky, 0);
    assert_eq!(night.block, 5);
}

#[test]
fn transparent_states_use_the_second_pass() {
    let scene = Scene::new(&[
        ([0, 0, 0], BlockState::new(1, 0)),   // stone: opaque
        ([4, 0, 0], BlockState::new(9, 0)),   // water: liquid
        ([8, 0, 0], BlockState::new(20, 0)),  // glass: transparent
        ([12, 0, 0], BlockState::new(18, 0)), // leaves: transparent
    ]);
    let mesh = mesh_chunk(ChunkPos::new(0, 0), &scene);

    // Glass and leaves are six faces each; a lone water block is eleven:
    // the surface and its underside, the bottom, and four sides drawn from
    // both sides (MCP `BlockFluidRenderer.renderFluid`).
    assert_eq!(mesh.opaque_face_count(), 6);
    assert_eq!(mesh.transparent_face_count(), 23);
    assert_eq!(mesh.face_count(), 29, "the counts cover both passes");
    assert_eq!(mesh.triangle_count(), 58);
    assert_eq!(mesh.vertices.len(), 24);
    assert_eq!(mesh.indices.len(), 36);
    assert_eq!(mesh.transparent_vertices.len(), 92);
    assert_eq!(mesh.transparent_indices.len(), 138);

    // The two index streams are independent: each starts at zero and every
    // index stays inside its own vertex buffer.
    let mut opaque = mesh.indices.clone();
    opaque.sort_unstable();
    opaque.dedup();
    assert_eq!(opaque, (0..24).collect::<Vec<u32>>());
    let mut transparent = mesh.transparent_indices.clone();
    transparent.sort_unstable();
    transparent.dedup();
    assert_eq!(transparent, (0..92).collect::<Vec<u32>>());

    // The stone uses the procedural stone layer (3), the fluids and the
    // see-through cubes their own layers; none leaked into the opaque pass.
    assert!(mesh.vertices.iter().all(|vertex| vertex.layer == 3));
}

#[test]
fn the_no_light_fallback_keeps_the_legacy_geometry() {
    let source = Blocks(vec![([0, 0, 0], BlockState::new(1, 0))]);
    let mesh = mesh_chunk(ChunkPos::new(0, 0), &source);

    // Six quads, four vertices each, emitted in the frozen face order with
    // the canonical two-triangle pattern.
    assert_eq!(mesh.face_count(), 6);
    assert_eq!(mesh.vertices.len(), 24);
    let expected_indices: Vec<u32> = (0..6)
        .flat_map(|face| {
            let base = face * 4;
            [base, base + 1, base + 2, base, base + 2, base + 3]
        })
        .collect();
    assert_eq!(mesh.indices, expected_indices);
    assert!(mesh.transparent_indices.is_empty());

    // Unit-cube corners only, and every quad stays in its face plane.
    for quad in mesh.vertices.chunks_exact(4) {
        assert_quad_is_planar(quad);
        for vertex in quad {
            assert!(vertex
                .position
                .iter()
                .all(|coordinate| *coordinate == 0.0 || *coordinate == 1.0));
        }
    }

    // No light source means the trait default: full daylight, no block
    // light, open AO and face shading on.
    assert!(all_vertices(&mesh).all(|vertex| {
        let light = VertexLight::unpack(vertex.light);
        light.block == 0 && light.sky == 15 && light.ao == 15 && light.shade
    }));
}
