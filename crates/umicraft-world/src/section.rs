// SPDX-License-Identifier: MIT OR Apache-2.0

//! Compact 16×16×16 section storage: palette + bit-packed indices.
//!
//! The 1.8 wire stores every block as a fixed 16-bit state. Most sections use
//! only a handful of states (a flat-world section has four), so we store the
//! distinct states once and pack an index per block. The wire's nibble light
//! arrays are kept verbatim for later lighting work.
//!
//! Index order matches MCP `ExtendedBlockStorage`/`NibbleArray`:
//! `y << 8 | z << 4 | x`.

use std::collections::HashMap;

use crate::block::BlockState;

/// Edge length of a section, in blocks.
pub const SECTION_SIZE: usize = 16;
/// Number of blocks in one section.
pub const BLOCKS_PER_SECTION: usize = SECTION_SIZE * SECTION_SIZE * SECTION_SIZE;
/// Bytes in one 4-bit light array (4096 blocks / 2).
pub const LIGHT_BYTES: usize = BLOCKS_PER_SECTION / 2;
/// Highest palette index width used by a section (4 096 distinct states).
const MAX_PALETTE_BITS: u8 = 12;

/// One 16³ block section with palette/bit-packed storage and light nibbles.
#[derive(Debug, Clone)]
pub struct Section {
    palette: Vec<BlockState>,
    bits: u8,
    packed: Vec<u64>,
    block_light: Box<[u8; LIGHT_BYTES]>,
    sky_light: Option<Box<[u8; LIGHT_BYTES]>>,
    non_air: u16,
}

impl Section {
    /// Creates an all-air section with no sky-light array.
    #[must_use]
    pub(crate) fn empty() -> Self {
        Self {
            palette: vec![BlockState::AIR],
            bits: 1,
            packed: vec![0; word_count(1)],
            block_light: Box::new([0; LIGHT_BYTES]),
            sky_light: None,
            non_air: 0,
        }
    }

    /// Decodes the wire block/light arrays. The caller (`ChunkColumn`) has
    /// already validated the total payload length; this constructor is
    /// tolerant of short inputs (missing bytes read as zero) so it can never
    /// panic on server data.
    pub(crate) fn from_wire(
        block_bytes: &[u8],
        block_light: &[u8],
        sky_light: Option<&[u8]>,
    ) -> Self {
        let mut palette: Vec<BlockState> = Vec::new();
        let mut lookup: HashMap<u16, u16> = HashMap::new();
        let mut entries = [0u16; BLOCKS_PER_SECTION];
        let mut non_air = 0u16;

        for (slot, pair) in block_bytes
            .chunks_exact(2)
            .take(BLOCKS_PER_SECTION)
            .enumerate()
        {
            let raw = u16::from_le_bytes([pair[0], pair[1]]);
            if raw != 0 {
                non_air += 1;
            }
            let next_index = palette.len() as u16;
            let entry = *lookup.entry(raw).or_insert_with(|| {
                palette.push(BlockState::from_wire(raw));
                next_index
            });
            entries[slot] = entry;
        }

        if palette.is_empty() {
            palette.push(BlockState::AIR);
        }

        let bits = bits_for_entries(palette.len());
        let mut section = Self {
            palette,
            bits,
            packed: vec![0; word_count(bits)],
            block_light: light_array(block_light),
            sky_light: sky_light.map(light_array),
            non_air,
        };
        for (slot, entry) in entries.iter().enumerate() {
            section.write_entry(slot, *entry);
        }
        section
    }

    /// Number of distinct block states in the palette.
    #[must_use]
    pub fn palette_len(&self) -> usize {
        self.palette.len()
    }

    /// Bits used per block index (1..=12).
    #[must_use]
    pub fn bits_per_block(&self) -> u8 {
        self.bits
    }

