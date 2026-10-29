// SPDX-License-Identifier: MIT OR Apache-2.0

//! Server-sent block entity data and block-break animations.
//!
//! Source: MCP `NetHandlerPlayClient.handleUpdateSign` /
//! `handleUpdateTileEntity` and `RenderGlobal.sendBlockBreakProgress`. The
//! reference only applies sign and tile entity updates to a block entity that
//! already exists in a loaded chunk; the chunk store of Umicraft keeps no
//! block entities yet, so the latest update per position is kept here until
//! the renderer consumes it.

use std::collections::BTreeMap;

use umicraft_protocol::packets::play::clientbound::block_break_anim::BlockBreakAnim;
use umicraft_protocol::packets::play::clientbound::update_block_entity::UpdateBlockEntity;
use umicraft_protocol::packets::play::clientbound::update_sign::UpdateSign;

/// A block position `(x, y, z)`.
pub type Pos = (i32, i32, i32);

/// Most positions kept per table. The positions are server-chosen, so the
/// tables are capped; an update for a known position always applies.
pub const MAX_ENTRIES: usize = 65_536;

/// The last NBT update of a block entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockEntityData {
    /// Entity type: 1 spawner, 2 command block, 3 beacon, 4 skull,
    /// 5 flower pot, 6 banner.
    pub action: u8,
    /// The raw NBT compound, `None` when the packet carried none.
    pub nbt: Option<Vec<u8>>,
}

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
    signs: BTreeMap<Pos, [String; 4]>,
    entities: BTreeMap<Pos, BlockEntityData>,
    breaking: BTreeMap<i32, BreakProgress>,
}

fn insert_capped<V>(map: &mut BTreeMap<Pos, V>, pos: Pos, value: V) {
    if map.len() < MAX_ENTRIES || map.contains_key(&pos) {
        map.insert(pos, value);
    }
}

impl BlockEntities {
    /// Stores the four lines of a sign (0x33).
    pub fn apply_sign(&mut self, packet: &UpdateSign) {
        insert_capped(
            &mut self.signs,
            (packet.x, packet.y, packet.z),
            packet.lines.clone(),
        );
    }

    /// Stores a block entity update (0x35).
    pub fn apply_update(&mut self, packet: &UpdateBlockEntity) {
        let data = BlockEntityData {
            action: packet.action,
            nbt: packet.nbt.clone(),
        };
        insert_capped(&mut self.entities, (packet.x, packet.y, packet.z), data);
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
        self.signs.remove(&pos);
        self.entities.remove(&pos);
    }

    /// Forgets everything (the dimension changed).
    pub fn clear(&mut self) {
        self.signs.clear();
        self.entities.clear();
        self.breaking.clear();
    }

    /// The four lines of the sign at a position.
    pub fn sign(&self, pos: Pos) -> Option<&[String; 4]> {
        self.signs.get(&pos)
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
    fn clear_forgets_everything() {
        let mut blocks = BlockEntities::default();
        blocks.apply_sign(&sign(1, "a"));
        blocks.apply_break_anim(&anim(1, 1, 1));
        blocks.clear();
        assert_eq!(blocks, BlockEntities::default());
    }
}
