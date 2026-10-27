// SPDX-License-Identifier: MIT OR Apache-2.0

//! The block interaction rules of the local player.
//!
//! [`Interaction`] takes the inputs of the player (a ray-cast hit, the held
//! item, the tick) and the confirmations of the server (block changes, the
//! placement packet) and answers with the [`Outgoing`] packets to send. It
//! owns no channel and reads no clock, so the whole dig / place / acknowledge
//! cycle is unit-testable; the client only maps [`Outgoing`] onto its network
//! commands.
//!
//! The rules follow MCP `PlayerControllerMP` (`clickBlock`,
//! `onPlayerRightClick`) and `EntityPlayerSP.onUpdate`; the per-mode
//! differences live in [`GameMode`].

use glam::DVec3;
use umicraft_protocol::packets::play::serverbound::DiggingStatus;
use umicraft_protocol::packets::play::{Facing, ItemStack};
use umicraft_world::BlockPos;

use crate::gamemode::GameMode;
use crate::raycast::{Face, RayHit};
use crate::shapes::Aabb;

/// The number of ticks between the start and the stop of a block break.
///
/// The server destroys the block when the stop packet arrives and the
/// accumulated break progress `hardness * (ticks + 1)` reaches `0.7`
/// (MCP `ItemInWorldManager.func_180785_a`). The effective hardness of the
/// softest block is 0.054 for a player without the right tool, so the stop
/// needs about thirteen server ticks. **Measured** against the local 1.8
/// server: a stop after six ticks did not destroy the block, a stop after
/// twelve ticks did (see `docs/sources.md`). A fixed stand-in for the vanilla
/// progress that the hold-to-break work of milestone P2 replaces.
pub const BREAK_TICKS: u64 = 12;

/// The position the acknowledgement of a server placement repeats.
const SAME_POSITION_TOLERANCE: f64 = 1e-6;

/// A packet the interaction rules ask the client to send.
#[derive(Debug, Clone, PartialEq)]
pub enum Outgoing {
    /// `Player Digging` (0x07).
    Dig {
        /// The stage of the interaction.
        status: DiggingStatus,
        /// The block that is dug.
        block: BlockPos,
        /// The face of the block that was hit.
        face: Facing,
    },
    /// `Player Block Placement` (0x08).
    Place {
        /// The block that was clicked.
        block: BlockPos,
        /// The face of the block that was clicked.
        face: Facing,
        /// The item the player holds.
        item: ItemStack,
        /// The click offset in sixteenths of a block, as `[x, y, z]`.
        cursor: [u8; 3],
    },
    /// `Player Position and Look` (0x06) answering a server placement.
    AckPosition(PositionAck),
}

/// The acknowledgement of a position the server imposed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionAck {
    /// The feet X.
    pub x: f64,
    /// The feet Y.
    pub y: f64,
    /// The feet Z.
    pub z: f64,
    /// The yaw in degrees.
    pub yaw: f32,
    /// The pitch in degrees.
    pub pitch: f32,
    /// Whether the player rests on the ground.
    pub on_ground: bool,
}

/// What a server block change confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockAck {
    /// The block the player broke.
    Break,
    /// The block the player placed.
    Placement,
}

/// A break in progress.
#[derive(Debug, Clone, Copy)]
struct Digging {
    block: BlockPos,
    face: Facing,
    /// The tick at which the stop packet must be sent.
    stop_at: u64,
}

/// The interaction state of the local player.
#[derive(Debug, Default)]
pub struct Interaction {
    digging: Option<Digging>,
    /// The block whose break the server is expected to confirm.
    expected_break: Option<BlockPos>,
    /// The block whose placement the server is expected to confirm.
    expected_placement: Option<BlockPos>,
    /// The last position that was acknowledged to the server.
    last_acknowledged: Option<PositionAck>,
}

impl Interaction {
    /// Creates an idle interaction.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The block that is being broken.
    #[must_use]
    pub fn digging_block(&self) -> Option<BlockPos> {
        self.digging.map(|digging| digging.block)
    }

    /// Starts breaking the targeted block.
    ///
    /// Returns the start packet, or `None` when the mode forbids breaking.
    /// In creative mode only the start packet exists (the block breaks at
    /// once, MCP `PlayerControllerMP.clickBlock`) and the caller removes the
    /// block locally; in the other modes the stop packet follows
    /// [`BREAK_TICKS`] ticks later (see [`Interaction::finish_break`]).
    pub fn start_break(&mut self, mode: GameMode, hit: &RayHit, tick: u64) -> Option<Outgoing> {
        if !mode.can_break() {
            return None;
        }
        let face = wire_face(hit.face);
        self.digging = (!mode.instant_break()).then_some(Digging {
            block: hit.block,
            face,
            stop_at: tick + BREAK_TICKS,
        });
        self.expected_break = Some(hit.block);
        Some(Outgoing::Dig {
            status: DiggingStatus::StartDestroyBlock,
            block: hit.block,
            face,
        })
    }

