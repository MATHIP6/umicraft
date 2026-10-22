// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-block collision geometry.
//!
//! Every 1.8 block state maps to at most [`MAX_BOXES`] axis-aligned boxes
//! that the player collides with. The mapping is data driven so that the M5
//! block registry can replace it wholesale; today it is a `match` on the 1.8
//! block id (MCP `net/minecraft/block/Block.java`, `registerBlocks`).
//!
//! The three shapes that matter for movement are derived from the MCP
//! reference:
//!
//! * **Slabs** — `BlockSlab.setBlockBoundsBasedOnState`:
//!   bottom `(0, 0, 0)-(1, 0.5, 1)`, top `(0, 0.5, 0)-(1, 1, 1)`.
//! * **Stairs** — `BlockStairs.setBaseCollisionBounds` plus the step box of
//!   `BlockStairs.func_176306_h`.
//! * **Fences** — `BlockFence.addCollisionBoxesToList`: a post plus the arms
//!   towards the connected neighbours, 1.5 blocks tall.
//!
//! Everything else is either a full cube or has no collision at all; the
//! partial-height blocks (plates, carpet, snow layers, cake, ...) use the
//! bounds of their MCP block class.

use glam::DVec3;
use umicraft_world::{BlockPos, BlockState, World};

/// Read access to block states by world coordinate.
///
/// The trait exists so the game logic can be driven by any block provider in
/// tests without a loaded world. Implementations must answer [`BlockState::AIR`]
/// for positions outside the loaded world, exactly like
/// [`umicraft_world::World::block`] does.
pub trait BlockSource {
    /// The block state at the given world position.
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState;
}

impl BlockSource for World {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        World::block(self, BlockPos::new(x, y, z))
    }
}

impl<T: BlockSource + ?Sized> BlockSource for &T {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        (**self).block(x, y, z)
    }
}

/// An axis-aligned bounding box.
///
/// The box is the shared geometry primitive of this crate: block collision
/// shapes are expressed in it, and the collision solver moves the player's
/// box through the voxel grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    /// The corner with the smallest coordinates.
    pub min: DVec3,
    /// The corner with the largest coordinates.
    pub max: DVec3,
}

impl Aabb {
    /// Creates a box from two opposite corners; the corners may be given in
    /// any order.
    #[must_use]
    pub fn new(a: DVec3, b: DVec3) -> Self {
        Self {
            min: a.min(b),
            max: a.max(b),
        }
    }

    /// Creates a box from its minimum corner and its size.
    #[must_use]
    pub fn from_min_size(min: DVec3, size: DVec3) -> Self {
        Self {
            min,
            max: min + size,
        }
    }

    /// Moves the whole box by `delta`.
    #[must_use]
    pub fn offset(&self, delta: DVec3) -> Self {
        Self {
            min: self.min + delta,
            max: self.max + delta,
        }
    }

    /// The box that covers both this box and the box moved by `delta`.
    ///
    /// The box is only extended in the direction of the movement, which is
    /// what `AxisAlignedBB.addCoord` (MCP
    /// `net/minecraft/util/AxisAlignedBB.java:35`) does. The vanilla entity
    /// mover uses it to find the blocks a move touches.
    #[must_use]
    pub fn swept(&self, delta: DVec3) -> Self {
        let mut min = self.min;
        let mut max = self.max;
        if delta.x < 0.0 {
            min.x += delta.x;
        } else if delta.x > 0.0 {
            max.x += delta.x;
        }
        if delta.y < 0.0 {
            min.y += delta.y;
        } else if delta.y > 0.0 {
            max.y += delta.y;
        }
        if delta.z < 0.0 {
            min.z += delta.z;
        } else if delta.z > 0.0 {
            max.z += delta.z;
        }
        Self { min, max }
    }

    /// True when the two boxes overlap on all three axes.
    ///
    /// Touching faces do not count as an overlap, matching
    /// `AxisAlignedBB.intersectsWith` (MCP `net/minecraft/util/AxisAlignedBB.java:233`).
    #[must_use]
    pub fn intersects(&self, other: &Aabb) -> bool {
        other.max.x > self.min.x
            && other.min.x < self.max.x
            && other.max.y > self.min.y
            && other.min.y < self.max.y
            && other.max.z > self.min.z
            && other.min.z < self.max.z
    }

