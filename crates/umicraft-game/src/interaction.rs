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

use crate::digging::{self, Digger};
use crate::gamemode::GameMode;
use crate::raycast::{Face, RayHit};
use crate::shapes::{Aabb, BlockSource};

/// The ticks between two creative breaks and after a survival one
/// (MCP `PlayerControllerMP.blockHitDelay`, lines 224, 292, 324).
const BLOCK_HIT_DELAY: u8 = 5;

/// The ticks the attack button is ignored after a swing in the air
/// (MCP `Minecraft.leftClickCounter`, line 1513).
const MISS_DELAY: u8 = 10;

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
    /// `Animation` (0x0A): the arm swings (MCP `EntityPlayerSP.swingItem`).
    Swing,
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

/// The cracks of the block being dug (MCP `sendBlockBreakProgress`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreakProgress {
    /// The block that cracks.
    pub block: BlockPos,
    /// The crack stage `0..=9`, `-1` when the cracks go away.
    pub stage: i8,
}

/// What the attack button does during one tick.
#[derive(Debug, Clone, Copy)]
pub struct AttackTick<'a, W: BlockSource + ?Sized> {
    /// The gamemode of the player.
    pub mode: GameMode,
    /// Whether the button went down during this tick.
    pub pressed: bool,
    /// Whether the button is down.
    pub held: bool,
    /// The block under the crosshair.
    pub target: Option<RayHit>,
    /// The loaded world.
    pub world: &'a W,
    /// The state that decides how fast the player digs.
    pub digger: Digger<'a>,
}

/// The result of one tick of the attack button.
#[derive(Debug, Default, PartialEq)]
pub struct AttackOutput {
    /// The packets to send, in order.
    pub packets: Vec<Outgoing>,
    /// The blocks the client removes at once, before the server confirms.
    pub broken: Vec<BlockPos>,
    /// The new crack stage of the block being dug, if it changed.
    pub progress: Option<BreakProgress>,
}

/// The block being dug (MCP `PlayerControllerMP.currentBlock` and friends).
#[derive(Debug, Clone)]
struct Digging {
    block: BlockPos,
    /// The item held when the dig started.
    item: Option<ItemStack>,
    /// The accumulated progress, the block breaks at 1.
    damage: f32,
}

/// The interaction state of the local player.
#[derive(Debug, Default)]
pub struct Interaction {
    /// The block that is being dug, `None` when no dig is in progress.
    digging: Option<Digging>,
    /// Ticks before the next block may be dug (`blockHitDelay`).
    hit_delay: u8,
    /// Ticks before the attack button reacts (`leftClickCounter`).
    click_delay: u8,
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

    /// The block that is being dug.
    #[must_use]
    pub fn digging_block(&self) -> Option<BlockPos> {
        self.digging.as_ref().map(|digging| digging.block)
    }

    /// Runs one tick of the attack button: the click, then the held button.
    ///
    /// MCP `Minecraft.runTick` counts `leftClickCounter` down, `clickMouse`
    /// handles a press and `sendClickBlockToController` handles the held
    /// button, which digs with `PlayerControllerMP.onPlayerDamageBlock` or
    /// aborts the dig. Every swing also sends `Animation`
    /// (`EntityPlayerSP.swingItem`).
    pub fn attack<W: BlockSource + ?Sized>(&mut self, tick: &AttackTick<'_, W>) -> AttackOutput {
        let mut run = AttackRun {
            interaction: self,
            tick,
            output: AttackOutput::default(),
        };
        run.interaction.click_delay = run.interaction.click_delay.saturating_sub(1);
        if tick.pressed && run.interaction.click_delay == 0 {
            run.click();
        }
        run.hold();
        run.output
    }

