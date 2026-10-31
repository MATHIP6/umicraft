// SPDX-License-Identifier: MIT OR Apache-2.0

//! Chest lid animation state.
//!
//! Source: MCP `TileEntityChest.update` / `receiveClientEvent`,
//! `TileEntityEnderChest.update` and `TileEntityChestRenderer.renderTileEntityAt`.
//! The server announces the viewer count (`numPlayersUsing`) of a chest as a
//! `Block Action` with instrument 1 and the count as pitch; the client then
//! moves the lid by 0.1 per 20 Hz tick towards open (viewers > 0) or shut and
//! the renderer eases the interpolated value.

use std::collections::BTreeMap;

use umicraft_protocol::packets::play::clientbound::block_action::BlockAction;

use super::block_entities::{Pos, MAX_ENTRIES};

/// Block ids whose `Block Action` drives a lid: chest, ender chest, trapped
/// chest.
const LID_BLOCKS: [i32; 3] = [54, 130, 146];
/// The chest event id (`receiveClientEvent` id 1: viewer count).
const VIEWERS_EVENT: u8 = 1;
/// Lid movement per tick (`float f1 = 0.1F` in `TileEntityChest.update`).
const LID_STEP: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Lid {
    viewers: u8,
    prev: f32,
    angle: f32,
}

/// The lids of all chests that are open or still closing.
///
/// Positions are server-chosen, so the table holds at most
/// [`MAX_ENTRIES`] chests; an update for a known chest always applies.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChestLids {
    lids: BTreeMap<Pos, Lid>,
}

impl ChestLids {
    /// Records the viewer count of a chest from a `Block Action` (0x24).
    /// Other blocks and other events are ignored. A count of 0 for an
    /// unknown chest is ignored too, since its lid is already shut.
    pub fn apply_action(&mut self, packet: &BlockAction) {
        if packet.instrument != VIEWERS_EVENT || !LID_BLOCKS.contains(&packet.block_id) {
            return;
        }
        let pos = (packet.x, packet.y, packet.z);
        if let Some(lid) = self.lids.get_mut(&pos) {
            lid.viewers = packet.pitch;
        } else if packet.pitch > 0 && self.lids.len() < MAX_ENTRIES {
            self.lids.insert(
                pos,
                Lid {
                    viewers: packet.pitch,
                    prev: 0.0,
                    angle: 0.0,
                },
            );
        }
    }

    /// Advances every lid one tick like `TileEntityChest.update`: the
    /// previous angle is kept for interpolation, the angle moves by 0.1
    /// towards open or shut and is clamped to `0..=1`. A shut lid with no
    /// viewers is dropped to bound memory.
    pub fn tick(&mut self) {
        self.lids.retain(|_, lid| {
            lid.prev = lid.angle;
            if lid.viewers > 0 {
                lid.angle = (lid.angle + LID_STEP).min(1.0);
            } else {
                lid.angle = (lid.angle - LID_STEP).max(0.0);
            }
            lid.viewers > 0 || lid.angle > 0.0
        });
    }

    /// Forgets every lid (dimension change or disconnect).
    pub fn forget_all(&mut self) {
        self.lids.clear();
    }

