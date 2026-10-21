// SPDX-License-Identifier: MIT OR Apache-2.0

//! The loaded-world container and packet application.

use std::collections::HashMap;
use std::sync::Arc;

use umicraft_protocol::packets::play::clientbound::chunk_data::chunk_payload_len;
use umicraft_protocol::packets::play::clientbound::{
    BlockChange, ChunkData, MapChunkBulk, MultiBlockChange,
};

use crate::block::BlockState;
use crate::chunk::{ChunkColumn, WORLD_HEIGHT};
use crate::error::WorldError;
use crate::pos::{BlockPos, ChunkPos};

/// Chunks loaded from the server, keyed by column position.
///
/// Columns are stored behind [`Arc`] so a mesher can take cheap snapshots for
/// worker threads; updates use copy-on-write.
#[derive(Debug, Default)]
pub struct World {
    dimension: i8,
    chunks: HashMap<ChunkPos, Arc<ChunkColumn>>,
}

impl World {
    /// Creates an empty world for a Join Game dimension (`0` overworld, `-1`
    /// nether, `1` end).
    #[must_use]
    pub fn new(dimension: i8) -> Self {
        Self {
            dimension,
            chunks: HashMap::new(),
        }
    }

    /// Join Game dimension.
    #[must_use]
    pub const fn dimension(&self) -> i8 {
        self.dimension
    }

    /// True when decoded chunks carry sky-light nibbles (overworld).
    #[must_use]
    pub const fn has_sky_light(&self) -> bool {
        self.dimension == 0
    }

    /// Number of loaded chunk columns.
    #[must_use]
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Loaded chunk column, if present.
    #[must_use]
    pub fn chunk(&self, pos: ChunkPos) -> Option<&ChunkColumn> {
        self.chunks.get(&pos).map(Arc::as_ref)
    }

    /// Shared handle to a loaded chunk column, for snapshot-based meshing.
    #[must_use]
    pub fn chunk_shared(&self, pos: ChunkPos) -> Option<Arc<ChunkColumn>> {
        self.chunks.get(&pos).cloned()
    }

    /// Iterates over loaded chunks.
    pub fn chunks(&self) -> impl Iterator<Item = (ChunkPos, &ChunkColumn)> {
        self.chunks
            .iter()
            .map(|(pos, column)| (*pos, column.as_ref()))
    }

    /// Block at a world position; missing chunks and out-of-range Y read air.
    #[must_use]
    pub fn block(&self, pos: BlockPos) -> BlockState {
        if !(0..WORLD_HEIGHT as i32).contains(&pos.y) {
            return BlockState::AIR;
        }
        self.chunks
            .get(&pos.chunk())
            .map_or(BlockState::AIR, |column| {
                column.block(
                    pos.local_x() as usize,
                    pos.y as usize,
                    pos.local_z() as usize,
                )
            })
    }

    /// Writes a block, returning `true` when it changed.
    ///
    /// Updates for unloaded chunks, out-of-range Y, or an unchanged state are
    /// ignored; servers may race block updates against chunk unloads.
    pub fn set_block(&mut self, pos: BlockPos, state: BlockState) -> bool {
        if !(0..WORLD_HEIGHT as i32).contains(&pos.y) {
            return false;
        }
        let Some(column) = self.chunks.get_mut(&pos.chunk()) else {
            return false;
        };
        Arc::make_mut(column).set_block(
            pos.local_x() as usize,
            pos.y as usize,
            pos.local_z() as usize,
            state,
        )
    }

    /// Applies one `Chunk Data` (0x21) packet.
    ///
    /// Returns the chunk positions whose rendered geometry may have changed
    /// (the column itself plus any loaded neighbours). A `ground_up` packet
    /// with an empty mask unloads the column.
    ///
    /// # Errors
    /// [`WorldError::ChunkPayloadLength`] for a malformed payload.
    pub fn apply_chunk_data(&mut self, packet: &ChunkData) -> Result<Vec<ChunkPos>, WorldError> {
        let pos = ChunkPos::new(packet.chunk_x, packet.chunk_z);
        if packet.ground_up && packet.primary_bit_mask == 0 {
            return Ok(self.unload_chunk(pos));
        }
        let has_sky_light = self.has_sky_light();
        self.apply_payload(
            pos,
            packet.primary_bit_mask,
            packet.ground_up,
            has_sky_light,
            &packet.data,
        )?;
        Ok(self.dirty_with_neighbours(pos))
    }