    /// The X displacement that keeps `mover` from penetrating this box.
    ///
    /// `self` is the obstacle and `mover` the box that is being moved, which
    /// mirrors `AxisAlignedBB.calculateXOffset` (MCP
    /// `net/minecraft/util/AxisAlignedBB.java:127`).
    #[must_use]
    pub fn calculate_x_offset(&self, mover: &Aabb, offset_x: f64) -> f64 {
        if mover.max.y > self.min.y
            && mover.min.y < self.max.y
            && mover.max.z > self.min.z
            && mover.min.z < self.max.z
        {
            if offset_x > 0.0 && mover.max.x <= self.min.x {
                let gap = self.min.x - mover.max.x;
                if gap < offset_x {
                    return gap;
                }
            } else if offset_x < 0.0 && mover.min.x >= self.max.x {
                let gap = self.max.x - mover.min.x;
                if gap > offset_x {
                    return gap;
                }
            }
        }
        offset_x
    }

    /// The Y displacement that keeps `mover` from penetrating this box.
    ///
    /// See [`Aabb::calculate_x_offset`] for the argument order.
    #[must_use]
    pub fn calculate_y_offset(&self, mover: &Aabb, offset_y: f64) -> f64 {
        if mover.max.x > self.min.x
            && mover.min.x < self.max.x
            && mover.max.z > self.min.z
            && mover.min.z < self.max.z
        {
            if offset_y > 0.0 && mover.max.y <= self.min.y {
                let gap = self.min.y - mover.max.y;
                if gap < offset_y {
                    return gap;
                }
            } else if offset_y < 0.0 && mover.min.y >= self.max.y {
                let gap = self.max.y - mover.min.y;
                if gap > offset_y {
                    return gap;
                }
            }
        }
        offset_y
    }

    /// The Z displacement that keeps `mover` from penetrating this box.
    ///
    /// See [`Aabb::calculate_x_offset`] for the argument order.
    #[must_use]
    pub fn calculate_z_offset(&self, mover: &Aabb, offset_z: f64) -> f64 {
        if mover.max.x > self.min.x
            && mover.min.x < self.max.x
            && mover.max.y > self.min.y
            && mover.min.y < self.max.y
        {
            if offset_z > 0.0 && mover.max.z <= self.min.z {
                let gap = self.min.z - mover.max.z;
                if gap < offset_z {
                    return gap;
                }
            } else if offset_z < 0.0 && mover.min.z >= self.max.z {
                let gap = self.max.z - mover.min.z;
                if gap > offset_z {
                    return gap;
                }
            }
        }
        offset_z
    }
}

/// The maximum number of collision boxes a single block state contributes.
///
/// Stairs and fences need two boxes; nothing in 1.8 needs more.
pub const MAX_BOXES: usize = 2;

/// The collision geometry of a block state, relative to the block origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockShape {
    boxes: [Aabb; MAX_BOXES],
    count: usize,
}

impl BlockShape {
    /// A shape without any collision box (air, liquids, decorations).
    pub const EMPTY: Self = Self {
        boxes: [Aabb {
            min: DVec3::ZERO,
            max: DVec3::ZERO,
        }; MAX_BOXES],
        count: 0,
    };

    /// The full cube `(0, 0, 0)-(1, 1, 1)`.
    #[must_use]
    pub fn cube() -> Self {
        Self::single(Aabb::new(DVec3::ZERO, DVec3::ONE))
    }

    /// A slab: the lower or the upper half of a cube.
    #[must_use]
    pub fn slab(top: bool) -> Self {
        let (min_y, max_y) = if top { (0.5, 1.0) } else { (0.0, 0.5) };
        Self::single(Aabb::new(
            DVec3::new(0.0, min_y, 0.0),
            DVec3::new(1.0, max_y, 1.0),
        ))
    }

    /// A box that is `height` blocks tall and covers the full block footprint.
    #[must_use]
    pub fn column(height: f64) -> Self {
        Self::single(Aabb::new(DVec3::ZERO, DVec3::new(1.0, height, 1.0)))
    }

    /// A single collision box.
    #[must_use]
    pub fn single(bounding_box: Aabb) -> Self {
        Self {
            boxes: [bounding_box; MAX_BOXES],
            count: 1,
        }
    }

