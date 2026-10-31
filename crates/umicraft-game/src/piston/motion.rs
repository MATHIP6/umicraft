// SPDX-License-Identifier: MIT OR Apache-2.0

//! Starting and advancing piston motion on the client.
//!
//! Source: MCP `BlockPistonBase.onBlockEventReceived` and `doMove` (the
//! client runs the same code as the server for a `Block Action`),
//! `BlockPistonMoving` and `TileEntityPiston.update`.
//!
//! The server sends `Block Action` before the block changes of the same
//! tick, so the world still holds the state from before the move, which is
//! what the structure helper needs. [`start`] then does what the reference
//! does to the client world: the moved cells become air and their
//! destinations `piston_extension` blocks, each with a [`MovingBlock`]
//! holding the real block. The final blocks come from the server's own
//! `Block Change` packets; [`tick`] only falls back to the stored state when
//! a cell still holds `piston_extension` at the end of its animation.

use umicraft_protocol::packets::play::clientbound::block_action::BlockAction;
use umicraft_world::{BlockPos, BlockState, ChunkPos, World};

use super::mobility::can_push;
use super::{plan_move, MovingBlock, MovingBlocks, PistonMove};

/// Block id of the piston.
const PISTON: u16 = 33;
/// Block id of the sticky piston.
const STICKY_PISTON: u16 = 29;
/// Block id of the piston head.
const HEAD: u16 = 34;
/// Block id of `piston_extension`, the placeholder cell of a moving block.
pub(super) const EXTENSION: u16 = 36;

/// `Block Action` event: the piston extends.
const EVENT_EXTEND: u8 = 0;
/// `Block Action` event: the piston retracts.
const EVENT_RETRACT: u8 = 1;

/// Metadata bit: extended (piston) or sticky (head, extension).
const FLAG_BIT: u8 = 8;
/// Animation progress added per tick (`TileEntityPiston.update`).
const PROGRESS_STEP: f32 = 0.5;
/// Extra distance an extending block pushes, so the entity stays ahead of
/// the face (`TileEntityPiston.update`).
const PUSH_MARGIN: f32 = 0.0625;
/// Distance moved on the tick an animation ends.
const FINAL_PUSH: f32 = 0.25;

/// The facing indices of `EnumFacing`: down, up, north, south, west, east.
const FACINGS: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

/// Collects the chunks that must be re-meshed while the world is edited.
struct Edits<'a> {
    world: &'a mut World,
    dirty: Vec<ChunkPos>,
}

impl Edits<'_> {
    fn set(&mut self, pos: [i32; 3], state: BlockState) {
        let [x, y, z] = pos;
        self.dirty
            .extend(self.world.apply_block_change(BlockPos::new(x, y, z), state));
    }

    fn block(&self, pos: [i32; 3]) -> BlockState {
        let [x, y, z] = pos;
        self.world.block(BlockPos::new(x, y, z))
    }
}

fn offset(pos: [i32; 3], direction: [i32; 3], distance: i32) -> [i32; 3] {
    [0, 1, 2].map(|axis| pos[axis] + direction[axis] * distance)
}

fn opposite(direction: [i32; 3]) -> [i32; 3] {
    direction.map(|axis| -axis)
}

/// The `piston_extension` block for a given facing; sticky is metadata bit 3.
fn extension_state(facing: u8, sticky: bool) -> BlockState {
    let sticky_bit = if sticky { FLAG_BIT } else { 0 };
    BlockState::new(EXTENSION, facing | sticky_bit)
}

/// Starts the motion a piston `Block Action` announces.
///
/// Packets for other blocks, other events or a facing out of range are
/// ignored. `packet.instrument` is the event (0 extend, 1 retract) and
/// `packet.pitch` the facing metadata. Returns the chunks whose blocks
/// changed, to be meshed again.
pub fn start(world: &mut World, moving: &mut MovingBlocks, packet: &BlockAction) -> Vec<ChunkPos> {
    let id = match u16::try_from(packet.block_id) {
        Ok(id @ (PISTON | STICKY_PISTON)) => id,
        _ => return Vec::new(),
    };
    let facing = packet.pitch & 7;
    let Some(&direction) = FACINGS.get(usize::from(facing)) else {
        return Vec::new();
    };
    let mut piston = Piston {
        edits: Edits {
            world,
            dirty: Vec::new(),
        },
        moving,
        pos: [packet.x, packet.y, packet.z],
        id,
        facing,
        direction,
    };
    match packet.instrument {
        EVENT_EXTEND => piston.extend(),
        EVENT_RETRACT => piston.retract(),
        _ => {}
    }
    piston.edits.dirty
}