    /// Applies a `Map Chunk Bulk` (0x26) packet, validating every payload
    /// before mutating the world.
    ///
    /// # Errors
    /// [`WorldError::ChunkPayloadLength`] for a malformed payload; no chunk is
    /// applied in that case.
    pub fn apply_map_chunk_bulk(
        &mut self,
        packet: &MapChunkBulk,
    ) -> Result<Vec<ChunkPos>, WorldError> {
        for chunk in &packet.chunks {
            let expected = chunk_payload_len(chunk.primary_bit_mask, packet.sky_light, true);
            if chunk.data.len() != expected {
                return Err(WorldError::ChunkPayloadLength {
                    expected,
                    actual: chunk.data.len(),
                    primary_bit_mask: chunk.primary_bit_mask,
                    sky_light: packet.sky_light,
                    ground_up: true,
                });
            }
        }

        let mut dirty = Vec::with_capacity(packet.chunks.len() * 2);
        for chunk in &packet.chunks {
            let pos = ChunkPos::new(chunk.chunk_x, chunk.chunk_z);
            self.apply_payload(
                pos,
                chunk.primary_bit_mask,
                true,
                packet.sky_light,
                &chunk.data,
            )?;
            push_with_neighbours(&self.chunks, pos, &mut dirty);
        }
        dirty.sort_unstable();
        dirty.dedup();
        Ok(dirty)
    }

    /// Applies one `Block Change` (0x23) packet and returns the dirty chunks.
    ///
    /// Border updates also dirty the adjacent chunk so its face culling can be
    /// rebuilt.
    pub fn apply_block_change(&mut self, packet: &BlockChange) -> Vec<ChunkPos> {
        let (x, y, z) = packet.position();
        let pos = BlockPos::new(x, y, z);
        if !(0..WORLD_HEIGHT as i32).contains(&pos.y) {
            return Vec::new();
        }
        let chunk_pos = pos.chunk();
        let Some(column) = self.chunks.get_mut(&chunk_pos) else {
            return Vec::new();
        };
        let changed = Arc::make_mut(column).set_block(
            pos.local_x() as usize,
            pos.y as usize,
            pos.local_z() as usize,
            BlockState::from_wire(packet.block_state),
        );
        if changed {
            self.dirty_for_block(chunk_pos, pos.local_x(), pos.local_z())
        } else {
            Vec::new()
        }
    }

    /// Applies one `Multi Block Change` (0x22) packet.
    pub fn apply_multi_block_change(&mut self, packet: &MultiBlockChange) -> Vec<ChunkPos> {
        let chunk_pos = ChunkPos::new(packet.chunk_x, packet.chunk_z);
        let Some(column) = self.chunks.get_mut(&chunk_pos) else {
            return Vec::new();
        };
        let column = Arc::make_mut(column);
        let mut changed = false;
        let mut touches_west = false;
        let mut touches_east = false;
        let mut touches_north = false;
        let mut touches_south = false;
        for record in &packet.records {
            let (x, y, z) = record.local_position();
            if column.set_block(
                usize::from(x),
                usize::from(y),
                usize::from(z),
                BlockState::from_wire(record.block_state),
            ) {
                changed = true;
                touches_west |= x == 0;
                touches_east |= x == 15;
                touches_north |= z == 0;
                touches_south |= z == 15;
            }
        }
        if !changed {
            return Vec::new();
        }
        self.dirty_for_edges(
            chunk_pos,
            (touches_west, touches_east, touches_north, touches_south),
        )
    }

    /// Removes a column, returning the dirty positions (the column plus loaded
    /// neighbours that now expose a border face).
    pub fn unload_chunk(&mut self, pos: ChunkPos) -> Vec<ChunkPos> {
        if self.chunks.remove(&pos).is_none() {
            return Vec::new();
        }
        let mut dirty = vec![pos];
        for neighbour in pos.neighbours() {
            if self.chunks.contains_key(&neighbour) {
                dirty.push(neighbour);
            }
        }
        dirty.sort_unstable();
        dirty.dedup();
        dirty
    }

