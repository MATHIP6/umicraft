// SPDX-License-Identifier: MIT OR Apache-2.0

//! Which blocks a piston moves or breaks: a port of
//! `BlockPistonStructureHelper`.
//!
//! The walk follows the reference exactly, including the order of the two
//! lists (the animation and the push to the player depend on it): at most 12
//! blocks move, slime blocks drag the blocks around them, and a destroyable
//! block (a flower) ends the chain and is broken instead of pushed.

use umicraft_world::{BlockPos, BlockState, World};

use super::mobility::{self, SLIME};

/// A piston can push at most this many blocks.
const PUSH_LIMIT: usize = 12;

const DIRECTIONS: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

/// Read access to the blocks of a world.
pub trait BlockLookup {
    /// The block at a position; unloaded cells read as air.
    fn block_at(&self, pos: [i32; 3]) -> BlockState;
}

impl BlockLookup for World {
    fn block_at(&self, pos: [i32; 3]) -> BlockState {
        self.block(BlockPos::new(pos[0], pos[1], pos[2]))
    }
}

/// The result of a legal piston move.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PistonMove {
    /// Blocks pushed one cell along the move direction, in reference order.
    pub to_move: Vec<[i32; 3]>,
    /// Blocks broken by the move, in reference order.
    pub to_destroy: Vec<[i32; 3]>,
}

/// `BlockPistonStructureHelper.canMove` and the two lists.
///
/// `pos` is the piston base, `direction` the unit vector it faces, and
/// `extending` whether it is extending (retracting looks at the block two
/// cells ahead and moves it back). `None` means the piston cannot move.
#[must_use]
pub fn plan_move<W: BlockLookup>(
    world: &W,
    pos: [i32; 3],
    direction: [i32; 3],
    extending: bool,
) -> Option<PistonMove> {
    let (move_direction, block_to_move) = if extending {
        (direction, offset(pos, direction, 1))
    } else {
        (negate(direction), offset(pos, direction, 2))
    };
    let mut walk = Walk {
        world,
        piston: pos,
        direction: move_direction,
        result: PistonMove::default(),
    };
    walk.can_move(block_to_move).then_some(walk.result)
}

struct Walk<'a, W> {
    world: &'a W,
    piston: [i32; 3],
    direction: [i32; 3],
    result: PistonMove,
}

impl<W: BlockLookup> Walk<'_, W> {
    fn state(&self, pos: [i32; 3]) -> BlockState {
        self.world.block_at(pos)
    }

    fn can_push(&self, pos: [i32; 3], allow_destroy: bool) -> bool {
        mobility::can_push(self.state(pos), pos, self.direction, allow_destroy)
    }

    fn is_slime(&self, pos: [i32; 3]) -> bool {
        self.state(pos).id() == SLIME
    }

    fn can_move(&mut self, start: [i32; 3]) -> bool {
        if !self.can_push(start, false) {
            if mobility::mobility(self.state(start).id()) != mobility::Mobility::Destroy {
                return false;
            }
            self.result.to_destroy.push(start);
            return true;
        }
        if !self.add_chain(start) {
            return false;
        }
        (0..self.result.to_move.len()).all(|i| {
            let pos = self.result.to_move[i];
            !self.is_slime(pos) || self.add_slime_sides(pos)
        })
    }

    /// `func_177251_a`: adds `origin`, the slime behind it, and the chain in
    /// front of it. `false` means the structure is stuck.
    fn add_chain(&mut self, origin: [i32; 3]) -> bool {
        let state = self.state(origin);
        if state.is_air()
            || !self.can_push(origin, false)
            || origin == self.piston
            || self.result.to_move.contains(&origin)
        {
            return true;
        }
        let Some(behind) = self.slime_run_behind(origin) else {
            return false;
        };
        for back in (0..behind).rev() {
            self.result
                .to_move
                .push(offset(origin, self.direction, -(back as i32)));
        }
        self.walk_ahead(origin, behind)
    }

    /// Counts `origin` and the slime blocks pulling on it from behind (the
    /// reference's `i`); `None` when the 12 block limit is passed.
    fn slime_run_behind(&self, origin: [i32; 3]) -> Option<usize> {
        let mut count = 1;
        if count + self.result.to_move.len() > PUSH_LIMIT {
            return None;
        }
        let mut id = self.state(origin).id();
        while id == SLIME {
            let pos = offset(origin, self.direction, -(count as i32));
            let state = self.state(pos);
            if state.is_air() || !self.can_push(pos, false) || pos == self.piston {
                break;
            }
            id = state.id();
            count += 1;
            if count + self.result.to_move.len() > PUSH_LIMIT {
                return None;
            }
        }
        Some(count)
    }

    /// The `while (true)` loop of the reference: follows the blocks in front.
    fn walk_ahead(&mut self, origin: [i32; 3], added: usize) -> bool {
        let mut added = added;
        let mut distance = 1;
        loop {
            let pos = offset(origin, self.direction, distance);
            if let Some(index) = self.result.to_move.iter().position(|p| *p == pos) {
                self.reorder(added, index);
                return (0..=index + added).all(|i| {
                    let pos = self.result.to_move[i];
                    !self.is_slime(pos) || self.add_slime_sides(pos)
                });
            }
            let state = self.state(pos);
            if state.is_air() {
                return true;
            }
            if !self.can_push(pos, true) || pos == self.piston {
                return false;
            }
            if mobility::mobility(state.id()) == mobility::Mobility::Destroy {
                self.result.to_destroy.push(pos);
                return true;
            }
            if self.result.to_move.len() >= PUSH_LIMIT {
                return false;
            }
            self.result.to_move.push(pos);
            added += 1;
            distance += 1;
        }
    }

    /// `func_177255_a`: moves the `added` newest entries in front of the
    /// entries from `index` on, so a loop through slime is ordered front to
    /// back.
    fn reorder(&mut self, added: usize, index: usize) {
        self.result.to_move[index..].rotate_right(added);
    }

    /// `func_177250_b`: a slime block drags the blocks on its four sides.
    fn add_slime_sides(&mut self, pos: [i32; 3]) -> bool {
        let direction = self.direction;
        DIRECTIONS
            .iter()
            .filter(|side| !same_axis(**side, direction))
            .all(|side| self.add_chain(offset(pos, *side, 1)))
    }
}

