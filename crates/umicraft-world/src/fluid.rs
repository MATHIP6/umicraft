// SPDX-License-Identifier: MIT OR Apache-2.0

//! The rules of the 1.8 liquids that both the physics and the renderer read:
//! which blocks are water or lava, how full a liquid block is, which way it
//! flows, and the two material properties the flow depends on (ADR-0014).
//!
//! The functions take a block accessor instead of a world type, so the game
//! thread's world and the mesher's chunk snapshots share one implementation.

use crate::block::BlockState;
use crate::pos::BlockPos;

/// A 1.8 liquid material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liquid {
    /// Still (9) and flowing (8) water.
    Water,
    /// Still (11) and flowing (10) lava.
    Lava,
}

impl Liquid {
    /// The liquid a block state is made of, if any.
    ///
    /// ```
    /// use umicraft_world::fluid::Liquid;
    /// use umicraft_world::BlockState;
    ///
    /// assert_eq!(Liquid::of(BlockState::new(9, 0)), Some(Liquid::Water));
    /// assert_eq!(Liquid::of(BlockState::new(1, 0)), None);
    /// ```
    #[must_use]
    pub fn of(state: BlockState) -> Option<Self> {
        match state.id() {
            8 | 9 => Some(Self::Water),
            10 | 11 => Some(Self::Lava),
            _ => None,
        }
    }

    /// True when `state` is made of this liquid.
    #[must_use]
    pub fn contains(self, state: BlockState) -> bool {
        Self::of(state) == Some(self)
    }
}

/// `BlockLiquid.getLiquidHeightPercent`: the empty share of a liquid block at
/// a level (0 is a source, 1-7 flow away from it, 8 and above fall).
///
/// ```
/// use umicraft_world::fluid::height_percent;
///
/// // A source block is filled to 8/9.
/// assert_eq!(height_percent(0), 1.0 / 9.0);
/// assert_eq!(height_percent(8), height_percent(0));
/// ```
#[must_use]
pub fn height_percent(level: u8) -> f32 {
    let level = if level >= 8 { 0 } else { level };
    f32::from(level + 1) / 9.0
}

/// True when the block's material does not block movement: air, liquids,
/// plants, vines, fire, circuits, carpets, snow layers, portals and cobwebs.
///
/// Derived from the materials of MCP `Block.registerBlocks` and
/// `Material.blocksMovement` (`MaterialLogic`, `MaterialTransparent`,
/// `MaterialLiquid`, `MaterialPortal` and the anonymous `web` material).
#[must_use]
pub fn lets_movement_through(state: BlockState) -> bool {
    matches!(
        state.id(),
        0 | 6
            | 8..=11
            | 27
            | 28
            | 30..=32
            | 37..=40
            | 50
            | 51
            | 55
            | 59
            | 65
            | 66
            | 69
            | 75..=78
            | 83
            | 90
            | 93
            | 94
            | 104..=106
            | 111
            | 115
            | 119
            | 127
            | 131
            | 132
            | 140..=144
            | 149
            | 150
            | 157
            | 171
            | 175
    )
}

/// `Material.isSolid`: every material that blocks movement, and the cobweb.
#[must_use]
pub fn is_solid_material(state: BlockState) -> bool {
    state.id() == 30 || !lets_movement_through(state)
}

/// `BlockLiquid.isBlockSolid` towards a horizontal side: never the liquid
/// itself nor ice, otherwise [`is_solid_material`].
fn holds_back(liquid: Liquid, state: BlockState) -> bool {
    !liquid.contains(state) && state.id() != 79 && is_solid_material(state)
}

/// The horizontal neighbours in the order of `EnumFacing.Plane.HORIZONTAL`:
/// north, east, south, west, as `(dx, dz)`.
const HORIZONTAL: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

/// `BlockLiquid.getEffectiveFlowDecay`: the level of a block of `liquid`, 0
/// for a falling block, -1 when the block is not that liquid.
fn flow_decay(liquid: Liquid, state: BlockState) -> i32 {
    if !liquid.contains(state) {
        return -1;
    }
    let level = i32::from(state.metadata());
    if level >= 8 {
        0
    } else {
        level
    }
}