    fn apply_payload(
        &mut self,
        pos: ChunkPos,
        primary_bit_mask: u16,
        ground_up: bool,
        has_sky_light: bool,
        data: &[u8],
    ) -> Result<(), WorldError> {
        if let Some(existing) = self.chunks.get_mut(&pos) {
            Arc::make_mut(existing).apply_payload(
                primary_bit_mask,
                ground_up,
                has_sky_light,
                data,
            )?;
        } else {
            let mut column = ChunkColumn::new(pos);
            column.apply_payload(primary_bit_mask, ground_up, has_sky_light, data)?;
            self.chunks.insert(pos, Arc::new(column));
        }
        Ok(())
    }

    fn dirty_with_neighbours(&self, pos: ChunkPos) -> Vec<ChunkPos> {
        let mut dirty = Vec::with_capacity(5);
        push_with_neighbours(&self.chunks, pos, &mut dirty);
        dirty.sort_unstable();
        dirty.dedup();
        dirty
    }

    fn dirty_for_block(&self, chunk: ChunkPos, local_x: u8, local_z: u8) -> Vec<ChunkPos> {
        self.dirty_for_edges(
            chunk,
            (local_x == 0, local_x == 15, local_z == 0, local_z == 15),
        )
    }

    fn dirty_for_edges(&self, chunk: ChunkPos, edges: (bool, bool, bool, bool)) -> Vec<ChunkPos> {
        let (west, east, north, south) = edges;
        let mut dirty = vec![chunk];
        if west {
            push_if_loaded(
                &self.chunks,
                ChunkPos::new(chunk.x - 1, chunk.z),
                &mut dirty,
            );
        }
        if east {
            push_if_loaded(
                &self.chunks,
                ChunkPos::new(chunk.x + 1, chunk.z),
                &mut dirty,
            );
        }
        if north {
            push_if_loaded(
                &self.chunks,
                ChunkPos::new(chunk.x, chunk.z - 1),
                &mut dirty,
            );
        }
        if south {
            push_if_loaded(
                &self.chunks,
                ChunkPos::new(chunk.x, chunk.z + 1),
                &mut dirty,
            );
        }
        dirty.sort_unstable();
        dirty.dedup();
        dirty
    }
}

fn push_with_neighbours(
    chunks: &HashMap<ChunkPos, Arc<ChunkColumn>>,
    pos: ChunkPos,
    out: &mut Vec<ChunkPos>,
) {
    out.push(pos);
    for neighbour in pos.neighbours() {
        push_if_loaded(chunks, neighbour, out);
    }
}