/// One piston while its event is handled.
struct Piston<'a> {
    edits: Edits<'a>,
    moving: &'a mut MovingBlocks,
    pos: [i32; 3],
    id: u16,
    facing: u8,
    direction: [i32; 3],
}

impl Piston<'_> {
    fn sticky(&self) -> bool {
        self.id == STICKY_PISTON
    }

    /// `onBlockEventReceived`, event 0.
    fn extend(&mut self) {
        let Some(plan) = plan_move(self.edits.world, self.pos, self.direction, true) else {
            return;
        };
        self.apply_plan(&plan, true);
        self.edits
            .set(self.pos, BlockState::new(self.id, self.facing | FLAG_BIT));
    }

    /// `onBlockEventReceived`, event 1.
    fn retract(&mut self) {
        let head = offset(self.pos, self.direction, 1);
        self.finish_at(head);
        self.edits
            .set(self.pos, extension_state(self.facing, self.sticky()));
        self.moving.push(MovingBlock {
            state: BlockState::new(self.id, self.facing),
            show_head: true,
            ..self.start_of_animation(self.pos)
        });
        if self.sticky() {
            self.pull();
        } else {
            self.edits.set(head, BlockState::AIR);
        }
    }

    /// The sticky half of a retraction: the block behind the head follows
    /// the piston, unless it is a block still sliding out, which just stops.
    fn pull(&mut self) {
        let behind = offset(self.pos, self.direction, 2);
        let still_sliding_out = self.edits.block(behind).id() == EXTENSION
            && self
                .moving
                .at(behind)
                .is_some_and(|block| block.direction == self.direction && block.extending);
        if still_sliding_out {
            self.finish_at(behind);
            return;
        }
        let state = self.edits.block(behind);
        if !state.is_air() && can_push(state, behind, opposite(self.direction), false) {
            self.retract_blocks();
        }
    }

    /// `doMove(extending = false)`: the head cell empties, then the block
    /// behind it, with whatever it drags along, moves one cell back.
    fn retract_blocks(&mut self) {
        self.edits
            .set(offset(self.pos, self.direction, 1), BlockState::AIR);
        if let Some(plan) = plan_move(self.edits.world, self.pos, self.direction, false) {
            self.apply_plan(&plan, false);
        }
    }

    /// The world edits of `doMove` once the plan is known good: broken
    /// blocks vanish, moved blocks leave a `piston_extension` cell one step
    /// on, and an extension also grows the head.
    fn apply_plan(&mut self, plan: &PistonMove, extending: bool) {
        let step = if extending {
            self.direction
        } else {
            opposite(self.direction)
        };
        for &cell in plan.to_destroy.iter().rev() {
            self.edits.set(cell, BlockState::AIR);
        }
        for &cell in plan.to_move.iter().rev() {
            let state = self.edits.block(cell);
            self.edits.set(cell, BlockState::AIR);
            self.place(
                offset(cell, step, 1),
                extension_state(self.facing, false),
                state,
                extending,
                false,
            );
        }
        if extending {
            let sticky_bit = if self.sticky() { FLAG_BIT } else { 0 };
            self.place(
                offset(self.pos, self.direction, 1),
                extension_state(self.facing, self.sticky()),
                BlockState::new(HEAD, self.facing | sticky_bit),
                true,
                false,
            );
        }
    }

    /// Puts a `piston_extension` cell and the block that moves into it.
    fn place(
        &mut self,
        pos: [i32; 3],
        placeholder: BlockState,
        state: BlockState,
        extending: bool,
        show_head: bool,
    ) {
        let block = MovingBlock {
            state,
            extending,
            show_head,
            ..self.start_of_animation(pos)
        };
        self.edits.set(pos, placeholder);
        self.moving.push(block);
    }

    /// A block at the start of its animation in `pos`, with the light the
    /// cell has now; the caller fills in the rest.
    fn start_of_animation(&self, pos: [i32; 3]) -> MovingBlock {
        let [x, y, z] = pos;
        MovingBlock {
            pos,
            state: BlockState::AIR,
            direction: self.direction,
            extending: false,
            show_head: false,
            progress: 0.0,
            last_progress: 0.0,
            light: self.edits.world.light(BlockPos::new(x, y, z)),
        }
    }

    /// `TileEntityPiston.clearPistonTileEntity`: ends an animation at once.
    fn finish_at(&mut self, pos: [i32; 3]) {
        if let Some(block) = self.moving.remove_at(pos) {
            finish(&mut self.edits, &block);
        }
    }
}

