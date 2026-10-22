// SPDX-License-Identifier: MIT OR Apache-2.0

//! # umicraft-mesh
//!
//! Naive CPU chunk meshing (spec RND-2, M2 slice): for every renderable block
//! in a 16×256×16 column, emit a quad for each of the six faces whose
//! neighbour does not occlude it. Opaque cubes cull faces; identical
//! transparent/liquid blocks (glass-glass, water-water) cull each other;
//! non-cube decorations are skipped until the block registry lands in M5.
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
//!
//! M5 adds the model path in [`models`]: [`mesh_chunk_with_models`] places the
//! quads that `umicraft-model` baked from the 1.8 block models and writes the
//! layers of the real texture atlas. [`mesh_chunk`] stays the no-assets
//! fallback.

use bytemuck::{Pod, Zeroable};
use umicraft_world::{BlockKind, BlockPos, BlockState, ChunkColumn, ChunkPos, LightLevel, World};

use std::sync::Arc;

pub mod models;

pub use models::{mesh_chunk_with_models, ModelMeshContext};

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
    /// Packed [`VertexLight`] payload (block/sky light, AO and face shading).
    ///
    /// The shader unpacks bits 0..=3 (block), 4..=7 (sky), 8..=11 (AO) and
    /// bit 12 (shade). Keeping the payload in one `u32` leaves the vertex at
    /// 40 bytes.
    pub light: u32,
}

/// Per-vertex lighting payload.
///
/// Light levels and ambient occlusion are nibbles; `shade` is bit 12. The
/// renderer uses the two light channels independently so sky light can follow
/// the time of day while block light stays constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VertexLight {
    /// Block-light level (`0..=15`).
    pub block: u8,
    /// Sky-light level (`0..=15`).
    pub sky: u8,
    /// Ambient-occlusion strength (`0..=15`, where 15 is fully open).
    pub ao: u8,
    /// Whether the renderer applies the 1.8 per-face shade factor.
    pub shade: bool,
}

impl VertexLight {
    /// Full daylight, no ambient occlusion, with face shading enabled.
    pub const DAY: Self = Self {
        block: 0,
        sky: 15,
        ao: 15,
        shade: true,
    };

    /// Fully open ambient occlusion.
    pub const AO_OPEN: u8 = 15;

    /// Builds a payload, clamping every nibble to `0..=15`.
    #[must_use]
    pub const fn new(block: u8, sky: u8, ao: u8, shade: bool) -> Self {
        Self {
            block: clamp_light(block),
            sky: clamp_light(sky),
            ao: clamp_light(ao),
            shade,
        }
    }

    /// Packs the payload for [`Vertex::light`].
    #[must_use]
    pub const fn pack(self) -> u32 {
        (self.block as u32)
            | ((self.sky as u32) << 4)
            | ((self.ao as u32) << 8)
            | ((self.shade as u32) << 12)
    }

    /// Unpacks a [`Vertex::light`] payload.
    #[must_use]
    pub const fn unpack(packed: u32) -> Self {
        Self {
            block: (packed & 0x0F) as u8,
            sky: ((packed >> 4) & 0x0F) as u8,
            ao: ((packed >> 8) & 0x0F) as u8,
            shade: (packed & (1 << 12)) != 0,
        }
    }
}

const fn clamp_light(level: u8) -> u8 {
    if level > 15 {
        15
    } else {
        level
    }
}

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

/// Read access to block states and light by world coordinate.
///
/// Implemented for [`World`], [`ChunkSnapshot`], and references to any
/// source. Keep implementations cheap: the mesher performs up to six block
/// lookups per block and samples light around each visible corner.
pub trait BlockSource {
    /// Block at world coordinates; unloaded or out-of-range positions must
    /// read as [`BlockState::AIR`].
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState;

    /// Light at world coordinates.
    ///
    /// Sources without light data keep the M2 full-bright look by returning
    /// [`LightLevel::FULL_SKY`]. Real world sources override this with their
    /// nibble arrays; unloaded positions use the same daylight default so the
    /// visible edge of a snapshot is not artificially dark.
    fn light(&self, _x: i32, _y: i32, _z: i32) -> LightLevel {
        LightLevel::FULL_SKY
    }
}

impl BlockSource for World {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        World::block(self, BlockPos::new(x, y, z))
    }

    fn light(&self, x: i32, y: i32, z: i32) -> LightLevel {
        World::light(self, BlockPos::new(x, y, z))
    }
}

