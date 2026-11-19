// SPDX-License-Identifier: MIT OR Apache-2.0

//! Server-sent block entity data and block-break animations.
//!
//! Source: MCP `NetHandlerPlayClient.handleUpdateSign` /
//! `handleUpdateTileEntity` and `RenderGlobal.sendBlockBreakProgress`. The
//! reference only applies sign and tile entity updates to a block entity that
//! already exists in a loaded chunk; the chunk store of Umicraft keeps no
//! block entities yet, so the latest update per position is kept here until
//! the renderer consumes it. An entry goes with its block, as a tile entity
//! does: the game thread calls [`BlockEntities::remove`] when the block at the
//! position becomes another block and [`BlockEntities::remove_column`] when a
//! column is unloaded or replaced (MCP `Chunk.setBlockState`,
//! `ChunkProviderClient.unloadChunk`).

use std::collections::BTreeMap;
use std::sync::Arc;

use umicraft_protocol::packets::play::clientbound::block_break_anim::BlockBreakAnim;
use umicraft_protocol::packets::play::clientbound::update_block_entity::UpdateBlockEntity;
use umicraft_protocol::packets::play::clientbound::update_sign::UpdateSign;

/// A block position `(x, y, z)`.
pub type Pos = (i32, i32, i32);

/// Most positions kept per table. The positions are server-chosen, so the
/// tables are capped; an update for a known position always applies.
pub const MAX_ENTRIES: usize = 65_536;

/// The four lines of every known sign, by position.
pub type SignTable = BTreeMap<Pos, [String; 4]>;

/// The last NBT update of a block entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockEntityData {
    /// Entity type: 1 spawner, 2 command block, 3 beacon, 4 skull,
    /// 5 flower pot, 6 banner.
    pub action: u8,
    /// The raw NBT compound, `None` when the packet carried none.
    pub nbt: Option<Vec<u8>>,
}

/// The last update of every block entity that sent one.
pub type EntityTable = BTreeMap<Pos, BlockEntityData>;

/// A running block-break animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreakProgress {
    /// The block being broken.
    pub pos: Pos,
    /// Crack stage, `0..=9`.
    pub stage: u8,
}

/// Signs, other block entities and break animations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockEntities {
    /// Shared so the render thread gets a snapshot for the price of a
    /// pointer; a change copies the table only while a snapshot is held.
    signs: Arc<SignTable>,
    entities: Arc<EntityTable>,
    breaking: BTreeMap<i32, BreakProgress>,
}

impl BlockEntities {
    /// Stores the four lines of a sign (0x33).
    pub fn apply_sign(&mut self, packet: &UpdateSign) {
        self.set_sign((packet.x, packet.y, packet.z), packet.lines.clone());
    }

    /// Sets the four lines (chat JSON) of the sign at `pos`: a server update,
    /// or the text the player is typing in the sign editor.
    pub fn set_sign(&mut self, pos: Pos, lines: [String; 4]) {
        if self.signs.len() < MAX_ENTRIES || self.signs.contains_key(&pos) {
            Arc::make_mut(&mut self.signs).insert(pos, lines);
        }
    }

    /// Stores a block entity update (0x35).
    pub fn apply_update(&mut self, packet: &UpdateBlockEntity) {
        let data = BlockEntityData {
            action: packet.action,
            nbt: packet.nbt.clone(),
        };
        let pos = (packet.x, packet.y, packet.z);
        if self.entities.len() < MAX_ENTRIES || self.entities.contains_key(&pos) {
            Arc::make_mut(&mut self.entities).insert(pos, data);
        }
    }

    /// Applies a break animation (0x25): per breaker, stages `0..=9` set the
    /// block and stage, anything else ends the animation.
    pub fn apply_break_anim(&mut self, packet: &BlockBreakAnim) {
        if packet.stage <= 9 {
            self.breaking.insert(
                packet.entity_id,
                BreakProgress {
                    pos: (packet.x, packet.y, packet.z),
                    stage: packet.stage,
                },
            );
        } else {
            self.breaking.remove(&packet.entity_id);
        }
    }

