// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-block collision geometry.
//!
//! Every 1.8 block state maps to at most [`MAX_BOXES`] axis-aligned boxes
//! that the player collides with. [`shape_of`] follows, block class by block
//! class, the `getCollisionBoundingBox`, `setBlockBounds*` and
//! `addCollisionBoxesToList` code of MCP `net/minecraft/block`: blocks
//! without a collision box (plants, rails, wire, plates, signs, ...), partial
//! heights (slabs, snow layers, soul sand, carpets, ...), shapes that depend
//! on the metadata (ladders, trap doors, gates, pistons, anvils, ...) and
//! shapes that depend on the neighbours (doors and their two halves, chests,
//! fences, walls, panes, stair corners). The derivation of each table is
//! recorded in `docs/sources.md`.

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
/// The cauldron and the hopper need five: a floor and four walls
/// (MCP `BlockCauldron.addCollisionBoxesToList`,
/// `BlockHopper.addCollisionBoxesToList`).
pub const MAX_BOXES: usize = 5;

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
        if top {
            Self::single(local(0.0, 0.5, 0.0, 1.0, 1.0, 1.0))
        } else {
            Self::column(0.5)
        }
    }

    /// A box that is `height` blocks tall and covers the full block footprint.
    #[must_use]
    pub fn column(height: f64) -> Self {
        Self::single(local(0.0, 0.0, 0.0, 1.0, height, 1.0))
    }

    /// A single collision box.
    #[must_use]
    pub fn single(bounding_box: Aabb) -> Self {
        Self::EMPTY.with(bounding_box)
    }

    /// Two collision boxes.
    #[must_use]
    pub fn pair(first: Aabb, second: Aabb) -> Self {
        Self::EMPTY.with(first).with(second)
    }

    /// This shape with one more box.
    ///
    /// # Panics
    /// When the shape already holds [`MAX_BOXES`] boxes; every shape of this
    /// module is built with a fixed number of boxes below that limit.
    #[must_use]
    fn with(mut self, bounding_box: Aabb) -> Self {
        self.boxes[self.count] = bounding_box;
        self.count += 1;
        self
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

/// A box in block-local coordinates, given like MCP `setBlockBounds`.
fn local(min_x: f64, min_y: f64, min_z: f64, max_x: f64, max_y: f64, max_z: f64) -> Aabb {
    Aabb {
        min: DVec3::new(min_x, min_y, min_z),
        max: DVec3::new(max_x, max_y, max_z),
    }
}

/// A direction of MCP `EnumFacing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Facing {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Facing {
    /// `EnumFacing.getFront`: down, up, north, south, west, east.
    fn front(index: u8) -> Self {
        match index % 6 {
            0 => Self::Down,
            1 => Self::Up,
            2 => Self::North,
            3 => Self::South,
            4 => Self::West,
            _ => Self::East,
        }
    }

    /// `EnumFacing.getHorizontal`: south, west, north, east.
    fn horizontal(index: u8) -> Self {
        match index % 4 {
            0 => Self::South,
            1 => Self::West,
            2 => Self::North,
            _ => Self::East,
        }
    }

    /// `EnumFacing.rotateYCCW` for the horizontal directions.
    fn rotate_ccw(self) -> Self {
        match self {
            Self::North => Self::West,
            Self::West => Self::South,
            Self::South => Self::East,
            Self::East => Self::North,
            vertical => vertical,
        }
    }

    /// True for west and east.
    fn on_x_axis(self) -> bool {
        matches!(self, Self::West | Self::East)
    }
}

/// The block next to `pos` in a horizontal direction.
fn neighbour<S: BlockSource + ?Sized>(world: &S, pos: BlockPos, facing: Facing) -> BlockState {
    let (x, z) = match facing {
        Facing::North => (pos.x, pos.z - 1),
        Facing::South => (pos.x, pos.z + 1),
        Facing::West => (pos.x - 1, pos.z),
        Facing::East => (pos.x + 1, pos.z),
        Facing::Down | Facing::Up => (pos.x, pos.z),
    };
    let y = match facing {
        Facing::Down => pos.y - 1,
        Facing::Up => pos.y + 1,
        _ => pos.y,
    };
    world.block(x, y, z)
}

