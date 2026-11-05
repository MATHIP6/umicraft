// SPDX-License-Identifier: MIT OR Apache-2.0

//! Local light updates after a block change.
//!
//! A 1.8 server sends light only with chunk data; when a block changes, the
//! client recomputes the light around it itself (`World.checkLightFor`).
//! Without that, breaking a roof leaves its shadow in place and placing a
//! block leaves the cell it fills lit.
//!
//! The update is the classic two-phase flood fill, run once per channel:
//! first the light that flowed through the changed cell is removed, then the
//! surviving neighbours and any emitter re-spread it. Work stays proportional
//! to the cells whose light actually changes (at most 15 blocks away, or the
//! length of a freshly opened or closed sky shaft).

use std::collections::{HashSet, VecDeque};

use crate::block::BlockState;
use crate::chunk::WORLD_HEIGHT;
use crate::pos::{BlockPos, ChunkPos};
use crate::world::World;

/// Offsets of the six neighbours; index 0 is straight down, which sky light
/// follows without losing strength.
const NEIGHBOURS: [(i32, i32, i32); 6] = [
    (0, -1, 0),
    (0, 1, 0),
    (-1, 0, 0),
    (1, 0, 0),
    (0, 0, -1),
    (0, 0, 1),
];
/// Index of "down" in [`NEIGHBOURS`].
const DOWN: usize = 0;
/// Full light.
const MAX_LIGHT: u8 = 15;
/// `Block.getLightOpacity` of water and ice: they dim light by 3 per cell.
const LIQUID_OPACITY: u8 = 3;

/// Which of the two light arrays a pass edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Channel {
    Block,
    Sky,
}

/// How many levels of light `state` absorbs: [`MAX_LIGHT`] for a solid cube,
/// 3 for water and ice, 0 for everything else (each step still costs 1).
fn opacity(state: BlockState) -> u8 {
    if state.is_opaque() {
        MAX_LIGHT
    } else if matches!(state.id(), 8 | 9 | 79) {
        LIQUID_OPACITY
    } else {
        0
    }
}

/// `Block.getLightValue` of the 1.8 blocks that emit light (public so the
/// mesher can keep them out of the smooth-lighting path).
///
/// Source: the `setLightLevel` calls in MCP `Block.registerBlocks` (the block
/// ids are the 1.8 numeric ids). Blocks that are not listed emit nothing.
pub fn light_emission(state: BlockState) -> u8 {
    match state.id() {
        10 | 11 | 51 | 89 | 91 | 119 | 124 | 138 | 169 => 15,
        50 => 14,
        // Only the lit furnace glows; id 61 is the unlit one.
        62 => 13,
        90 => 11,
        74 => 9,
        76 | 130 => 7,
        39 | 117 | 120 | 122 => 1,
        _ => 0,
    }
}

impl World {
    /// Recomputes block and sky light around a block that just changed.
    ///
    /// Returns every loaded column whose light changed, plus the loaded
    /// neighbours of a column when a changed cell touches their shared border
    /// (smooth lighting reads one cell across it). The caller merges them into
    /// its remesh set.
    pub fn relight_around(&mut self, pos: BlockPos) -> Vec<ChunkPos> {
        let mut editor = LightEditor {
            has_sky: self.has_sky_light(),
            world: self,
            touched: HashSet::new(),
        };
        editor.relight(pos);
        let mut touched: Vec<ChunkPos> = editor.touched.into_iter().collect();
        touched.sort_unstable();
        touched
    }
}

/// Mutable view of the world for one relight, remembering what it changed.
struct LightEditor<'a> {
    world: &'a mut World,
    /// The dimension has sky light, so elided sections are full of it.
    has_sky: bool,
    touched: HashSet<ChunkPos>,
}