    /// Returns the stop packet of a running break once its time has come.
    pub fn finish_break(&mut self, tick: u64) -> Option<Outgoing> {
        let digging = self.digging.filter(|digging| tick >= digging.stop_at)?;
        self.digging = None;
        Some(Outgoing::Dig {
            status: DiggingStatus::StopDestroyBlock,
            block: digging.block,
            face: digging.face,
        })
    }

    /// Places a block against the targeted face.
    ///
    /// `player` is the bounding box of the player: a block inside it would
    /// suffocate the player (the server applies the damage), so the
    /// placement is skipped. The packet is sent in every mode, like MCP
    /// `onPlayerRightClick`; only the confirmation is awaited when the mode
    /// can place.
    pub fn place_block(
        &mut self,
        mode: GameMode,
        hit: &RayHit,
        player: Option<&Aabb>,
        item: ItemStack,
    ) -> Option<Outgoing> {
        let face = wire_face(hit.face);
        let offset = face_offset(face);
        let target = BlockPos::new(
            hit.block.x + offset.x,
            hit.block.y + offset.y,
            hit.block.z + offset.z,
        );
        if player.is_some_and(|player| player.intersects(&block_box(target))) {
            return None;
        }
        if mode.can_place() {
            self.expected_placement = Some(target);
        }
        Some(Outgoing::Place {
            block: hit.block,
            face,
            item,
            cursor: [
                cursor(hit.position.x - f64::from(hit.block.x)),
                cursor(hit.position.y - f64::from(hit.block.y)),
                cursor(hit.position.z - f64::from(hit.block.z)),
            ],
        })
    }

    /// Records a block change of the server and tells what it confirmed.
    pub fn acknowledge_block(&mut self, position: BlockPos) -> Option<BlockAck> {
        if self.expected_break == Some(position) {
            self.expected_break = None;
            return Some(BlockAck::Break);
        }
        if self.expected_placement == Some(position) {
            self.expected_placement = None;
            return Some(BlockAck::Placement);
        }
        None
    }

    /// The acknowledgement to send for a position the server imposed.
    ///
    /// The acknowledgement is a position packet itself, and the server
    /// answers every position packet it does not accept with a resync. A
    /// server that keeps resyncing the same position (for example because the
    /// player stands inside a block) would therefore get an acknowledgement
    /// per resync, and the two would feed each other until the packet limiter
    /// of the server kicks the client. A position is acknowledged only once.
    pub fn acknowledge_position(&mut self, ack: PositionAck) -> Option<Outgoing> {
        let repeated = self.last_acknowledged.is_some_and(|last| {
            (last.x - ack.x).abs() < SAME_POSITION_TOLERANCE
                && (last.y - ack.y).abs() < SAME_POSITION_TOLERANCE
                && (last.z - ack.z).abs() < SAME_POSITION_TOLERANCE
        });
        if repeated {
            return None;
        }
        self.last_acknowledged = Some(ack);
        Some(Outgoing::AckPosition(ack))
    }
}

/// The unit box of a block.
fn block_box(block: BlockPos) -> Aabb {
    let min = DVec3::new(f64::from(block.x), f64::from(block.y), f64::from(block.z));
    Aabb::new(min, min + DVec3::ONE)
}

/// The offset of a face, in blocks.
#[must_use]
pub fn face_offset(face: Facing) -> BlockPos {
    match face {
        Facing::Down => BlockPos::new(0, -1, 0),
        Facing::Up => BlockPos::new(0, 1, 0),
        Facing::North => BlockPos::new(0, 0, -1),
        Facing::South => BlockPos::new(0, 0, 1),
        Facing::West => BlockPos::new(-1, 0, 0),
        Facing::East => BlockPos::new(1, 0, 0),
    }
}

/// The click offset in sixteenths of a block, clamped to `0..=15`.
#[must_use]
pub fn cursor(offset: f64) -> u8 {
    (offset * 16.0).floor().clamp(0.0, 15.0) as u8
}