/// Classifies a block state into its collision geometry.
///
/// Every arm follows the `getCollisionBoundingBox`, `setBlockBounds*` and
/// `addCollisionBoxesToList` code of the block's MCP class (sources in
/// `docs/sources.md`). `world` is consulted for the shapes that depend on
/// their neighbours (doors, chests, fences, walls, panes, stairs); positions
/// outside the loaded world must read as air.
#[must_use]
pub fn shape_of<S: BlockSource + ?Sized>(
    state: BlockState,
    world: &S,
    pos: BlockPos,
) -> BlockShape {
    let meta = state.metadata();
    match state.id() {
        // `getCollisionBoundingBox` returns null: air, liquids, plants and
        // crops (`BlockBush`), torches, rails, redstone wire, pressure
        // plates, signs, levers, buttons, fire, portals, cobwebs, vines,
        // tripwire, stems, banners and the moving piston.
        0
        | 6
        | 8..=11
        | 27
        | 28
        | 30..=32
        | 36..=40
        | 50
        | 51
        | 55
        | 59
        | 63
        | 66
        | 68..=70
        | 72
        | 75..=77
        | 83
        | 90
        | 104..=106
        | 115
        | 119
        | 131
        | 132
        | 141..=143
        | 147
        | 148
        | 157
        | 175..=177 => BlockShape::EMPTY,
        44 | 126 | 182 => BlockShape::slab(meta & 8 != 0),
        53 | 67 | 108 | 109 | 114 | 128 | 134..=136 | 156 | 163 | 164 | 180 => {
            stair_shape(state, world, pos)
        }
        85 | 113 | 188..=192 => fence_shape(state, world, pos),
        107 | 183..=187 => fence_gate_shape(meta),
        139 => wall_shape(world, pos),
        101 | 102 | 160 => pane_shape(world, pos),
        64 | 71 | 193..=197 => door_shape(state, world, pos),
        96 | 167 => trapdoor_shape(meta),
        65 => ladder_shape(meta),
        29 | 33 => piston_base_shape(meta),
        34 => piston_head_shape(meta),
        54 | 146 => chest_shape(state, world, pos),
        130 => BlockShape::single(local(0.0625, 0.0, 0.0625, 0.9375, 0.875, 0.9375)),
        26 => BlockShape::column(0.5625),
        // `BlockSnow.getCollisionBoundingBox`: one eighth per layer above the
        // first; a single layer is a box without height, which collides like
        // the block below it.
        78 => match meta & 7 {
            0 => BlockShape::EMPTY,
            layers => BlockShape::column(f64::from(layers) * 0.125),
        },
        81 => BlockShape::single(local(0.0625, 0.0, 0.0625, 0.9375, 1.0, 0.9375)),
        88 => BlockShape::column(0.875),
        92 => cake_shape(meta),
        93 | 94 | 149 | 150 => BlockShape::column(0.125),
        111 => BlockShape::column(0.015625),
        116 => BlockShape::column(0.75),
        117 => BlockShape::pair(
            local(0.4375, 0.0, 0.4375, 0.5625, 0.875, 0.5625),
            local(0.0, 0.0, 0.0, 1.0, 0.125, 1.0),
        ),
        118 => basin_shape(0.3125),
        120 => end_portal_frame_shape(meta),
        122 => BlockShape::single(local(0.0625, 0.0, 0.0625, 0.9375, 1.0, 0.9375)),
        127 => cocoa_shape(meta),
        140 => BlockShape::single(local(0.3125, 0.0, 0.3125, 0.6875, 0.375, 0.6875)),
        144 => skull_shape(meta),
        145 => anvil_shape(meta),
        151 | 178 => BlockShape::column(0.375),
        154 => basin_shape(0.625),
        171 => BlockShape::column(0.0625),
        _ => BlockShape::cube(),
    }
}

/// A floor of the given height and four walls an eighth thick: the cauldron
/// (floor 0.3125) and the hopper (floor 0.625).
fn basin_shape(floor: f64) -> BlockShape {
    let wall = 0.125;
    BlockShape::column(floor)
        .with(local(0.0, 0.0, 0.0, wall, 1.0, 1.0))
        .with(local(0.0, 0.0, 0.0, 1.0, 1.0, wall))
        .with(local(1.0 - wall, 0.0, 0.0, 1.0, 1.0, 1.0))
        .with(local(0.0, 0.0, 1.0 - wall, 1.0, 1.0, 1.0))
}

/// `BlockCake.getCollisionBoundingBox`: each bite removes two sixteenths from
/// the west side.
fn cake_shape(meta: u8) -> BlockShape {
    let west = f64::from(1 + 2 * (meta & 7)) / 16.0;
    BlockShape::single(local(west, 0.0, 0.0625, 0.9375, 0.5, 0.9375))
}

/// `BlockEndPortalFrame.addCollisionBoxesToList`: the frame, and the eye on
/// top when one is inserted (metadata bit 2).
fn end_portal_frame_shape(meta: u8) -> BlockShape {
    let frame = BlockShape::column(0.8125);
    if meta & 4 != 0 {
        frame.with(local(0.3125, 0.8125, 0.3125, 0.6875, 1.0, 0.6875))
    } else {
        frame
    }
}

/// `BlockLadder.setBlockBoundsBasedOnState`: an eighth thick against the
/// block it hangs on; `getFront(meta)`, a vertical facing reads as north.
fn ladder_shape(meta: u8) -> BlockShape {
    let thickness = 0.125;
    BlockShape::single(match Facing::front(meta) {
        Facing::South => local(0.0, 0.0, 0.0, 1.0, 1.0, thickness),
        Facing::West => local(1.0 - thickness, 0.0, 0.0, 1.0, 1.0, 1.0),
        Facing::East => local(0.0, 0.0, 0.0, thickness, 1.0, 1.0),
        Facing::North | Facing::Down | Facing::Up => {
            local(0.0, 0.0, 1.0 - thickness, 1.0, 1.0, 1.0)
        }
    })
}

/// `BlockTrapDoor.setBounds`: a closed trap door lies on the bottom or the
/// top of the block (metadata bit 3); an open one stands against its hinge
/// side (bits 0-1: north, south, west, east).
fn trapdoor_shape(meta: u8) -> BlockShape {
    let thickness = 0.1875;
    BlockShape::single(if meta & 4 == 0 {
        if meta & 8 != 0 {
            local(0.0, 1.0 - thickness, 0.0, 1.0, 1.0, 1.0)
        } else {
            local(0.0, 0.0, 0.0, 1.0, thickness, 1.0)
        }
    } else {
        match meta & 3 {
            0 => local(0.0, 0.0, 1.0 - thickness, 1.0, 1.0, 1.0),
            1 => local(0.0, 0.0, 0.0, 1.0, 1.0, thickness),
            2 => local(1.0 - thickness, 0.0, 0.0, 1.0, 1.0, 1.0),
            _ => local(0.0, 0.0, 0.0, thickness, 1.0, 1.0),
        }
    })
}