impl LightEditor<'_> {
    /// Runs the removal and re-spread of every channel for one changed cell.
    fn relight(&mut self, pos: BlockPos) {
        if self.cell(pos).is_none() {
            return;
        }
        self.relight_channel(Channel::Block, pos);
        if self.world.has_sky_light() {
            self.relight_channel(Channel::Sky, pos);
        }
    }

    fn relight_channel(&mut self, channel: Channel, pos: BlockPos) {
        let Some((state, old)) = self.cell_level(channel, pos) else {
            return;
        };
        let mut respread = VecDeque::new();
        if old > 0 {
            self.remove(channel, pos, old, &mut respread);
        }

        if opacity(state) < MAX_LIGHT {
            // Light flows back in from every neighbour that kept its own.
            for (dx, dy, dz) in NEIGHBOURS {
                respread.push_back(BlockPos::new(pos.x + dx, pos.y + dy, pos.z + dz));
            }
            // Above the world the sky is always open.
            if channel == Channel::Sky && pos.y == WORLD_HEIGHT as i32 - 1 {
                self.set(channel, pos, MAX_LIGHT.saturating_sub(opacity(state)));
                respread.push_back(pos);
            }
        }
        self.seed_emitter(channel, pos, state, &mut respread);
        self.spread(channel, respread);
    }

    /// Phase one: zeroes every cell whose light came through `start`.
    ///
    /// Neighbours that are at least as bright were lit by something else and
    /// are queued in `respread` instead.
    fn remove(
        &mut self,
        channel: Channel,
        start: BlockPos,
        level: u8,
        respread: &mut VecDeque<BlockPos>,
    ) {
        let mut queue = VecDeque::from([(start, level)]);
        self.set(channel, start, 0);
        while let Some((pos, level)) = queue.pop_front() {
            for (index, (dx, dy, dz)) in NEIGHBOURS.into_iter().enumerate() {
                let next = BlockPos::new(pos.x + dx, pos.y + dy, pos.z + dz);
                let Some((state, next_level)) = self.cell_level(channel, next) else {
                    continue;
                };
                if next_level == 0 {
                    continue;
                }
                let sky_shaft = channel == Channel::Sky
                    && index == DOWN
                    && level == MAX_LIGHT
                    && next_level == MAX_LIGHT;
                if next_level < level || sky_shaft {
                    self.set(channel, next, 0);
                    queue.push_back((next, next_level));
                    self.seed_emitter(channel, next, state, respread);
                } else {
                    respread.push_back(next);
                }
            }
        }
    }

    /// Phase two: floods light outward from the queued cells.
    fn spread(&mut self, channel: Channel, mut queue: VecDeque<BlockPos>) {
        while let Some(pos) = queue.pop_front() {
            let Some((_, level)) = self.cell_level(channel, pos) else {
                continue;
            };
            if level == 0 {
                continue;
            }
            for (index, (dx, dy, dz)) in NEIGHBOURS.into_iter().enumerate() {
                let next = BlockPos::new(pos.x + dx, pos.y + dy, pos.z + dz);
                let Some((state, next_level)) = self.cell_level(channel, next) else {
                    continue;
                };
                let absorbed = opacity(state);
                if absorbed >= MAX_LIGHT {
                    continue;
                }
                let sky_shaft =
                    channel == Channel::Sky && index == DOWN && level == MAX_LIGHT && absorbed == 0;
                let new_level = if sky_shaft {
                    MAX_LIGHT
                } else {
                    level.saturating_sub(absorbed.max(1))
                };
                if new_level > next_level {
                    self.set(channel, next, new_level);
                    queue.push_back(next);
                }
            }
        }
    }

    /// Makes an emitting block shine again after its cell was cleared.
    fn seed_emitter(
        &mut self,
        channel: Channel,
        pos: BlockPos,
        state: BlockState,
        queue: &mut VecDeque<BlockPos>,
    ) {
        let glow = light_emission(state);
        if channel == Channel::Block && glow > 0 {
            self.set(channel, pos, glow);
            queue.push_back(pos);
        }
    }

    /// Block state and light at `pos`; `None` outside the world or in an
    /// unloaded column, which light never crosses into.
    fn cell(&self, pos: BlockPos) -> Option<(BlockState, crate::LightLevel)> {
        if !(0..WORLD_HEIGHT as i32).contains(&pos.y) {
            return None;
        }
        let column = self.world.chunk(pos.chunk())?;
        let (x, z) = (usize::from(pos.local_x()), usize::from(pos.local_z()));
        let y = pos.y as usize;
        Some((column.block(x, y, z), column.light(x, y, z)))
    }

    fn cell_level(&self, channel: Channel, pos: BlockPos) -> Option<(BlockState, u8)> {
        let (state, light) = self.cell(pos)?;
        let level = match channel {
            Channel::Block => light.block,
            Channel::Sky => light.sky,
        };
        Some((state, level))
    }

    /// Writes one light level and records the columns that need a remesh.
    fn set(&mut self, channel: Channel, pos: BlockPos, level: u8) {
        let chunk = pos.chunk();
        let Some(column) = self.world.chunk_mut(chunk) else {
            return;
        };
        let (x, z) = (usize::from(pos.local_x()), usize::from(pos.local_z()));
        let y = pos.y as usize;
        if self.has_sky {
            column.ensure_open_sky_section(y);
        }
        match channel {
            Channel::Block => column.set_block_light(x, y, z, level),
            Channel::Sky => column.set_sky_light(x, y, z, level),
        }
        self.touched.insert(chunk);
        for (edge, neighbour) in [
            (x == 0, ChunkPos::new(chunk.x - 1, chunk.z)),
            (x == 15, ChunkPos::new(chunk.x + 1, chunk.z)),
            (z == 0, ChunkPos::new(chunk.x, chunk.z - 1)),
            (z == 15, ChunkPos::new(chunk.x, chunk.z + 1)),
        ] {
            if edge && self.world.chunk(neighbour).is_some() {
                self.touched.insert(neighbour);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::ChunkColumn;

    const STONE: BlockState = BlockState::new(1, 0);
    const TORCH: BlockState = BlockState::new(50, 0);

    /// A loaded overworld column with stone up to `ground` (inclusive) and
    /// open sky above, as a server would send it.
    fn world_with_ground(ground: usize) -> World {
        let mut world = World::new(0);
        let mut column = ChunkColumn::new(ChunkPos::new(0, 0));
        column.ensure_open_sky_section(0);
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..=ground {
                    column.set_block(x, y, z, STONE);
                    column.set_sky_light(x, y, z, 0);
                }
            }
        }
        world.insert_column_for_test(column);
        world
    }

    fn sky(world: &World, x: i32, y: i32, z: i32) -> u8 {
        world.light(BlockPos::new(x, y, z)).sky
    }

    fn block_light(world: &World, x: i32, y: i32, z: i32) -> u8 {
        world.light(BlockPos::new(x, y, z)).block
    }

    #[test]
    fn placing_a_roof_darkens_the_shaft_below_it() {
        let mut world = world_with_ground(10);
        world.set_block(BlockPos::new(8, 14, 8), STONE);
        world.relight_around(BlockPos::new(8, 14, 8));
        // One block of roof only blocks the shaft: light seeps in from the
        // open sides, one level weaker.
        assert_eq!(sky(&world, 8, 14, 8), 0, "the roof itself is solid");
        assert_eq!(sky(&world, 8, 13, 8), 14);
        assert_eq!(sky(&world, 8, 11, 8), 14);
    }

    /// Regression: a shadow stayed after the block casting it was broken.
    #[test]
    fn breaking_the_roof_removes_its_shadow() {
        let mut world = world_with_ground(10);
        world.set_block(BlockPos::new(8, 14, 8), STONE);
        world.relight_around(BlockPos::new(8, 14, 8));
        world.set_block(BlockPos::new(8, 14, 8), BlockState::AIR);
        world.relight_around(BlockPos::new(8, 14, 8));
        for y in 11..=14 {
            assert_eq!(sky(&world, 8, y, 8), 15, "y = {y}");
        }
    }

    #[test]
    fn opening_a_hole_in_the_ground_lights_a_cave_below_it() {
        let mut world = world_with_ground(10);
        world.set_block(BlockPos::new(4, 8, 4), BlockState::AIR);
        world.relight_around(BlockPos::new(4, 8, 4));
        assert_eq!(sky(&world, 4, 8, 4), 0, "walled in, still dark");
        world.set_block(BlockPos::new(4, 10, 4), BlockState::AIR);
        world.set_block(BlockPos::new(4, 9, 4), BlockState::AIR);
        world.relight_around(BlockPos::new(4, 9, 4));
        world.relight_around(BlockPos::new(4, 10, 4));
        assert_eq!(sky(&world, 4, 8, 4), 15, "the shaft carries full sky light");
    }

    #[test]
    fn a_torch_lights_its_surroundings_and_breaking_it_darkens_them() {
        let mut world = world_with_ground(10);
        let at = BlockPos::new(8, 11, 8);
        world.set_block(at, TORCH);
        world.relight_around(at);
        assert_eq!(block_light(&world, 8, 11, 8), 14);
        assert_eq!(block_light(&world, 10, 11, 8), 12);
        world.set_block(at, BlockState::AIR);
        world.relight_around(at);
        assert_eq!(block_light(&world, 8, 11, 8), 0);
        assert_eq!(block_light(&world, 10, 11, 8), 0);
    }

    #[test]
    fn a_change_on_a_column_border_dirties_the_loaded_neighbour() {
        let mut world = world_with_ground(10);
        world.insert_column_for_test(ChunkColumn::new(ChunkPos::new(1, 0)));
        let at = BlockPos::new(15, 14, 8);
        world.set_block(at, STONE);
        let touched = world.relight_around(at);
        assert!(touched.contains(&ChunkPos::new(0, 0)));
        assert!(touched.contains(&ChunkPos::new(1, 0)));
    }

    #[test]
    fn an_unloaded_position_is_ignored() {
        let mut world = world_with_ground(10);
        assert!(world.relight_around(BlockPos::new(100, 20, 100)).is_empty());
    }
}