/// Puts the stored block where the animation ended, if the cell still holds
/// the placeholder.
fn finish(edits: &mut Edits<'_>, block: &MovingBlock) {
    if edits.block(block.pos).id() == EXTENSION {
        edits.set(block.pos, block.state);
    }
}

/// Advances every moving block by one tick (`TileEntityPiston.update`).
///
/// `launch` is called for every block that pushes entities this tick with
/// the world (still before the block finishes), the block, and the
/// `progress` and `distance` arguments of `launchWithSlimeBlock`; the caller
/// turns them into a [`super::push::Push`] for the entities it owns. Blocks
/// whose animation ended on the previous tick are removed and put into the
/// world. Returns the chunks that changed.
pub fn tick(
    world: &mut World,
    moving: &mut MovingBlocks,
    mut launch: impl FnMut(&World, &MovingBlock, f32, f32),
) -> Vec<ChunkPos> {
    let mut edits = Edits {
        world,
        dirty: Vec::new(),
    };
    for block in moving.blocks_mut().iter_mut() {
        block.last_progress = block.progress;
        if block.last_progress >= 1.0 {
            launch(edits.world, block, 1.0, FINAL_PUSH);
            finish(&mut edits, block);
            continue;
        }
        block.progress = (block.progress + PROGRESS_STEP).min(1.0);
        if block.extending {
            let distance = block.progress - block.last_progress + PUSH_MARGIN;
            launch(edits.world, block, block.progress, distance);
        }
    }
    moving
        .blocks_mut()
        .retain(|block| block.last_progress < 1.0);
    edits.dirty
}

#[cfg(test)]
mod tests {
    use glam::DVec3;
    use umicraft_world::{ChunkPayload, LightLevel};

    use super::*;
    use crate::physics::Player;
    use crate::piston::push;

    const STONE: u16 = 1;
    const EAST: u8 = 5;

    /// One loaded column of air with full sky light.
    fn world() -> World {
        let mut data = vec![0u8; 8192 + 2048 + 2048 + 256];
        data[8192 + 2048..8192 + 4096].fill(0xFF);
        let mut world = World::new(0);
        world
            .apply_chunk(
                ChunkPos::new(0, 0),
                ChunkPayload {
                    primary_bit_mask: 1,
                    ground_up: true,
                    data: &data,
                },
            )
            .expect("a well-formed column");
        world
    }

    fn set(world: &mut World, pos: [i32; 3], id: u16, meta: u8) {
        world.set_block(
            BlockPos::new(pos[0], pos[1], pos[2]),
            BlockState::new(id, meta),
        );
    }

    fn at(world: &World, pos: [i32; 3]) -> BlockState {
        world.block(BlockPos::new(pos[0], pos[1], pos[2]))
    }

    fn action(id: i32, event: u8, facing: u8) -> BlockAction {
        BlockAction {
            x: 5,
            y: 1,
            z: 5,
            instrument: event,
            pitch: facing,
            block_id: id,
        }
    }

    /// A piston at (5,1,5) facing east with a stone in front of it.
    fn pushing_world(piston: u16) -> World {
        let mut world = world();
        set(&mut world, [5, 1, 5], piston, EAST);
        set(&mut world, [6, 1, 5], STONE, 0);
        world
    }