/// `BlockFenceGate.getCollisionBoundingBox`: nothing when open, otherwise a
/// 1.5 blocks tall bar across the gate's axis.
fn fence_gate_shape(meta: u8) -> BlockShape {
    if meta & 4 != 0 {
        return BlockShape::EMPTY;
    }
    BlockShape::single(if Facing::horizontal(meta).on_x_axis() {
        local(0.375, 0.0, 0.0, 0.625, 1.5, 1.0)
    } else {
        local(0.0, 0.0, 0.375, 1.0, 1.5, 0.625)
    })
}

/// `BlockSkull.setBlockBoundsBasedOnState`: on the floor, or half a block
/// against a wall (`getFront(meta & 7)`).
fn skull_shape(meta: u8) -> BlockShape {
    BlockShape::single(match Facing::front(meta & 7) {
        Facing::North => local(0.25, 0.25, 0.5, 0.75, 0.75, 1.0),
        Facing::South => local(0.25, 0.25, 0.0, 0.75, 0.75, 0.5),
        Facing::West => local(0.5, 0.25, 0.25, 1.0, 0.75, 0.75),
        Facing::East => local(0.0, 0.25, 0.25, 0.5, 0.75, 0.75),
        Facing::Up | Facing::Down => local(0.25, 0.0, 0.25, 0.75, 0.5, 0.75),
    })
}

/// `BlockCocoa.setBlockBoundsBasedOnState`: a pod that grows with its age
/// (bits 2-3) and hangs on the log in its facing (`getHorizontal(meta)`).
fn cocoa_shape(meta: u8) -> BlockShape {
    let age = f64::from(((meta & 15) >> 2).min(2));
    let width = 4.0 + age * 2.0;
    let height = 5.0 + age * 2.0;
    let half = width / 2.0;
    let bottom = (12.0 - height) / 16.0;
    let (low, high) = ((8.0 - half) / 16.0, (8.0 + half) / 16.0);
    BlockShape::single(match Facing::horizontal(meta) {
        Facing::South => local(low, bottom, (15.0 - width) / 16.0, high, 0.75, 0.9375),
        Facing::North => local(low, bottom, 0.0625, high, 0.75, (1.0 + width) / 16.0),
        Facing::West => local(0.0625, bottom, low, (1.0 + width) / 16.0, 0.75, high),
        _ => local((15.0 - width) / 16.0, bottom, low, 0.9375, 0.75, high),
    })
}

/// `BlockAnvil.setBlockBoundsBasedOnState`: inset by an eighth on the axis
/// across its facing (`getHorizontal(meta & 3)`).
fn anvil_shape(meta: u8) -> BlockShape {
    BlockShape::single(if Facing::horizontal(meta & 3).on_x_axis() {
        local(0.0, 0.0, 0.125, 1.0, 1.0, 0.875)
    } else {
        local(0.125, 0.0, 0.0, 0.875, 1.0, 1.0)
    })
}

/// `BlockPistonBase.getFacing`: `getFront(meta & 7)`, nothing above 5.
fn piston_facing(meta: u8) -> Option<Facing> {
    let index = meta & 7;
    (index <= 5).then(|| Facing::front(index))
}

/// `BlockPistonBase.setBlockBoundsBasedOnState`: an extended piston
/// (metadata bit 3) loses the quarter its head occupies.
fn piston_base_shape(meta: u8) -> BlockShape {
    let Some(facing) = piston_facing(meta).filter(|_| meta & 8 != 0) else {
        return BlockShape::cube();
    };
    BlockShape::single(match facing {
        Facing::Down => local(0.0, 0.25, 0.0, 1.0, 1.0, 1.0),
        Facing::Up => local(0.0, 0.0, 0.0, 1.0, 0.75, 1.0),
        Facing::North => local(0.0, 0.0, 0.25, 1.0, 1.0, 1.0),
        Facing::South => local(0.0, 0.0, 0.0, 1.0, 1.0, 0.75),
        Facing::West => local(0.25, 0.0, 0.0, 1.0, 1.0, 1.0),
        Facing::East => local(0.0, 0.0, 0.0, 0.75, 1.0, 1.0),
    })
}