    /// The eased lid rotation in radians (`0.0` to `-PI/2`) of the chest at
    /// `pos`, `partial_tick` in `0..=1` between the last two ticks.
    ///
    /// `TileEntityChestRenderer` computes it per chest half; a double chest
    /// renders both halves with the larger magnitude, so callers combine the
    /// two results themselves (the more negative value wins). Unknown or
    /// shut chests give `0.0`.
    pub fn lid_angle(&self, pos: Pos, partial_tick: f32) -> f32 {
        let Some(lid) = self.lids.get(&pos) else {
            return 0.0;
        };
        let f = 1.0 - (lid.prev + (lid.angle - lid.prev) * partial_tick);
        let eased = 1.0 - f * f * f;
        // Adding 0.0 turns the -0.0 of a shut lid into +0.0.
        -(eased * std::f32::consts::FRAC_PI_2) + 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POS: Pos = (1, 64, 2);

    fn action(pos: Pos, instrument: u8, pitch: u8, block_id: i32) -> BlockAction {
        BlockAction {
            x: pos.0,
            y: pos.1,
            z: pos.2,
            instrument,
            pitch,
            block_id,
        }
    }

    fn open(lids: &mut ChestLids, viewers: u8) {
        lids.apply_action(&action(POS, 1, viewers, 54));
    }

    fn ticks(lids: &mut ChestLids, n: usize) {
        for _ in 0..n {
            lids.tick();
        }
    }

    fn eased(angle: f32) -> f32 {
        let f = 1.0 - angle;
        -((1.0 - f * f * f) * std::f32::consts::FRAC_PI_2)
    }

    #[test]
    fn opening_takes_ten_ticks() {
        let mut lids = ChestLids::default();
        open(&mut lids, 1);
        ticks(&mut lids, 9);
        assert!(lids.lid_angle(POS, 1.0) > -std::f32::consts::FRAC_PI_2);
        lids.tick();
        assert!((lids.lid_angle(POS, 1.0) + std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn closing_ramps_back_and_drops_the_entry() {
        let mut lids = ChestLids::default();
        open(&mut lids, 1);
        ticks(&mut lids, 12);
        open(&mut lids, 0);
        lids.tick();
        assert!((lids.lids[&POS].angle - 0.9).abs() < 1e-6);
        ticks(&mut lids, 10);
        assert!(lids.lids.is_empty());
        assert_eq!(lids.lid_angle(POS, 1.0), 0.0);
    }

    #[test]
    fn easing_matches_the_renderer_formula() {
        let mut lids = ChestLids::default();
        open(&mut lids, 2);
        ticks(&mut lids, 5);
        let angle = lids.lids[&POS].angle;
        assert!((lids.lid_angle(POS, 1.0) - eased(angle)).abs() < 1e-6);
    }

    #[test]
    fn partial_tick_interpolates_between_ticks() {
        let mut lids = ChestLids::default();
        open(&mut lids, 1);
        ticks(&mut lids, 3);
        let Lid { prev, angle, .. } = lids.lids[&POS];
        assert!((lids.lid_angle(POS, 0.0) - eased(prev)).abs() < 1e-6);
        let mid = lids.lid_angle(POS, 0.5);
        assert!((mid - eased((prev + angle) / 2.0)).abs() < 1e-6);
    }

    #[test]
    fn unrelated_blocks_and_events_are_ignored() {
        let mut lids = ChestLids::default();
        lids.apply_action(&action(POS, 1, 1, 25));
        lids.apply_action(&action(POS, 0, 1, 54));
        lids.apply_action(&action(POS, 1, 0, 54));
        assert_eq!(lids, ChestLids::default());
        lids.apply_action(&action(POS, 1, 1, 130));
        lids.apply_action(&action((9, 9, 9), 1, 1, 146));
        assert_eq!(lids.lids.len(), 2);
    }

    #[test]
    fn table_is_capped_but_known_chests_still_update() {
        let mut lids = ChestLids::default();
        for x in 0..MAX_ENTRIES as i32 {
            lids.apply_action(&action((x, 0, 0), 1, 1, 54));
        }
        lids.apply_action(&action((-1, 0, 0), 1, 1, 54));
        assert_eq!(lids.lids.len(), MAX_ENTRIES);
        lids.apply_action(&action((0, 0, 0), 1, 0, 54));
        assert_eq!(lids.lids[&(0, 0, 0)].viewers, 0);
    }

    #[test]
    fn forget_all_clears() {
        let mut lids = ChestLids::default();
        open(&mut lids, 1);
        lids.forget_all();
        assert_eq!(lids, ChestLids::default());
    }
}