    #[test]
    fn extending_moves_the_stone_and_grows_the_head() {
        let mut world = pushing_world(PISTON);
        let mut moving = MovingBlocks::default();
        let dirty = start(&mut world, &mut moving, &action(33, 0, EAST));
        assert!(!dirty.is_empty());
        assert_eq!(at(&world, [5, 1, 5]), BlockState::new(PISTON, EAST | 8));
        assert_eq!(at(&world, [6, 1, 5]), BlockState::new(36, EAST));
        assert_eq!(at(&world, [7, 1, 5]), BlockState::new(36, EAST));
        let stone = moving.at([7, 1, 5]).expect("the stone moves");
        assert_eq!(stone.state, BlockState::new(STONE, 0));
        assert!(stone.extending && !stone.show_head);
        assert_eq!(stone.direction, [1, 0, 0]);
        assert_eq!(stone.light, LightLevel::FULL_SKY);
        let head = moving.at([6, 1, 5]).expect("the head grows");
        assert_eq!(head.state, BlockState::new(34, EAST));
    }

    #[test]
    fn a_sticky_head_is_a_sticky_extension() {
        let mut world = pushing_world(STICKY_PISTON);
        let mut moving = MovingBlocks::default();
        start(&mut world, &mut moving, &action(29, 0, EAST));
        assert_eq!(at(&world, [6, 1, 5]), BlockState::new(36, EAST | 8));
        let head = moving.at([6, 1, 5]).expect("the head grows");
        assert_eq!(head.state, BlockState::new(34, EAST | 8));
    }

    #[test]
    fn a_blocked_piston_changes_nothing() {
        let mut world = pushing_world(PISTON);
        set(&mut world, [7, 1, 5], 7, 0);
        let mut moving = MovingBlocks::default();
        let dirty = start(&mut world, &mut moving, &action(33, 0, EAST));
        assert!(dirty.is_empty() && moving.is_empty());
        assert_eq!(at(&world, [5, 1, 5]), BlockState::new(PISTON, EAST));
        assert_eq!(at(&world, [6, 1, 5]), BlockState::new(STONE, 0));
    }

    #[test]
    fn other_blocks_and_events_are_ignored() {
        let mut world = pushing_world(PISTON);
        let mut moving = MovingBlocks::default();
        assert!(start(&mut world, &mut moving, &action(54, 0, EAST)).is_empty());
        assert!(start(&mut world, &mut moving, &action(33, 2, EAST)).is_empty());
        assert!(start(&mut world, &mut moving, &action(33, 0, 7)).is_empty());
        assert!(moving.is_empty());
    }

    #[test]
    fn progress_runs_in_two_ticks_and_the_third_removes_the_block() {
        let mut world = pushing_world(PISTON);
        let mut moving = MovingBlocks::default();
        start(&mut world, &mut moving, &action(33, 0, EAST));
        let mut launches = Vec::new();
        let mut run = |world: &mut World, moving: &mut MovingBlocks| {
            tick(world, moving, |_, block, progress, distance| {
                launches.push((block.pos, progress, distance));
            });
        };
        run(&mut world, &mut moving);
        let stone = moving.at([7, 1, 5]).expect("still moving");
        assert_eq!((stone.last_progress, stone.progress), (0.0, 0.5));
        run(&mut world, &mut moving);
        let stone = moving.at([7, 1, 5]).expect("still moving");
        assert_eq!((stone.last_progress, stone.progress), (0.5, 1.0));
        // The cells still hold the placeholder until the animation is over.
        assert_eq!(at(&world, [7, 1, 5]).id(), 36);
        run(&mut world, &mut moving);
        assert!(moving.is_empty());
        // The server's own block change normally lands first; this is the
        // fallback of the reference.
        assert_eq!(at(&world, [7, 1, 5]), BlockState::new(STONE, 0));
        assert_eq!(at(&world, [6, 1, 5]), BlockState::new(34, EAST));
        assert!(launches.contains(&([7, 1, 5], 0.5, 0.5625)));
        assert!(launches.contains(&([7, 1, 5], 1.0, 0.5625)));
        assert!(launches.contains(&([7, 1, 5], 1.0, 0.25)));
    }

    #[test]
    fn the_server_block_change_wins_over_the_fallback() {
        let mut world = pushing_world(PISTON);
        let mut moving = MovingBlocks::default();
        start(&mut world, &mut moving, &action(33, 0, EAST));
        set(&mut world, [7, 1, 5], 4, 0);
        for _ in 0..3 {
            tick(&mut world, &mut moving, |_, _, _, _| {});
        }
        assert_eq!(at(&world, [7, 1, 5]), BlockState::new(4, 0));
    }