/// `BlockPistonExtension.addCollisionBoxesToList`: the head plate and the
/// arm. The west arm box is reproduced as the reference has it (its Y and Z
/// ranges do not match the other arms).
fn piston_head_shape(meta: u8) -> BlockShape {
    let Some(facing) = piston_facing(meta) else {
        return BlockShape::cube();
    };
    let (head, arm) = match facing {
        Facing::Down => (
            local(0.0, 0.0, 0.0, 1.0, 0.25, 1.0),
            local(0.375, 0.25, 0.375, 0.625, 1.0, 0.625),
        ),
        Facing::Up => (
            local(0.0, 0.75, 0.0, 1.0, 1.0, 1.0),
            local(0.375, 0.0, 0.375, 0.625, 0.75, 0.625),
        ),
        Facing::North => (
            local(0.0, 0.0, 0.0, 1.0, 1.0, 0.25),
            local(0.25, 0.375, 0.25, 0.75, 0.625, 1.0),
        ),
        Facing::South => (
            local(0.0, 0.0, 0.75, 1.0, 1.0, 1.0),
            local(0.25, 0.375, 0.0, 0.75, 0.625, 0.75),
        ),
        Facing::West => (
            local(0.0, 0.0, 0.0, 0.25, 1.0, 1.0),
            local(0.375, 0.25, 0.25, 0.625, 0.75, 1.0),
        ),
        Facing::East => (
            local(0.75, 0.0, 0.0, 1.0, 1.0, 1.0),
            local(0.0, 0.375, 0.25, 0.75, 0.625, 0.75),
        ),
    };
    BlockShape::pair(head, arm)
}

/// `BlockChest.setBlockBoundsBasedOnState`: inset by a sixteenth and 0.875
/// tall, reaching the edge towards a chest of the same kind (north, south,
/// west, east in that order).
fn chest_shape<S: BlockSource + ?Sized>(state: BlockState, world: &S, pos: BlockPos) -> BlockShape {
    let same = |facing| neighbour(world, pos, facing).id() == state.id();
    let (mut min_x, mut min_z, mut max_x, mut max_z) = (0.0625, 0.0625, 0.9375, 0.9375);
    if same(Facing::North) {
        min_z = 0.0;
    } else if same(Facing::South) {
        max_z = 1.0;
    } else if same(Facing::West) {
        min_x = 0.0;
    } else if same(Facing::East) {
        max_x = 1.0;
    }
    BlockShape::single(local(min_x, 0.0, min_z, max_x, 0.875, max_z))
}

/// `BlockDoor.setBoundBasedOnMeta` on the metadata `combineMetadata` builds
/// from both halves: the lower half holds the facing (bits 0-1) and the open
/// flag (bit 2), the upper half the hinge (bit 0).
fn door_shape<S: BlockSource + ?Sized>(state: BlockState, world: &S, pos: BlockPos) -> BlockShape {
    let meta = state.metadata();
    let (lower, upper) = if meta & 8 != 0 {
        (neighbour(world, pos, Facing::Down).metadata(), meta)
    } else {
        (meta, neighbour(world, pos, Facing::Up).metadata())
    };
    let facing = Facing::horizontal(lower & 3).rotate_ccw();
    let open = lower & 4 != 0;
    let hinge_left = upper & 1 != 0;
    let thickness = 0.1875;
    let north_edge = local(0.0, 0.0, 0.0, 1.0, 1.0, thickness);
    let south_edge = local(0.0, 0.0, 1.0 - thickness, 1.0, 1.0, 1.0);
    let west_edge = local(0.0, 0.0, 0.0, thickness, 1.0, 1.0);
    let east_edge = local(1.0 - thickness, 0.0, 0.0, 1.0, 1.0, 1.0);
    BlockShape::single(match (facing, open, hinge_left) {
        (Facing::East, true, false) | (Facing::West, true, true) | (Facing::South, false, _) => {
            north_edge
        }
        (Facing::East, true, true) | (Facing::West, true, false) | (Facing::North, false, _) => {
            south_edge
        }
        (Facing::South, true, false) | (Facing::North, true, true) | (Facing::West, false, _) => {
            east_edge
        }
        _ => west_edge,
    })
}

/// True for the stair blocks (`instanceof BlockStairs`).
fn is_stairs(state: BlockState) -> bool {
    matches!(
        state.id(),
        53 | 67 | 108 | 109 | 114 | 128 | 134..=136 | 156 | 163 | 164 | 180
    )
}

/// `BlockStairs.getStateFromMeta`: `getFront(5 - (meta & 3))`.
fn stair_facing(state: BlockState) -> Facing {
    Facing::front(5 - (state.metadata() & 3))
}

/// True when the stair state is the upper half (metadata bit 2).
fn stair_top(state: BlockState) -> bool {
    state.metadata() & 4 != 0
}