fn offset(pos: [i32; 3], direction: [i32; 3], distance: i32) -> [i32; 3] {
    [0, 1, 2].map(|axis| pos[axis] + direction[axis] * distance)
}

fn negate(direction: [i32; 3]) -> [i32; 3] {
    direction.map(|axis| -axis)
}

fn same_axis(a: [i32; 3], b: [i32; 3]) -> bool {
    (0..3).any(|axis| a[axis] != 0 && b[axis] != 0)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct Grid(HashMap<[i32; 3], BlockState>);

    impl Grid {
        fn with(mut self, pos: [i32; 3], id: u16) -> Self {
            self.0.insert(pos, BlockState::new(id, 0));
            self
        }

        fn row(mut self, from_x: i32, count: i32, id: u16) -> Self {
            for x in from_x..from_x + count {
                self.0.insert([x, 64, 0], BlockState::new(id, 0));
            }
            self
        }
    }

    impl BlockLookup for Grid {
        fn block_at(&self, pos: [i32; 3]) -> BlockState {
            self.0.get(&pos).copied().unwrap_or(BlockState::AIR)
        }
    }

    const EAST: [i32; 3] = [1, 0, 0];
    const PISTON_AT: [i32; 3] = [0, 64, 0];

    fn piston_grid() -> Grid {
        Grid::default().with(PISTON_AT, 33)
    }

    #[test]
    fn pushes_a_single_block() {
        let grid = piston_grid().with([1, 64, 0], 1);
        let plan = plan_move(&grid, PISTON_AT, EAST, true).unwrap();
        assert_eq!(plan.to_move, vec![[1, 64, 0]]);
        assert!(plan.to_destroy.is_empty());
    }

    #[test]
    fn an_empty_cell_moves_nothing() {
        let plan = plan_move(&piston_grid(), PISTON_AT, EAST, true).unwrap();
        assert_eq!(plan, PistonMove::default());
    }

    #[test]
    fn twelve_blocks_move_and_thirteen_do_not() {
        let twelve = piston_grid().row(1, 12, 1);
        let plan = plan_move(&twelve, PISTON_AT, EAST, true).unwrap();
        assert_eq!(plan.to_move.len(), 12);
        assert_eq!(plan.to_move[0], [1, 64, 0]);
        assert_eq!(plan.to_move[11], [12, 64, 0]);
        let thirteen = piston_grid().row(1, 13, 1);
        assert_eq!(plan_move(&thirteen, PISTON_AT, EAST, true), None);
    }

    #[test]
    fn obsidian_and_bedrock_block_the_push() {
        for id in [49, 7] {
            let first = piston_grid().with([1, 64, 0], id);
            assert_eq!(plan_move(&first, PISTON_AT, EAST, true), None);
            let chained = piston_grid().row(1, 2, 1).with([3, 64, 0], id);
            assert_eq!(plan_move(&chained, PISTON_AT, EAST, true), None);
        }
    }

    #[test]
    fn a_flower_is_destroyed_not_pushed() {
        let at_end = piston_grid().row(1, 2, 1).with([3, 64, 0], 38);
        let plan = plan_move(&at_end, PISTON_AT, EAST, true).unwrap();
        assert_eq!(plan.to_move, vec![[1, 64, 0], [2, 64, 0]]);
        assert_eq!(plan.to_destroy, vec![[3, 64, 0]]);
        let first = piston_grid().with([1, 64, 0], 38);
        let plan = plan_move(&first, PISTON_AT, EAST, true).unwrap();
        assert!(plan.to_move.is_empty());
        assert_eq!(plan.to_destroy, vec![[1, 64, 0]]);
    }

    #[test]
    fn a_chest_cannot_be_pushed() {
        let grid = piston_grid().with([1, 64, 0], 54);
        assert_eq!(plan_move(&grid, PISTON_AT, EAST, true), None);
    }

    #[test]
    fn an_extended_piston_is_not_pushable() {
        let mut grid = piston_grid();
        grid.0.insert([1, 64, 0], BlockState::new(33, 8));
        assert_eq!(plan_move(&grid, PISTON_AT, EAST, true), None);
        grid.0.insert([1, 64, 0], BlockState::new(33, 0));
        assert!(plan_move(&grid, PISTON_AT, EAST, true).is_some());
    }

    #[test]
    fn a_sticky_piston_retracting_pulls_the_block_two_ahead() {
        // `doMove` removes the head before it builds the helper, so cell 1 is air.
        let grid = Grid::default().with(PISTON_AT, 29).with([2, 64, 0], 1);
        let plan = plan_move(&grid, PISTON_AT, EAST, false).unwrap();
        assert_eq!(plan.to_move, vec![[2, 64, 0]]);
        let bare = Grid::default().with(PISTON_AT, 29);
        assert_eq!(
            plan_move(&bare, PISTON_AT, EAST, false),
            Some(PistonMove::default())
        );
    }

    #[test]
    fn retraction_never_pulls_a_flower() {
        let grid = Grid::default().with(PISTON_AT, 29).with([2, 64, 0], 38);
        let plan = plan_move(&grid, PISTON_AT, EAST, false).unwrap();
        assert_eq!(plan.to_destroy, vec![[2, 64, 0]]);
    }

    #[test]
    fn the_world_height_stops_vertical_pushes() {
        let up = Grid::default().with([0, 254, 0], 33).with([0, 255, 0], 1);
        assert_eq!(plan_move(&up, [0, 254, 0], [0, 1, 0], true), None);
        let down = Grid::default().with([0, 1, 0], 33).with([0, 0, 0], 1);
        assert_eq!(plan_move(&down, [0, 1, 0], [0, -1, 0], true), None);
        let ok = Grid::default().with([0, 253, 0], 33).with([0, 254, 0], 1);
        assert!(plan_move(&ok, [0, 253, 0], [0, 1, 0], true).is_some());
    }

    #[test]
    fn a_block_beyond_the_world_edge_blocks_the_chain() {
        let grid = Grid::default()
            .with([0, 253, 0], 33)
            .with([0, 254, 0], 1)
            .with([0, 255, 0], 1);
        assert_eq!(plan_move(&grid, [0, 253, 0], [0, 1, 0], true), None);
    }

    #[test]
    fn slime_drags_the_blocks_around_it() {
        let grid = piston_grid()
            .with([1, 64, 0], 165)
            .with([1, 65, 0], 1)
            .with([1, 64, 1], 1);
        let plan = plan_move(&grid, PISTON_AT, EAST, true).unwrap();
        assert_eq!(plan.to_move.len(), 3);
        assert_eq!(plan.to_move[0], [1, 64, 0]);
        assert!(plan.to_move.contains(&[1, 65, 0]) && plan.to_move.contains(&[1, 64, 1]));
    }

    #[test]
    fn works_on_a_real_world() {
        let world = World::new(0);
        assert_eq!(
            plan_move(&world, [0, 64, 0], EAST, true),
            Some(PistonMove::default())
        );
    }
}
