// SPDX-License-Identifier: MIT OR Apache-2.0

//! # umicraft-mesh
//!
//! Naive CPU chunk meshing (spec RND-2, M2 slice): for every renderable block
//! in a 16×256×16 column, emit a quad for each of the six faces whose
//! neighbour does not occlude it. Opaque cubes cull faces; identical
//! transparent/liquid blocks (glass-glass, water-water) cull each other;
//! non-cube decorations are skipped until the block registry lands in M6.
//!
//! Meshing reads blocks through [`BlockSource`]. [`ChunkSnapshot`] captures a
//! 3×3 neighbourhood of [`umicraft_world::ChunkColumn`] handles, so a worker
//! thread can mesh a column without touching the live [`World`]:
//!
//! ```
//! use umicraft_mesh::{mesh_chunk, ChunkSnapshot};
//! use umicraft_world::{ChunkPos, World};
//!
//! let world = World::new(0);
//! let snapshot = ChunkSnapshot::from_world(&world, ChunkPos::new(0, 0));
//! let mesh = mesh_chunk(ChunkPos::new(0, 0), &snapshot);
//! assert!(mesh.is_empty());
//! ```
//!
//! Texture layer indices in [`Vertex::layer`] index the renderer's fallback
//! texture array; see [`texture_layer`].

use bytemuck::{Pod, Zeroable};
use umicraft_world::{BlockKind, BlockPos, BlockState, ChunkColumn, ChunkPos, World};

use std::sync::Arc;

/// Number of layers in the fallback block texture array.
pub const TEXTURE_LAYERS: u32 = 19;

/// One chunk face vertex, laid out for direct GPU upload.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    /// World-space position.
    pub position: [f32; 3],
    /// Face normal (unit axis).
    pub normal: [f32; 3],
    /// Texture coordinates; `v = 0` is the top of the texture.
    pub uv: [f32; 2],
    /// Fallback texture-array layer.
    pub layer: u32,
}

/// CPU mesh for one chunk column.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    /// Interleaved vertices, four per face.
    pub vertices: Vec<Vertex>,
    /// Triangle indices, six per face.
    pub indices: Vec<u32>,
}

impl Mesh {
    /// True when no faces were emitted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Number of triangles.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Number of quads (faces).
    #[must_use]
    pub fn face_count(&self) -> usize {
        self.vertices.len() / 4
    }
}

/// The six cube faces, in the order of the internal face table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// −X.
    West,
    /// +X.
    East,
    /// −Y.
    Down,
    /// +Y.
    Up,
    /// −Z.
    North,
    /// +Z.
    South,
}

impl Face {
    const fn index(self) -> usize {
        match self {
            Self::West => 0,
            Self::East => 1,
            Self::Down => 2,
            Self::Up => 3,
            Self::North => 4,
            Self::South => 5,
        }
    }
}

/// Read access to block states by world coordinate.
///
/// Implemented for [`World`], [`ChunkSnapshot`], and references to any
/// source. Keep implementations cheap: the mesher performs up to six lookups
/// per block.
pub trait BlockSource {
    /// Block at world coordinates; unloaded or out-of-range positions must
    /// read as [`BlockState::AIR`].
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState;
}

impl BlockSource for World {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        World::block(self, BlockPos::new(x, y, z))
    }
}

impl<T: BlockSource + ?Sized> BlockSource for &T {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        (**self).block(x, y, z)
    }
}

/// A 3×3 chunk neighbourhood captured for off-thread meshing.
///
/// Chunk columns are shared behind [`Arc`], so capturing a snapshot is cheap
/// even for a full view distance.
#[derive(Debug, Clone)]
pub struct ChunkSnapshot {
    origin: ChunkPos,
    chunks: [Option<Arc<ChunkColumn>>; 9],
}