/// The collision boxes of a stair block, MCP `BlockStairs.addCollisionBoxesToList`.
///
/// The base is the half slab (`setBaseCollisionBounds`); the step is the half
/// of the other layer towards the facing, shortened to a quarter by a
/// perpendicular stair in front (outer corner, `func_176306_h`); a
/// perpendicular stair behind adds the quarter of an inner corner
/// (`func_176304_i`).
fn stair_shape<S: BlockSource + ?Sized>(state: BlockState, world: &S, pos: BlockPos) -> BlockShape {
    let top = stair_top(state);
    let facing = stair_facing(state);
    let (base, (step_low, step_high)) = if top {
        (local(0.0, 0.5, 0.0, 1.0, 1.0, 1.0), (0.0, 0.5))
    } else {
        (local(0.0, 0.0, 0.0, 1.0, 0.5, 1.0), (0.5, 1.0))
    };
    // A stair of the same half next to this one, and its facing.
    let stair_towards = |direction| {
        let other = neighbour(world, pos, direction);
        (is_stairs(other) && stair_top(other) == top).then(|| stair_facing(other))
    };
    // `isSameStair`: the same half and the same facing.
    let same_stair = |direction| {
        let other = neighbour(world, pos, direction);
        is_stairs(other) && stair_top(other) == top && stair_facing(other) == facing
    };

    // The step, `func_176306_h`.
    let (mut x0, mut x1, mut z0, mut z1) = (0.0, 1.0, 0.0, 0.5);
    let mut whole_step = true;
    match facing {
        Facing::East | Facing::West => {
            if facing == Facing::East {
                x0 = 0.5;
            } else {
                x1 = 0.5;
            }
            z1 = 1.0;
            match stair_towards(facing) {
                Some(Facing::North) if !same_stair(Facing::South) => {
                    z1 = 0.5;
                    whole_step = false;
                }
                Some(Facing::South) if !same_stair(Facing::North) => {
                    z0 = 0.5;
                    whole_step = false;
                }
                _ => {}
            }
        }
        Facing::South | Facing::North => {
            if facing == Facing::South {
                z0 = 0.5;
                z1 = 1.0;
            }
            match stair_towards(facing) {
                Some(Facing::West) if !same_stair(Facing::East) => {
                    x1 = 0.5;
                    whole_step = false;
                }
                Some(Facing::East) if !same_stair(Facing::West) => {
                    x0 = 0.5;
                    whole_step = false;
                }
                _ => {}
            }
        }
        Facing::Down | Facing::Up => {}
    }
    let shape = BlockShape::pair(base, local(x0, step_low, z0, x1, step_high, z1));
    if !whole_step {
        return shape;
    }

    // The inner corner, `func_176304_i`: a perpendicular stair behind.
    let corner = match facing {
        Facing::East | Facing::West => {
            let behind = if facing == Facing::East {
                Facing::West
            } else {
                Facing::East
            };
            let (cx0, cx1) = if facing == Facing::East {
                (0.0, 0.5)
            } else {
                (0.5, 1.0)
            };
            match stair_towards(behind) {
                Some(Facing::North) if !same_stair(Facing::North) => Some((cx0, cx1, 0.0, 0.5)),
                Some(Facing::South) if !same_stair(Facing::South) => Some((cx0, cx1, 0.5, 1.0)),
                _ => None,
            }
        }
        Facing::South | Facing::North => {
            let behind = if facing == Facing::South {
                Facing::North
            } else {
                Facing::South
            };
            let (cz0, cz1) = if facing == Facing::South {
                (0.0, 0.5)
            } else {
                (0.5, 1.0)
            };
            match stair_towards(behind) {
                Some(Facing::West) if !same_stair(Facing::West) => Some((0.0, 0.5, cz0, cz1)),
                Some(Facing::East) if !same_stair(Facing::East) => Some((0.5, 1.0, cz0, cz1)),
                _ => None,
            }
        }
        Facing::Down | Facing::Up => None,
    };
    match corner {
        Some((cx0, cx1, cz0, cz1)) => shape.with(local(cx0, step_low, cz0, cx1, step_high, cz1)),
        None => shape,
    }
}

/// True for the wooden fences, whose material is wood.
fn is_wooden_fence(id: u16) -> bool {
    matches!(id, 85 | 188..=192)
}

/// True for the fence gates (`instanceof BlockFenceGate`).
fn is_fence_gate(id: u16) -> bool {
    matches!(id, 107 | 183..=187)
}

/// True for the pumpkins and the melon, whose material is `gourd`: fences
/// and walls never attach to them.
fn is_gourd(id: u16) -> bool {
    matches!(id, 86 | 91 | 103)
}

/// Whether a block's material is opaque and the block is a full cube
/// (`material.isOpaque() && isFullCube()`), derived like
/// [`is_normal_cube`] but without the power check: it adds the redstone
/// block.
fn is_solid_full_cube(state: BlockState) -> bool {
    is_normal_cube(state) || state.id() == 152
}

/// `BlockFence.canConnectTo` (line 161): a fence of the same material, a
/// fence gate, or a solid full cube that is not a gourd; never the barrier.
fn fence_connects(fence: BlockState, other: BlockState) -> bool {
    let id = other.id();
    if id == 166 {
        return false;
    }
    let is_fence = is_wooden_fence(id) || id == 113;
    if is_fence {
        return is_wooden_fence(id) == is_wooden_fence(fence.id());
    }
    is_fence_gate(id) || (is_solid_full_cube(other) && !is_gourd(id))
}

/// `BlockFence.addCollisionBoxesToList`: a 1.5 blocks tall post with its
/// north/south arms, and a second box with the west/east arms.
fn fence_shape<S: BlockSource + ?Sized>(state: BlockState, world: &S, pos: BlockPos) -> BlockShape {
    let connects = |facing| fence_connects(state, neighbour(world, pos, facing));
    let (north, south) = (connects(Facing::North), connects(Facing::South));
    let (west, east) = (connects(Facing::West), connects(Facing::East));
    let mut shape = BlockShape::EMPTY;
    if north || south {
        let min_z = if north { 0.0 } else { 0.375 };
        let max_z = if south { 1.0 } else { 0.625 };
        shape = shape.with(local(0.375, 0.0, min_z, 0.625, 1.5, max_z));
    }
    if west || east || (!north && !south) {
        let min_x = if west { 0.0 } else { 0.375 };
        let max_x = if east { 1.0 } else { 0.625 };
        shape = shape.with(local(min_x, 0.0, 0.375, max_x, 1.5, 0.625));
    }
    shape
}