/// `Vec3.normalize`: a unit vector, or zero below a length of 1e-4.
fn normalize([x, y, z]: [f64; 3]) -> [f64; 3] {
    let length = (x * x + y * y + z * z).sqrt();
    if length < 1.0e-4 {
        [0.0; 3]
    } else {
        [x / length, y / length, z / length]
    }
}

/// The unit direction `liquid` flows in at `pos`, or zero when it is still.
///
/// MCP `BlockLiquid.getFlowVector` (line 150): towards the neighbours with a
/// higher level, towards a lower neighbour across an opening, and straight
/// down for falling liquid next to a side that holds it back. `block` reads
/// the world; positions outside it must read as air.
///
/// ```
/// use umicraft_world::fluid::{flow_vector, Liquid};
/// use umicraft_world::{BlockPos, BlockState};
///
/// // Level 1 water with level 2 water to the east flows east.
/// let block = |pos: BlockPos| match (pos.x, pos.y, pos.z) {
///     (0, 0, 0) => BlockState::new(8, 1),
///     (1, 0, 0) => BlockState::new(8, 2),
///     _ => BlockState::new(1, 0),
/// };
/// assert_eq!(flow_vector(Liquid::Water, BlockPos::new(0, 0, 0), block), [1.0, 0.0, 0.0]);
/// ```
pub fn flow_vector(
    liquid: Liquid,
    pos: BlockPos,
    block: impl Fn(BlockPos) -> BlockState,
) -> [f64; 3] {
    let here = block(pos);
    let decay = flow_decay(liquid, here);
    let mut flow = [0.0; 3];
    for (dx, dz) in HORIZONTAL {
        let side = BlockPos::new(pos.x + dx, pos.y, pos.z + dz);
        let neighbour = block(side);
        let weight = match flow_decay(liquid, neighbour) {
            level if level >= 0 => level - decay,
            _ if lets_movement_through(neighbour) => {
                let below = flow_decay(liquid, block(BlockPos::new(side.x, side.y - 1, side.z)));
                if below < 0 {
                    continue;
                }
                below - (decay - 8)
            }
            _ => continue,
        };
        flow[0] += f64::from(dx * weight);
        flow[2] += f64::from(dz * weight);
    }
    if here.metadata() >= 8 {
        let walled = HORIZONTAL.iter().any(|(dx, dz)| {
            holds_back(liquid, block(BlockPos::new(pos.x + dx, pos.y, pos.z + dz)))
                || holds_back(
                    liquid,
                    block(BlockPos::new(pos.x + dx, pos.y + 1, pos.z + dz)),
                )
        });
        if walled {
            let [x, y, z] = normalize(flow);
            flow = [x, y - 6.0, z];
        }
    }
    normalize(flow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falling_liquid_next_to_a_wall_flows_down() {
        let block = |pos: BlockPos| match (pos.x, pos.y, pos.z) {
            (0, 0, 0) => BlockState::new(8, 8),
            (1, 0, 0) => BlockState::new(1, 0),
            _ => BlockState::AIR,
        };
        let [x, y, z] = flow_vector(Liquid::Water, BlockPos::new(0, 0, 0), block);
        assert_eq!((x, z), (0.0, 0.0));
        assert!((y + 1.0).abs() < 1e-12, "straight down, got {y}");
    }

    #[test]
    fn liquid_flows_over_an_edge_towards_the_liquid_below() {
        // Air east of a source, with water one block lower: the source flows
        // east across the opening.
        let block = |pos: BlockPos| match (pos.x, pos.y, pos.z) {
            (0, 0, 0) => BlockState::new(9, 0),
            (1, -1, 0) => BlockState::new(8, 1),
            (1, 0, 0) => BlockState::AIR,
            _ => BlockState::new(1, 0),
        };
        assert_eq!(
            flow_vector(Liquid::Water, BlockPos::new(0, 0, 0), block),
            [1.0, 0.0, 0.0]
        );
    }

    #[test]
    fn water_and_lava_do_not_mix_their_flow() {
        let block = |pos: BlockPos| match (pos.x, pos.y, pos.z) {
            (0, 0, 0) => BlockState::new(11, 1),
            (1, 0, 0) => BlockState::new(8, 2),
            _ => BlockState::new(1, 0),
        };
        assert_eq!(
            flow_vector(Liquid::Lava, BlockPos::new(0, 0, 0), block),
            [0.0; 3]
        );
    }
}