fn push_if_loaded(
    chunks: &HashMap<ChunkPos, Arc<ChunkColumn>>,
    pos: ChunkPos,
    out: &mut Vec<ChunkPos>,
) {
    if chunks.contains_key(&pos) {
        out.push(pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_protocol::packets::play::clientbound::{
        chunk_data::chunk_payload_len, BlockUpdate, BulkChunk,
    };

    fn flat_payload(block_state: u16) -> Vec<u8> {
        let mut data = vec![0u8; chunk_payload_len(1, true, true)];
        for pair in data[..8192].chunks_exact_mut(2) {
            pair.copy_from_slice(&block_state.to_le_bytes());
        }
        data[8192 + 2048..8192 + 4096].fill(0xFF);
        data[8192 + 4096..].fill(1);
        data
    }

    fn world_with_chunk(pos: ChunkPos) -> World {
        let mut world = World::new(0);
        let packet = ChunkData {
            chunk_x: pos.x,
            chunk_z: pos.z,
            ground_up: true,
            primary_bit_mask: 1,
            data: flat_payload(0x20),
        };
        world.apply_chunk_data(&packet).unwrap();
        world
    }

    #[test]
    fn applies_chunk_data_and_marks_neighbours() {
        let mut world = World::new(0);
        let packet = ChunkData {
            chunk_x: 0,
            chunk_z: 0,
            ground_up: true,
            primary_bit_mask: 1,
            data: flat_payload(0x20),
        };
        assert_eq!(
            world.apply_chunk_data(&packet).unwrap(),
            vec![ChunkPos::new(0, 0)]
        );
        assert_eq!(world.chunk_count(), 1);
        assert_eq!(world.block(BlockPos::new(15, 0, 15)), BlockState::new(2, 0));

        // Loading the western neighbour dirties both columns.
        let mut west = packet.clone();
        west.chunk_x = -1;
        assert_eq!(
            world.apply_chunk_data(&west).unwrap(),
            vec![ChunkPos::new(-1, 0), ChunkPos::new(0, 0)]
        );
    }

    #[test]
    fn ground_up_empty_mask_unloads_the_column() {
        let mut world = world_with_chunk(ChunkPos::new(0, 0));
        assert_eq!(world.chunk_count(), 1);
        let unload = ChunkData {
            chunk_x: 0,
            chunk_z: 0,
            ground_up: true,
            primary_bit_mask: 0,
            data: Vec::new(),
        };
        assert_eq!(
            world.apply_chunk_data(&unload).unwrap(),
            vec![ChunkPos::new(0, 0)]
        );
        assert_eq!(world.chunk_count(), 0);
        assert_eq!(world.block(BlockPos::new(0, 0, 0)), BlockState::AIR);
    }

    #[test]
    fn bulk_is_atomic_on_bad_payload() {
        let mut world = World::new(0);
        let good = BulkChunk {
            chunk_x: 0,
            chunk_z: 0,
            primary_bit_mask: 1,
            data: flat_payload(0x20),
        };
        let bad = BulkChunk {
            chunk_x: 1,
            chunk_z: 0,
            primary_bit_mask: 1,
            data: vec![0; 5],
        };
        let packet = MapChunkBulk {
            sky_light: true,
            chunks: vec![good, bad],
        };
        let err = world.apply_map_chunk_bulk(&packet).unwrap_err();
        assert!(matches!(err, WorldError::ChunkPayloadLength { .. }));
        assert_eq!(world.chunk_count(), 0, "validation happens before mutation");
    }

    #[test]
    fn block_change_updates_and_dirties_border_neighbour() {
        let mut world = world_with_chunk(ChunkPos::new(0, 0));
        world
            .apply_chunk_data(&ChunkData {
                chunk_x: -1,
                chunk_z: 0,
                ground_up: true,
                primary_bit_mask: 1,
                data: flat_payload(0x20),
            })
            .unwrap();

        let packet = BlockChange::at(-1, 5, 3, 0x10);
        assert_eq!(
            world.apply_block_change(&packet),
            vec![ChunkPos::new(-1, 0), ChunkPos::new(0, 0)]
        );
        assert_eq!(world.block(BlockPos::new(-1, 5, 3)), BlockState::new(1, 0));
    }

    #[test]
    fn multi_block_change_applies_chunk_local_offsets() {
        let mut world = world_with_chunk(ChunkPos::new(2, -3));
        let packet = MultiBlockChange {
            chunk_x: 2,
            chunk_z: -3,
            records: vec![
                BlockUpdate::at(0, 0, 0, 0x70),
                BlockUpdate::at(15, 255, 15, 0x10),
            ],
        };
        let dirty = world.apply_multi_block_change(&packet);
        assert_eq!(dirty, vec![ChunkPos::new(2, -3)]);
        assert_eq!(
            world.block(BlockPos::new(32, 0, -48)),
            BlockState::new(7, 0)
        );
        assert_eq!(
            world.block(BlockPos::new(47, 255, -33)),
            BlockState::new(1, 0)
        );
    }

    #[test]
    fn updates_outside_loaded_chunks_are_ignored() {
        let mut world = World::new(0);
        assert!(world
            .apply_block_change(&BlockChange::at(0, 0, 0, 0x10))
            .is_empty());
        assert!(!world.set_block(BlockPos::new(0, 0, 0), BlockState::new(1, 0)));
        assert!(world
            .apply_multi_block_change(&MultiBlockChange {
                chunk_x: 0,
                chunk_z: 0,
                records: vec![BlockUpdate::at(0, 0, 0, 1)],
            })
            .is_empty());
    }
}