impl ChunkSnapshot {
    /// Captures the centre column and its eight neighbours from `world`.
    #[must_use]
    pub fn from_world(world: &World, center: ChunkPos) -> Self {
        let origin = ChunkPos::new(center.x - 1, center.z - 1);
        let mut chunks: [Option<Arc<ChunkColumn>>; 9] = std::array::from_fn(|_| None);
        for dz in 0..3 {
            for dx in 0..3 {
                let pos = ChunkPos::new(origin.x + dx, origin.z + dz);
                if let Some(column) = world.chunk_shared(pos) {
                    chunks[(dz * 3 + dx) as usize] = Some(column);
                }
            }
        }
        Self { origin, chunks }
    }

    /// Builds a snapshot directly from shared columns (used by tests and
    /// benches).
    #[must_use]
    pub fn from_columns(
        center: ChunkPos,
        columns: impl IntoIterator<Item = Arc<ChunkColumn>>,
    ) -> Self {
        let origin = ChunkPos::new(center.x - 1, center.z - 1);
        let mut chunks: [Option<Arc<ChunkColumn>>; 9] = std::array::from_fn(|_| None);
        for column in columns {
            let pos = column.pos();
            let dx = pos.x - origin.x;
            let dz = pos.z - origin.z;
            if (0..3).contains(&dx) && (0..3).contains(&dz) {
                chunks[(dz * 3 + dx) as usize] = Some(column);
            }
        }
        Self { origin, chunks }
    }
}

impl BlockSource for ChunkSnapshot {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        if !(0..256).contains(&y) {
            return BlockState::AIR;
        }
        let chunk = ChunkPos::new(x >> 4, z >> 4);
        let dx = chunk.x - self.origin.x;
        let dz = chunk.z - self.origin.z;
        if !(0..3).contains(&dx) || !(0..3).contains(&dz) {
            return BlockState::AIR;
        }
        self.chunks[(dz * 3 + dx) as usize]
            .as_ref()
            .map_or(BlockState::AIR, |column| {
                column.block((x & 15) as usize, y as usize, (z & 15) as usize)
            })
    }
}

/// Builds the mesh for one chunk column.
///
/// `source` must answer queries in world coordinates and cover at least the
/// 3×3 chunk neighbourhood; [`ChunkSnapshot`] does this.
#[must_use]
pub fn mesh_chunk<S: BlockSource + ?Sized>(pos: ChunkPos, source: &S) -> Mesh {
    let mut mesh = Mesh::default();
    let base_x = pos.x * 16;
    let base_z = pos.z * 16;

    for y in 0..256i32 {
        for z in 0..16i32 {
            for x in 0..16i32 {
                let wx = base_x + x;
                let wz = base_z + z;
                let state = source.block(wx, y, wz);
                if !state.is_renderable() {
                    continue;
                }

                for face in ALL_FACES {
                    let def = &FACE_DEFS[face.index()];
                    let neighbour =
                        source.block(wx + def.offset[0], y + def.offset[1], wz + def.offset[2]);
                    if occludes(neighbour, state) {
                        continue;
                    }
                    emit_face(&mut mesh, wx, y, wz, state, face, def);
                }
            }
        }
    }
    mesh
}

/// Texture-array layer for a block face.
///
/// The layers are generated procedurally by `umicraft-render`; grass tops,
/// grass sides, and dirt bottoms get distinct layers, as do log ends and
/// sides. Unknown blocks use the checker layer.
#[must_use]
pub fn texture_layer(state: BlockState, face: Face) -> u32 {
    match state.id() {
        2 => match face {
            Face::Up => 0,   // grass top
            Face::Down => 2, // dirt
            _ => 1,          // grass side
        },
        3 => 2,
        1 => 3,
        12 => 4,
        7 => 5,
        8 | 9 => 6,
        17 | 162 => match face {
            Face::Up | Face::Down => 9,
            _ => 10,
        },
        18 => 11,
        5 => 12,
        13 => 13,
        20 | 95 => 14,
        24 => 15,
        45 => 16,
        49 => 17,
        _ => 18,
    }
}