/// The wire face of a ray-cast face.
#[must_use]
pub fn wire_face(face: Face) -> Facing {
    match face {
        Face::West => Facing::West,
        Face::East => Facing::East,
        Face::Down => Facing::Down,
        Face::Up => Facing::Up,
        Face::North => Facing::North,
        Face::South => Facing::South,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(block: BlockPos, face: Face) -> RayHit {
        RayHit {
            block,
            face,
            position: DVec3::new(
                f64::from(block.x) + 0.5,
                f64::from(block.y) + 1.0,
                f64::from(block.z) + 0.25,
            ),
            distance: 2.0,
        }
    }

    fn stone() -> RayHit {
        hit(BlockPos::new(3, 4, 5), Face::Up)
    }

    fn ack(x: f64) -> PositionAck {
        PositionAck {
            x,
            y: 64.0,
            z: 0.5,
            yaw: 90.0,
            pitch: 10.0,
            on_ground: true,
        }
    }

    #[test]
    fn the_cursor_is_clamped_to_a_block() {
        assert_eq!(cursor(-0.5), 0);
        assert_eq!(cursor(0.5), 8);
        assert_eq!(cursor(1.0), 15);
    }

    #[test]
    fn the_raycast_faces_map_to_the_wire_faces() {
        assert_eq!(wire_face(Face::Up), Facing::Up);
        assert_eq!(wire_face(Face::North), Facing::North);
        assert_eq!(wire_face(Face::West), Facing::West);
    }

    #[test]
    fn the_face_offsets_point_away_from_the_block() {
        assert_eq!(face_offset(Facing::Up), BlockPos::new(0, 1, 0));
        assert_eq!(face_offset(Facing::West), BlockPos::new(-1, 0, 0));
    }

    #[test]
    fn a_survival_break_sends_start_then_stop_after_the_delay() {
        let mut interaction = Interaction::new();
        let start = interaction.start_break(GameMode::Survival, &stone(), 100);
        assert_eq!(
            start,
            Some(Outgoing::Dig {
                status: DiggingStatus::StartDestroyBlock,
                block: BlockPos::new(3, 4, 5),
                face: Facing::Up,
            })
        );
        assert_eq!(interaction.digging_block(), Some(BlockPos::new(3, 4, 5)));
        assert_eq!(interaction.finish_break(100 + BREAK_TICKS - 1), None);
        let stop = interaction.finish_break(100 + BREAK_TICKS);
        assert!(matches!(
            stop,
            Some(Outgoing::Dig {
                status: DiggingStatus::StopDestroyBlock,
                ..
            })
        ));
        assert_eq!(interaction.digging_block(), None);
        assert_eq!(interaction.finish_break(1000), None);
    }

    #[test]
    fn a_creative_break_sends_no_stop_packet() {
        let mut interaction = Interaction::new();
        assert!(interaction
            .start_break(GameMode::Creative, &stone(), 0)
            .is_some());
        assert_eq!(interaction.digging_block(), None);
        assert_eq!(interaction.finish_break(1000), None);
    }

    #[test]
    fn adventure_and_spectator_never_start_a_break() {
        let mut interaction = Interaction::new();
        for mode in [GameMode::Adventure, GameMode::Spectator] {
            assert_eq!(interaction.start_break(mode, &stone(), 0), None);
        }
        assert_eq!(interaction.acknowledge_block(BlockPos::new(3, 4, 5)), None);
    }

    #[test]
    fn the_server_confirms_a_break_once() {
        let mut interaction = Interaction::new();
        interaction.start_break(GameMode::Survival, &stone(), 0);
        let position = BlockPos::new(3, 4, 5);
        assert_eq!(
            interaction.acknowledge_block(position),
            Some(BlockAck::Break)
        );
        assert_eq!(interaction.acknowledge_block(position), None);
    }

    #[test]
    fn a_placement_targets_the_block_next_to_the_face() {
        let mut interaction = Interaction::new();
        let placed = interaction.place_block(GameMode::Survival, &stone(), None, ItemStack::EMPTY);
        assert_eq!(
            placed,
            Some(Outgoing::Place {
                block: BlockPos::new(3, 4, 5),
                face: Facing::Up,
                item: ItemStack::EMPTY,
                cursor: [8, 15, 4],
            })
        );
        assert_eq!(
            interaction.acknowledge_block(BlockPos::new(3, 5, 5)),
            Some(BlockAck::Placement)
        );
    }

    #[test]
    fn a_block_inside_the_player_is_not_placed() {
        let mut interaction = Interaction::new();
        let player = block_box(BlockPos::new(3, 5, 5));
        let placed = interaction.place_block(
            GameMode::Survival,
            &stone(),
            Some(&player),
            ItemStack::EMPTY,
        );
        assert_eq!(placed, None);
        assert_eq!(interaction.acknowledge_block(BlockPos::new(3, 5, 5)), None);
    }

    #[test]
    fn a_spectator_click_is_sent_but_never_awaited() {
        let mut interaction = Interaction::new();
        let placed = interaction.place_block(GameMode::Spectator, &stone(), None, ItemStack::EMPTY);
        assert!(placed.is_some());
        assert_eq!(interaction.acknowledge_block(BlockPos::new(3, 5, 5)), None);
    }

    #[test]
    fn a_position_is_acknowledged_only_once() {
        let mut interaction = Interaction::new();
        assert_eq!(
            interaction.acknowledge_position(ack(1.0)),
            Some(Outgoing::AckPosition(ack(1.0)))
        );
        assert_eq!(interaction.acknowledge_position(ack(1.0)), None);
        assert!(interaction.acknowledge_position(ack(2.0)).is_some());
    }
}
