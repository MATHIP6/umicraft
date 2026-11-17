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
//!   towards the connected neighbours, 1.5 blocks tall. Walls (`BlockWall`)
//!   are their 1.5 tall bounds, gates (`BlockFenceGate`) a 1.5 tall plank
//!   when closed and nothing when open.
//!
//! Everything else is either a full cube or has no collision at all; the
//! partial-height blocks (plates, carpet, snow layers, cake, ...) use the
//! bounds of their MCP block class.

use glam::DVec3;
use umicraft_world::{BlockPos, BlockState, World};

use crate::piston::MovingBlock;
use crate::piston_shape;
use crate::selection::{fence_connections, horizontal_neighbours, pane_connects};

/// Read access to block states by world coordinate.
///
/// The trait exists so the game logic can be driven by any block provider in
/// tests without a loaded world. Implementations must answer [`BlockState::AIR`]
/// for positions outside the loaded world, exactly like
/// [`umicraft_world::World::block`] does.
pub trait BlockSource {
    /// The block state at the given world position.
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState;

    /// Whether the chunk column holding block `(x, z)` has arrived.
    ///
    /// The vanilla client keeps simulating the player over a missing column
    /// (its `ChunkProviderClient.chunkExists` is always true) but replaces the
    /// gravity there (MCP `EntityLivingBase.moveEntityWithHeading`). Test
    /// sources are fully loaded unless they say otherwise.
    fn is_column_loaded(&self, _x: i32, _z: i32) -> bool {
        true
    }

    /// Hands `f` the collision boxes of the solid entities (boats) near
    /// `area`, in world space. MCP `World.getCollidingBoundingBoxes` adds
    /// them after the blocks; test sources have none.
    fn for_each_entity_box(&self, _area: &Aabb, _f: &mut dyn FnMut(&Aabb)) {}

    /// The block a piston is moving into `(x, y, z)`, the client's
    /// `TileEntityPiston` there. A `piston_extension` (36) block without one
    /// has no collision box (`BlockPistonMoving.getCollisionBoundingBox`
    /// returns `null`); test sources have none.
    fn moving_block(&self, _x: i32, _y: i32, _z: i32) -> Option<MovingBlock> {
        None
    }
}

impl BlockSource for World {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        World::block(self, BlockPos::new(x, y, z))
    }

    fn is_column_loaded(&self, x: i32, z: i32) -> bool {
        self.chunk(umicraft_world::ChunkPos::new(x >> 4, z >> 4))
            .is_some()
    }
}