    /// Stops digging, telling the server (MCP
    /// `PlayerControllerMP.resetBlockRemoving`).
    pub fn abort(&mut self) -> AttackOutput {
        let mut output = AttackOutput::default();
        if let Some(digging) = self.digging.take() {
            output.packets.push(Outgoing::Dig {
                status: DiggingStatus::AbortDestroyBlock,
                block: digging.block,
                face: Facing::Down,
            });
            output.progress = Some(BreakProgress {
                block: digging.block,
                stage: -1,
            });
        }
        output
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

/// One tick of the attack button, with the state it reads and writes.
struct AttackRun<'a, 'b, W: BlockSource + ?Sized> {
    interaction: &'a mut Interaction,
    tick: &'a AttackTick<'b, W>,
    output: AttackOutput,
}

impl<W: BlockSource + ?Sized> AttackRun<'_, '_, W> {
    /// The id of a block, air once the client removed it this tick.
    fn block_id(&self, position: BlockPos) -> u16 {
        if self.output.broken.contains(&position) {
            return 0;
        }
        self.tick
            .world
            .block(position.x, position.y, position.z)
            .id()
    }

    fn swing(&mut self) {
        self.output.packets.push(Outgoing::Swing);
    }

    /// MCP `Minecraft.clickMouse`.
    fn click(&mut self) {
        self.swing();
        match self.tick.target {
            Some(hit) if self.block_id(hit.block) != 0 => {
                self.click_block(&hit);
            }
            _ if self.tick.mode != GameMode::Creative => self.interaction.click_delay = MISS_DELAY,
            _ => {}
        }
    }

    /// MCP `Minecraft.sendClickBlockToController`.
    fn hold(&mut self) {
        if !self.tick.held {
            self.interaction.click_delay = 0;
        }
        if self.interaction.click_delay > 0 {
            return;
        }
        match self.tick.target {
            Some(hit) if self.tick.held && self.block_id(hit.block) != 0 => {
                if self.damage_block(&hit) {
                    self.swing();
                }
            }
            _ => self.reset(),
        }
    }

    fn reset(&mut self) {
        let aborted = self.interaction.abort();
        self.output.packets.extend(aborted.packets);
        if aborted.progress.is_some() {
            self.output.progress = aborted.progress;
        }
    }

    /// MCP `PlayerControllerMP.onPlayerDestroyBlock`: the client removes the
    /// block before the server confirms. A creative player with a sword
    /// cannot.
    fn destroy(&mut self, block: BlockPos) {
        let sword = self
            .tick
            .digger
            .held
            .is_some_and(|stack| digging::is_sword(stack.item_id));
        if self.tick.mode == GameMode::Creative && sword {
            return;
        }
        self.output.broken.push(block);
        self.interaction.digging = None;
    }

    fn send_dig(&mut self, status: DiggingStatus, block: BlockPos, face: Facing) {
        self.output.packets.push(Outgoing::Dig {
            status,
            block,
            face,
        });
    }

    /// MCP `PlayerControllerMP.clickBlock`.
    fn click_block(&mut self, hit: &RayHit) -> bool {
        if !self.tick.mode.can_break() {
            return false;
        }
        let face = wire_face(hit.face);
        let block = hit.block;
        if self.tick.mode.instant_break() {
            self.interaction.expected_break = Some(block);
            self.send_dig(DiggingStatus::StartDestroyBlock, block, face);
            self.destroy(block);
            self.interaction.hit_delay = BLOCK_HIT_DELAY;
        } else if !self.is_digging(block) {
            if let Some(current) = self.interaction.digging_block() {
                self.send_dig(DiggingStatus::AbortDestroyBlock, current, face);
            }
            self.interaction.expected_break = Some(block);
            self.send_dig(DiggingStatus::StartDestroyBlock, block, face);
            let id = self.block_id(block);
            if id != 0 && digging::relative_hardness(id, &self.tick.digger) >= 1.0 {
                self.destroy(block);
            } else {
                self.interaction.digging = Some(Digging {
                    block,
                    item: self.tick.digger.held.cloned(),
                    damage: 0.0,
                });
                self.output.progress = Some(BreakProgress { block, stage: -1 });
            }
        }
        true
    }

    /// MCP `PlayerControllerMP.onPlayerDamageBlock`.
    fn damage_block(&mut self, hit: &RayHit) -> bool {
        let block = hit.block;
        if self.interaction.hit_delay > 0 {
            self.interaction.hit_delay -= 1;
            return true;
        }
        if self.tick.mode.instant_break() {
            self.interaction.hit_delay = BLOCK_HIT_DELAY;
            let face = wire_face(hit.face);
            self.interaction.expected_break = Some(block);
            self.send_dig(DiggingStatus::StartDestroyBlock, block, face);
            self.destroy(block);
            return true;
        }
        if !self.is_digging(block) {
            return self.click_block(hit);
        }
        let id = self.block_id(block);
        let per_tick = digging::relative_hardness(id, &self.tick.digger);
        let Some(digging) = self.interaction.digging.as_mut() else {
            return false;
        };
        digging.damage += per_tick;
        let damage = digging.damage;
        if damage >= 1.0 {
            self.send_dig(DiggingStatus::StopDestroyBlock, block, wire_face(hit.face));
            self.destroy(block);
            self.interaction.hit_delay = BLOCK_HIT_DELAY;
        }
        // MCP line 327: `(int) (damage * 10) - 1`, also after the break
        // (the damage is back to 0 then).
        let shown = if damage >= 1.0 { 0.0 } else { damage };
        self.output.progress = Some(BreakProgress {
            block,
            stage: ((shown * 10.0) as i32 - 1).clamp(-1, 9) as i8,
        });
        true
    }

    /// MCP `PlayerControllerMP.isHittingPosition`: the dig is on this block
    /// and the player still holds the item it started with.
    fn is_digging(&self, block: BlockPos) -> bool {
        let Some(digging) = self.interaction.digging.as_ref() else {
            return false;
        };
        let held = self.tick.digger.held;
        let same_item = match (digging.item.as_ref(), held) {
            (None, None) => true,
            (Some(started), Some(now)) => {
                started.item_id == now.item_id
                    && started.nbt == now.nbt
                    && (started.damage == now.damage || digging::is_damageable_tool(now.item_id))
            }
            _ => false,
        };
        digging.block == block && same_item
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

    /// A world with one block id at the target and air elsewhere.
    struct One(BlockPos, u16);

    impl BlockSource for One {
        fn block(&self, x: i32, y: i32, z: i32) -> umicraft_world::BlockState {
            if BlockPos::new(x, y, z) == self.0 {
                umicraft_world::BlockState::new(self.1, 0)
            } else {
                umicraft_world::BlockState::AIR
            }
        }
    }

    const DIRT: u16 = 3;
    const STONE: u16 = 1;
    const TALL_GRASS: u16 = 31;

    fn tick(
        interaction: &mut Interaction,
        mode: GameMode,
        world: &One,
        target: Option<RayHit>,
        pressed: bool,
        held: bool,
    ) -> AttackOutput {
        let digger = Digger {
            on_ground: true,
            ..Digger::default()
        };
        interaction.attack(&AttackTick {
            mode,
            pressed,
            held,
            target,
            world,
            digger,
        })
    }

    fn dig_status(output: &AttackOutput) -> Vec<DiggingStatus> {
        output
            .packets
            .iter()
            .filter_map(|packet| match packet {
                Outgoing::Dig { status, .. } => Some(*status),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_held_click_digs_dirt_in_the_vanilla_time() {
        let world = One(BlockPos::new(3, 4, 5), DIRT);
        let mut interaction = Interaction::new();
        let first = tick(
            &mut interaction,
            GameMode::Survival,
            &world,
            Some(stone()),
            true,
            true,
        );
        assert_eq!(dig_status(&first), [DiggingStatus::StartDestroyBlock]);
        // clickMouse swings, so does the held button.
        assert_eq!(
            first
                .packets
                .iter()
                .filter(|packet| **packet == Outgoing::Swing)
                .count(),
            2
        );
        assert_eq!(interaction.digging_block(), Some(BlockPos::new(3, 4, 5)));

        let mut ticks = 1;
        let last = loop {
            ticks += 1;
            let output = tick(
                &mut interaction,
                GameMode::Survival,
                &world,
                Some(stone()),
                false,
                true,
            );
            assert!(ticks < 40, "the block never broke");
            if !output.broken.is_empty() {
                break output;
            }
        };
        // 1 / (1 / 0.5 / 30) = 15 ticks of progress, the first one in tick 1.
        assert!((15..=16).contains(&ticks), "{ticks} ticks");
        assert_eq!(dig_status(&last), [DiggingStatus::StopDestroyBlock]);
        assert_eq!(last.broken, [BlockPos::new(3, 4, 5)]);
        assert_eq!(interaction.digging_block(), None);
    }

    #[test]
    fn the_cracks_grow_with_the_progress() {
        let world = One(BlockPos::new(3, 4, 5), STONE);
        let mut interaction = Interaction::new();
        let mut stages = Vec::new();
        for index in 0..30 {
            let output = tick(
                &mut interaction,
                GameMode::Survival,
                &world,
                Some(stone()),
                index == 0,
                true,
            );
            stages.extend(output.progress.map(|progress| progress.stage));
        }
        assert_eq!(stages[0], -1, "the dig starts without cracks");
        assert!(stages.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(stages.iter().all(|stage| (-1..=9).contains(stage)));
    }

    #[test]
    fn a_block_that_breaks_at_once_has_no_stop_packet() {
        let world = One(BlockPos::new(3, 4, 5), TALL_GRASS);
        let mut interaction = Interaction::new();
        let output = tick(
            &mut interaction,
            GameMode::Survival,
            &world,
            Some(stone()),
            true,
            true,
        );
        assert_eq!(dig_status(&output), [DiggingStatus::StartDestroyBlock]);
        assert_eq!(output.broken, [BlockPos::new(3, 4, 5)]);
        assert_eq!(interaction.digging_block(), None);
    }

    #[test]
    fn releasing_the_button_aborts_the_dig() {
        let world = One(BlockPos::new(3, 4, 5), STONE);
        let mut interaction = Interaction::new();
        tick(
            &mut interaction,
            GameMode::Survival,
            &world,
            Some(stone()),
            true,
            true,
        );
        let output = tick(
            &mut interaction,
            GameMode::Survival,
            &world,
            Some(stone()),
            false,
            false,
        );
        assert_eq!(dig_status(&output), [DiggingStatus::AbortDestroyBlock]);
        assert_eq!(
            output.progress,
            Some(BreakProgress {
                block: BlockPos::new(3, 4, 5),
                stage: -1
            })
        );
        assert_eq!(interaction.digging_block(), None);
        let again = tick(
            &mut interaction,
            GameMode::Survival,
            &world,
            Some(stone()),
            false,
            false,
        );
        assert!(again.packets.is_empty(), "nothing to abort twice");
    }

    #[test]
    fn looking_at_another_block_aborts_and_starts_again() {
        let world = One(BlockPos::new(3, 4, 5), STONE);
        let mut interaction = Interaction::new();
        tick(
            &mut interaction,
            GameMode::Survival,
            &world,
            Some(stone()),
            true,
            true,
        );
        let other = hit(BlockPos::new(3, 4, 5), Face::North);
        let moved = RayHit {
            block: BlockPos::new(3, 4, 6),
            ..other
        };
        let world = One(BlockPos::new(3, 4, 6), STONE);
        let output = tick(
            &mut interaction,
            GameMode::Survival,
            &world,
            Some(moved),
            false,
            true,
        );
        assert_eq!(
            dig_status(&output),
            [
                DiggingStatus::AbortDestroyBlock,
                DiggingStatus::StartDestroyBlock
            ]
        );
        assert_eq!(interaction.digging_block(), Some(BlockPos::new(3, 4, 6)));
    }

    #[test]
    fn a_creative_player_breaks_a_block_every_six_ticks_without_a_stop_packet() {
        let world = One(BlockPos::new(3, 4, 5), STONE);
        let mut interaction = Interaction::new();
        let mut starts = Vec::new();
        for index in 0..13 {
            let output = tick(
                &mut interaction,
                GameMode::Creative,
                &world,
                Some(stone()),
                index == 0,
                true,
            );
            assert!(!dig_status(&output).contains(&DiggingStatus::StopDestroyBlock));
            if dig_status(&output).contains(&DiggingStatus::StartDestroyBlock) {
                starts.push(index);
                assert_eq!(output.broken.len(), 1);
            }
        }
        assert_eq!(starts, [0, 6, 12]);
        assert_eq!(interaction.digging_block(), None);
    }

    #[test]
    fn a_creative_sword_does_not_break_blocks() {
        let world = One(BlockPos::new(3, 4, 5), STONE);
        let sword = ItemStack {
            item_id: 276,
            stack_size: 1,
            damage: 0,
            nbt: None,
        };
        let mut interaction = Interaction::new();
        let output = interaction.attack(&AttackTick {
            mode: GameMode::Creative,
            pressed: true,
            held: true,
            target: Some(stone()),
            world: &world,
            digger: Digger {
                held: Some(&sword),
                on_ground: true,
                ..Digger::default()
            },
        });
        assert!(output.broken.is_empty());
    }

    #[test]
    fn adventure_and_spectator_never_start_a_dig() {
        let world = One(BlockPos::new(3, 4, 5), STONE);
        for mode in [GameMode::Adventure, GameMode::Spectator] {
            let mut interaction = Interaction::new();
            let output = tick(&mut interaction, mode, &world, Some(stone()), true, true);
            assert!(dig_status(&output).is_empty());
            assert!(output.broken.is_empty());
            assert_eq!(interaction.acknowledge_block(BlockPos::new(3, 4, 5)), None);
        }
    }

    #[test]
    fn a_swing_in_the_air_blocks_the_button_for_ten_ticks() {
        let world = One(BlockPos::new(3, 4, 5), STONE);
        let mut interaction = Interaction::new();
        let miss = tick(
            &mut interaction,
            GameMode::Survival,
            &world,
            None,
            true,
            true,
        );
        assert_eq!(miss.packets, [Outgoing::Swing]);
        for _ in 0..9 {
            let output = tick(
                &mut interaction,
                GameMode::Survival,
                &world,
                Some(stone()),
                true,
                true,
            );
            assert!(output.packets.is_empty());
        }
        let ready = tick(
            &mut interaction,
            GameMode::Survival,
            &world,
            Some(stone()),
            true,
            true,
        );
        assert!(dig_status(&ready).contains(&DiggingStatus::StartDestroyBlock));
    }

    #[test]
    fn changing_the_held_item_restarts_the_dig() {
        let world = One(BlockPos::new(3, 4, 5), STONE);
        let pickaxe = ItemStack {
            item_id: 270,
            stack_size: 1,
            damage: 0,
            nbt: None,
        };
        let mut interaction = Interaction::new();
        tick(
            &mut interaction,
            GameMode::Survival,
            &world,
            Some(stone()),
            true,
            true,
        );
        let output = interaction.attack(&AttackTick {
            mode: GameMode::Survival,
            pressed: false,
            held: true,
            target: Some(stone()),
            world: &world,
            digger: Digger {
                held: Some(&pickaxe),
                on_ground: true,
                ..Digger::default()
            },
        });
        assert_eq!(
            dig_status(&output),
            [
                DiggingStatus::AbortDestroyBlock,
                DiggingStatus::StartDestroyBlock
            ]
        );
    }

    #[test]
    fn the_server_confirms_a_break_once() {
        let world = One(BlockPos::new(3, 4, 5), STONE);
        let mut interaction = Interaction::new();
        tick(
            &mut interaction,
            GameMode::Survival,
            &world,
            Some(stone()),
            true,
            true,
        );
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
