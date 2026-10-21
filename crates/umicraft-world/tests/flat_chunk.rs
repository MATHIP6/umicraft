// SPDX-License-Identifier: MIT OR Apache-2.0

//! Golden test: decode a real 1.8 flat-world chunk payload captured from a
//! live PandaSpigot server.

use umicraft_world::{BlockState, ChunkColumn, ChunkPos};

/// One captured section payload; provenance in `fixtures/README.md`.
const FLAT_SECTION: &[u8] = include_bytes!("fixtures/flat-chunk-section.bin");

const BEDROCK: BlockState = BlockState::new(7, 0);
const DIRT: BlockState = BlockState::new(3, 0);
const GRASS: BlockState = BlockState::new(2, 0);

#[test]
fn captured_flat_chunk_decodes_to_the_expected_terrain() {
    let mut column = ChunkColumn::new(ChunkPos::new(-107, 6));
    column
        .apply_payload(0x0001, true, true, FLAT_SECTION)
        .unwrap();

    for z in 0..16 {
        for x in 0..16 {
            assert_eq!(column.block(x, 0, z), BEDROCK, "bedrock at ({x}, 0, {z})");
            assert_eq!(column.block(x, 1, z), DIRT, "dirt at ({x}, 1, {z})");
            assert_eq!(column.block(x, 2, z), DIRT, "dirt at ({x}, 2, {z})");
            assert_eq!(column.block(x, 3, z), GRASS, "grass at ({x}, 3, {z})");
            assert_eq!(column.block(x, 4, z), BlockState::AIR);
            assert_eq!(column.block(x, 255, z), BlockState::AIR);
        }
    }

    // Flat preset uses plains (id 1) everywhere.
    assert!(column.biomes().iter().all(|biome| *biome == 1));

    // The captured light arrays: bedrock is dark, open sky above y=3 is 15.
    let section = column.section(0).expect("section 0 is present");
    assert_eq!(section.sky_light(0, 0, 0), Some(0));
    assert_eq!(section.sky_light(0, 4, 0), Some(15));
    assert_eq!(section.sky_light(15, 15, 15), Some(15));
    assert_eq!(section.block_light(0, 0, 0), 0);
    assert_eq!(
        section.bits_per_block(),
        2,
        "four states: air/bedrock/dirt/grass"
    );
}