fn occludes(neighbour: BlockState, current: BlockState) -> bool {
    if neighbour.is_opaque() {
        return true;
    }
    neighbour == current && matches!(current.kind(), BlockKind::Transparent | BlockKind::Liquid)
}

fn emit_face(
    mesh: &mut Mesh,
    x: i32,
    y: i32,
    z: i32,
    state: BlockState,
    face: Face,
    def: &FaceDef,
) {
    let base = mesh.vertices.len() as u32;
    let layer = texture_layer(state, face);
    for (corner, uv) in def.corners.iter().zip(def.uvs) {
        mesh.vertices.push(Vertex {
            position: [
                x as f32 + corner[0],
                y as f32 + corner[1],
                z as f32 + corner[2],
            ],
            normal: def.normal,
            uv,
            layer,
        });
    }
    mesh.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

const ALL_FACES: [Face; 6] = [
    Face::West,
    Face::East,
    Face::Down,
    Face::Up,
    Face::North,
    Face::South,
];

struct FaceDef {
    normal: [f32; 3],
    offset: [i32; 3],
    /// Corners in counter-clockwise order seen from outside.
    corners: [[f32; 3]; 4],
    uvs: [[f32; 2]; 4],
}

/// Face order: west, east, down, up, north, south. Winding is CCW from
/// outside for wgpu's default front-face setting; side UVs keep `v = 0` at the
/// top so textures are upright.
const FACE_DEFS: [FaceDef; 6] = [
    FaceDef {
        normal: [-1.0, 0.0, 0.0],
        offset: [-1, 0, 0],
        corners: [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
        ],
        uvs: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    },
    FaceDef {
        normal: [1.0, 0.0, 0.0],
        offset: [1, 0, 0],
        corners: [
            [1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
        ],
        uvs: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    },
    FaceDef {
        normal: [0.0, -1.0, 0.0],
        offset: [0, -1, 0],
        corners: [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ],
        uvs: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
    },
    FaceDef {
        normal: [0.0, 1.0, 0.0],
        offset: [0, 1, 0],
        corners: [
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 0.0],
        ],
        uvs: [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
    },
    FaceDef {
        normal: [0.0, 0.0, -1.0],
        offset: [0, 0, -1],
        corners: [
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ],
        uvs: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    },
    FaceDef {
        normal: [0.0, 0.0, 1.0],
        offset: [0, 0, 1],
        corners: [
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        uvs: [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    struct TestSource {
        blocks: Vec<(i32, i32, i32, BlockState)>,
    }

    impl BlockSource for TestSource {
        fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
            self.blocks
                .iter()
                .find(|(bx, by, bz, _)| (*bx, *by, *bz) == (x, y, z))
                .map_or(BlockState::AIR, |(_, _, _, state)| *state)
        }
    }

    fn source(blocks: &[(i32, i32, i32, BlockState)]) -> TestSource {
        TestSource {
            blocks: blocks.to_vec(),
        }
    }

    #[test]
    fn single_block_emits_six_faces() {
        let mesh = mesh_chunk(
            ChunkPos::new(0, 0),
            &source(&[(0, 0, 0, BlockState::new(1, 0))]),
        );
        assert_eq!(mesh.face_count(), 6);
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.indices.len(), 36);
        // Every vertex belongs to the unit cube.
        assert!(mesh.vertices.iter().all(|vertex| {
            (0.0..=1.0).contains(&vertex.position[0])
                && (0.0..=1.0).contains(&vertex.position[1])
                && (0.0..=1.0).contains(&vertex.position[2])
        }));
    }

    #[test]
    fn adjacent_opaque_blocks_cull_their_shared_faces() {
        let mesh = mesh_chunk(
            ChunkPos::new(0, 0),
            &source(&[
                (0, 0, 0, BlockState::new(1, 0)),
                (1, 0, 0, BlockState::new(1, 0)),
            ]),
        );
        assert_eq!(mesh.face_count(), 10);
    }

    #[test]
    fn fully_enclosed_block_emits_nothing() {
        let mut blocks = vec![(1, 1, 1, BlockState::new(1, 0))];
        for (dx, dy, dz) in [
            (-1, 0, 0),
            (1, 0, 0),
            (0, -1, 0),
            (0, 1, 0),
            (0, 0, -1),
            (0, 0, 1),
        ] {
            blocks.push((1 + dx, 1 + dy, 1 + dz, BlockState::new(1, 0)));
        }
        let mesh = mesh_chunk(ChunkPos::new(0, 0), &source(&blocks));
        // Only the outer shell of the 3×3×3 cross is visible: 6 neighbours ×
        // 5 exposed faces, plus the centre contributes none.
        assert_eq!(mesh.face_count(), 30);
    }

    #[test]
    fn transparent_blocks_only_cull_against_the_same_state() {
        let stone = BlockState::new(1, 0);
        let glass = BlockState::new(20, 0);
        let mesh = mesh_chunk(
            ChunkPos::new(0, 0),
            &source(&[(0, 0, 0, stone), (1, 0, 0, glass)]),
        );
        // Stone keeps all six faces (glass does not occlude); glass loses the
        // face touching the stone, so 11 faces.
        assert_eq!(mesh.face_count(), 11);

        let mesh = mesh_chunk(
            ChunkPos::new(0, 0),
            &source(&[(0, 0, 0, glass), (1, 0, 0, glass)]),
        );
        assert_eq!(mesh.face_count(), 10);
    }

    #[test]
    fn flat_terrain_mesh_has_top_bottom_and_border_faces() {
        let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
        for z in 0..16 {
            for x in 0..16 {
                column.set_block(x, 0, z, BlockState::new(7, 0));
                column.set_block(x, 1, z, BlockState::new(3, 0));
                column.set_block(x, 2, z, BlockState::new(3, 0));
                column.set_block(x, 3, z, BlockState::new(2, 0));
            }
        }
        let snapshot = ChunkSnapshot::from_columns(ChunkPos::new(0, 0), [Arc::new(column)]);
        let mesh = mesh_chunk(ChunkPos::new(0, 0), &snapshot);
        // 256 top + 256 bottom + 4 sides * 16 columns * 4 blocks = 768.
        assert_eq!(mesh.face_count(), 768);
        assert_eq!(mesh.triangle_count(), 1536);
    }

    #[test]
    fn grass_and_log_use_face_specific_layers() {
        let grass = BlockState::new(2, 0);
        assert_eq!(texture_layer(grass, Face::Up), 0);
        assert_eq!(texture_layer(grass, Face::Down), 2);
        assert_eq!(texture_layer(grass, Face::North), 1);
        assert_eq!(texture_layer(grass, Face::South), 1);

        let log = BlockState::new(17, 0);
        assert_eq!(texture_layer(log, Face::Up), 9);
        assert_eq!(texture_layer(log, Face::East), 10);
        assert!(texture_layer(BlockState::new(1, 0), Face::Up) < TEXTURE_LAYERS);
    }

    #[test]
    fn snapshot_reads_neighbour_chunks() {
        let mut west = ChunkColumn::new(ChunkPos::new(-1, 0));
        west.set_block(15, 0, 0, BlockState::new(1, 0));
        let mut centre = ChunkColumn::new(ChunkPos::new(0, 0));
        centre.set_block(0, 0, 0, BlockState::new(1, 0));
        let snapshot =
            ChunkSnapshot::from_columns(ChunkPos::new(0, 0), [Arc::new(west), Arc::new(centre)]);
        assert_eq!(snapshot.block(-1, 0, 0), BlockState::new(1, 0));
        assert_eq!(snapshot.block(0, 0, 0), BlockState::new(1, 0));
        assert_eq!(snapshot.block(1, 0, 0), BlockState::AIR);
        assert_eq!(snapshot.block(0, 256, 0), BlockState::AIR);
    }
}
