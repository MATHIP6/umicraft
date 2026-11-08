// SPDX-License-Identifier: MIT OR Apache-2.0

//! The per-section spans and connectivity of a column mesh (F2-3, ADR-0062).

use std::sync::Arc;

use umicraft_mesh::visibility::Connectivity;
use umicraft_mesh::{mesh_chunk, ChunkSnapshot, Mesh};
use umicraft_world::{BlockState, ChunkColumn, ChunkPos};

const POS: ChunkPos = ChunkPos::new(0, 0);

fn mesh_of(column: ChunkColumn) -> Mesh {
    let snapshot = ChunkSnapshot::from_columns(POS, [Arc::new(column)]);
    mesh_chunk(POS, &snapshot)
}

/// A column with one stone block in section 0 (y = 3) and one in section 2
/// (y = 40); section 1 and the rest are air.
fn two_blocks() -> ChunkColumn {
    let mut column = ChunkColumn::new(POS);
    column.set_block(4, 3, 4, BlockState::new(1, 0));
    column.set_block(4, 40, 4, BlockState::new(1, 0));
    column
}

#[test]
fn the_spans_tile_both_passes_in_section_order() {
    let mesh = mesh_of(two_blocks());
    assert_eq!(mesh.sections.len(), 16);
    let mut next = 0;
    for span in &mesh.sections {
        assert_eq!(span.opaque.start, next);
        assert_eq!(span.opaque.len % 4, 0);
        next += span.opaque.len;
    }
    assert_eq!(next as usize, mesh.vertices.len());
    let transparent: u32 = mesh.sections.iter().map(|span| span.transparent.len).sum();
    assert_eq!(transparent as usize, mesh.transparent_vertices.len());
}

#[test]
fn a_block_puts_its_six_faces_in_its_own_section_only() {
    let mesh = mesh_of(two_blocks());
    let lens: Vec<u32> = mesh.sections.iter().map(|span| span.opaque.len).collect();
    assert_eq!(lens[0], 24, "six faces of four vertices");
    assert_eq!(lens[2], 24);
    assert_eq!(lens.iter().sum::<u32>(), 48);
    // The vertices of section 2 sit at y = 40..41.
    let span = mesh.sections[2].opaque;
    let slice = &mesh.vertices[span.start as usize..(span.start + span.len) as usize];
    assert!(slice
        .iter()
        .all(|vertex| (40.0..=41.0).contains(&vertex.position[1])));
}

#[test]
fn an_air_section_is_open_and_a_stone_one_is_not_closed_by_one_block() {
    let mesh = mesh_of(two_blocks());
    assert_eq!(mesh.sections[1].connectivity, Connectivity::OPEN);
    assert_eq!(mesh.sections[1].opaque.len, 0);
    // One block does not separate the faces of its section.
    assert!(mesh.sections[0].connectivity.connects(0, 1));
}

#[test]
fn a_solid_floor_cuts_the_down_face_from_the_up_face() {
    let mut column = ChunkColumn::new(POS);
    for x in 0..16 {
        for z in 0..16 {
            column.set_block(x, 8, z, BlockState::new(1, 0));
        }
    }
    let mesh = mesh_of(column);
    let connectivity = mesh.sections[0].connectivity;
    // Faces: 2 = down, 3 = up.
    assert!(!connectivity.connects(2, 3));
    assert!(
        connectivity.connects(0, 1),
        "west still sees east above the floor"
    );
}
