// SPDX-License-Identifier: MIT OR Apache-2.0

//! The legacy procedural-cube mesher: the no-assets fallback of the model
//! path, and the cube emission shared with it.

use umicraft_world::fluid::Liquid;
use umicraft_world::{BlockKind, BlockState, ChunkPos};

use crate::face::{Face, FaceDef, ALL_FACES, FACE_DEFS};
use crate::lighting::{quad_light, Lighting};
use crate::visibility::Connectivity;
use crate::{fluid, tint, BlockSource, Mesh, Vertex};

/// Builds the mesh for one chunk column with the legacy procedural cubes.
///
/// `source` must answer queries in world coordinates and cover at least the
/// 3×3 chunk neighbourhood; [`ChunkSnapshot`] does this.
///
/// This is the no-assets fallback of [`mesh_chunk_with_models`].
#[must_use]
pub fn mesh_chunk<S: BlockSource + ?Sized>(pos: ChunkPos, source: &S) -> Mesh {
    mesh_chunk_lit(pos, source, Lighting::Smooth)
}

/// [`mesh_chunk`] with the lighting of the `Smooth Lighting` option.
#[must_use]
pub fn mesh_chunk_lit<S: BlockSource + ?Sized>(
    pos: ChunkPos,
    source: &S,
    lighting: Lighting,
) -> Mesh {
    mesh_sections(pos, source, |mesh, [wx, y, wz]| {
        let state = source.block(wx, y, wz);
        if !state.is_renderable() {
            return;
        }
        if let Some(liquid) = Liquid::of(state) {
            let layer = texture_layer(state, Face::Up);
            let layers = fluid::FluidLayers {
                still: layer,
                flow: layer,
            };
            fluid::emit_fluid(mesh, source, [wx, y, wz], liquid, layers, tint::WHITE);
            return;
        }
        emit_legacy_faces(mesh, source, wx, y, wz, state, lighting);
    })
}

/// Runs `emit` on every cell of the column `pos`, section by section from the
/// bottom, and returns the mesh with its [`SectionSpan`]s.
///
/// The order of the cells (y, then z, then x) is the one the mesher always
/// had; a section that holds only air is skipped without reading a block.
pub(crate) fn mesh_sections<S: BlockSource + ?Sized>(
    pos: ChunkPos,
    source: &S,
    mut emit: impl FnMut(&mut Mesh, [i32; 3]),
) -> Mesh {
    let mut mesh = Mesh::default();
    for section in 0..16_usize {
        let start = mesh.section_start();
        if !source.section_is_empty(pos, section) {
            let (base_x, base_y, base_z) = (pos.x * 16, section as i32 * 16, pos.z * 16);
            for y in base_y..base_y + 16 {
                for z in base_z..base_z + 16 {
                    for x in base_x..base_x + 16 {
                        emit(&mut mesh, [x, y, z]);
                    }
                }
            }
        }
        mesh.finish_section(start, Connectivity::of_section(source, pos, section));
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
    lighting: Lighting,
) {
    emit_legacy_faces_with(
        mesh,
        source,
        [x, y, z],
        (state, lighting),
        tint::WHITE,
        |face| texture_layer(state, face),
    );
}

/// Emits the legacy cube faces with a caller-selected texture layer.
///
/// [`mesh_chunk`] uses the procedural [`texture_layer`] table. The M5 model
/// path passes an atlas-aware layer function so fluids such as water and lava,
/// which have no blockstate model, still sample the real texture array. The
/// same caller supplies the biome tint (white when no colormap loaded).
pub(crate) fn emit_legacy_faces_with<S, F>(
    mesh: &mut Mesh,
    source: &S,
    position: [i32; 3],
    (state, lighting): (BlockState, Lighting),
    tint: u32,
    mut layer_for_face: F,
) where
    S: BlockSource + ?Sized,
    F: FnMut(Face) -> u32,
{
    let [x, y, z] = position;
    for face in ALL_FACES {
        let def = &FACE_DEFS[face.index()];
        let neighbour = source.block(x + def.offset[0], y + def.offset[1], z + def.offset[2]);
        if occludes(neighbour, state) {
            continue;
        }
        emit_face(
            mesh,
            source,
            position,
            def,
            state,
            FaceStyle {
                layer: layer_for_face(face),
                tint,
                lighting,
            },
        );
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

/// True when faces of `state` belong to the blended (second) pass.
///
/// ADR-0008 §2 routes `Transparent` and `Liquid` states there so water, glass
/// and leaves share one pass; plants stay in the opaque cutout pass.
pub(crate) fn is_transparent_pass(state: BlockState) -> bool {
    matches!(state.kind(), BlockKind::Transparent | BlockKind::Liquid)
}

/// What a legacy face is drawn with besides its geometry.
struct FaceStyle {
    layer: u32,
    tint: u32,
    lighting: Lighting,
}

fn emit_face<S: BlockSource + ?Sized>(
    mesh: &mut Mesh,
    source: &S,
    position: [i32; 3],
    def: &FaceDef,
    state: BlockState,
    style: FaceStyle,
) {
    let FaceStyle {
        layer,
        tint,
        lighting,
    } = style;
    let [x, y, z] = position;
    let lights = quad_light(
        source,
        position,
        def.normal,
        &def.corners,
        true,
        true,
        lighting,
    );
    let vertices = std::array::from_fn(|index| {
        let corner = def.corners[index];
        Vertex {
            position: [
                x as f32 + corner[0],
                y as f32 + corner[1],
                z as f32 + corner[2],
            ],
            normal: def.normal,
            uv: def.uvs[index],
            layer,
            light: lights[index].pack(),
            tint,
        }
    });
    mesh.push_quad(is_transparent_pass(state), vertices);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use umicraft_world::ChunkColumn;

    use super::*;
    use crate::ChunkSnapshot;
    use crate::TEXTURE_LAYERS;

    #[test]
    fn pass_split_matches_the_block_kind() {
        assert!(!is_transparent_pass(BlockState::new(1, 0)), "stone");
        assert!(is_transparent_pass(BlockState::new(8, 0)), "water");
        assert!(is_transparent_pass(BlockState::new(20, 0)), "glass");
        assert!(is_transparent_pass(BlockState::new(18, 0)), "leaves");
        // Plants are cutout geometry, not blended.
        assert!(!is_transparent_pass(BlockState::new(31, 0)), "tall grass");
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

    /// A slab (Flat) next to a full cube must NOT cull the cube's face.
    #[test]
    fn slab_does_not_occlude_adjacent_full_block() {
        let stone = BlockState::new(1, 0);
        let slab = BlockState::new(44, 0);
        assert!(!occludes(slab, stone));
        let stair = BlockState::new(53, 0);
        assert!(!occludes(stair, stone));
    }
}