    /// Forgets the data of a position (the block was replaced).
    pub fn remove(&mut self, pos: Pos) {
        if self.signs.contains_key(&pos) {
            Arc::make_mut(&mut self.signs).remove(&pos);
        }
        if self.entities.contains_key(&pos) {
            Arc::make_mut(&mut self.entities).remove(&pos);
        }
    }

    /// Forgets the data of every position in the chunk column `(chunk_x,
    /// chunk_z)`: the column was unloaded or replaced by a ground-up payload,
    /// whose block entities the server sends again (MCP
    /// `ChunkProviderClient.unloadChunk` drops the tile entities with the
    /// chunk).
    pub fn remove_column(&mut self, chunk_x: i32, chunk_z: i32) {
        remove_column_of(&mut self.signs, chunk_x, chunk_z);
        remove_column_of(&mut self.entities, chunk_x, chunk_z);
    }

    /// Forgets everything (the dimension changed).
    pub fn clear(&mut self) {
        if !self.signs.is_empty() {
            self.signs = Arc::default();
        }
        if !self.entities.is_empty() {
            self.entities = Arc::default();
        }
        self.breaking.clear();
    }

    /// A shared snapshot of the updates of the other block entities (skulls,
    /// banners, beacons, spawners, flower pots), for the render thread.
    #[must_use]
    pub fn entity_table(&self) -> Arc<EntityTable> {
        Arc::clone(&self.entities)
    }

    /// The four lines of the sign at a position.
    pub fn sign(&self, pos: Pos) -> Option<&[String; 4]> {
        self.signs.get(&pos)
    }

    /// A shared snapshot of all signs, for the render thread.
    #[must_use]
    pub fn sign_table(&self) -> Arc<SignTable> {
        Arc::clone(&self.signs)
    }

    /// All signs.
    pub fn signs(&self) -> impl Iterator<Item = (Pos, &[String; 4])> {
        self.signs.iter().map(|(pos, lines)| (*pos, lines))
    }

    /// The last update of the block entity at a position.
    pub fn block_entity(&self, pos: Pos) -> Option<&BlockEntityData> {
        self.entities.get(&pos)
    }

    /// The animation of one breaker.
    pub fn break_progress(&self, breaker: i32) -> Option<&BreakProgress> {
        self.breaking.get(&breaker)
    }

    /// All running animations as `(breaker entity id, progress)`.
    pub fn break_animations(&self) -> impl Iterator<Item = (i32, &BreakProgress)> {
        self.breaking.iter().map(|(id, progress)| (*id, progress))
    }
}