/// `BlockWall.canConnectTo` (line 122): another wall, a fence gate, or a solid
/// full cube that is not a gourd; never the barrier.
fn wall_connects(other: BlockState) -> bool {
    let id = other.id();
    id != 166 && (id == 139 || is_fence_gate(id) || (is_solid_full_cube(other) && !is_gourd(id)))
}

/// `BlockWall.setBlockBoundsBasedOnState` with the collision height of 1.5:
/// one box from the post to every connected side, slimmer for a straight
/// wall.
fn wall_shape<S: BlockSource + ?Sized>(world: &S, pos: BlockPos) -> BlockShape {
    let connects = |facing| wall_connects(neighbour(world, pos, facing));
    let (north, south) = (connects(Facing::North), connects(Facing::South));
    let (west, east) = (connects(Facing::West), connects(Facing::East));
    let mut min_x = if west { 0.0 } else { 0.25 };
    let mut max_x = if east { 1.0 } else { 0.75 };
    let mut min_z = if north { 0.0 } else { 0.25 };
    let mut max_z = if south { 1.0 } else { 0.75 };
    if north && south && !west && !east {
        (min_x, max_x) = (0.3125, 0.6875);
    } else if !north && !south && west && east {
        (min_z, max_z) = (0.3125, 0.6875);
    }
    BlockShape::single(local(min_x, 0.0, min_z, max_x, 1.5, max_z))
}

/// `Block.isFullBlock` (`isOpaqueCube`) for the pane connection, derived from
/// the `isOpaqueCube` overrides of `Block.registerBlocks`. The double slabs
/// are opaque (`BlockSlab.isOpaqueCube` returns `isDouble()`); the leaves
/// are, as on the server, where `BlockLeaves.fancyGraphics` stays false.
fn is_opaque_cube(state: BlockState) -> bool {
    matches!(
        state.id(),
        1..=5
            | 7
            | 12..=19
            | 21..=25
            | 35
            | 41..=43
            | 45..=49
            | 56..=58
            | 61
            | 62
            | 73
            | 74
            | 80
            | 82
            | 84
            | 86..=89
            | 91
            | 97..=100
            | 103
            | 110
            | 112
            | 121
            | 123..=125
            | 129
            | 133
            | 137
            | 152
            | 153
            | 155
            | 158
            | 159
            | 161
            | 162
            | 168..=170
            | 172..=174
            | 179
            | 181
    )
}

/// `BlockPane.canPaneConnectToBlock` (line 177): an opaque cube, glass,
/// stained glass or another pane.
fn pane_connects(other: BlockState) -> bool {
    is_opaque_cube(other) || matches!(other.id(), 20 | 95 | 101 | 102 | 160)
}

/// `BlockPane.addCollisionBoxesToList`: a west/east and a north/south plate
/// two sixteenths thick, each reaching the connected sides; a pane without
/// connections is a cross.
fn pane_shape<S: BlockSource + ?Sized>(world: &S, pos: BlockPos) -> BlockShape {
    let connects = |facing| pane_connects(neighbour(world, pos, facing));
    let (north, south) = (connects(Facing::North), connects(Facing::South));
    let (west, east) = (connects(Facing::West), connects(Facing::East));
    let any = north || south || west || east;
    let mut shape = BlockShape::EMPTY;
    if (!west || !east) && any {
        if west {
            shape = shape.with(local(0.0, 0.0, 0.4375, 0.5, 1.0, 0.5625));
        } else if east {
            shape = shape.with(local(0.5, 0.0, 0.4375, 1.0, 1.0, 0.5625));
        }
    } else {
        shape = shape.with(local(0.0, 0.0, 0.4375, 1.0, 1.0, 0.5625));
    }
    if (!north || !south) && any {
        if north {
            shape = shape.with(local(0.4375, 0.0, 0.0, 0.5625, 1.0, 0.5));
        } else if south {
            shape = shape.with(local(0.4375, 0.0, 0.5, 0.5625, 1.0, 1.0));
        }
    } else {
        shape = shape.with(local(0.4375, 0.0, 0.0, 0.5625, 1.0, 1.0));
    }
    shape
}

