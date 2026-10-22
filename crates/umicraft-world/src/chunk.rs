// SPDX-License-Identifier: MIT OR Apache-2.0

//! A full 16×256×16 chunk column.

use umicraft_protocol::packets::play::clientbound::chunk_data::chunk_payload_len;

use crate::block::BlockState;
use crate::error::WorldError;
use crate::light::LightLevel;
use crate::pos::ChunkPos;
use crate::section::{Section, BLOCKS_PER_SECTION, LIGHT_BYTES, SECTION_SIZE};

/// Sections in a 1.8 chunk column (256 blocks / 16).
pub const SECTION_COUNT: usize = 16;
/// World height in blocks (1.8).
pub const WORLD_HEIGHT: usize = 256;
/// Biome bytes per column.
const BIOME_COUNT: usize = 256;

/// One chunk column: up to 16 sections plus the 256-biome array.
#[derive(Debug, Clone)]
pub struct ChunkColumn {
    pos: ChunkPos,
    sections: [Option<Section>; SECTION_COUNT],
    biomes: [u8; BIOME_COUNT],
}

impl ChunkColumn {
    /// Creates an empty (all-air) column.
    #[must_use]
    pub fn new(pos: ChunkPos) -> Self {
        Self {
            pos,
            sections: std::array::from_fn(|_| None),
            biomes: [0; BIOME_COUNT],
        }
    }

    /// Chunk column coordinates.
    #[must_use]
    pub const fn pos(&self) -> ChunkPos {
        self.pos
    }

    /// Raw 256-byte biome array (index `z << 4 | x`).
    #[must_use]
    pub const fn biomes(&self) -> &[u8; BIOME_COUNT] {
        &self.biomes
    }

    /// Section at a Y index (`0..16`), or `None` when the server did not send
    /// it (an all-air section).
    #[must_use]
    pub fn section(&self, index: usize) -> Option<&Section> {
        self.sections.get(index).and_then(Option::as_ref)
    }

    /// Block at column-local coordinates; out-of-range reads air.
    #[must_use]
    #[inline]
    pub fn block(&self, x: usize, y: usize, z: usize) -> BlockState {
        if x >= SECTION_SIZE || z >= SECTION_SIZE || y >= WORLD_HEIGHT {
            return BlockState::AIR;
        }
        self.sections[y / SECTION_SIZE]
            .as_ref()
            .map_or(BlockState::AIR, |section| {
                section.block(x, y % SECTION_SIZE, z)
            })
    }

    /// Combined light at column-local coordinates.
    ///
    /// A section the server did not send is all air. Vanilla treats that as
    /// full sky light above the terrain, which is also the safe default for
    /// columns whose empty sections were elided. A present section without a
    /// sky array reports zero sky light (the Nether/End case).
    #[must_use]
    #[inline]
    pub fn light(&self, x: usize, y: usize, z: usize) -> LightLevel {
        if x >= SECTION_SIZE || z >= SECTION_SIZE || y >= WORLD_HEIGHT {
            return LightLevel::DARK;
        }
        self.sections[y / SECTION_SIZE]
            .as_ref()
            .map_or(LightLevel::FULL_SKY, |section| {
                LightLevel::new(
                    section.block_light(x, y % SECTION_SIZE, z),
                    section.sky_light(x, y % SECTION_SIZE, z).unwrap_or(0),
                )
            })
    }

    /// Writes block light at column-local coordinates.
    ///
    /// Out-of-range coordinates are ignored. Values above 15 are clamped.
    pub fn set_block_light(&mut self, x: usize, y: usize, z: usize, level: u8) {
        if x >= SECTION_SIZE || z >= SECTION_SIZE || y >= WORLD_HEIGHT {
            return;
        }
        let section = self.sections[y / SECTION_SIZE].get_or_insert_with(Section::empty);
        section.set_block_light(x, y % SECTION_SIZE, z, level);
    }

    /// Writes sky light at column-local coordinates.
    ///
    /// Out-of-range coordinates are ignored. Values above 15 are clamped.
    /// A section created by this method starts with no blocks.
    pub fn set_sky_light(&mut self, x: usize, y: usize, z: usize, level: u8) {
        if x >= SECTION_SIZE || z >= SECTION_SIZE || y >= WORLD_HEIGHT {
            return;
        }
        let section = self.sections[y / SECTION_SIZE].get_or_insert_with(Section::empty);
        section.set_sky_light(x, y % SECTION_SIZE, z, level);
    }