/// Removes the entries of `table` inside the chunk column, copying a shared
/// table only when something is removed. The table is ordered by `x` first,
/// so only the sixteen `x` slices of the column are visited.
fn remove_column_of<V: Clone>(table: &mut Arc<BTreeMap<Pos, V>>, chunk_x: i32, chunk_z: i32) {
    let x0 = chunk_x.saturating_mul(16);
    let x1 = x0.saturating_add(15);
    let z_range = chunk_z.saturating_mul(16)..=chunk_z.saturating_mul(16).saturating_add(15);
    let doomed: Vec<Pos> = table
        .range((x0, i32::MIN, i32::MIN)..=(x1, i32::MAX, i32::MAX))
        .map(|(pos, _)| *pos)
        .filter(|&(_, _, z)| z_range.contains(&z))
        .collect();
    if doomed.is_empty() {
        return;
    }
    let table = Arc::make_mut(table);
    for pos in doomed {
        table.remove(&pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign(x: i32, text: &str) -> UpdateSign {
        UpdateSign {
            x,
            y: 64,
            z: 0,
            lines: [text.to_owned(), String::new(), String::new(), String::new()],
        }
    }

    fn anim(entity_id: i32, x: i32, stage: u8) -> BlockBreakAnim {
        BlockBreakAnim {
            entity_id,
            x,
            y: 5,
            z: 6,
            stage,
        }
    }

    #[test]
    fn a_sign_update_replaces_the_lines() {
        let mut blocks = BlockEntities::default();
        blocks.apply_sign(&sign(1, "a"));
        blocks.apply_sign(&sign(1, "b"));
        assert_eq!(blocks.sign((1, 64, 0)).unwrap()[0], "b");
        assert_eq!(blocks.signs().count(), 1);
        assert!(blocks.sign((2, 64, 0)).is_none());
    }

    #[test]
    fn a_snapshot_is_shared_until_a_sign_changes_and_never_sees_the_change() {
        let mut blocks = BlockEntities::default();
        blocks.apply_sign(&sign(1, "a"));
        let before = blocks.sign_table();
        assert!(Arc::ptr_eq(&before, &blocks.sign_table()));
        blocks.apply_sign(&sign(1, "b"));
        assert_eq!(before[&(1, 64, 0)][0], "a");
        assert_eq!(blocks.sign_table()[&(1, 64, 0)][0], "b");
    }

    #[test]
    fn a_block_entity_update_keeps_action_and_nbt() {
        let mut blocks = BlockEntities::default();
        blocks.apply_update(&UpdateBlockEntity {
            x: 1,
            y: 2,
            z: 3,
            action: 3,
            nbt: Some(vec![10, 0, 0, 0]),
        });
        let data = blocks.block_entity((1, 2, 3)).unwrap();
        assert_eq!(data.action, 3);
        assert_eq!(data.nbt.as_deref(), Some(&[10, 0, 0, 0][..]));
        blocks.remove((1, 2, 3));
        assert!(blocks.block_entity((1, 2, 3)).is_none());
    }

    #[test]
    fn break_stages_set_move_and_end_the_animation() {
        let mut blocks = BlockEntities::default();
        blocks.apply_break_anim(&anim(7, 1, 0));
        blocks.apply_break_anim(&anim(7, 2, 9));
        let progress = blocks.break_progress(7).unwrap();
        assert_eq!((progress.pos, progress.stage), ((2, 5, 6), 9));
        assert_eq!(blocks.break_animations().count(), 1);
        blocks.apply_break_anim(&anim(7, 2, 10));
        assert!(blocks.break_progress(7).is_none());
        blocks.apply_break_anim(&anim(8, 2, 255));
        assert_eq!(blocks.break_animations().count(), 0);
    }

    #[test]
    fn the_tables_are_capped_but_known_positions_still_update() {
        let mut blocks = BlockEntities::default();
        for x in 0..(MAX_ENTRIES as i32 + 5) {
            blocks.apply_sign(&sign(x, "a"));
        }
        assert_eq!(blocks.signs().count(), MAX_ENTRIES);
        blocks.apply_sign(&sign(0, "new"));
        assert_eq!(blocks.sign((0, 64, 0)).unwrap()[0], "new");
    }

    #[test]
    fn removing_a_column_forgets_only_its_signs_and_block_entities() {
        let mut blocks = BlockEntities::default();
        for (x, z) in [(0, 0), (15, 15), (16, 0), (-1, 0), (3, 16)] {
            blocks.apply_sign(&UpdateSign {
                x,
                y: 64,
                z,
                lines: Default::default(),
            });
        }
        blocks.apply_update(&UpdateBlockEntity {
            x: 7,
            y: 2,
            z: 9,
            action: 4,
            nbt: None,
        });
        blocks.remove_column(0, 0);
        let left: Vec<Pos> = blocks.signs().map(|(pos, _)| pos).collect();
        assert_eq!(left, [(-1, 64, 0), (3, 64, 16), (16, 64, 0)]);
        assert!(blocks.block_entity((7, 2, 9)).is_none());
    }

    #[test]
    fn removing_an_empty_column_keeps_the_snapshot_shared() {
        let mut blocks = BlockEntities::default();
        blocks.apply_sign(&sign(40, "a"));
        let before = blocks.sign_table();
        blocks.remove_column(0, 0);
        assert!(Arc::ptr_eq(&before, &blocks.sign_table()));
    }

    #[test]
    fn clear_forgets_everything() {
        let mut blocks = BlockEntities::default();
        blocks.apply_sign(&sign(1, "a"));
        blocks.apply_break_anim(&anim(1, 1, 1));
        blocks.clear();
        assert_eq!(blocks, BlockEntities::default());
    }
}