    #[test]
    fn a_normal_piston_retracting_drops_its_head() {
        let mut world = world();
        set(&mut world, [5, 1, 5], PISTON, EAST | 8);
        set(&mut world, [6, 1, 5], HEAD, EAST);
        let mut moving = MovingBlocks::default();
        start(&mut world, &mut moving, &action(33, 1, EAST));
        assert_eq!(at(&world, [5, 1, 5]), BlockState::new(36, EAST));
        assert!(at(&world, [6, 1, 5]).is_air());
        let base = moving.at([5, 1, 5]).expect("the base slides back");
        assert_eq!(base.state, BlockState::new(PISTON, EAST));
        assert!(!base.extending && base.show_head);
    }

    #[test]
    fn a_sticky_piston_pulls_the_block_behind_its_head() {
        let mut world = world();
        set(&mut world, [5, 1, 5], STICKY_PISTON, EAST | 8);
        set(&mut world, [6, 1, 5], HEAD, EAST | 8);
        set(&mut world, [7, 1, 5], STONE, 0);
        let mut moving = MovingBlocks::default();
        start(&mut world, &mut moving, &action(29, 1, EAST));
        assert!(at(&world, [7, 1, 5]).is_air());
        assert_eq!(at(&world, [6, 1, 5]).id(), 36);
        let stone = moving.at([6, 1, 5]).expect("the stone is pulled");
        assert_eq!(stone.state, BlockState::new(STONE, 0));
        assert!(!stone.extending);
    }

    #[test]
    fn a_sticky_piston_does_not_pull_a_flower() {
        let mut world = world();
        set(&mut world, [5, 1, 5], STICKY_PISTON, EAST | 8);
        set(&mut world, [6, 1, 5], HEAD, EAST | 8);
        set(&mut world, [7, 1, 5], 38, 0);
        let mut moving = MovingBlocks::default();
        start(&mut world, &mut moving, &action(29, 1, EAST));
        assert_eq!(at(&world, [7, 1, 5]).id(), 38);
        assert_eq!(moving.blocks().len(), 1);
    }

    /// Runs one tick the way the client does: the launches push the player.
    fn tick_with_player(world: &mut World, moving: &mut MovingBlocks, player: &mut Player) {
        tick(world, moving, |world, block, progress, distance| {
            if let Some(found) =
                push::launch(world, block, progress, distance, &player.bounding_box())
            {
                push::apply(player, found, world);
            }
        });
    }

    #[test]
    fn the_player_in_the_swept_box_is_pushed_on_both_ticks() {
        let mut world = pushing_world(PISTON);
        let mut moving = MovingBlocks::default();
        start(&mut world, &mut moving, &action(33, 0, EAST));
        let mut player = Player::new(DVec3::new(7.25, 1.0, 5.5));
        tick_with_player(&mut world, &mut moving, &mut player);
        assert!((player.position.x - 7.8125).abs() < 1e-9);
        tick_with_player(&mut world, &mut moving, &mut player);
        assert!((player.position.x - 8.375).abs() < 1e-9);
    }

    #[test]
    fn a_player_behind_a_wall_is_not_pushed_through_it() {
        let mut world = pushing_world(PISTON);
        set(&mut world, [8, 1, 5], 4, 0);
        let mut moving = MovingBlocks::default();
        start(&mut world, &mut moving, &action(33, 0, EAST));
        let mut player = Player::new(DVec3::new(7.25, 1.0, 5.5));
        for _ in 0..3 {
            tick_with_player(&mut world, &mut moving, &mut player);
        }
        // The wall starts at x = 8 and the player is 0.3 to each side.
        assert!(player.position.x <= 7.7 + 1e-9);
    }

    #[test]
    fn a_retracting_block_does_not_push_while_it_moves() {
        let mut world = world();
        set(&mut world, [5, 1, 5], STICKY_PISTON, EAST | 8);
        set(&mut world, [6, 1, 5], HEAD, EAST | 8);
        set(&mut world, [7, 1, 5], STONE, 0);
        let mut moving = MovingBlocks::default();
        start(&mut world, &mut moving, &action(29, 1, EAST));
        let mut player = Player::new(DVec3::new(6.5, 1.0, 5.5));
        tick_with_player(&mut world, &mut moving, &mut player);
        tick_with_player(&mut world, &mut moving, &mut player);
        assert_eq!(player.position.x, 6.5);
    }
}