    /// Two collision boxes.
    #[must_use]
    pub fn pair(first: Aabb, second: Aabb) -> Self {
        Self {
            boxes: [first, second],
            count: 2,
        }
    }

    /// The collision boxes of this shape.
    #[must_use]
    pub fn boxes(&self) -> &[Aabb] {
        &self.boxes[..self.count]
    }

    /// True when the shape has no collision box at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// The horizontal direction a stair faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StairFacing {
    /// −X.
    West,
    /// +X.
    East,
    /// −Z.
    North,
    /// +Z.
    South,
}

/// The classification of a block id, independent of its neighbourhood.
///
/// Splitting the classification from the geometry keeps the fence lookup
/// non-recursive: a fence asks its neighbours whether they are solid cubes,
/// which must not ask the neighbours again.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    /// No collision box at all.
    Empty,
    /// A full cube.
    Cube,
    /// The lower or upper half of a cube.
    Slab(bool),
    /// A stair block.
    Stairs,
    /// A fence, wall or fence gate.
    Fence,
    /// A thin plate (glass pane, iron bars).
    Pane,
    /// A box that covers the full footprint and is `f64` blocks tall.
    Column(f64),
    /// A cube inset by one sixteenth on both horizontal axes (cactus).
    Cactus,
    /// An anvil, whose slab orientation depends on the metadata.
    Anvil,
    /// A snow layer, whose height depends on the metadata.
    SnowLayer,
}

/// Classifies a 1.8 block id (and its metadata) into a collision kind.
///
/// The block ids are the registration ids of MCP
/// `net/minecraft/block/Block.java` (`registerBlocks`).
fn classify(id: u16, metadata: u8) -> Kind {
    match id {
        // Air and liquids: no collision at all.
        0 | 8..=11 => Kind::Empty,
        // Slabs; the metadata bit 3 selects the upper half.
        44 | 126 | 182 => Kind::Slab(metadata & 8 != 0),
        // Double slabs are full cubes.
        43 | 125 | 181 => Kind::Cube,
        // Stairs.
        53 | 67 | 108 | 109 | 114 | 128 | 134 | 135 | 136 | 156 | 163 | 164 | 180 => Kind::Stairs,
        // Fences, walls and fence gates.
        85 | 107 | 113 | 139 | 183..=192 => Kind::Fence,
        // Glass panes and iron bars.
        101 | 102 | 160 => Kind::Pane,
        // Trap doors.
        96 | 167 => Kind::Column(0.1875),
        // Pressure plates, rails, repeaters, comparators, redstone wire and
        // carpet are one sixteenth high.
        27 | 28 | 55 | 66 | 70 | 72 | 93 | 94 | 147 | 148 | 149 | 150 | 171 => Kind::Column(0.0625),
        // Daylight detectors.
        151 | 178 => Kind::Column(0.375),
        // Cake, bed, enchanting table, brewing stand, cauldron and hopper.
        92 => Kind::Column(0.5),
        26 => Kind::Column(0.5625),
        116..=118 => Kind::Column(0.75),
        154 => Kind::Column(0.625),
        81 => Kind::Cactus,
        145 => Kind::Anvil,
        78 => Kind::SnowLayer,
        // Decorations and other blocks without a collision box.
        6
        | 29
        | 30
        | 31
        | 32
        | 34
        | 35
        | 36
        | 37
        | 38
        | 39
        | 40
        | 50
        | 51
        | 59
        | 63
        | 64
        | 65
        | 68
        | 69
        | 71
        | 75
        | 76
        | 77
        | 83
        | 106
        | 111
        | 115
        | 119
        | 120
        | 127
        | 131
        | 132
        | 140
        | 141
        | 142
        | 143
        | 144
        | 175
        | 176
        | 177
        | 193..=197 => Kind::Empty,
        // Everything else is a full cube.
        _ => Kind::Cube,
    }
}