    /// True when the section contains only air.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.non_air == 0
    }

    /// Block state at section-local coordinates; out-of-range reads air.
    #[must_use]
    pub fn block(&self, x: usize, y: usize, z: usize) -> BlockState {
        block_index(x, y, z).map_or(BlockState::AIR, |index| self.state_at(index))
    }

    /// Writes a block state at section-local coordinates.
    ///
    /// Returns `true` when the state changed. Out-of-range coordinates are
    /// ignored.
    pub fn set_block(&mut self, x: usize, y: usize, z: usize, state: BlockState) -> bool {
        let Some(index) = block_index(x, y, z) else {
            return false;
        };
        let old = self.state_at(index);
        if old == state {
            return false;
        }
        let entry = self.palette_index_for(state, index);
        self.write_entry(index, entry);
        match (old.is_air(), state.is_air()) {
            (true, false) => self.non_air += 1,
            (false, true) => self.non_air -= 1,
            _ => {}
        }
        true
    }

    /// Block-light level (0..=15) at section-local coordinates.
    #[must_use]
    pub fn block_light(&self, x: usize, y: usize, z: usize) -> u8 {
        nibble(self.block_light.as_slice(), x, y, z)
    }

    /// Sky-light level (0..=15), or `None` when this dimension has no sky.
    #[must_use]
    pub fn sky_light(&self, x: usize, y: usize, z: usize) -> Option<u8> {
        self.sky_light
            .as_deref()
            .map(|data| nibble(data.as_slice(), x, y, z))
    }

    fn state_at(&self, index: usize) -> BlockState {
        self.palette[self.read_entry(index) as usize]
    }

    fn read_entry(&self, index: usize) -> u16 {
        let per_word = entries_per_word(self.bits);
        let word = self.packed[index / per_word];
        let shift = (index % per_word) as u32 * u32::from(self.bits);
        ((word >> shift) & ((1u64 << self.bits) - 1)) as u16
    }

    fn write_entry(&mut self, index: usize, entry: u16) {
        let per_word = entries_per_word(self.bits);
        let shift = (index % per_word) as u32 * u32::from(self.bits);
        let mask = (1u64 << self.bits) - 1;
        let word = &mut self.packed[index / per_word];
        *word = (*word & !(mask << shift)) | ((u64::from(entry) & mask) << shift);
    }

    /// Palette slot for `state`, growing the index width or compacting the
    /// palette when needed. `replacing` is the slot about to be overwritten,
    /// which is excluded when the palette is full.
    fn palette_index_for(&mut self, state: BlockState, replacing: usize) -> u16 {
        if let Some(position) = self.palette.iter().position(|known| *known == state) {
            return position as u16;
        }
        if self.palette.len() >= 1 << self.bits {
            if self.bits < MAX_PALETTE_BITS {
                self.grow_bits(self.bits + 1);
            } else {
                self.rebuild_without(replacing);
                if let Some(position) = self.palette.iter().position(|known| *known == state) {
                    return position as u16;
                }
            }
        }
        let position = self.palette.len();
        self.palette.push(state);
        position as u16
    }

    fn grow_bits(&mut self, new_bits: u8) {
        let old: Vec<u16> = (0..BLOCKS_PER_SECTION)
            .map(|index| self.read_entry(index))
            .collect();
        self.bits = new_bits;
        self.packed = vec![0; word_count(new_bits)];
        for (index, entry) in old.iter().enumerate() {
            self.write_entry(index, *entry);
        }
    }

    /// Rebuilds the palette from the blocks currently present, skipping the
    /// slot that is about to be overwritten. Used only when all 4 096 palette
    /// slots are taken and a new state arrives.
    fn rebuild_without(&mut self, excluded: usize) {
        let old_states: Vec<BlockState> = (0..BLOCKS_PER_SECTION)
            .map(|index| self.state_at(index))
            .collect();
        let mut palette: Vec<BlockState> = Vec::new();
        let mut lookup: HashMap<u16, u16> = HashMap::new();
        let mut entries = vec![0u16; BLOCKS_PER_SECTION];
        for (index, state) in old_states.iter().enumerate() {
            if index == excluded {
                continue;
            }
            let next = palette.len() as u16;
            let entry = *lookup.entry(state.wire()).or_insert_with(|| {
                palette.push(*state);
                next
            });
            entries[index] = entry;
        }
        if palette.is_empty() {
            palette.push(BlockState::AIR);
        }
        self.palette = palette;
        self.bits = bits_for_entries(self.palette.len());
        self.packed = vec![0; word_count(self.bits)];
        for (index, entry) in entries.iter().enumerate() {
            if index != excluded {
                self.write_entry(index, *entry);
            }
        }
    }
}

fn block_index(x: usize, y: usize, z: usize) -> Option<usize> {
    if x < SECTION_SIZE && y < SECTION_SIZE && z < SECTION_SIZE {
        Some((y << 8) | (z << 4) | x)
    } else {
        None
    }
}

fn nibble(data: &[u8], x: usize, y: usize, z: usize) -> u8 {
    let Some(index) = block_index(x, y, z) else {
        return 0;
    };
    let byte = data[index >> 1];
    if index & 1 == 0 {
        byte & 0x0F
    } else {
        byte >> 4
    }
}

fn light_array(source: &[u8]) -> Box<[u8; LIGHT_BYTES]> {
    let mut data = Box::new([0; LIGHT_BYTES]);
    let len = source.len().min(LIGHT_BYTES);
    data[..len].copy_from_slice(&source[..len]);
    data
}

