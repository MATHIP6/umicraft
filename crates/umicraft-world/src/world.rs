// SPDX-License-Identifier: MIT OR Apache-2.0

//! The loaded-world container and the updates the server sends.

use std::collections::HashMap;
use std::sync::Arc;

use crate::block::BlockState;
use crate::chunk::{ChunkColumn, ChunkPayload, WORLD_HEIGHT};
use crate::error::WorldError;
use crate::light::LightLevel;
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
    #[inline]
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

    /// Biome id at a world position.
    ///
    /// Biomes are stored once per column: Y is ignored. `None` means the chunk
    /// is not loaded, which the mesher renders untinted (white).
    #[must_use]
    #[inline]
    pub fn biome(&self, pos: BlockPos) -> Option<u8> {
        self.chunks
            .get(&pos.chunk())
            .map(|column| column.biomes()[(pos.local_z() as usize) << 4 | pos.local_x() as usize])
    }

    /// Combined light at a world position.
    ///
    /// Missing chunks and out-of-range Y read black. In the Nether and the
    /// End the sky channel is forced to zero even for an elided air section.
    #[must_use]
    #[inline]
    pub fn light(&self, pos: BlockPos) -> LightLevel {
        if !(0..WORLD_HEIGHT as i32).contains(&pos.y) {
            return LightLevel::DARK;
        }
        let Some(column) = self.chunks.get(&pos.chunk()) else {
            return LightLevel::DARK;
        };
        let light = column.light(
            pos.local_x() as usize,
            pos.y as usize,
            pos.local_z() as usize,
        );
        if self.has_sky_light() {
            light
        } else {
            LightLevel::new(light.block, 0)
        }
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

    /// Applies one chunk column payload (`Chunk Data`, 0x21).
    ///
    /// The sky-light layout follows the Join Game dimension. Returns the chunk
    /// positions whose rendered geometry may have changed (the column itself
    /// plus any loaded neighbours). A `ground_up` payload with an empty mask
    /// unloads the column.
    ///
    /// # Errors
    /// [`WorldError::ChunkPayloadLength`] for a malformed payload; the world is
    /// not modified in that case.
    pub fn apply_chunk(
        &mut self,
        pos: ChunkPos,
        payload: ChunkPayload<'_>,
    ) -> Result<Vec<ChunkPos>, WorldError> {
        if payload.ground_up && payload.primary_bit_mask == 0 {
            return Ok(self.unload_chunk(pos));
        }
        let has_sky_light = self.has_sky_light();
        self.apply_payload(pos, payload, has_sky_light)?;
        Ok(self.dirty_with_neighbours(pos))
    }

    /// Applies a batch of column payloads (`Map Chunk Bulk`, 0x26) that share
    /// one sky-light flag.
    ///
    /// Every payload is validated before the world changes, so a malformed
    /// batch applies nothing.
    ///
    /// # Errors
    /// [`WorldError::ChunkPayloadLength`] for the first malformed payload.
    pub fn apply_chunk_bulk(
        &mut self,
        has_sky_light: bool,
        chunks: &[(ChunkPos, ChunkPayload<'_>)],
    ) -> Result<Vec<ChunkPos>, WorldError> {
        for (_, payload) in chunks {
            payload.check_len(has_sky_light)?;
        }
        let mut dirty = Vec::with_capacity(chunks.len() * 2);
        for &(pos, payload) in chunks {
            self.apply_payload(pos, payload, has_sky_light)?;
            push_with_neighbours(&self.chunks, pos, &mut dirty);
        }
        dirty.sort_unstable();
        dirty.dedup();
        Ok(dirty)
    }

    /// Writes one block (`Block Change`, 0x23) and returns the dirty chunks.
    ///
    /// Border updates also dirty the adjacent chunk so its face culling can be
    /// rebuilt; ignored updates (see [`Self::set_block`]) dirty nothing.
    pub fn apply_block_change(&mut self, pos: BlockPos, state: BlockState) -> Vec<ChunkPos> {
        if self.set_block(pos, state) {
            self.dirty_for_block(pos.chunk(), pos.local_x(), pos.local_z())
        } else {
            Vec::new()
        }
    }

    /// Writes a batch of blocks of one column (`Multi Block Change`, 0x22).
    ///
    /// Each change is a chunk-local `(x, y, z)` and its new state. Returns the
    /// column and the loaded neighbours whose shared border changed.
    pub fn apply_block_changes(
        &mut self,
        chunk: ChunkPos,
        changes: impl IntoIterator<Item = ((u8, u8, u8), BlockState)>,
    ) -> Vec<ChunkPos> {
        let Some(column) = self.chunks.get_mut(&chunk) else {
            return Vec::new();
        };
        let column = Arc::make_mut(column);
        let mut changed = false;
        let mut edges = (false, false, false, false);
        for ((x, y, z), state) in changes {
            if column.set_block(usize::from(x), usize::from(y), usize::from(z), state) {
                changed = true;
                edges.0 |= x == 0;
                edges.1 |= x == 15;
                edges.2 |= z == 0;
                edges.3 |= z == 15;
            }
        }
        if changed {
            self.dirty_for_edges(chunk, edges)
        } else {
            Vec::new()
        }
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
        payload: ChunkPayload<'_>,
        has_sky_light: bool,
    ) -> Result<(), WorldError> {
        let ChunkPayload {
            primary_bit_mask,
            ground_up,
            data,
        } = payload;
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
    use crate::chunk::payload_len;

    fn payload(data: &[u8]) -> ChunkPayload<'_> {
        ChunkPayload {
            primary_bit_mask: 1,
            ground_up: true,
            data,
        }
    }

    fn flat_payload(block_state: u16) -> Vec<u8> {
        let mut data = vec![0u8; payload_len(1, true, true)];
        for pair in data[..8192].chunks_exact_mut(2) {
            pair.copy_from_slice(&block_state.to_le_bytes());
        }
        data[8192 + 2048..8192 + 4096].fill(0xFF);
        data[8192 + 4096..].fill(1);
        data
    }

    fn flat_payload_without_sky_light(block_state: u16) -> Vec<u8> {
        let mut data = vec![0u8; payload_len(1, false, true)];
        for pair in data[..8192].chunks_exact_mut(2) {
            pair.copy_from_slice(&block_state.to_le_bytes());
        }
        data[8192 + 2048..].fill(1);
        data
    }

    fn world_with_chunk(pos: ChunkPos) -> World {
        let mut world = World::new(0);
        world
            .apply_chunk(pos, payload(&flat_payload(0x20)))
            .unwrap();
        world
    }

    #[test]
    fn biome_reads_follow_the_column_and_missing_chunks_are_none() {
        let pos = ChunkPos::new(2, -3);
        let world = world_with_chunk(pos);
        let inside = BlockPos::new(pos.x * 16 + 15, 70, pos.z * 16 + 1);
        assert_eq!(world.biome(inside), Some(1));
        // Y does not matter: one biome per column.
        assert_eq!(world.biome(BlockPos::new(inside.x, 0, inside.z)), Some(1));
        assert_eq!(world.biome(BlockPos::new(1000, 70, 1000)), None);
    }

    #[test]
    fn light_follows_the_dimension_sky_rule() {
        let world = world_with_chunk(ChunkPos::new(0, 0));
        assert_eq!(
            world.light(BlockPos::new(0, 0, 0)),
            LightLevel { block: 0, sky: 15 }
        );
        // The Nether has no sky nibbles even where the column omits a section.
        let nether = World::new(-1);
        assert_eq!(nether.light(BlockPos::new(0, 0, 0)), LightLevel::DARK);
        // The loaded overworld column can still be mutated by the test.
        assert_eq!(world.chunk_count(), 1);
    }

    #[test]
    fn nether_forces_sky_light_to_zero_for_loaded_columns() {
        let mut world = World::new(-1);
        world
            .apply_chunk(
                ChunkPos::new(0, 0),
                payload(&flat_payload_without_sky_light(0x20)),
            )
            .unwrap();

        // Section 0 is present but has no sky array (Nether wire format).
        assert_eq!(
            world.light(BlockPos::new(0, 0, 0)),
            LightLevel { block: 0, sky: 0 }
        );
        // Section 1 is elided and would read FULL_SKY from the column; the
        // world still has to report no sky light.
        assert_eq!(world.light(BlockPos::new(0, 20, 0)), LightLevel::DARK);
    }

    #[test]
    fn light_outside_a_loaded_chunk_is_dark() {
        let world = World::new(0);
        assert_eq!(world.light(BlockPos::new(0, 0, 0)), LightLevel::DARK);
        let world = world_with_chunk(ChunkPos::new(0, 0));
        assert_eq!(world.light(BlockPos::new(16, 0, 0)), LightLevel::DARK);
        assert_eq!(world.light(BlockPos::new(0, 256, 0)), LightLevel::DARK);
    }

    #[test]
    fn applies_chunk_data_and_marks_neighbours() {
        let mut world = World::new(0);
        let data = flat_payload(0x20);
        assert_eq!(
            world
                .apply_chunk(ChunkPos::new(0, 0), payload(&data))
                .unwrap(),
            vec![ChunkPos::new(0, 0)]
        );
        assert_eq!(world.chunk_count(), 1);
        assert_eq!(world.block(BlockPos::new(15, 0, 15)), BlockState::new(2, 0));

        // Loading the western neighbour dirties both columns.
        assert_eq!(
            world
                .apply_chunk(ChunkPos::new(-1, 0), payload(&data))
                .unwrap(),
            vec![ChunkPos::new(-1, 0), ChunkPos::new(0, 0)]
        );
    }

    #[test]
    fn ground_up_empty_mask_unloads_the_column() {
        let mut world = world_with_chunk(ChunkPos::new(0, 0));
        assert_eq!(world.chunk_count(), 1);
        let unload = ChunkPayload {
            primary_bit_mask: 0,
            ground_up: true,
            data: &[],
        };
        assert_eq!(
            world.apply_chunk(ChunkPos::new(0, 0), unload).unwrap(),
            vec![ChunkPos::new(0, 0)]
        );
        assert_eq!(world.chunk_count(), 0);
        assert_eq!(world.block(BlockPos::new(0, 0, 0)), BlockState::AIR);
    }

    #[test]
    fn bulk_is_atomic_on_bad_payload() {
        let mut world = World::new(0);
        let good = flat_payload(0x20);
        let chunks = [
            (ChunkPos::new(0, 0), payload(&good)),
            (ChunkPos::new(1, 0), payload(&[0; 5])),
        ];
        let err = world.apply_chunk_bulk(true, &chunks).unwrap_err();
        assert!(matches!(err, WorldError::ChunkPayloadLength { .. }));
        assert_eq!(world.chunk_count(), 0, "validation happens before mutation");
    }

    #[test]
    fn a_malformed_chunk_leaves_the_world_unchanged() {
        let mut world = world_with_chunk(ChunkPos::new(0, 0));
        let err = world
            .apply_chunk(ChunkPos::new(0, 0), payload(&[0; 5]))
            .unwrap_err();
        assert!(matches!(err, WorldError::ChunkPayloadLength { .. }));
        assert_eq!(world.block(BlockPos::new(0, 0, 0)), BlockState::new(2, 0));
    }

    #[test]
    fn block_change_updates_and_dirties_border_neighbour() {
        let mut world = world_with_chunk(ChunkPos::new(0, 0));
        world
            .apply_chunk(ChunkPos::new(-1, 0), payload(&flat_payload(0x20)))
            .unwrap();

        assert_eq!(
            world.apply_block_change(BlockPos::new(-1, 5, 3), BlockState::from_wire(0x10)),
            vec![ChunkPos::new(-1, 0), ChunkPos::new(0, 0)]
        );
        assert_eq!(world.block(BlockPos::new(-1, 5, 3)), BlockState::new(1, 0));
    }

    #[test]
    fn multi_block_change_applies_chunk_local_offsets() {
        let mut world = world_with_chunk(ChunkPos::new(2, -3));
        let dirty = world.apply_block_changes(
            ChunkPos::new(2, -3),
            [
                ((0, 0, 0), BlockState::from_wire(0x70)),
                ((15, 255, 15), BlockState::from_wire(0x10)),
            ],
        );
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
        let stone = BlockState::new(1, 0);
        assert!(world
            .apply_block_change(BlockPos::new(0, 0, 0), stone)
            .is_empty());
        assert!(!world.set_block(BlockPos::new(0, 0, 0), stone));
        assert!(world
            .apply_block_changes(ChunkPos::new(0, 0), [((0, 0, 0), stone)])
            .is_empty());
    }
}