/// Classifies a block state into its collision geometry.
///
/// `world` is only consulted for the shapes that depend on their
/// neighbourhood (fences, walls and gates). Out-of-range positions must read
/// as air.
#[must_use]
pub fn shape_of<S: BlockSource + ?Sized>(
    state: BlockState,
    world: &S,
    pos: BlockPos,
) -> BlockShape {
    match classify(state.id(), state.metadata()) {
        Kind::Empty => BlockShape::EMPTY,
        Kind::Cube => BlockShape::cube(),
        Kind::Slab(top) => BlockShape::slab(top),
        Kind::Column(height) => BlockShape::column(height),
        Kind::Cactus => BlockShape::single(Aabb::new(
            DVec3::new(0.0625, 0.0, 0.0625),
            DVec3::new(0.9375, 1.0, 0.9375),
        )),
        Kind::Anvil => {
            if state.metadata() & 1 == 0 {
                BlockShape::single(Aabb::new(
                    DVec3::new(0.0, 0.0, 0.125),
                    DVec3::new(1.0, 1.0, 0.875),
                ))
            } else {
                BlockShape::single(Aabb::new(
                    DVec3::new(0.125, 0.0, 0.0),
                    DVec3::new(0.875, 1.0, 1.0),
                ))
            }
        }
        Kind::SnowLayer => BlockShape::column(f64::from(state.metadata() + 1) / 8.0),
        Kind::Pane => BlockShape::pair(
            Aabb::new(DVec3::new(0.0, 0.0, 0.4375), DVec3::new(1.0, 1.0, 0.5625)),
            Aabb::new(DVec3::new(0.4375, 0.0, 0.0), DVec3::new(0.5625, 1.0, 1.0)),
        ),
        Kind::Stairs => stair_shape(state),
        Kind::Fence => fence_shape(world, pos),
    }
}

/// The shape used for block targeting.
///
/// It is the collision shape of the block, except for decorations: a
/// decoration has no collision box but must still be targetable, so it falls
/// back to the full cube. Air is never targetable.
#[must_use]
pub fn selection_shape<S: BlockSource + ?Sized>(
    state: BlockState,
    world: &S,
    pos: BlockPos,
) -> BlockShape {
    let shape = shape_of(state, world, pos);
    if shape.is_empty() && !state.is_air() {
        BlockShape::cube()
    } else {
        shape
    }
}

/// The two collision boxes of a stair block.
///
/// MCP `BlockStairs.getStateFromMeta` (line 726) maps the metadata to a
/// facing and a half; `setBaseCollisionBounds` (line 80) gives the lower (or
/// upper) half of the cube and `func_176306_h` (line 292) the step.
fn stair_shape(state: BlockState) -> BlockShape {
    let meta = state.metadata();
    let top = meta & 4 != 0;
    let facing = match meta & 3 {
        0 => StairFacing::East,
        1 => StairFacing::West,
        2 => StairFacing::South,
        _ => StairFacing::North,
    };

    let base = if top {
        Aabb::new(DVec3::new(0.0, 0.5, 0.0), DVec3::new(1.0, 1.0, 1.0))
    } else {
        Aabb::new(DVec3::ZERO, DVec3::new(1.0, 0.5, 1.0))
    };

    let step = match (facing, top) {
        (StairFacing::East, false) => {
            Aabb::new(DVec3::new(0.5, 0.5, 0.0), DVec3::new(1.0, 1.0, 1.0))
        }
        (StairFacing::West, false) => {
            Aabb::new(DVec3::new(0.0, 0.5, 0.0), DVec3::new(0.5, 1.0, 1.0))
        }
        (StairFacing::South, false) => {
            Aabb::new(DVec3::new(0.0, 0.5, 0.5), DVec3::new(1.0, 1.0, 1.0))
        }
        (StairFacing::North, false) => {
            Aabb::new(DVec3::new(0.0, 0.5, 0.0), DVec3::new(1.0, 1.0, 0.5))
        }
        (StairFacing::East, true) => {
            Aabb::new(DVec3::new(0.5, 0.0, 0.0), DVec3::new(1.0, 0.5, 1.0))
        }
        (StairFacing::West, true) => {
            Aabb::new(DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.5, 0.5, 1.0))
        }
        (StairFacing::South, true) => {
            Aabb::new(DVec3::new(0.0, 0.0, 0.5), DVec3::new(1.0, 0.5, 1.0))
        }
        (StairFacing::North, true) => {
            Aabb::new(DVec3::new(0.0, 0.0, 0.0), DVec3::new(1.0, 0.5, 0.5))
        }
    };

    BlockShape::pair(base, step)
}