impl<T: BlockSource + ?Sized> BlockSource for &T {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        (**self).block(x, y, z)
    }

    fn is_column_loaded(&self, x: i32, z: i32) -> bool {
        (**self).is_column_loaded(x, z)
    }

    fn for_each_entity_box(&self, area: &Aabb, f: &mut dyn FnMut(&Aabb)) {
        (**self).for_each_entity_box(area, f);
    }

    fn moving_block(&self, x: i32, y: i32, z: i32) -> Option<MovingBlock> {
        (**self).moving_block(x, y, z)
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
        let overlaps = (mover.max.y > self.min.y)
            & (mover.min.y < self.max.y)
            & (mover.max.z > self.min.z)
            & (mover.min.z < self.max.z);
        if overlaps {
            if offset_x > 0.0 {
                if mover.max.x <= self.min.x {
                    let gap = self.min.x - mover.max.x;
                    if gap < offset_x {
                        return gap;
                    }
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
        let overlaps = (mover.max.x > self.min.x)
            & (mover.min.x < self.max.x)
            & (mover.max.z > self.min.z)
            & (mover.min.z < self.max.z);
        if overlaps {
            if offset_y > 0.0 {
                if mover.max.y <= self.min.y {
                    let gap = self.min.y - mover.max.y;
                    if gap < offset_y {
                        return gap;
                    }
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
        let overlaps = (mover.max.x > self.min.x)
            & (mover.min.x < self.max.x)
            & (mover.max.y > self.min.y)
            & (mover.min.y < self.max.y);
        if overlaps {
            if offset_z > 0.0 {
                if mover.max.z <= self.min.z {
                    let gap = self.min.z - mover.max.z;
                    if gap < offset_z {
                        return gap;
                    }
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
/// Cauldrons and hoppers contribute 5 boxes (1 base + 4 walls); a stair at
/// an inner corner three, fences and brewing stands two; nothing in 1.8
/// needs more.
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
        let mut boxes = [Aabb {
            min: DVec3::ZERO,
            max: DVec3::ZERO,
        }; MAX_BOXES];
        boxes[0] = first;
        boxes[1] = second;
        Self { boxes, count: 2 }
    }

    /// Three collision boxes.
    #[must_use]
    pub fn triple(first: Aabb, second: Aabb, third: Aabb) -> Self {
        let mut boxes = [Aabb {
            min: DVec3::ZERO,
            max: DVec3::ZERO,
        }; MAX_BOXES];
        boxes[0] = first;
        boxes[1] = second;
        boxes[2] = third;
        Self { boxes, count: 3 }
    }

    /// Five collision boxes (cauldron, hopper).
    #[must_use]
    pub fn five(b0: Aabb, b1: Aabb, b2: Aabb, b3: Aabb, b4: Aabb) -> Self {
        Self {
            boxes: [b0, b1, b2, b3, b4],
            count: 5,
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
    /// A fence: a post and arms towards its connections.
    Fence,
    /// A cobblestone wall: its bounds, 1.5 blocks tall.
    Wall,
    /// A fence gate: a plank across its facing, or nothing when open.
    FenceGate,
    /// A brewing stand: a rod on a thin base.
    BrewingStand,
    /// A `piston_extension`: the box of the block a piston is moving in.
    MovingPiston,
    /// A thin plate (glass pane, iron bars) that reaches its connected neighbours.
    Pane,
    /// A box that covers the full footprint and is `f64` blocks tall.
    Column(f64),
    /// A cube inset by one sixteenth on both horizontal axes and on top (cactus).
    Cactus,
    /// A snow layer, whose height depends on the metadata.
    SnowLayer,
    /// A chest, trapped chest or ender chest: 0.875 high, inset by a sixteenth
    /// except towards a neighbouring chest.
    Chest,
    /// A piston base, shorter when extended.
    PistonBase,
    /// A piston head: plate and rod.
    PistonHead,
    /// A skull, whose box follows its facing (the same as its selection box).
    Skull,
    /// A door: a thin board that turns with its facing, hinge and open state,
    /// read from both halves (the same as its selection box).
    Door,
    /// A block that collides with exactly its selection box: a ladder, a trapdoor,
    /// a lily pad, a cocoa pod, a flower pot, an anvil, a cake or a dragon egg.
    Bounds,
    /// The frame of the End portal: a base, and the eye of Ender when set.
    EndPortalFrame,
    /// A cauldron: a base plus four perimeter walls (5 boxes).
    Cauldron,
    /// A hopper: a base plus four perimeter walls (5 boxes).
    Hopper,
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
        85 | 113 | 188..=192 => Kind::Fence,
        139 => Kind::Wall,
        107 | 183..=187 => Kind::FenceGate,
        // Glass panes and iron bars.
        101 | 102 | 160 => Kind::Pane,
        // No collision box at all: `getCollisionBoundingBox` returns null in
        // `BlockBush` (the stems 104 and 105 included, with the saplings, the
        // flowers, the crops...), `BlockRailBase` (rails 27, 28, 66, 157),
        // `BlockBasePressurePlate` (70, 72, 147, 148), `BlockRedstoneWire` (55)
        // and `BlockPortal` (90). Their selection boxes are separate.
        27 | 28 | 55 | 66 | 70 | 72 | 90 | 104 | 105 | 147 | 148 | 157 => Kind::Empty,
        // Repeaters and comparators (`BlockRedstoneDiode` bounds) are an eighth
        // high, carpet (`BlockCarpet`) a sixteenth.
        93 | 94 | 149 | 150 => Kind::Column(0.125),
        171 => Kind::Column(0.0625),
        // Daylight detectors.
        151 | 178 => Kind::Column(0.375),
        // Bed, enchanting table and brewing stand.
        26 => Kind::Column(0.5625),
        116 => Kind::Column(0.75),
        117 => Kind::BrewingStand,
        // Cauldron (5 boxes: 0.3125 base + 4 walls).
        118 => Kind::Cauldron,
        // Hopper (5 boxes: 0.625 base + 4 walls).
        154 => Kind::Hopper,
        // Soul sand is 0.875 high (MCP `BlockSoulSand.java:23`).
        88 => Kind::Column(0.875),
        29 | 33 => Kind::PistonBase,
        36 => Kind::MovingPiston,
        34 => Kind::PistonHead,
        144 => Kind::Skull,
        54 | 130 | 146 => Kind::Chest,
        81 => Kind::Cactus,
        78 => Kind::SnowLayer,
        // Wooden, iron and the five other doors: `BlockDoor` collides with the
        // bounds of its state.
        64 | 71 | 193..=197 => Kind::Door,
        // Ladders (`BlockLadder`), trapdoors (`BlockTrapDoor`), lily pads
        // (`BlockLilyPad`), cocoa pods (`BlockCocoa`), flower pots
        // (`BlockFlowerPot`), anvils (`BlockAnvil`), cakes (`BlockCake`) and the
        // dragon egg (`BlockDragonEgg`) collide with their bounds, which are
        // their selection bounds.
        65 | 92 | 96 | 111 | 122 | 127 | 140 | 145 | 167 => Kind::Bounds,
        // The frame of the End portal.
        120 => Kind::EndPortalFrame,
        // Decorations and other blocks without a collision box.
        6 | 30 | 31 | 32 | 37 | 38 | 39 | 40 | 50 | 51 | 59 | 63 | 68 | 69 | 75 | 76 | 77 | 83
        | 106 | 115 | 119 | 131 | 132 | 141 | 142 | 143 | 175 | 176 | 177 => Kind::Empty,
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
        // `BlockCactus.getCollisionBoundingBox` insets the top by a sixteenth
        // too, unlike its 1.0 tall selection bounds.
        Kind::Cactus => BlockShape::single(Aabb::new(
            DVec3::new(0.0625, 0.0, 0.0625),
            DVec3::new(0.9375, 0.9375, 0.9375),
        )),
        // `BlockSnow.getCollisionBoundingBox` is `layers - 1` eighths high, one
        // layer under the selection box (`getBoundsForLayers`): a player
        // walks onto a single layer and steps up to five.
        Kind::SnowLayer => BlockShape::column(f64::from(state.metadata() & 7) / 8.0),
        Kind::Pane => pane_shape(state, world, pos),
        Kind::PistonBase => piston_shape::base_shape(state.metadata()),
        Kind::PistonHead => piston_shape::head_shape(state.metadata()),
        // `BlockSkull.getCollisionBoundingBox` applies
        // `setBlockBoundsBasedOnState` and returns the block bounds, which
        // are exactly the selection bounds.
        Kind::Skull => selection_shape(state, world, pos),
        // `BlockDoor.getCollisionBoundingBox` applies `setBlockBoundsBasedOnState`
        // (`combineMetadata` over the two halves) and returns the bounds, which
        // are exactly the selection bounds.
        Kind::Door => selection_shape(state, world, pos),
        // `BlockLadder` and `BlockCocoa` apply `setBlockBoundsBasedOnState` and
        // return the bounds; `BlockLilyPad` returns its bounds directly and the
        // flower pot and dragon egg have no override, so the default is their
        // constructor bounds. `BlockCake.getCollisionBoundingBox` builds the
        // same box as its bounds. `BlockAnvil` has no override either: the
        // bounds are the ones its facing sets (vanilla reuses the bounds the
        // last `setBlockBoundsBasedOnState` call left, which is this anvil's
        // own as soon as it was looked at or rendered).
        Kind::Bounds => selection_shape(state, world, pos),
        // `BlockEndPortalFrame.addCollisionBoxesToList`: the 0.8125 high base,
        // then the eye (bit 4 of the metadata) as a small box on top.
        Kind::EndPortalFrame => {
            let base = Aabb::new(DVec3::ZERO, DVec3::new(1.0, 0.8125, 1.0));
            if state.metadata() & 4 == 0 {
                BlockShape::single(base)
            } else {
                let eye = Aabb::new(
                    DVec3::new(0.3125, 0.8125, 0.3125),
                    DVec3::new(0.6875, 1.0, 0.6875),
                );
                BlockShape::pair(base, eye)
            }
        }
        // The chest blocks have no `getCollisionBoundingBox` of their own, so
        // the collision is their bounds, which are the selection bounds.
        Kind::Chest => selection_shape(state, world, pos),
        Kind::Stairs => stair_shape(state, world, pos),
        Kind::Fence => fence_shape(state, world, pos),
        Kind::Wall => wall_shape(state, world, pos),
        Kind::FenceGate => fence_gate_shape(state.metadata()),
        Kind::BrewingStand => brewing_stand_shape(),
        Kind::MovingPiston => moving_piston_shape(world, pos),
        Kind::Cauldron => cauldron_shape(),
        Kind::Hopper => hopper_shape(),
    }
}

/// The 5 collision boxes of a cauldron (MCP `net/minecraft/block/BlockCauldron.java:41-55`):
/// one bottom plate 0.3125 high (5/16), plus four perimeter walls 1.0 high and 0.125 thick.
fn cauldron_shape() -> BlockShape {
    let f = 0.125;
    BlockShape::five(
        // Bottom: [0.0, 0.0, 0.0] -> [1.0, 0.3125, 1.0]
        Aabb::new(DVec3::ZERO, DVec3::new(1.0, 0.3125, 1.0)),
        // West wall: [0.0, 0.0, 0.0] -> [0.125, 1.0, 1.0]
        Aabb::new(DVec3::ZERO, DVec3::new(f, 1.0, 1.0)),
        // North wall: [0.0, 0.0, 0.0] -> [1.0, 1.0, 0.125]
        Aabb::new(DVec3::ZERO, DVec3::new(1.0, 1.0, f)),
        // East wall: [0.875, 0.0, 0.0] -> [1.0, 1.0, 1.0]
        Aabb::new(DVec3::new(1.0 - f, 0.0, 0.0), DVec3::new(1.0, 1.0, 1.0)),
        // South wall: [0.0, 0.0, 0.875] -> [1.0, 1.0, 1.0]
        Aabb::new(DVec3::new(0.0, 0.0, 1.0 - f), DVec3::new(1.0, 1.0, 1.0)),
    )
}

/// The 5 collision boxes of a hopper (MCP `net/minecraft/block/BlockHopper.java:56-70`):
/// one base box 0.625 high (5/8), plus four perimeter walls 1.0 high and 0.125 thick.
fn hopper_shape() -> BlockShape {
    let f = 0.125;
    BlockShape::five(
        // Base / bottom: [0.0, 0.0, 0.0] -> [1.0, 0.625, 1.0]
        Aabb::new(DVec3::ZERO, DVec3::new(1.0, 0.625, 1.0)),
        // West wall: [0.0, 0.0, 0.0] -> [0.125, 1.0, 1.0]
        Aabb::new(DVec3::ZERO, DVec3::new(f, 1.0, 1.0)),
        // North wall: [0.0, 0.0, 0.0] -> [1.0, 1.0, 0.125]
        Aabb::new(DVec3::ZERO, DVec3::new(1.0, 1.0, f)),
        // East wall: [0.875, 0.0, 0.0] -> [1.0, 1.0, 1.0]
        Aabb::new(DVec3::new(1.0 - f, 0.0, 0.0), DVec3::new(1.0, 1.0, 1.0)),
        // South wall: [0.0, 0.0, 0.875] -> [1.0, 1.0, 1.0]
        Aabb::new(DVec3::new(0.0, 0.0, 1.0 - f), DVec3::new(1.0, 1.0, 1.0)),
    )
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
/// Vanilla targets the block's selection bounds, not its collision boxes, so
/// this delegates to [`crate::selection::selection_shape`]. Air and liquids
/// are never targetable.
#[must_use]
pub fn selection_shape<S: BlockSource + ?Sized>(
    state: BlockState,
    world: &S,
    pos: BlockPos,
) -> BlockShape {
    crate::selection::selection_shape(state, world, pos)
}

/// The collision boxes of a glass pane or iron bars.
///
/// MCP `BlockPane.addCollisionBoxesToList` (line 75): one thin box per axis.
/// An axis that is connected on both sides, or on neither side while the
/// pane is alone, spans the whole block; one connected side reaches the
/// middle towards that side; and an axis with no connection while the other
/// has one gets no box at all. The selection box (`setBlockBoundsBasedOnState`)
/// merges both axes instead, which is why it differs.
fn pane_shape<S: BlockSource + ?Sized>(state: BlockState, world: &S, pos: BlockPos) -> BlockShape {
    const LO: f64 = 0.4375;
    const HI: f64 = 0.5625;
    let [north, south, west, east] =
        horizontal_neighbours(world, pos).map(|other| pane_connects(state.id(), other));
    let any = north || south || west || east;
    let x_span = pane_span(west, east, any);
    let z_span = pane_span(north, south, any);
    let along_x =
        x_span.map(|(min, max)| Aabb::new(DVec3::new(min, 0.0, LO), DVec3::new(max, 1.0, HI)));
    let along_z =
        z_span.map(|(min, max)| Aabb::new(DVec3::new(LO, 0.0, min), DVec3::new(HI, 1.0, max)));
    match (along_x, along_z) {
        (Some(x), Some(z)) => BlockShape::pair(x, z),
        (Some(only), None) | (None, Some(only)) => BlockShape::single(only),
        (None, None) => BlockShape::EMPTY,
    }
}

/// The extent of a pane box along one axis, or `None` when it has no box.
fn pane_span(negative: bool, positive: bool, any_connection: bool) -> Option<(f64, f64)> {
    if (negative && positive) || !any_connection {
        Some((0.0, 1.0))
    } else if negative {
        Some((0.0, 0.5))
    } else if positive {
        Some((0.5, 1.0))
    } else {
        None
    }
}

impl StairFacing {
    /// The facing of a stair from its metadata (MCP `BlockStairs.getStateFromMeta`).
    fn from_metadata(metadata: u8) -> Self {
        match metadata & 3 {
            0 => Self::East,
            1 => Self::West,
            2 => Self::South,
            _ => Self::North,
        }
    }

    /// The `(x, z)` step towards this facing.
    const fn delta(self) -> (i32, i32) {
        match self {
            Self::West => (-1, 0),
            Self::East => (1, 0),
            Self::North => (0, -1),
            Self::South => (0, 1),
        }
    }
}

/// The facing and the half (`true` for the top) of the stair at a position,
/// `None` for any other block.
fn stair_at<S: BlockSource + ?Sized>(
    world: &S,
    x: i32,
    y: i32,
    z: i32,
) -> Option<(StairFacing, bool)> {
    let state = world.block(x, y, z);
    matches!(classify(state.id(), state.metadata()), Kind::Stairs).then(|| {
        (
            StairFacing::from_metadata(state.metadata()),
            state.metadata() & 4 != 0,
        )
    })
}

/// The one to three collision boxes of a stair block.
///
/// MCP `BlockStairs.addCollisionBoxesToList` (line 533): the base half
/// (`setBaseCollisionBounds`), then the step (`func_176306_h`), which shrinks
/// to a quarter when the stair ahead is turned sideways (outer corner), and
/// when it does not, a quarter behind it (`func_176304_i`) when the stair
/// behind is turned sideways (inner corner). Both look at the stair of the
/// same half only, and ignore a corner that a stair like this one already
/// fills (`isSameStair`).
fn stair_shape<S: BlockSource + ?Sized>(state: BlockState, world: &S, pos: BlockPos) -> BlockShape {
    let me = (
        StairFacing::from_metadata(state.metadata()),
        state.metadata() & 4 != 0,
    );
    let (facing, top) = me;
    let (fx, fz) = facing.delta();
    let (base_y, step_y) = if top {
        ((0.5, 1.0), (0.0, 0.5))
    } else {
        ((0.0, 0.5), (0.5, 1.0))
    };

    let at = |dx: i32, dz: i32| stair_at(world, pos.x + dx, pos.y, pos.z + dz);
    let same = |dx: i32, dz: i32| at(dx, dz) == Some(me);
    // The facing of the stair beside, when it is of the same half and turned
    // sideways to this one.
    let turned = |dx: i32, dz: i32| {
        at(dx, dz)
            .filter(|&(other, half)| {
                let (ox, oz) = other.delta();
                half == top && ox * fx + oz * fz == 0
            })
            .map(|(other, _)| other.delta())
    };
    // The box over the half of the block that `toward` points to, narrowed to
    // the half that `side` points to.
    let region = |toward: (i32, i32), side: Option<(i32, i32)>, (y0, y1): (f64, f64)| {
        let mut min = DVec3::new(0.0, y0, 0.0);
        let mut max = DVec3::new(1.0, y1, 1.0);
        for (dx, dz) in std::iter::once(toward).chain(side) {
            if dx > 0 {
                min.x = 0.5;
            } else if dx < 0 {
                max.x = 0.5;
            }
            if dz > 0 {
                min.z = 0.5;
            } else if dz < 0 {
                max.z = 0.5;
            }
        }
        Aabb::new(min, max)
    };

    let base = region((0, 0), None, base_y);
    let outer = turned(fx, fz).filter(|&(gx, gz)| !same(-gx, -gz));
    let step = region((fx, fz), outer, step_y);
    let inner = if outer.is_none() {
        turned(-fx, -fz)
            .filter(|&(gx, gz)| !same(gx, gz))
            .map(|side| region((-fx, -fz), Some(side), step_y))
    } else {
        None
    };
    match inner {
        Some(corner) => BlockShape::triple(base, step, corner),
        None => BlockShape::pair(base, step),
    }
}

/// The collision boxes of a fence.
///
/// MCP `BlockFence.addCollisionBoxesToList` (line 50) emits a post-and-arm
/// box for the north/south axis when it has a connection there, then one for
/// the west/east axis when it has a connection there or none on the other
/// axis (the lone post).
fn fence_shape<S: BlockSource + ?Sized>(state: BlockState, world: &S, pos: BlockPos) -> BlockShape {
    const LO: f64 = 0.375;
    const HI: f64 = 0.625;
    let [north, south, west, east] = fence_connections(state, world, pos);
    let north_south = Aabb::new(
        DVec3::new(LO, 0.0, if north { 0.0 } else { LO }),
        DVec3::new(HI, FENCE_HEIGHT, if south { 1.0 } else { HI }),
    );
    let west_east = Aabb::new(
        DVec3::new(if west { 0.0 } else { LO }, 0.0, LO),
        DVec3::new(if east { 1.0 } else { HI }, FENCE_HEIGHT, HI),
    );
    match (north || south, west || east) {
        (true, true) => BlockShape::pair(north_south, west_east),
        (true, false) => BlockShape::single(north_south),
        (false, _) => BlockShape::single(west_east),
    }
}

/// How tall fences, walls and gates are for collisions (the `1.5F` of
/// `BlockFence`, `BlockWall` and `BlockFenceGate`).
const FENCE_HEIGHT: f64 = 1.5;

/// The collision box of a cobblestone wall.
///
/// MCP `BlockWall.getCollisionBoundingBox` applies
/// `setBlockBoundsBasedOnState`, which are the selection bounds, and raises
/// `maxY` to 1.5.
fn wall_shape<S: BlockSource + ?Sized>(state: BlockState, world: &S, pos: BlockPos) -> BlockShape {
    let bounds = selection_shape(state, world, pos).boxes()[0];
    BlockShape::single(Aabb::new(
        bounds.min,
        DVec3::new(bounds.max.x, FENCE_HEIGHT, bounds.max.z),
    ))
}

/// The collision box of a fence gate from its metadata.
///
/// MCP `BlockFenceGate.getCollisionBoundingBox`: nothing when open (bit 2),
/// otherwise a 0.25 thick plank, 1.5 tall, across the facing: along X for a
/// gate facing north or south (`getHorizontal(meta & 3)` is even), along Z
/// for one facing west or east.
fn fence_gate_shape(metadata: u8) -> BlockShape {
    if metadata & 4 != 0 {
        return BlockShape::EMPTY;
    }
    let plank = if metadata & 1 == 0 {
        Aabb::new(
            DVec3::new(0.0, 0.0, 0.375),
            DVec3::new(1.0, FENCE_HEIGHT, 0.625),
        )
    } else {
        Aabb::new(
            DVec3::new(0.375, 0.0, 0.0),
            DVec3::new(0.625, FENCE_HEIGHT, 1.0),
        )
    };
    BlockShape::single(plank)
}

/// The collision box of a `piston_extension` block, from the block moving
/// into it.
///
/// MCP `BlockPistonMoving.getCollisionBoundingBox` and `getBoundingBox`: the
/// collision box of the moved block at this position, stretched back along
/// the facing by the progress of the previous tick (`getProgress(0.0F)`,
/// turned to `1 - progress` while extending). Only the face on the side the
/// facing points to moves: a box pushed east keeps its west face and pulls
/// its east face back, which leaves a flat (still blocking) box at the start
/// of an extension. A moved block without a box, air, another moving block or
/// no animation at all give nothing.
///
/// `getCollisionBoundingBox` is a single box; for the blocks that collide
/// through a box list (stairs, fences, cauldrons...) the reference returns
/// the bounds the block last set, which this approximates with the
/// selection bounds.
fn moving_piston_shape<S: BlockSource + ?Sized>(world: &S, pos: BlockPos) -> BlockShape {
    let Some(moving) = world.moving_block(pos.x, pos.y, pos.z) else {
        return BlockShape::EMPTY;
    };
    if moving.state.is_air() || moving.state.id() == 36 {
        return BlockShape::EMPTY;
    }
    let shape = if adds_a_box_list(classify(moving.state.id(), moving.state.metadata())) {
        selection_shape(moving.state, world, pos)
    } else {
        shape_of(moving.state, world, pos)
    };
    let [moved] = shape.boxes() else {
        return BlockShape::EMPTY;
    };
    let moved = *moved;
    let progress = if moving.extending {
        1.0 - moving.last_progress
    } else {
        moving.last_progress
    };
    let mut min = moved.min.to_array();
    let mut max = moved.max.to_array();
    for axis in 0..3 {
        // `(double)((float)offset * progress)`, exact for a unit offset.
        let shift = f64::from(moving.direction[axis] as f32 * progress);
        if moving.direction[axis] < 0 {
            min[axis] -= shift;
        } else {
            max[axis] -= shift;
        }
    }
    BlockShape::single(Aabb::new(DVec3::from_array(min), DVec3::from_array(max)))
}

/// Whether the block collides through `addCollisionBoxesToList` (fences,
/// panes, stairs, cauldrons, hoppers, brewing stands, End portal frames,
/// piston heads) rather than a single `getCollisionBoundingBox`, whose
/// default is the block bounds.
fn adds_a_box_list(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Fence
            | Kind::Pane
            | Kind::Stairs
            | Kind::Cauldron
            | Kind::Hopper
            | Kind::BrewingStand
            | Kind::EndPortalFrame
            | Kind::PistonHead
    )
}

/// The two collision boxes of a brewing stand.
///
/// MCP `BlockBrewingStand.addCollisionBoxesToList`: the rod
/// `(0.4375, 0, 0.4375)-(0.5625, 0.875, 0.5625)`, then the base
/// `setBlockBoundsForItemRender`, `(0, 0, 0)-(1, 0.125, 1)`.
fn brewing_stand_shape() -> BlockShape {
    BlockShape::pair(
        Aabb::new(
            DVec3::new(0.4375, 0.0, 0.4375),
            DVec3::new(0.5625, 0.875, 0.5625),
        ),
        Aabb::new(DVec3::ZERO, DVec3::new(1.0, 0.125, 1.0)),
    )
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

    /// Regression test: wool was listed among the blocks without a collision
    /// box, so the player fell through every wool floor (a public server's
    /// lobby, 2026-11-02) and the anti-cheat set the player back.
    #[test]
    fn wool_is_a_full_cube_of_every_colour() {
        let world = TestBlocks::default();
        for colour in 0..16 {
            let shape = shape_of(BlockState::new(35, colour), &world, BlockPos::new(0, 0, 0));
            assert_eq!(shape.boxes().len(), 1, "colour {colour}");
            assert_eq!(shape.boxes()[0].max, DVec3::ONE);
        }
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

    /// Builds a world of oak stairs `(x, z, metadata)` at y = 0 and returns the
    /// boxes of the first one as `(min, max)` corners.
    fn stair_boxes(stairs: &[(i32, i32, u8)]) -> Vec<([f64; 3], [f64; 3])> {
        let mut world = TestBlocks::default();
        for &(x, z, metadata) in stairs {
            world
                .blocks
                .insert((x, 0, z), BlockState::new(53, metadata));
        }
        let (x, z, metadata) = stairs[0];
        shape_of(
            BlockState::new(53, metadata),
            &world,
            BlockPos::new(x, 0, z),
        )
        .boxes()
        .iter()
        .map(|b| (b.min.to_array(), b.max.to_array()))
        .collect()
    }

    #[test]
    fn a_stair_beside_a_sideways_stair_ahead_is_an_outer_corner() {
        // East stair with a north stair ahead (x = 1) and nothing south of it:
        // the step shrinks to its north quarter.
        let boxes = stair_boxes(&[(0, 0, 0), (1, 0, 3)]);
        assert_eq!(boxes.len(), 2);
        assert_eq!(boxes[1], ([0.5, 0.5, 0.0], [1.0, 1.0, 0.5]));
        // A stair like this one to the south makes it a straight run again.
        let boxes = stair_boxes(&[(0, 0, 0), (1, 0, 3), (0, 1, 0)]);
        assert_eq!(boxes[1], ([0.5, 0.5, 0.0], [1.0, 1.0, 1.0]));
    }

    #[test]
    fn a_stair_with_a_sideways_stair_behind_is_an_inner_corner() {
        // East stair with a south stair behind it (x = -1): a third box fills
        // the south quarter of the back half.
        let boxes = stair_boxes(&[(0, 0, 0), (-1, 0, 2)]);
        assert_eq!(boxes.len(), 3);
        assert_eq!(boxes[1], ([0.5, 0.5, 0.0], [1.0, 1.0, 1.0]));
        assert_eq!(boxes[2], ([0.0, 0.5, 0.5], [0.5, 1.0, 1.0]));
    }

    #[test]
    fn upside_down_stairs_mirror_the_corners_and_ignore_the_other_half() {
        // Top half (metadata 4): base on top, step below.
        let boxes = stair_boxes(&[(0, 0, 4), (-1, 0, 6)]);
        assert_eq!(boxes.len(), 3);
        assert_eq!(boxes[0], ([0.0, 0.5, 0.0], [1.0, 1.0, 1.0]));
        assert_eq!(boxes[2], ([0.0, 0.0, 0.5], [0.5, 0.5, 1.0]));
        // A neighbour of the other half does not make a corner.
        assert_eq!(stair_boxes(&[(0, 0, 4), (-1, 0, 2)]).len(), 2);
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
        // Only the north/south box: the west/east one is skipped when that
        // axis has no connection and the other has one.
        assert_eq!(fence.boxes().len(), 1);
        // The north arm reaches the edge of the block.
        assert_eq!(fence.boxes()[0].min.z, 0.0);
        assert_eq!(fence.boxes()[0].max.z, 0.625);
    }

    /// The `(min x, max x, min z, max z)` footprint of every box of the oak
    /// fence at the origin, with `east` as its only neighbour (at x = 1).
    fn fence_footprint(east: BlockState) -> Vec<[f64; 4]> {
        let world = TestBlocks::with(&[(1, 0, 0, east)]);
        shape_of(BlockState::new(85, 0), &world, BlockPos::new(0, 0, 0))
            .boxes()
            .iter()
            .map(|b| [b.min.x, b.max.x, b.min.z, b.max.z])
            .collect()
    }

    /// Regression test: the collision fence connected to every block the
    /// collision table calls a cube (glass, leaves, glowstone, a lit
    /// pumpkin...) and to walls, so a player pressed against such a fence
    /// stopped 0.375 short of where the server put them, and the anti-cheat
    /// logged it. `BlockFence.canConnectTo` only takes opaque full cubes that
    /// are not gourds, fences of its material and gates.
    #[test]
    fn a_fence_connects_only_to_what_vanilla_connects_it_to() {
        let post = vec![[0.375, 0.625, 0.375, 0.625]];
        let arm = vec![[0.375, 1.0, 0.375, 0.625]];
        let cases = [
            (1, true),    // stone
            (20, false),  // glass
            (18, false),  // leaves
            (89, false),  // glowstone
            (46, false),  // TNT
            (86, false),  // pumpkin
            (91, false),  // lit pumpkin
            (103, false), // melon
            (166, false), // barrier
            (139, false), // cobblestone wall
            (113, false), // nether brick fence: another material
            (152, true),  // redstone block
            (120, true),  // End portal frame
            (188, true),  // spruce fence
            (107, true),  // oak gate
            (186, true),  // dark oak gate
        ];
        for (id, connects) in cases {
            let expected = if connects { &arm } else { &post };
            assert_eq!(
                &fence_footprint(BlockState::new(id, 0)),
                expected,
                "id {id}"
            );
        }
    }

    #[test]
    fn a_nether_brick_fence_connects_to_nether_brick_fences_only() {
        let world = TestBlocks::with(&[
            (0, 0, -1, BlockState::new(113, 0)),
            (1, 0, 0, BlockState::new(85, 0)),
        ]);
        let fence = shape_of(BlockState::new(113, 0), &world, BlockPos::new(0, 0, 0));
        assert_eq!(fence.boxes().len(), 1);
        assert_eq!(fence.boxes()[0].min.z, 0.0);
        assert_eq!(fence.boxes()[0].max.x, 0.625);
    }

    /// Regression test: gates were fences, so an open gate still blocked the
    /// player and a closed one was a post with arms.
    #[test]
    fn a_closed_gate_is_a_plank_across_its_facing_and_an_open_one_is_empty() {
        let world = TestBlocks::default();
        let pos = BlockPos::new(0, 0, 0);
        for id in [107, 183, 184, 185, 186, 187] {
            // Facing south (0) or north (2): the plank runs along X.
            for meta in [0, 2] {
                let gate = shape_of(BlockState::new(id, meta), &world, pos);
                assert_eq!(
                    gate.boxes(),
                    &[Aabb::new(
                        DVec3::new(0.0, 0.0, 0.375),
                        DVec3::new(1.0, 1.5, 0.625)
                    )]
                );
            }
            // Facing west (1) or east (3): the plank runs along Z.
            for meta in [1, 3] {
                let gate = shape_of(BlockState::new(id, meta), &world, pos);
                assert_eq!(
                    gate.boxes(),
                    &[Aabb::new(
                        DVec3::new(0.375, 0.0, 0.0),
                        DVec3::new(0.625, 1.5, 1.0)
                    )]
                );
            }
            for meta in 4..8 {
                assert!(shape_of(BlockState::new(id, meta), &world, pos).is_empty());
            }
        }
    }

    /// Regression test: walls collided as fences (a 0.25 wide post) instead
    /// of their 0.5 wide bounds.
    #[test]
    fn a_wall_collides_with_its_bounds_raised_to_one_and_a_half() {
        let pos = BlockPos::new(0, 0, 0);
        let wall = BlockState::new(139, 0);
        let alone = shape_of(wall, &TestBlocks::default(), pos);
        assert_eq!(
            alone.boxes(),
            &[Aabb::new(
                DVec3::new(0.25, 0.0, 0.25),
                DVec3::new(0.75, 1.5, 0.75)
            )]
        );
        // A straight north/south run is narrower (and lower when selected).
        let run = TestBlocks::with(&[(0, 0, -1, wall), (0, 0, 1, wall)]);
        assert_eq!(
            shape_of(wall, &run, pos).boxes(),
            &[Aabb::new(
                DVec3::new(0.3125, 0.0, 0.0),
                DVec3::new(0.6875, 1.5, 1.0)
            )]
        );
    }

    /// Regression test: the anvil boxes were swapped, so a player pressed
    /// against the narrow side of an anvil stood 0.125 inside it for the
    /// server. `getHorizontal(meta & 3)` is west or east for odd values,
    /// whose axis is X and whose bounds are inset on Z.
    #[test]
    fn an_anvil_is_narrow_across_its_facing() {
        let world = TestBlocks::default();
        let pos = BlockPos::new(0, 0, 0);
        for damage in [0, 4, 8] {
            for meta in [1, 3] {
                let anvil = shape_of(BlockState::new(145, meta | damage), &world, pos);
                assert_eq!(
                    anvil.boxes(),
                    &[Aabb::new(
                        DVec3::new(0.0, 0.0, 0.125),
                        DVec3::new(1.0, 1.0, 0.875)
                    )]
                );
            }
            for meta in [0, 2] {
                let anvil = shape_of(BlockState::new(145, meta | damage), &world, pos);
                assert_eq!(
                    anvil.boxes(),
                    &[Aabb::new(
                        DVec3::new(0.125, 0.0, 0.0),
                        DVec3::new(0.875, 1.0, 1.0)
                    )]
                );
            }
        }
    }

    /// Regression test: the brewing stand was a 0.75 tall column.
    #[test]
    fn a_brewing_stand_is_a_rod_on_a_thin_base() {
        let stand = shape_of(
            BlockState::new(117, 0),
            &TestBlocks::default(),
            BlockPos::new(0, 0, 0),
        );
        assert_eq!(
            stand.boxes(),
            &[
                Aabb::new(
                    DVec3::new(0.4375, 0.0, 0.4375),
                    DVec3::new(0.5625, 0.875, 0.5625)
                ),
                Aabb::new(DVec3::ZERO, DVec3::new(1.0, 0.125, 1.0)),
            ]
        );
    }

    /// Regression test: the cake was a full-footprint column, the cactus 1.0
    /// tall and the dragon egg a full cube.
    #[test]
    fn cakes_cacti_and_dragon_eggs_are_inset() {
        let world = TestBlocks::default();
        let pos = BlockPos::new(0, 0, 0);
        let boxes = |id, meta| shape_of(BlockState::new(id, meta), &world, pos).boxes()[0];
        assert_eq!(
            boxes(92, 0),
            Aabb::new(
                DVec3::new(0.0625, 0.0, 0.0625),
                DVec3::new(0.9375, 0.5, 0.9375)
            )
        );
        // Three bites: (1 + 3 * 2) / 16 from the west.
        assert_eq!(boxes(92, 3).min.x, 0.4375);
        assert_eq!(
            boxes(81, 0),
            Aabb::new(
                DVec3::new(0.0625, 0.0, 0.0625),
                DVec3::new(0.9375, 0.9375, 0.9375)
            )
        );
        assert_eq!(
            boxes(122, 0),
            Aabb::new(
                DVec3::new(0.0625, 0.0, 0.0625),
                DVec3::new(0.9375, 1.0, 0.9375)
            )
        );
    }

    /// Regression test: panes connected through the normal-cube table, which
    /// leaves out leaves, glowstone and the redstone block and keeps the
    /// mob spawner and slime, unlike `Block.fullBlock`.
    #[test]
    fn panes_connect_to_full_blocks_as_vanilla_counts_them() {
        let pos = BlockPos::new(0, 0, 0);
        let pane = BlockState::new(102, 0);
        let west_arm = |id| {
            let world = TestBlocks::with(&[(-1, 0, 0, BlockState::new(id, 0))]);
            shape_of(pane, &world, pos).boxes().len() == 1
        };
        for id in [1, 18, 20, 46, 89, 152, 161, 169, 101, 160] {
            assert!(west_arm(id), "id {id} connects");
        }
        for id in [52, 79, 120, 165, 166, 138] {
            assert!(!west_arm(id), "id {id} does not connect");
        }
    }

    #[test]
    fn plants_rails_plates_wire_and_portals_have_no_collision_box() {
        let world = TestBlocks::default();
        let pos = BlockPos::new(0, 0, 0);
        // The ids of every class whose `getCollisionBoundingBox` is null in
        // the reference source (`BlockBush` and its subclasses, the rails, the
        // pressure plates, the redstone wire, the portal), stems included.
        let ids = [
            6, 27, 28, 30, 31, 32, 37, 38, 39, 40, 55, 59, 66, 70, 72, 83, 90, 104, 105, 106, 115,
            141, 142, 147, 148, 157, 175,
        ];
        for id in ids {
            for metadata in [0, 7, 15] {
                let shape = shape_of(BlockState::new(id, metadata), &world, pos);
                assert!(shape.is_empty(), "block {id}:{metadata} must not collide");
            }
        }
    }

    /// A door of `id`: the lower half with `lower` metadata at the origin, the
    /// upper half (bit 8, hinge bit 1 for the left hinge) above it.
    fn door_world(id: u16, lower: u8, hinge_left: bool) -> TestBlocks {
        TestBlocks::with(&[
            (0, 0, 0, BlockState::new(id, lower)),
            (0, 1, 0, BlockState::new(id, 8 | u8::from(hinge_left))),
        ])
    }

    #[test]
    fn every_door_block_collides_with_a_board_a_sixteenth_and_a_half_thick() {
        let pos = BlockPos::new(0, 0, 0);
        for id in [64, 71, 193, 194, 195, 196, 197] {
            // Closed, facing east: a board on the west side of the block.
            let world = door_world(id, 0, false);
            for half in [BlockPos::new(0, 0, 0), BlockPos::new(0, 1, 0)] {
                let state = world.block(half.x, half.y, half.z);
                let door = shape_of(state, &world, half);
                assert_eq!(door.boxes().len(), 1, "door {id}");
                assert_eq!(door.boxes()[0].max.x, 0.1875, "door {id}");
                assert_eq!(door.boxes()[0].max.y, 1.0);
            }
            let _ = pos;
        }
    }

    #[test]
    fn an_open_door_swings_to_the_side_its_hinge_says() {
        let pos = BlockPos::new(0, 0, 0);
        let boards = |hinge_left| {
            // Open (bit 4), facing east.
            let world = door_world(64, 4, hinge_left);
            shape_of(world.block(0, 0, 0), &world, pos).boxes()[0]
        };
        let right = boards(false);
        let left = boards(true);
        // Right hinge: along the north side; left hinge: along the south side.
        assert_eq!((right.min.z, right.max.z), (0.0, 0.1875));
        assert_eq!((left.min.z, left.max.z), (0.8125, 1.0));
        assert_eq!((right.min.x, right.max.x), (0.0, 1.0));
    }

    #[test]
    fn ladders_lily_pads_cocoa_and_flower_pots_collide_with_their_selection_box() {
        let world = TestBlocks::default();
        let pos = BlockPos::new(0, 0, 0);
        for (id, metadata) in [(65, 2), (65, 5), (111, 0), (127, 0), (127, 9), (140, 0)] {
            let state = BlockState::new(id, metadata);
            let collision = shape_of(state, &world, pos);
            let selection = selection_shape(state, &world, pos);
            assert!(!collision.is_empty(), "block {id}:{metadata}");
            assert_eq!(collision, selection, "block {id}:{metadata}");
        }
    }

    #[test]
    fn trapdoors_collide_with_all_six_configurations() {
        let world = TestBlocks::default();
        let pos = BlockPos::new(0, 0, 0);
        // Wooden trapdoor (96) and iron trapdoor (167).
        for id in [96, 167] {
            // 1. Closed, bottom half (meta 0): 0.1875 high on the floor.
            let closed_bottom = shape_of(BlockState::new(id, 0), &world, pos);
            assert_eq!(closed_bottom.boxes().len(), 1);
            assert_eq!(
                closed_bottom.boxes()[0],
                Aabb::new(DVec3::ZERO, DVec3::new(1.0, 0.1875, 1.0))
            );

            // 2. Closed, top half (meta 8): 0.1875 thick under the ceiling.
            let closed_top = shape_of(BlockState::new(id, 8), &world, pos);
            assert_eq!(closed_top.boxes().len(), 1);
            assert_eq!(
                closed_top.boxes()[0],
                Aabb::new(DVec3::new(0.0, 0.8125, 0.0), DVec3::ONE)
            );

            // 3. Open, facing north (meta 4): wall on south side.
            let open_north = shape_of(BlockState::new(id, 4), &world, pos);
            assert_eq!(open_north.boxes().len(), 1);
            assert_eq!(
                open_north.boxes()[0],
                Aabb::new(DVec3::new(0.0, 0.0, 0.8125), DVec3::ONE)
            );

            // 4. Open, facing south (meta 5): wall on north side.
            let open_south = shape_of(BlockState::new(id, 5), &world, pos);
            assert_eq!(open_south.boxes().len(), 1);
            assert_eq!(
                open_south.boxes()[0],
                Aabb::new(DVec3::ZERO, DVec3::new(1.0, 1.0, 0.1875))
            );

            // 5. Open, facing west (meta 6): wall on east side.
            let open_west = shape_of(BlockState::new(id, 6), &world, pos);
            assert_eq!(open_west.boxes().len(), 1);
            assert_eq!(
                open_west.boxes()[0],
                Aabb::new(DVec3::new(0.8125, 0.0, 0.0), DVec3::ONE)
            );

            // 6. Open, facing east (meta 7): wall on west side.
            let open_east = shape_of(BlockState::new(id, 7), &world, pos);
            assert_eq!(open_east.boxes().len(), 1);
            assert_eq!(
                open_east.boxes()[0],
                Aabb::new(DVec3::ZERO, DVec3::new(0.1875, 1.0, 1.0))
            );
        }
    }

    #[test]
    fn soul_sand_collision_is_seven_eighths_high() {
        let world = TestBlocks::default();
        let pos = BlockPos::new(0, 0, 0);
        let soul_sand = shape_of(BlockState::new(88, 0), &world, pos);
        assert_eq!(soul_sand.boxes().len(), 1);
        assert_eq!(
            soul_sand.boxes()[0],
            Aabb::new(DVec3::ZERO, DVec3::new(1.0, 0.875, 1.0))
        );
    }

    #[test]
    fn cauldrons_and_hoppers_have_five_boxes() {
        let world = TestBlocks::default();
        let pos = BlockPos::new(0, 0, 0);

        // Cauldron (id 118): base 0.3125 high + 4 walls 0.125 thick and 1.0 high.
        let cauldron = shape_of(BlockState::new(118, 0), &world, pos);
        assert_eq!(cauldron.boxes().len(), 5);
        let cb = cauldron.boxes();
        assert_eq!(cb[0], Aabb::new(DVec3::ZERO, DVec3::new(1.0, 0.3125, 1.0)));
        assert_eq!(cb[1], Aabb::new(DVec3::ZERO, DVec3::new(0.125, 1.0, 1.0))); // West wall
        assert_eq!(cb[2], Aabb::new(DVec3::ZERO, DVec3::new(1.0, 1.0, 0.125))); // North wall
        assert_eq!(cb[3], Aabb::new(DVec3::new(0.875, 0.0, 0.0), DVec3::ONE)); // East wall
        assert_eq!(cb[4], Aabb::new(DVec3::new(0.0, 0.0, 0.875), DVec3::ONE)); // South wall

        // Hopper (id 154): base 0.625 high + 4 walls 0.125 thick and 1.0 high.
        let hopper = shape_of(BlockState::new(154, 0), &world, pos);
        assert_eq!(hopper.boxes().len(), 5);
        let hb = hopper.boxes();
        assert_eq!(hb[0], Aabb::new(DVec3::ZERO, DVec3::new(1.0, 0.625, 1.0)));
        assert_eq!(hb[1], Aabb::new(DVec3::ZERO, DVec3::new(0.125, 1.0, 1.0)));
        assert_eq!(hb[2], Aabb::new(DVec3::ZERO, DVec3::new(1.0, 1.0, 0.125)));
        assert_eq!(hb[3], Aabb::new(DVec3::new(0.875, 0.0, 0.0), DVec3::ONE));
        assert_eq!(hb[4], Aabb::new(DVec3::new(0.0, 0.0, 0.875), DVec3::ONE));
    }

    #[test]
    fn a_ladder_is_a_board_an_eighth_thick_against_its_wall() {
        let world = TestBlocks::default();
        // Facing 2 (north): the board lies against the south side.
        let ladder = shape_of(BlockState::new(65, 2), &world, BlockPos::new(0, 0, 0));
        let board = ladder.boxes()[0];
        assert_eq!(board.max.y, 1.0);
        assert_eq!(board.max.z - board.min.z, 0.125);
    }

    #[test]
    fn a_lily_pad_is_a_film_and_a_flower_pot_a_small_block() {
        let world = TestBlocks::default();
        let pos = BlockPos::new(0, 0, 0);
        let pad = shape_of(BlockState::new(111, 0), &world, pos).boxes()[0];
        assert_eq!(pad.max.y, 0.015_625);
        let pot = shape_of(BlockState::new(140, 0), &world, pos).boxes()[0];
        assert_eq!(pot.max.y, 0.375);
        assert_eq!((pot.min.x, pot.max.x), (0.3125, 0.6875));
    }

    #[test]
    fn an_end_portal_frame_has_a_base_and_an_eye_when_it_holds_one() {
        let world = TestBlocks::default();
        let pos = BlockPos::new(0, 0, 0);
        let empty = shape_of(BlockState::new(120, 0), &world, pos);
        assert_eq!(empty.boxes().len(), 1);
        assert_eq!(empty.boxes()[0].max.y, 0.8125);
        let eye = shape_of(BlockState::new(120, 4), &world, pos);
        assert_eq!(eye.boxes().len(), 2);
        assert_eq!(eye.boxes()[1].min.y, 0.8125);
        assert_eq!(eye.boxes()[1].max.y, 1.0);
        assert_eq!(
            (eye.boxes()[1].min.x, eye.boxes()[1].max.x),
            (0.3125, 0.6875)
        );
    }

    #[test]
    fn a_player_cannot_walk_through_a_closed_door() {
        // The shape is what the collision uses: no longer empty.
        let world = door_world(64, 0, false);
        assert!(!shape_of(world.block(0, 0, 0), &world, BlockPos::new(0, 0, 0)).is_empty());
    }

    #[test]
    fn the_stems_of_pumpkins_and_melons_do_not_block_the_player() {
        // The bug: 104 and 105 fell through to the full cube.
        let world = TestBlocks::default();
        for id in [104, 105] {
            let stem = BlockState::new(id, 7);
            assert!(shape_of(stem, &world, BlockPos::new(0, 0, 0)).is_empty());
            // Still targetable, so the stem can be broken.
            assert!(!selection_shape(stem, &world, BlockPos::new(0, 0, 0)).is_empty());
        }
    }

    #[test]
    fn repeaters_and_comparators_are_an_eighth_high_and_carpet_a_sixteenth() {
        let world = TestBlocks::default();
        let height = |id| {
            shape_of(BlockState::new(id, 0), &world, BlockPos::new(0, 0, 0)).boxes()[0]
                .max
                .y
        };
        for id in [93, 94, 149, 150] {
            assert_eq!(height(id), 0.125, "block {id}");
        }
        assert_eq!(height(171), 0.0625);
    }

    #[test]
    fn a_snow_layer_collides_one_layer_lower_than_it_is_selected() {
        let world = TestBlocks::default();
        let pos = BlockPos::new(0, 0, 0);
        for layers in 1..=8_u8 {
            let state = BlockState::new(78, layers - 1);
            let collision = shape_of(state, &world, pos);
            let top = collision.boxes().first().map_or(0.0, |b| b.max.y);
            assert_eq!(top, f64::from(layers - 1) / 8.0, "{layers} layers");
            let selected = selection_shape(state, &world, pos);
            assert_eq!(selected.boxes()[0].max.y, f64::from(layers) / 8.0);
        }
    }

    #[test]
    fn a_chest_is_an_inset_box_seven_eighths_high_not_a_full_cube() {
        let world = TestBlocks::default();
        for id in [54, 130, 146] {
            let shape = shape_of(BlockState::new(id, 2), &world, BlockPos::new(0, 0, 0));
            let chest = shape.boxes()[0];
            assert_eq!(chest.max.y, 0.875, "chest {id}");
            assert_eq!((chest.min.x, chest.max.x), (0.0625, 0.9375), "chest {id}");
        }
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
    fn skulls_collide_with_their_facing_dependent_box() {
        let world = TestBlocks::default();
        let pos = BlockPos::new(0, 0, 0);
        let floor = shape_of(BlockState::new(144, 1), &world, pos);
        assert_eq!(floor.boxes().len(), 1);
        assert_eq!(floor.boxes()[0].max.y, 0.5);
        let wall = shape_of(BlockState::new(144, 2), &world, pos);
        assert_eq!(wall.boxes()[0].min.z, 0.5);
        // The no-drop bit (8) does not change the facing.
        assert_eq!(shape_of(BlockState::new(144, 9), &world, pos), floor);
    }

    #[test]
    fn panes_follow_their_connections() {
        let pos = BlockPos::new(0, 0, 0);
        let pane = BlockState::new(102, 0);
        let alone = shape_of(pane, &TestBlocks::default(), pos);
        assert_eq!(alone.boxes().len(), 2);
        // A neighbour to the west only: a half box towards it, nothing on Z.
        let west = TestBlocks::with(&[(-1, 0, 0, BlockState::new(1, 0))]);
        let shape = shape_of(pane, &west, pos);
        assert_eq!(shape.boxes().len(), 1);
        assert_eq!((shape.boxes()[0].min.x, shape.boxes()[0].max.x), (0.0, 0.5));
        // Both west and east: one full-length box.
        let both = TestBlocks::with(&[
            (-1, 0, 0, BlockState::new(1, 0)),
            (1, 0, 0, BlockState::new(1, 0)),
        ]);
        let shape = shape_of(pane, &both, pos);
        assert_eq!(shape.boxes().len(), 1);
        assert_eq!((shape.boxes()[0].min.x, shape.boxes()[0].max.x), (0.0, 1.0));
        // A corner (west and south) gives two half boxes.
        let corner = TestBlocks::with(&[
            (-1, 0, 0, BlockState::new(1, 0)),
            (0, 0, 1, BlockState::new(1, 0)),
        ]);
        assert_eq!(shape_of(pane, &corner, pos).boxes().len(), 2);
    }

    /// A world with a `piston_extension` at the origin and the block
    /// `moved` sliding into it towards `direction`.
    struct Sliding {
        moving: MovingBlock,
    }

    impl BlockSource for Sliding {
        fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
            if (x, y, z) == (0, 0, 0) {
                BlockState::new(36, 0)
            } else {
                BlockState::AIR
            }
        }

        fn moving_block(&self, x: i32, y: i32, z: i32) -> Option<MovingBlock> {
            ((x, y, z) == (0, 0, 0)).then_some(self.moving)
        }
    }

    fn sliding(moved: BlockState, direction: [i32; 3], extending: bool, last: f32) -> Sliding {
        Sliding {
            moving: MovingBlock {
                pos: [0, 0, 0],
                state: moved,
                direction,
                extending,
                show_head: false,
                progress: (last + 0.5).min(1.0),
                last_progress: last,
                light: umicraft_world::LightLevel::default(),
            },
        }
    }

    fn moving_box(world: &Sliding) -> Vec<Aabb> {
        shape_of(BlockState::new(36, 0), world, BlockPos::new(0, 0, 0))
            .boxes()
            .to_vec()
    }

    /// Regression test: the `piston_extension` had no collision box, so a
    /// player against a block in flight walked into it while the server
    /// stopped them.
    #[test]
    fn a_block_pushed_east_keeps_its_west_face_and_grows_its_east_face() {
        let stone = BlockState::new(1, 0);
        // `1 - lastProgress` while extending: flat, half, then whole.
        let expected = [(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)];
        for (last, (_, max_x)) in [0.0, 0.5, 1.0].into_iter().zip(expected) {
            let boxes = moving_box(&sliding(stone, [1, 0, 0], true, last));
            assert_eq!(
                boxes,
                vec![Aabb::new(DVec3::ZERO, DVec3::new(max_x, 1.0, 1.0))],
                "last progress {last}"
            );
        }
    }

    #[test]
    fn a_block_moving_west_or_down_moves_its_low_face() {
        let stone = BlockState::new(1, 0);
        let west = moving_box(&sliding(stone, [-1, 0, 0], true, 0.5));
        assert_eq!(west, vec![Aabb::new(DVec3::new(0.5, 0.0, 0.0), DVec3::ONE)]);
        // Retracting uses the progress itself.
        let down = moving_box(&sliding(stone, [0, -1, 0], false, 0.5));
        assert_eq!(down, vec![Aabb::new(DVec3::new(0.0, 0.5, 0.0), DVec3::ONE)]);
    }

    #[test]
    fn a_moving_block_takes_the_collision_box_of_what_moves() {
        // A bottom slab pushed up, half way.
        let slab = moving_box(&sliding(BlockState::new(44, 0), [0, 1, 0], true, 0.5));
        assert_eq!(
            slab,
            vec![Aabb::new(DVec3::ZERO, DVec3::new(1.0, 0.0, 1.0))]
        );
        // A fence uses its 1.0 high bounds, not its 1.5 high post.
        let fence = moving_box(&sliding(BlockState::new(85, 0), [0, 0, 1], false, 0.0));
        assert_eq!(
            fence,
            vec![Aabb::new(
                DVec3::new(0.375, 0.0, 0.375),
                DVec3::new(0.625, 1.0, 0.625)
            )]
        );
    }

    #[test]
    fn a_moving_piston_without_a_block_in_flight_does_not_collide() {
        let world = TestBlocks::with(&[(0, 0, 0, BlockState::new(36, 0))]);
        assert!(shape_of(BlockState::new(36, 0), &world, BlockPos::new(0, 0, 0)).is_empty());
        let air = sliding(BlockState::AIR, [1, 0, 0], true, 0.5);
        assert!(moving_box(&air).is_empty());
        let flower = sliding(BlockState::new(38, 0), [1, 0, 0], true, 0.5);
        assert!(moving_box(&flower).is_empty());
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