    /// Writes a block at column-local coordinates.
    ///
    /// Returns `true` when the state changed. Out-of-range Y or X/Z coordinates
    /// are ignored.
    pub fn set_block(&mut self, x: usize, y: usize, z: usize, state: BlockState) -> bool {
        if x >= SECTION_SIZE || z >= SECTION_SIZE || y >= WORLD_HEIGHT {
            return false;
        }
        let section_index = y / SECTION_SIZE;
        let section = self.sections[section_index].get_or_insert_with(Section::empty);
        let changed = section.set_block(x, y % SECTION_SIZE, z, state);
        if changed && section.is_empty() && !section.has_light() {
            // Air-only sections are not sent by the server either; dropping
            // them keeps memory proportional to visible geometry. A section
            // that still carries light is retained: M6 writes light into air
            // sections for the cave scene, and a block update must not erase
            // it (ADR-0008 §1).
            self.sections[section_index] = None;
        }
        changed
    }

    /// Applies one `Chunk Data` / `Map Chunk Bulk` payload.
    ///
    /// `has_sky_light` comes from the packet (bulk) or the Join Game
    /// dimension (single chunk). `ground_up` payloads replace the whole
    /// column, including the biome array; partial payloads update only the
    /// sections present in `primary_bit_mask`.
    ///
    /// # Errors
    /// [`WorldError::ChunkPayloadLength`] when `data` does not match the
    /// layout derived from the mask and flags. The column is not modified in
    /// that case.
    pub fn apply_payload(
        &mut self,
        primary_bit_mask: u16,
        ground_up: bool,
        has_sky_light: bool,
        data: &[u8],
    ) -> Result<(), WorldError> {
        let expected = chunk_payload_len(primary_bit_mask, has_sky_light, ground_up);
        if data.len() != expected {
            return Err(WorldError::ChunkPayloadLength {
                expected,
                actual: data.len(),
                primary_bit_mask,
                sky_light: has_sky_light,
                ground_up,
            });
        }

        let section_indices: Vec<usize> = (0..SECTION_COUNT)
            .filter(|index| primary_bit_mask & (1 << index) != 0)
            .collect();
        let block_len = section_indices.len() * BLOCKS_PER_SECTION * 2;
        let light_len = section_indices.len() * LIGHT_BYTES;
        let sky_len = if has_sky_light { light_len } else { 0 };
        let biome_offset = block_len + light_len + sky_len;

        let blocks = &data[..block_len];
        let block_light = &data[block_len..block_len + light_len];
        let sky_light = if has_sky_light {
            Some(&data[block_len + light_len..biome_offset])
        } else {
            None
        };

        for (position, &section_index) in section_indices.iter().enumerate() {
            let block_start = position * BLOCKS_PER_SECTION * 2;
            let light_start = position * LIGHT_BYTES;
            self.sections[section_index] = Some(Section::from_wire(
                &blocks[block_start..block_start + BLOCKS_PER_SECTION * 2],
                &block_light[light_start..light_start + LIGHT_BYTES],
                sky_light.map(|array| &array[light_start..light_start + LIGHT_BYTES]),
            ));
        }

        if ground_up {
            for (index, section) in self.sections.iter_mut().enumerate() {
                if primary_bit_mask & (1 << index) == 0 {
                    *section = None;
                }
            }
            if let Some(bytes) = data.get(biome_offset..biome_offset + BIOME_COUNT) {
                self.biomes.copy_from_slice(bytes);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_section_payload(fill: u16) -> Vec<u8> {
        let mut data = vec![0u8; BLOCKS_PER_SECTION * 2 + LIGHT_BYTES * 2 + BIOME_COUNT];
        for chunk in data[..BLOCKS_PER_SECTION * 2].chunks_exact_mut(2) {
            chunk.copy_from_slice(&fill.to_le_bytes());
        }
        data[BLOCKS_PER_SECTION * 2 + LIGHT_BYTES] = 0xFF; // sky light all 15
        data[BLOCKS_PER_SECTION * 2 + LIGHT_BYTES * 2..].fill(1);
        data
    }

    #[test]
    fn applies_full_and_partial_payloads() {
        let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
        column
            .apply_payload(1, true, true, &one_section_payload(0x30))
            .unwrap();
        assert_eq!(column.block(0, 0, 0), BlockState::new(3, 0));
        assert_eq!(column.block(15, 15, 15), BlockState::new(3, 0));
        assert_eq!(column.biomes()[0], 1);
        assert_eq!(column.section(0).unwrap().sky_light(0, 0, 0), Some(15));

        // Partial update with only one section and no biome array.
        let mut partial = vec![0u8; BLOCKS_PER_SECTION * 2 + LIGHT_BYTES * 2];
        partial[..BLOCKS_PER_SECTION * 2].fill(0);
        partial[0] = 0x70; // bedrock at (0,0,0)
        column.apply_payload(1, false, true, &partial).unwrap();
        assert_eq!(column.block(0, 0, 0), BlockState::new(7, 0));
        assert_eq!(column.block(0, 1, 0), BlockState::AIR);
        assert_eq!(column.biomes()[0], 1, "partial updates keep biomes");
        assert_eq!(column.section(0).unwrap().sky_light(0, 0, 0), Some(0));
    }

    #[test]
    fn light_reads_nibbles_and_defaults_elided_sections_to_sky() {
        let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
        column
            .apply_payload(1, true, true, &one_section_payload(0x30))
            .unwrap();
        assert_eq!(column.light(0, 0, 0), LightLevel { block: 0, sky: 15 });
        column.set_block_light(0, 0, 0, 7);
        column.set_sky_light(0, 0, 0, 3);
        assert_eq!(column.light(0, 0, 0), LightLevel { block: 7, sky: 3 });

        // y=20 is an elided all-air section.
        assert_eq!(column.light(0, 20, 0), LightLevel::FULL_SKY);
        assert_eq!(column.light(16, 0, 0), LightLevel::DARK);
    }

    #[test]
    fn full_update_clears_unlisted_sections() {
        let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
        column
            .apply_payload(1, true, true, &one_section_payload(0x10))
            .unwrap();
        column.set_block(0, 20, 0, BlockState::new(1, 0));
        assert!(column.section(1).is_some());

        let mut empty = vec![0u8; BIOME_COUNT];
        empty.fill(2);
        column.apply_payload(0, true, true, &empty).unwrap();
        assert!(column.section(0).is_none());
        assert!(column.section(1).is_none());
        assert_eq!(column.biomes()[0], 2);
    }

    #[test]
    fn wrong_payload_length_is_rejected_without_mutation() {
        let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
        let err = column.apply_payload(1, true, true, &[0; 5]).unwrap_err();
        assert!(matches!(
            err,
            WorldError::ChunkPayloadLength {
                expected: 12_544,
                actual: 5,
                ..
            }
        ));
        assert!(column.section(0).is_none());
    }

    #[test]
    fn setting_blocks_out_of_range_is_ignored() {
        let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
        assert!(!column.set_block(0, 256, 0, BlockState::new(1, 0)));
        assert!(!column.set_block(16, 0, 0, BlockState::new(1, 0)));
        assert_eq!(column.block(0, 256, 0), BlockState::AIR);
    }

    #[test]
    fn created_sections_are_dropped_when_they_become_empty() {
        let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
        assert!(column.set_block(0, 0, 0, BlockState::new(1, 0)));
        assert!(column.section(0).is_some());
        assert!(column.set_block(0, 0, 0, BlockState::AIR));
        assert!(column.section(0).is_none());
    }

    #[test]
    fn light_only_sections_survive_an_air_update() {
        let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
        assert!(column.set_block(0, 0, 0, BlockState::new(1, 0)));
        column.set_block_light(1, 0, 0, 9);
        column.set_sky_light(0, 0, 0, 7);

        // Removing the last block must not discard the light the M6 cave
        // scene wrote into the section (ADR-0008 §1).
        assert!(column.set_block(0, 0, 0, BlockState::AIR));

        assert!(
            column.section(0).is_some(),
            "the light-only section must be kept"
        );
        assert_eq!(column.light(0, 0, 0), LightLevel::new(0, 7));
        assert_eq!(column.light(1, 0, 0), LightLevel::new(9, 0));
        assert_eq!(column.light(0, 0, 1), LightLevel::DARK);
    }

    #[test]
    fn present_sections_without_sky_read_zero_and_elided_sections_read_daylight() {
        let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
        assert!(column.set_block(0, 0, 0, BlockState::new(1, 0)));
        assert_eq!(column.light(0, 0, 0), LightLevel::DARK);
        // Section 1 is elided, so its air reads as daylight above the terrain.
        assert_eq!(column.light(0, 20, 0), LightLevel::FULL_SKY);
    }

    #[test]
    fn set_light_creates_a_section_and_ignores_out_of_range() {
        let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
        column.set_sky_light(0, 0, 0, 7);
        assert!(column.section(0).is_some());
        assert_eq!(column.light(0, 0, 0), LightLevel::new(0, 7));
        column.set_block_light(0, 0, 0, 9);
        assert_eq!(column.light(0, 0, 0), LightLevel::new(9, 7));

        // Out-of-range writes are ignored and never create a section.
        column.set_block_light(0, 256, 0, 15);
        column.set_sky_light(16, 0, 0, 15);
        assert_eq!(column.light(0, 0, 0), LightLevel::new(9, 7));
        assert!(column.section(8).is_none());
    }
}