fn bits_for_entries(entries: usize) -> u8 {
    let entries = entries.max(1);
    let bits = usize::BITS - (entries - 1).leading_zeros();
    bits.clamp(1, u32::from(MAX_PALETTE_BITS)) as u8
}

fn entries_per_word(bits: u8) -> usize {
    64 / usize::from(bits)
}

fn word_count(bits: u8) -> usize {
    BLOCKS_PER_SECTION.div_ceil(entries_per_word(bits))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire_blocks(states: &[(usize, u16)]) -> Vec<u8> {
        let mut bytes = vec![0u8; BLOCKS_PER_SECTION * 2];
        for (index, state) in states {
            bytes[index * 2..index * 2 + 2].copy_from_slice(&state.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn decodes_wire_states_and_light() {
        // y=0 bedrock, y=1 dirt, y=3 grass at x=1,z=2.
        let index = |y: usize| (y << 8) | (2 << 4) | 1;
        let blocks = wire_blocks(&[(index(0), 0x70), (index(1), 0x30), (index(3), 0x20)]);
        let mut light = vec![0u8; LIGHT_BYTES];
        light[0] = 0x21; // index 0 -> 1, index 1 -> 2
        let mut sky = vec![0u8; LIGHT_BYTES];
        // The nibble array index is odd for x=1, so the high nibble holds 15.
        sky[index(3) >> 1] = 0xF0;
        let section = Section::from_wire(&blocks, &light, Some(&sky));

        assert_eq!(section.block(1, 0, 2), BlockState::new(7, 0));
        assert_eq!(section.block(1, 1, 2), BlockState::new(3, 0));
        assert_eq!(section.block(1, 3, 2), BlockState::new(2, 0));
        assert_eq!(section.block(0, 0, 0), BlockState::AIR);
        assert!(!section.is_empty());
        assert_eq!(section.palette_len(), 4);
        assert_eq!(section.bits_per_block(), 2);
        assert_eq!(section.block_light(0, 0, 0), 1);
        assert_eq!(section.block_light(1, 0, 0), 2);
        assert_eq!(section.sky_light(1, 3, 2), Some(15));
        assert_eq!(section.sky_light(0, 0, 0), Some(0));
    }

    #[test]
    fn set_block_grows_the_palette() {
        let mut section = Section::empty();
        assert!(section.is_empty());
        assert_eq!(section.bits_per_block(), 1);

        assert!(section.set_block(0, 0, 0, BlockState::new(1, 0)));
        assert_eq!(section.palette_len(), 2);
        assert_eq!(section.bits_per_block(), 1);
        assert!(!section.set_block(0, 0, 0, BlockState::new(1, 0)));

        assert!(section.set_block(15, 15, 15, BlockState::new(3, 0)));
        assert_eq!(section.palette_len(), 3);
        assert_eq!(section.bits_per_block(), 2);
        assert_eq!(section.block(0, 0, 0), BlockState::new(1, 0));
        assert_eq!(section.block(15, 15, 15), BlockState::new(3, 0));

        assert!(section.set_block(0, 0, 0, BlockState::AIR));
        assert_eq!(section.block(0, 0, 0), BlockState::AIR);
        assert!(!section.is_empty());
    }

    #[test]
    fn full_palette_rebuilds_before_adding_a_new_state() {
        let mut section = Section::empty();
        for raw in 1..BLOCKS_PER_SECTION as u16 {
            let index = raw as usize;
            let (x, z, y) = (index & 15, (index >> 4) & 15, index >> 8);
            assert!(section.set_block(x, y, z, BlockState::from_wire(raw)));
        }
        assert_eq!(section.palette_len(), BLOCKS_PER_SECTION);
        assert_eq!(section.bits_per_block(), 12);

        // Slot 0 still holds air; replacing it with a brand-new state forces
        // the palette to compact before inserting.
        assert!(section.set_block(0, 0, 0, BlockState::from_wire(4096)));
        assert_eq!(section.palette_len(), BLOCKS_PER_SECTION);
        assert_eq!(section.block(0, 0, 0), BlockState::from_wire(4096));
        assert_eq!(section.block(1, 0, 0), BlockState::from_wire(1));
        assert_eq!(section.block(15, 15, 15), BlockState::from_wire(4095));
    }

    #[test]
    fn out_of_range_access_is_safe() {
        let mut section = Section::empty();
        assert_eq!(section.block(16, 0, 0), BlockState::AIR);
        assert!(!section.set_block(0, 16, 0, BlockState::new(1, 0)));
        assert_eq!(section.block_light(16, 0, 0), 0);
        assert_eq!(section.sky_light(0, 0, 0), None);
    }
}