/// Whether a block is a *normal cube*: an opaque, full, non-powering block.
///
/// MCP `Block.isNormalCube` (`Block.java:354`) is
/// `material.isOpaque() && isFullCube() && !canProvidePower()`. The ids below
/// were derived from the registrations of `Block.registerBlocks`, each
/// block's material (`Material.java`, translucent or non-solid materials are
/// not opaque) and its `isFullCube`/`canProvidePower` overrides; the double
/// slabs (43, 125, 181) are full because `BlockSlab.isFullCube` returns
/// `isDouble()`. Leaves, glass, glowstone, ice, TNT, chests, slabs, stairs,
/// plants and the redstone block are therefore *not* normal cubes.
#[must_use]
pub fn is_normal_cube(state: BlockState) -> bool {
    matches!(
        state.id(),
        1..=5
            | 7
            | 12..=17
            | 19
            | 21..=25
            | 35
            | 41..=43
            | 45
            | 47..=49
            | 52
            | 56..=58
            | 61
            | 62
            | 73
            | 74
            | 80
            | 82
            | 84
            | 86..=88
            | 91
            | 97..=100
            | 103
            | 110
            | 112
            | 120
            | 121
            | 123..=125
            | 129
            | 133
            | 137
            | 153
            | 155
            | 158
            | 159
            | 162
            | 165
            | 166
            | 168
            | 170
            | 172..=174
            | 179
            | 181
    )
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
        // Only the north/south box: the west/east one needs a west or east
        // arm, or no arm at all (MCP `BlockFence.addCollisionBoxesToList`).
        assert_eq!(fence.boxes().len(), 1);
        // The north arm reaches the edge of the block.
        assert_eq!(fence.boxes()[0].min.z, 0.0);
        assert_eq!(fence.boxes()[0].max.z, 0.625);
    }

    const ORIGIN: BlockPos = BlockPos::new(0, 1, 0);

    /// The boxes of `state` at [`ORIGIN`] among `neighbours`.
    fn boxes_of(state: BlockState, neighbours: &[(i32, i32, i32, BlockState)]) -> Vec<Aabb> {
        let world = TestBlocks::with(neighbours);
        shape_of(state, &world, ORIGIN).boxes().to_vec()
    }

    fn b(min_x: f64, min_y: f64, min_z: f64, max_x: f64, max_y: f64, max_z: f64) -> Aabb {
        local(min_x, min_y, min_z, max_x, max_y, max_z)
    }

    /// Regression: rails, redstone wire, pressure plates, stems, banners and
    /// the activator rail had collision boxes the 1.8 blocks do not have.
    #[test]
    fn rails_wire_plates_stems_and_banners_do_not_collide() {
        for id in [27, 28, 55, 66, 70, 72, 104, 105, 147, 148, 157, 176, 177] {
            assert!(
                boxes_of(BlockState::new(id, 0), &[]).is_empty(),
                "block {id}"
            );
        }
    }

    #[test]
    fn partial_blocks_have_their_reference_heights() {
        let height = |id, meta| boxes_of(BlockState::new(id, meta), &[])[0].max.y;
        assert_eq!(height(88, 0), 0.875, "soul sand");
        assert_eq!(height(93, 0), 0.125, "repeater");
        assert_eq!(height(149, 0), 0.125, "comparator");
        assert_eq!(height(111, 0), 0.015625, "lily pad");
        assert_eq!(height(171, 0), 0.0625, "carpet");
        // `BlockSnow`: one eighth per layer above the first.
        assert!(boxes_of(BlockState::new(78, 0), &[]).is_empty());
        assert_eq!(height(78, 3), 0.375);
        assert_eq!(height(78, 7), 0.875);
    }

    #[test]
    fn a_bitten_cake_shrinks_from_the_west() {
        let cake = boxes_of(BlockState::new(92, 3), &[]);
        assert_eq!(cake, vec![b(7.0 / 16.0, 0.0, 0.0625, 0.9375, 0.5, 0.9375)]);
    }

    #[test]
    fn cauldrons_and_hoppers_have_a_floor_and_four_walls() {
        let cauldron = boxes_of(BlockState::new(118, 0), &[]);
        assert_eq!(cauldron.len(), 5);
        assert_eq!(cauldron[0], b(0.0, 0.0, 0.0, 1.0, 0.3125, 1.0));
        assert_eq!(cauldron[1], b(0.0, 0.0, 0.0, 0.125, 1.0, 1.0));
        let hopper = boxes_of(BlockState::new(154, 0), &[]);
        assert_eq!(hopper.len(), 5);
        assert_eq!(hopper[0].max.y, 0.625);
        assert_eq!(
            boxes_of(BlockState::new(117, 0), &[]).len(),
            2,
            "brewing stand"
        );
    }

    #[test]
    fn a_ladder_is_an_eighth_against_its_wall() {
        // Metadata 2 faces north: the ladder hangs on the block south of it.
        assert_eq!(
            boxes_of(BlockState::new(65, 2), &[]),
            vec![b(0.0, 0.0, 0.875, 1.0, 1.0, 1.0)]
        );
        assert_eq!(
            boxes_of(BlockState::new(65, 5), &[]),
            vec![b(0.0, 0.0, 0.0, 0.125, 1.0, 1.0)]
        );
    }

    /// Regression: the anvil insets were swapped between the two axes.
    #[test]
    fn an_anvil_is_inset_across_its_facing() {
        // Metadata 0 faces south (Z axis): inset on X.
        assert_eq!(
            boxes_of(BlockState::new(145, 0), &[]),
            vec![b(0.125, 0.0, 0.0, 0.875, 1.0, 1.0)]
        );
        assert_eq!(
            boxes_of(BlockState::new(145, 1), &[]),
            vec![b(0.0, 0.0, 0.125, 1.0, 1.0, 0.875)]
        );
    }

    #[test]
    fn trap_doors_follow_their_half_and_their_open_flag() {
        assert_eq!(boxes_of(BlockState::new(96, 0), &[])[0].max.y, 0.1875);
        assert_eq!(boxes_of(BlockState::new(96, 8), &[])[0].min.y, 0.8125);
        // Open (bit 2), hinged on the north side (bits 0-1 = 0).
        assert_eq!(
            boxes_of(BlockState::new(96, 4), &[]),
            vec![b(0.0, 0.0, 0.8125, 1.0, 1.0, 1.0)]
        );
    }

    #[test]
    fn an_open_fence_gate_lets_the_player_through() {
        assert!(boxes_of(BlockState::new(107, 4), &[]).is_empty());
        // Closed and facing south: a bar across the Z axis, 1.5 tall.
        assert_eq!(
            boxes_of(BlockState::new(107, 0), &[]),
            vec![b(0.0, 0.0, 0.375, 1.0, 1.5, 0.625)]
        );
    }

    /// Regression: doors were full cubes, so an open door blocked the way.
    #[test]
    fn a_door_is_a_thin_panel_built_from_both_halves() {
        // Lower half: facing bits 0 (east after the rotation), closed.
        let lower_closed = BlockState::new(64, 0);
        let upper = BlockState::new(64, 8);
        assert_eq!(
            boxes_of(lower_closed, &[(0, 2, 0, upper)]),
            vec![b(0.0, 0.0, 0.0, 0.1875, 1.0, 1.0)]
        );
        // The upper half reads the facing and the open flag from below.
        let lower_open = BlockState::new(64, 4);
        let world = TestBlocks::with(&[(0, 0, 0, lower_open)]);
        assert_eq!(
            shape_of(upper, &world, ORIGIN).boxes().to_vec(),
            vec![b(0.0, 0.0, 0.0, 1.0, 1.0, 0.1875)]
        );
        // The hinge bit of the upper half moves the open panel.
        let upper_left = BlockState::new(64, 9);
        assert_eq!(
            boxes_of(lower_open, &[(0, 2, 0, upper_left)]),
            vec![b(0.0, 0.0, 0.8125, 1.0, 1.0, 1.0)]
        );
    }

    #[test]
    fn a_double_chest_reaches_its_other_half() {
        let chest = BlockState::new(54, 0);
        assert_eq!(
            boxes_of(chest, &[]),
            vec![b(0.0625, 0.0, 0.0625, 0.9375, 0.875, 0.9375)]
        );
        assert_eq!(
            boxes_of(chest, &[(1, 1, 0, chest)]),
            vec![b(0.0625, 0.0, 0.0625, 1.0, 0.875, 0.9375)]
        );
        // A trapped chest is another kind and does not join.
        assert_eq!(
            boxes_of(chest, &[(1, 1, 0, BlockState::new(146, 0))])[0]
                .max
                .x,
            0.9375
        );
    }

    #[test]
    fn fences_join_their_own_material_and_solid_cubes_only() {
        let oak = BlockState::new(85, 0);
        let arms = |neighbour: BlockState| {
            let shape = boxes_of(oak, &[(0, 1, -1, neighbour)]);
            shape.iter().any(|bounds| bounds.min.z == 0.0)
        };
        assert!(arms(BlockState::new(188, 0)), "another wooden fence");
        assert!(arms(BlockState::new(1, 0)), "stone");
        assert!(arms(BlockState::new(107, 0)), "a fence gate");
        assert!(!arms(BlockState::new(113, 0)), "nether brick fence");
        assert!(!arms(BlockState::new(20, 0)), "glass");
        assert!(!arms(BlockState::new(86, 0)), "pumpkin");
        assert!(!arms(BlockState::new(139, 0)), "wall");
    }

    #[test]
    fn a_straight_wall_is_slimmer_and_one_and_a_half_tall() {
        let wall = BlockState::new(139, 0);
        assert_eq!(
            boxes_of(wall, &[(0, 1, -1, wall), (0, 1, 1, wall)]),
            vec![b(0.3125, 0.0, 0.0, 0.6875, 1.5, 1.0)]
        );
        assert_eq!(
            boxes_of(wall, &[]),
            vec![b(0.25, 0.0, 0.25, 0.75, 1.5, 0.75)]
        );
    }

    #[test]
    fn a_pane_is_a_cross_alone_and_a_plate_in_a_line() {
        let pane = BlockState::new(102, 0);
        assert_eq!(boxes_of(pane, &[]).len(), 2);
        let line = boxes_of(pane, &[(-1, 1, 0, pane), (1, 1, 0, pane)]);
        assert_eq!(line, vec![b(0.0, 0.0, 0.4375, 1.0, 1.0, 0.5625)]);
    }

    #[test]
    fn stairs_form_inner_and_outer_corners() {
        // Metadata 0 faces east: the step is the east half.
        let east = BlockState::new(53, 0);
        let north = BlockState::new(53, 3);
        assert_eq!(boxes_of(east, &[])[1], b(0.5, 0.5, 0.0, 1.0, 1.0, 1.0));
        // A north-facing stair in front (east) makes an outer corner.
        let outer = boxes_of(east, &[(1, 1, 0, north)]);
        assert_eq!(outer.len(), 2);
        assert_eq!(outer[1], b(0.5, 0.5, 0.0, 1.0, 1.0, 0.5));
        // A north-facing stair behind (west) adds an inner corner quarter.
        let inner = boxes_of(east, &[(-1, 1, 0, north)]);
        assert_eq!(inner.len(), 3);
        assert_eq!(inner[2], b(0.0, 0.5, 0.0, 0.5, 1.0, 0.5));
    }

    #[test]
    fn an_extended_piston_frees_the_quarter_of_its_head() {
        // Facing up (1) and extended (bit 3).
        assert_eq!(
            boxes_of(BlockState::new(33, 9), &[]),
            vec![b(0.0, 0.0, 0.0, 1.0, 0.75, 1.0)]
        );
        assert_eq!(boxes_of(BlockState::new(33, 1), &[]).len(), 1);
        assert_eq!(
            boxes_of(BlockState::new(34, 1), &[]).len(),
            2,
            "head and arm"
        );
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