/// The collision boxes of a fence, wall or fence gate.
///
/// MCP `BlockFence.addCollisionBoxesToList` (line 50) emits a post-and-arm
/// box for the north/south axis and a second one for the west/east axis.
fn fence_shape<S: BlockSource + ?Sized>(world: &S, pos: BlockPos) -> BlockShape {
    let north = fence_connects(world, pos, 0);
    let south = fence_connects(world, pos, 1);
    let west = fence_connects(world, pos, 2);
    let east = fence_connects(world, pos, 3);

    // The post is 0.375..0.625 wide and 1.5 blocks tall; an arm reaches the
    // full edge of the block it is connected to.
    let mut min_x = 0.375;
    let mut max_x = 0.625;
    let mut min_z = 0.375;
    let mut max_z = 0.625;
    if north {
        min_z = 0.0;
    }
    if south {
        max_z = 1.0;
    }
    let north_south = Aabb::new(DVec3::new(min_x, 0.0, min_z), DVec3::new(max_x, 1.5, max_z));

    min_x = 0.375;
    max_x = 0.625;
    min_z = 0.375;
    max_z = 0.625;
    if west {
        min_x = 0.0;
    }
    if east {
        max_x = 1.0;
    }
    let west_east = Aabb::new(DVec3::new(min_x, 0.0, min_z), DVec3::new(max_x, 1.5, max_z));

    if north || south {
        BlockShape::pair(north_south, west_east)
    } else {
        // Without a north/south arm only the post (plus any west/east arm)
        // exists; the post is already part of `west_east`.
        BlockShape::single(west_east)
    }
}