impl<T: BlockSource + ?Sized> BlockSource for &T {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        (**self).block(x, y, z)
    }

    fn light(&self, x: i32, y: i32, z: i32) -> LightLevel {
        (**self).light(x, y, z)
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

    fn light(&self, x: i32, y: i32, z: i32) -> LightLevel {
        if !(0..256).contains(&y) {
            return LightLevel::DARK;
        }
        let chunk = ChunkPos::new(x >> 4, z >> 4);
        let dx = chunk.x - self.origin.x;
        let dz = chunk.z - self.origin.z;
        if !(0..3).contains(&dx) || !(0..3).contains(&dz) {
            return LightLevel::FULL_SKY;
        }
        self.chunks[(dz * 3 + dx) as usize]
            .as_ref()
            .map_or(LightLevel::FULL_SKY, |column| {
                column.light((x & 15) as usize, y as usize, (z & 15) as usize)
            })
    }
}

/// Builds the mesh for one chunk column with the legacy procedural cubes.
///
/// `source` must answer queries in world coordinates and cover at least the
/// 3×3 chunk neighbourhood; [`ChunkSnapshot`] does this.
///
/// This is the no-assets fallback of [`mesh_chunk_with_models`].
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
                emit_legacy_faces(&mut mesh, source, wx, y, wz, state);
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

/// Emits the legacy cube faces of one renderable state at `(x, y, z)`.
///
/// Shared by [`mesh_chunk`] and the no-model fallback of
/// [`mesh_chunk_with_models`], so both paths cull with exactly the same rule.
fn emit_legacy_faces<S: BlockSource + ?Sized>(
    mesh: &mut Mesh,
    source: &S,
    x: i32,
    y: i32,
    z: i32,
    state: BlockState,
) {
    emit_legacy_faces_with(mesh, source, x, y, z, state, |face| {
        texture_layer(state, face)
    });
}

/// Emits the legacy cube faces with a caller-selected texture layer.
///
/// [`mesh_chunk`] uses the procedural [`texture_layer`] table. The M5 model
/// path passes an atlas-aware layer function so fluids such as water and lava,
/// which have no blockstate model, still sample the real texture array.
pub(crate) fn emit_legacy_faces_with<S, F>(
    mesh: &mut Mesh,
    source: &S,
    x: i32,
    y: i32,
    z: i32,
    state: BlockState,
    mut layer_for_face: F,
) where
    S: BlockSource + ?Sized,
    F: FnMut(Face) -> u32,
{
    for face in ALL_FACES {
        let def = &FACE_DEFS[face.index()];
        let neighbour = source.block(x + def.offset[0], y + def.offset[1], z + def.offset[2]);
        if occludes(neighbour, state) {
            continue;
        }
        emit_face(mesh, x, y, z, def, layer_for_face(face));
    }
}

/// Whether `neighbour` hides the face that `current` would emit towards it.
///
/// A full opaque cube hides every face; a see-through cube hides the face of
/// the identical state only, which is the M2 rule frozen in ADR-0007 and also
/// used for `cullface` quads of baked models.
pub(crate) fn occludes(neighbour: BlockState, current: BlockState) -> bool {
    if neighbour.is_opaque() {
        return true;
    }
    neighbour == current && matches!(current.kind(), BlockKind::Transparent | BlockKind::Liquid)
}

fn emit_face(mesh: &mut Mesh, x: i32, y: i32, z: i32, def: &FaceDef, layer: u32) {
    let base = mesh.vertices.len() as u32;
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
            light: VertexLight::DAY.pack(),
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

    #[test]
    fn vertex_light_round_trips_and_clamps() {
        let light = VertexLight::new(20, 3, 9, false);
        assert_eq!(light.block, 15);
        assert_eq!(VertexLight::unpack(light.pack()), light);
        assert_eq!(
            VertexLight::unpack(VertexLight::DAY.pack()),
            VertexLight::DAY
        );
        assert_eq!(VertexLight::DAY.pack() & 0x0F, 0);
        assert_eq!((VertexLight::DAY.pack() >> 4) & 0x0F, 15);
        assert_eq!((VertexLight::DAY.pack() >> 8) & 0x0F, 15);
        assert_ne!(VertexLight::DAY.pack() & (1 << 12), 0);
    }

    #[test]
    fn mesh_counts_include_both_passes() {
        let vertex = Vertex {
            position: [0.0, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
            layer: 0,
            light: VertexLight::DAY.pack(),
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