/// True when the neighbour in the given direction connects to the fence.
///
/// `direction` is 0 = north (−Z), 1 = south (+Z), 2 = west (−X), 3 = east (+X).
///
/// MCP `BlockFence.canConnectTo` (line 161) connects a fence to other fences
/// and gates and to opaque full cubes.
fn fence_connects<S: BlockSource + ?Sized>(world: &S, pos: BlockPos, direction: u8) -> bool {
    let neighbour = match direction {
        0 => BlockPos::new(pos.x, pos.y, pos.z - 1),
        1 => BlockPos::new(pos.x, pos.y, pos.z + 1),
        2 => BlockPos::new(pos.x - 1, pos.y, pos.z),
        _ => BlockPos::new(pos.x + 1, pos.y, pos.z),
    };
    let other = world.block(neighbour.x, neighbour.y, neighbour.z);
    if other.is_air() {
        return false;
    }
    if matches!(classify(other.id(), other.metadata()), Kind::Fence) {
        return true;
    }
    // Solid full cubes hold a fence in place, pumpkins do not
    // (MCP `BlockFence.canConnectTo`, line 164).
    matches!(classify(other.id(), other.metadata()), Kind::Cube) && other.id() != 86
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A block source backed by a hash map, for tests.
    #[derive(Debug, Default)]
    struct TestBlocks {
        blocks: HashMap<(i32, i32, i32), BlockState>,
    }

    impl TestBlocks {
        fn with(blocks: &[(i32, i32, i32, BlockState)]) -> Self {
            let mut source = Self::default();
            for (x, y, z, state) in blocks {
                source.blocks.insert((*x, *y, *z), *state);
            }
            source
        }
    }

    impl BlockSource for TestBlocks {
        fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
            self.blocks
                .get(&(x, y, z))
                .copied()
                .unwrap_or(BlockState::AIR)
        }
    }

    #[test]
    fn full_cube_is_the_default_shape() {
        let world = TestBlocks::default();
        let shape = shape_of(BlockState::new(1, 0), &world, BlockPos::new(0, 0, 0));
        assert_eq!(shape.boxes().len(), 1);
        assert_eq!(shape.boxes()[0].min, DVec3::ZERO);
        assert_eq!(shape.boxes()[0].max, DVec3::ONE);
    }

    #[test]
    fn air_and_liquids_do_not_collide() {
        let world = TestBlocks::default();
        assert!(shape_of(BlockState::AIR, &world, BlockPos::new(0, 0, 0)).is_empty());
        assert!(shape_of(BlockState::new(9, 0), &world, BlockPos::new(0, 0, 0)).is_empty());
        assert!(shape_of(BlockState::new(11, 0), &world, BlockPos::new(0, 0, 0)).is_empty());
    }

    #[test]
    fn slabs_are_half_high() {
        let world = TestBlocks::default();
        // stone_slab (id 44), metadata 0 = bottom, metadata 8 = top.
        let bottom = shape_of(BlockState::new(44, 0), &world, BlockPos::new(0, 0, 0));
        assert_eq!(bottom.boxes()[0].min.y, 0.0);
        assert_eq!(bottom.boxes()[0].max.y, 0.5);

        let top = shape_of(BlockState::new(44, 8), &world, BlockPos::new(0, 0, 0));
        assert_eq!(top.boxes()[0].min.y, 0.5);
        assert_eq!(top.boxes()[0].max.y, 1.0);

        // A double slab is a full cube.
        let double = shape_of(BlockState::new(43, 0), &world, BlockPos::new(0, 0, 0));
        assert_eq!(double.boxes()[0].max.y, 1.0);
    }

    #[test]
    fn stairs_have_a_base_and_a_step() {
        let world = TestBlocks::default();
        // oak_stairs (id 53), metadata 0 = bottom half facing east.
        let stairs = shape_of(BlockState::new(53, 0), &world, BlockPos::new(0, 0, 0));
        assert_eq!(stairs.boxes().len(), 2);
        assert_eq!(stairs.boxes()[0].max.y, 0.5);
        assert_eq!(stairs.boxes()[1].min.y, 0.5);
        assert_eq!(stairs.boxes()[1].min.x, 0.5);
        assert_eq!(stairs.boxes()[1].max.x, 1.0);
    }

    #[test]
    fn a_lone_fence_is_a_post() {
        let world = TestBlocks::default();
        let fence = shape_of(BlockState::new(85, 0), &world, BlockPos::new(0, 0, 0));
        assert_eq!(fence.boxes().len(), 1);
        let post = fence.boxes()[0];
        assert_eq!(post.min.x, 0.375);
        assert_eq!(post.max.x, 0.625);
        assert_eq!(post.max.y, 1.5);
    }

    #[test]
    fn a_fence_reaches_towards_a_connected_neighbour() {
        let world = TestBlocks::with(&[(0, 0, -1, BlockState::new(85, 0))]);
        let fence = shape_of(BlockState::new(85, 0), &world, BlockPos::new(0, 0, 0));
        assert_eq!(fence.boxes().len(), 2);
        // The north arm reaches the edge of the block.
        assert_eq!(fence.boxes()[0].min.z, 0.0);
        assert_eq!(fence.boxes()[0].max.z, 0.625);
    }

    #[test]
    fn pressure_plates_are_one_sixteenth_high() {
        let world = TestBlocks::default();
        let plate = shape_of(BlockState::new(70, 0), &world, BlockPos::new(0, 0, 0));
        assert_eq!(plate.boxes()[0].max.y, 0.0625);
    }

    #[test]
    fn decorations_have_no_collision_but_stay_targetable() {
        let world = TestBlocks::default();
        let torch = BlockState::new(50, 0);
        assert!(shape_of(torch, &world, BlockPos::new(0, 0, 0)).is_empty());
        let selection = selection_shape(torch, &world, BlockPos::new(0, 0, 0));
        assert_eq!(selection.boxes().len(), 1);
        assert!(selection_shape(BlockState::AIR, &world, BlockPos::new(0, 0, 0)).is_empty());
    }

    #[test]
    fn box_offsets_keep_the_mover_outside() {
        let obstacle = Aabb::new(DVec3::ZERO, DVec3::ONE);
        // The mover is right of the obstacle.
        let mover = Aabb::new(DVec3::new(1.2, 0.0, 0.0), DVec3::new(1.8, 1.8, 0.6));
        // Moving left is stopped 0.2 blocks before the obstacle.
        assert!((obstacle.calculate_x_offset(&mover, -1.0) + 0.2).abs() < 1e-9);
        // Moving right is not blocked by the obstacle behind the mover.
        assert_eq!(obstacle.calculate_x_offset(&mover, 1.0), 1.0);
    }
}
