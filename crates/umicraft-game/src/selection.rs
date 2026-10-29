//! Block selection bounds: the box the targeting outline and the ray cast use.
//!
//! Vanilla draws the outline and ray traces against `Block.getSelectedBoundingBox`
//! (MCP `Block.java:479`), which is the block's *bounds* after
//! `setBlockBoundsBasedOnState`, not its collision boxes. The two differ for
//! many blocks: a fence is 1.0 tall here but 1.5 tall for collision, a door
//! half is 1.0 tall, a torch is a small post and so on. [`selection_shape`]
//! ports those bounds for every 1.8 block state.
//!
//! Every bounds is a single box, so it fits in one slot of a
//! [`BlockShape`]. The arithmetic is done in `f32`, like the `float`
//! arguments of `Block.setBlockBounds`, before widening to `f64`, so values
//! such as `0.5 - 0.2` carry the same rounding as vanilla.
//!
//! Air and liquids are never selectable (`Block.canCollideCheck` is false for
//! a liquid unless a boat-style ray cast asks for it).

use glam::DVec3;
use umicraft_world::{BlockPos, BlockState};

use crate::shapes::{is_normal_cube, Aabb, BlockShape, BlockSource};

/// `[min_x, min_y, min_z, max_x, max_y, max_z]` in block-local `f32` units.
type Bounds = [f32; 6];

/// The full cube, the default bounds of `Block` (`Block.java:292`).
const FULL: Bounds = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];

/// The horizontal directions in the order of `EnumFacing.getHorizontal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Facing {
    South,
    West,
    North,
    East,
}

impl Facing {
    /// MCP `EnumFacing.getHorizontal(index)`: 0 south, 1 west, 2 north, 3 east.
    fn from_horizontal(index: u8) -> Self {
        match index & 3 {
            0 => Self::South,
            1 => Self::West,
            2 => Self::North,
            _ => Self::East,
        }
    }

    /// MCP `EnumFacing.getFront` restricted to the horizontal sides, with the
    /// fallback to north that `BlockLadder`, `BlockWallSign` and
    /// `BlockBanner.BlockBannerHanging.getStateFromMeta` apply to the
    /// vertical values.
    fn from_front_or_north(meta: u8) -> Self {
        match meta % 6 {
            3 => Self::South,
            4 => Self::West,
            5 => Self::East,
            _ => Self::North,
        }
    }
}

/// The six directions of `EnumFacing.getFront` (0 down, 1 up, 2 north,
/// 3 south, 4 west, 5 east).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Side {
    /// MCP `EnumFacing.getFront(meta)`, which wraps modulo 6.
    fn from_front(meta: u8) -> Self {
        match meta % 6 {
            0 => Self::Down,
            1 => Self::Up,
            2 => Self::North,
            3 => Self::South,
            4 => Self::West,
            _ => Self::East,
        }
    }
}

/// The selection shape of a block state.
///
/// It is the vanilla `Block.getSelectedBoundingBox` of the block, relative to
/// the block origin: the block's bounds after `setBlockBoundsBasedOnState`,
/// and never the collision boxes. `world` is read for the blocks whose bounds
/// depend on their neighbourhood (fences, walls, panes, chests, vines, doors). Air and liquids give [`BlockShape::EMPTY`].
///
/// Stairs return the full cube: `BlockStairs.getSelectedBoundingBox`
/// (`BlockStairs.java:592`) delegates to its model block, which is a full
/// cube. One box is enough for every block, so [`crate::shapes::MAX_BOXES`]
/// is not a constraint here.
///
/// ```
/// use umicraft_game::selection::selection_shape;
/// use umicraft_world::{BlockPos, BlockState, World};
///
/// let world = World::new(0);
/// // A lone oak fence is selected 1.0 tall (it collides up to 1.5).
/// let fence = selection_shape(BlockState::new(85, 0), &world, BlockPos::new(0, 64, 0));
/// assert_eq!(fence.boxes()[0].max.y, 1.0);
/// ```
#[must_use]
pub fn selection_shape<S: BlockSource + ?Sized>(
    state: BlockState,
    world: &S,
    pos: BlockPos,
) -> BlockShape {
    match selection_bounds(state, world, pos) {
        Some(b) => BlockShape::single(Aabb::new(
            DVec3::new(f64::from(b[0]), f64::from(b[1]), f64::from(b[2])),
            DVec3::new(f64::from(b[3]), f64::from(b[4]), f64::from(b[5])),
        )),
        None => BlockShape::EMPTY,
    }
}

/// The bounds of a state, or `None` for air and liquids.
///
/// The ids are the registrations of MCP `Block.registerBlocks`.
fn selection_bounds<S: BlockSource + ?Sized>(
    state: BlockState,
    world: &S,
    pos: BlockPos,
) -> Option<Bounds> {
    let meta = state.metadata();
    let bounds = match state.id() {
        0 | 8..=11 => return None,
        // BlockSapling (f = 0.4, height f * 2), BlockTallGrass and
        // BlockDeadBush (f = 0.4, height 0.8).
        6 | 31 | 32 => plant(0.4, 0.8),
        // BlockBush constructor (flowers): f = 0.2, height f * 3.
        37 | 38 => plant(0.2, 0.2 * 3.0),
        // BlockMushroom: f = 0.2, height f * 2.
        39 | 40 => plant(0.2, 0.2 * 2.0),
        // BlockCrops (wheat, carrots, potatoes) and BlockNetherWart: half
        // width 0.5, height 0.25.
        59 | 115 | 141 | 142 => plant(0.5, 0.25),
        // BlockStem.setBlockBoundsBasedOnState: height (age * 2 + 2) / 16.
        104 | 105 => plant(0.125, f32::from((meta & 7) * 2 + 2) / 16.0),
        // BlockReed: f = 0.375, full height.
        83 => plant(0.375, 1.0),
        // BlockLilyPad: height 0.015625.
        111 => plant(0.5, 0.015_625),
        // BlockFlowerPot.setBlockBoundsForItemRender: 0.375 tall, half width 0.1875.
        140 => plant(0.1875, 0.375),
        // BlockSign and BlockBanner constructors (standing): half width 0.25.
        63 | 176 => plant(0.25, 1.0),
        // BlockDoublePlant.setBlockBoundsBasedOnState: full cube.
        175 => FULL,
        // BlockTorch.collisionRayTrace (torch, redstone torches).
        50 | 75 | 76 => torch(meta),
        // BlockLever.setBlockBoundsBasedOnState.
        69 => lever(meta),
        // BlockButton.updateBlockBounds (stone and wooden buttons).
        77 | 143 => button(meta),
        // BlockRailBase.setBlockBoundsBasedOnState: ascending shapes are 0.625
        // tall. The plain rail stores its shape in the low four bits, the
        // powered, detector and activator rails in the low three.
        66 => rail(meta & 15),
        27 | 28 | 157 => rail(meta & 7),
        // BlockRedstoneWire constructor.
        55 => [0.0, 0.0, 0.0, 1.0, 0.0625, 1.0],
        // BlockTripWire.setBlockBoundsBasedOnState: suspended (2), attached (4).
        132 => tripwire(meta),
        // BlockTripWireHook.setBlockBoundsBasedOnState.
        131 => tripwire_hook(meta),
        // BlockChest.setBlockBoundsBasedOnState (chest, trapped chest).
        54 | 146 => chest(state, world, pos),
        // BlockEnderChest constructor.
        130 => [0.0625, 0.0, 0.0625, 0.9375, 0.875, 0.9375],
        // BlockBed.setBedBounds.
        26 => [0.0, 0.0, 0.0, 1.0, 0.5625, 1.0],
        // BlockVine.setBlockBoundsBasedOnState.
        106 => vine(meta, world, pos),
        // BlockLadder.setBlockBoundsBasedOnState.
        65 => ladder(meta),
        // BlockTrapDoor.setBounds (wooden and iron).
        96 | 167 => trapdoor(meta),
        // BlockDoor.setBoundBasedOnMeta (all door blocks).
        64 | 71 | 193..=197 => door(meta, world, pos),
        // BlockWallSign / BlockBannerHanging.setBlockBoundsBasedOnState.
        68 => hanging(meta, 0.28125, 0.78125),
        177 => hanging(meta, 0.0, 0.78125),
        // BlockCocoa.setBlockBoundsBasedOnState.
        127 => cocoa(meta),
        // BlockEndPortalFrame: collision leaves the bounds at 0.8125 (the eye
        // is a separate collision box and never part of the bounds).
        120 => [0.0, 0.0, 0.0, 1.0, 0.8125, 1.0],
        // BlockAnvil.setBlockBoundsBasedOnState.
        145 => anvil(meta),
        // BlockBrewingStand.setBlockBoundsForItemRender (the last bounds the
        // collision pass leaves behind): the 0.125 high base.
        117 => [0.0, 0.0, 0.0, 1.0, 0.125, 1.0],
        // BlockEnchantmentTable constructor.
        116 => [0.0, 0.0, 0.0, 1.0, 0.75, 1.0],
        // BlockDaylightDetector.
        151 | 178 => [0.0, 0.0, 0.0, 1.0, 0.375, 1.0],
        // BlockPistonBase.setBlockBoundsBasedOnState.
        29 | 33 => piston_base(meta),
        // BlockPistonExtension.applyHeadBounds.
        34 => piston_head(meta),
        // BlockSkull.setBlockBoundsBasedOnState.
        144 => skull(meta),
        // BlockSnow.getBoundsForLayers: layers / 8 with layers = meta + 1.
        78 => [0.0, 0.0, 0.0, 1.0, f32::from((meta & 7) + 1) / 8.0, 1.0],
        // BlockSlab.setBlockBoundsBasedOnState (double slabs are full).
        44 | 126 | 182 if meta & 8 != 0 => [0.0, 0.5, 0.0, 1.0, 1.0, 1.0],
        44 | 126 | 182 => [0.0, 0.0, 0.0, 1.0, 0.5, 1.0],
        // BlockStairs.getSelectedBoundingBox delegates to the (full cube)
        // model block.
        53 | 67 | 108 | 109 | 114 | 128 | 134..=136 | 156 | 163 | 164 | 180 => FULL,
        // BlockFence / BlockWall / BlockPane / BlockFenceGate.
        85 | 113 | 188..=192 => fence(state, world, pos),
        139 => wall(world, pos),
        101 | 102 | 160 => pane(state, world, pos),
        107 | 183..=187 => fence_gate(meta),
        // BlockCactus.getSelectedBoundingBox: inset by 0.0625.
        81 => [0.0625, 0.0, 0.0625, 0.9375, 1.0, 0.9375],
        // BlockFarmland constructor.
        60 => [0.0, 0.0, 0.0, 1.0, 0.9375, 1.0],
        // BlockDragonEgg constructor.
        122 => [0.0625, 0.0, 0.0625, 0.9375, 1.0, 0.9375],
        // BlockBasePressurePlate.setBlockBoundsBasedOnState0 (a power of 0 is
        // 0.0625 tall, a pressed plate 0.03125).
        70 | 72 | 147 | 148 => pressure_plate(meta),
        // BlockCarpet.setBlockBoundsFromMeta.
        171 => [0.0, 0.0, 0.0, 1.0, 0.0625, 1.0],
        // BlockRedstoneDiode constructor (repeaters, comparators).
        93 | 94 | 149 | 150 => [0.0, 0.0, 0.0, 1.0, 0.125, 1.0],
        // BlockEndPortal.setBlockBoundsBasedOnState.
        119 => [0.0, 0.0, 0.0, 1.0, 0.0625, 1.0],
        // BlockPortal.setBlockBoundsBasedOnState.
        90 => nether_portal(meta),
        // BlockCake.setBlockBoundsBasedOnState.
        92 => cake(meta),
        // Everything else keeps the default full cube (hoppers and cauldrons
        // reset their bounds to the full cube after the collision pass, fire,
        // soul sand, cobweb and barriers never change them).
        _ => FULL,
    };
    Some(bounds)
}

/// A centred post `half_width` either side of the block centre, `height` tall.
fn plant(half_width: f32, height: f32) -> Bounds {
    [
        0.5 - half_width,
        0.0,
        0.5 - half_width,
        0.5 + half_width,
        height,
        0.5 + half_width,
    ]
}

/// MCP `BlockTorch.collisionRayTrace` (`BlockTorch.java:186`): metadata 1 to
/// 4 attach to the east, west, south or north neighbour, everything else
/// stands on the ground.
fn torch(meta: u8) -> Bounds {
    let f = 0.15_f32;
    match meta {
        1 => [0.0, 0.2, 0.5 - f, f * 2.0, 0.8, 0.5 + f],
        2 => [1.0 - f * 2.0, 0.2, 0.5 - f, 1.0, 0.8, 0.5 + f],
        3 => [0.5 - f, 0.2, 0.0, 0.5 + f, 0.8, f * 2.0],
        4 => [0.5 - f, 0.2, 1.0 - f * 2.0, 0.5 + f, 0.8, 1.0],
        _ => [0.5 - f, 0.0, 0.5 - f, 0.5 + f, 0.6, 0.5 + f],
    }
}

/// MCP `BlockLever.setBlockBoundsBasedOnState`; orientation is `meta & 7`
/// (`EnumOrientation`: 0 down_x, 1 east, 2 west, 3 south, 4 north, 5 up_z,
/// 6 up_x, 7 down_z).
fn lever(meta: u8) -> Bounds {
    let f = 0.1875_f32;
    match meta & 7 {
        1 => [0.0, 0.2, 0.5 - f, f * 2.0, 0.8, 0.5 + f],
        2 => [1.0 - f * 2.0, 0.2, 0.5 - f, 1.0, 0.8, 0.5 + f],
        3 => [0.5 - f, 0.2, 0.0, 0.5 + f, 0.8, f * 2.0],
        4 => [0.5 - f, 0.2, 1.0 - f * 2.0, 0.5 + f, 0.8, 1.0],
        5 | 6 => [0.25, 0.0, 0.25, 0.75, 0.6, 0.75],
        _ => [0.25, 0.4, 0.25, 0.75, 1.0, 0.75],
    }
}

/// MCP `BlockButton.updateBlockBounds`: the facing is `meta & 7` (0 down,
/// 1 east, 2 west, 3 south, 4 north, 5 and above up) and a pressed button
/// (`meta & 8`) is 1/16 deep instead of 2/16.
fn button(meta: u8) -> Bounds {
    let depth = if meta & 8 != 0 { 1.0_f32 } else { 2.0 } / 16.0;
    match meta & 7 {
        0 => [0.3125, 1.0 - depth, 0.375, 0.6875, 1.0, 0.625],
        1 => [0.0, 0.375, 0.3125, depth, 0.625, 0.6875],
        2 => [1.0 - depth, 0.375, 0.3125, 1.0, 0.625, 0.6875],
        3 => [0.3125, 0.375, 0.0, 0.6875, 0.625, depth],
        4 => [0.3125, 0.375, 1.0 - depth, 0.6875, 0.625, 1.0],
        _ => [0.3125, 0.0, 0.375, 0.6875, depth, 0.625],
    }
}

/// MCP `BlockRailBase.setBlockBoundsBasedOnState`: shapes 2 to 5 are the
/// ascending ones (`EnumRailDirection.isAscending`).
fn rail(shape: u8) -> Bounds {
    let height = if (2..=5).contains(&shape) {
        0.625
    } else {
        0.125
    };
    [0.0, 0.0, 0.0, 1.0, height, 1.0]
}

/// MCP `BlockTripWire.setBlockBoundsBasedOnState`.
fn tripwire(meta: u8) -> Bounds {
    let suspended = meta & 2 != 0;
    let attached = meta & 4 != 0;
    if !suspended {
        [0.0, 0.0, 0.0, 1.0, 0.09375, 1.0]
    } else if !attached {
        [0.0, 0.0, 0.0, 1.0, 0.5, 1.0]
    } else {
        [0.0, 0.0625, 0.0, 1.0, 0.15625, 1.0]
    }
}

/// MCP `BlockTripWireHook.setBlockBoundsBasedOnState`; the facing is
/// `getHorizontal(meta & 3)`.
fn tripwire_hook(meta: u8) -> Bounds {
    let f = 0.1875_f32;
    match Facing::from_horizontal(meta) {
        Facing::East => [0.0, 0.2, 0.5 - f, f * 2.0, 0.8, 0.5 + f],
        Facing::West => [1.0 - f * 2.0, 0.2, 0.5 - f, 1.0, 0.8, 0.5 + f],
        Facing::South => [0.5 - f, 0.2, 0.0, 0.5 + f, 0.8, f * 2.0],
        Facing::North => [0.5 - f, 0.2, 1.0 - f * 2.0, 0.5 + f, 0.8, 1.0],
    }
}

/// MCP `BlockChest.setBlockBoundsBasedOnState`: a chest reaches the block
/// edge towards a neighbouring chest of the same block, checked in the order
/// north, south, west, east (the 1.8 chest has no stored `type`).
fn chest<S: BlockSource + ?Sized>(state: BlockState, world: &S, pos: BlockPos) -> Bounds {
    let same = |dx, dz| world.block(pos.x + dx, pos.y, pos.z + dz).id() == state.id();
    if same(0, -1) {
        [0.0625, 0.0, 0.0, 0.9375, 0.875, 0.9375]
    } else if same(0, 1) {
        [0.0625, 0.0, 0.0625, 0.9375, 0.875, 1.0]
    } else if same(-1, 0) {
        [0.0, 0.0, 0.0625, 0.9375, 0.875, 0.9375]
    } else if same(1, 0) {
        [0.0625, 0.0, 0.0625, 1.0, 0.875, 0.9375]
    } else {
        [0.0625, 0.0, 0.0625, 0.9375, 0.875, 0.9375]
    }
}

/// MCP `BlockVine.setBlockBoundsBasedOnState`. The side flags are metadata
/// bits 1 (south), 2 (west), 4 (north) and 8 (east) (`getStateFromMeta`); a
/// vine without a side that hangs under a solid block is a slab at the top,
/// and one with no support at all collapses to the inverted box the
/// reference produces (normalised by [`Aabb::new`]).
fn vine<S: BlockSource + ?Sized>(meta: u8, world: &S, pos: BlockPos) -> Bounds {
    // [min_x, min_y, min_z, max_x, max_y, max_z] as f1..f6 of the reference.
    let mut b: Bounds = [1.0, 1.0, 1.0, 0.0, 0.0, 0.0];
    let mut attached = false;
    if meta & 2 != 0 {
        b = [0.0, 0.0, 0.0, b[3].max(0.0625), 1.0, 1.0];
        attached = true;
    }
    if meta & 8 != 0 {
        b = [b[0].min(0.9375), 0.0, 0.0, 1.0, 1.0, 1.0];
        attached = true;
    }
    if meta & 4 != 0 {
        b = [0.0, 0.0, 0.0, 1.0, 1.0, b[5].max(0.0625)];
        attached = true;
    }
    if meta & 1 != 0 {
        b = [0.0, 0.0, b[2].min(0.9375), 1.0, 1.0, 1.0];
        attached = true;
    }
    // `canPlaceOn`: a full cube that blocks movement; approximated by the
    // normal-cube table.
    if !attached && is_normal_cube(world.block(pos.x, pos.y + 1, pos.z)) {
        b = [0.0, b[1].min(0.9375), 0.0, 1.0, 1.0, 1.0];
    }
    b
}

/// MCP `BlockLadder.setBlockBoundsBasedOnState`: 0.125 thick against the
/// neighbour it hangs on (`getFront(meta)`, north for vertical values).
fn ladder(meta: u8) -> Bounds {
    let f = 0.125_f32;
    match Facing::from_front_or_north(meta) {
        Facing::North => [0.0, 0.0, 1.0 - f, 1.0, 1.0, 1.0],
        Facing::South => [0.0, 0.0, 0.0, 1.0, 1.0, f],
        Facing::West => [1.0 - f, 0.0, 0.0, 1.0, 1.0, 1.0],
        Facing::East => [0.0, 0.0, 0.0, f, 1.0, 1.0],
    }
}

/// MCP `BlockTrapDoor.setBounds`: facing `meta & 3` (0 north, 1 south,
/// 2 west, 3 east), open `meta & 4`, top half `meta & 8`. An open trap door
/// is a wall against its facing side whatever its half.
fn trapdoor(meta: u8) -> Bounds {
    if meta & 4 != 0 {
        match meta & 3 {
            0 => [0.0, 0.0, 0.8125, 1.0, 1.0, 1.0],
            1 => [0.0, 0.0, 0.0, 1.0, 1.0, 0.1875],
            2 => [0.8125, 0.0, 0.0, 1.0, 1.0, 1.0],
            _ => [0.0, 0.0, 0.0, 0.1875, 1.0, 1.0],
        }
    } else if meta & 8 != 0 {
        [0.0, 0.8125, 0.0, 1.0, 1.0, 1.0]
    } else {
        [0.0, 0.0, 0.0, 1.0, 0.1875, 1.0]
    }
}

/// MCP `BlockDoor.combineMetadata` and `setBoundBasedOnMeta`.
///
/// A door's state is split over its two halves: the lower half stores the
/// facing and the open bit, the upper half stores the hinge. Both halves
/// therefore read the metadata of the half above or below. The facing is
/// `getHorizontal(meta & 3).rotateYCCW()` (0 east, 1 south, 2 west, 3 north).
fn door<S: BlockSource + ?Sized>(meta: u8, world: &S, pos: BlockPos) -> Bounds {
    let is_top = meta & 8 != 0;
    let lower = if is_top {
        world.block(pos.x, pos.y - 1, pos.z).metadata()
    } else {
        meta
    };
    let upper = if is_top {
        meta
    } else {
        world.block(pos.x, pos.y + 1, pos.z).metadata()
    };
    let open = lower & 4 != 0;
    let hinge_left = upper & 1 != 0;
    let f = 0.1875_f32;
    let facing = match lower & 3 {
        0 => Facing::East,
        1 => Facing::South,
        2 => Facing::West,
        _ => Facing::North,
    };
    match (open, facing, hinge_left) {
        (true, Facing::East, false) => [0.0, 0.0, 0.0, 1.0, 1.0, f],
        (true, Facing::East, true) => [0.0, 0.0, 1.0 - f, 1.0, 1.0, 1.0],
        (true, Facing::South, false) => [1.0 - f, 0.0, 0.0, 1.0, 1.0, 1.0],
        (true, Facing::South, true) => [0.0, 0.0, 0.0, f, 1.0, 1.0],
        (true, Facing::West, false) => [0.0, 0.0, 1.0 - f, 1.0, 1.0, 1.0],
        (true, Facing::West, true) => [0.0, 0.0, 0.0, 1.0, 1.0, f],
        (true, Facing::North, false) => [0.0, 0.0, 0.0, f, 1.0, 1.0],
        (true, Facing::North, true) => [1.0 - f, 0.0, 0.0, 1.0, 1.0, 1.0],
        (false, Facing::East, _) => [0.0, 0.0, 0.0, f, 1.0, 1.0],
        (false, Facing::South, _) => [0.0, 0.0, 0.0, 1.0, 1.0, f],
        (false, Facing::West, _) => [1.0 - f, 0.0, 0.0, 1.0, 1.0, 1.0],
        (false, Facing::North, _) => [0.0, 0.0, 1.0 - f, 1.0, 1.0, 1.0],
    }
}

/// MCP `BlockWallSign` (`min_y` 0.28125) and `BlockBanner.BlockBannerHanging`
/// (`min_y` 0): a 0.125 thick board against the wall, 0.5 tall for the sign
/// and 0.78125 for the banner measured from `min_y`.
fn hanging(meta: u8, min_y: f32, max_y: f32) -> Bounds {
    let t = 0.125_f32;
    match Facing::from_front_or_north(meta) {
        Facing::North => [0.0, min_y, 1.0 - t, 1.0, max_y, 1.0],
        Facing::South => [0.0, min_y, 0.0, 1.0, max_y, t],
        Facing::West => [1.0 - t, min_y, 0.0, 1.0, max_y, 1.0],
        Facing::East => [0.0, min_y, 0.0, t, max_y, 1.0],
    }
}

/// MCP `BlockCocoa.setBlockBoundsBasedOnState`: facing `getHorizontal(meta)`,
/// age `(meta & 15) >> 2`; the pod grows with the age.
fn cocoa(meta: u8) -> Bounds {
    let age = f32::from((meta & 15) >> 2);
    let j = 4.0 + age * 2.0;
    let k = 5.0 + age * 2.0;
    let f = j / 2.0;
    match Facing::from_horizontal(meta) {
        Facing::South => [
            (8.0 - f) / 16.0,
            (12.0 - k) / 16.0,
            (15.0 - j) / 16.0,
            (8.0 + f) / 16.0,
            0.75,
            0.9375,
        ],
        Facing::North => [
            (8.0 - f) / 16.0,
            (12.0 - k) / 16.0,
            0.0625,
            (8.0 + f) / 16.0,
            0.75,
            (1.0 + j) / 16.0,
        ],
        Facing::West => [
            0.0625,
            (12.0 - k) / 16.0,
            (8.0 - f) / 16.0,
            (1.0 + j) / 16.0,
            0.75,
            (8.0 + f) / 16.0,
        ],
        Facing::East => [
            (15.0 - j) / 16.0,
            (12.0 - k) / 16.0,
            (8.0 - f) / 16.0,
            0.9375,
            0.75,
            (8.0 + f) / 16.0,
        ],
    }
}

/// MCP `BlockAnvil.setBlockBoundsBasedOnState`: the anvil is narrow across
/// its facing; `getHorizontal(meta & 3)` is east or west for odd values.
fn anvil(meta: u8) -> Bounds {
    if meta & 1 == 1 {
        [0.0, 0.0, 0.125, 1.0, 1.0, 0.875]
    } else {
        [0.125, 0.0, 0.0, 0.875, 1.0, 1.0]
    }
}

/// MCP `BlockPistonBase.setBlockBoundsBasedOnState`: an extended piston
/// (`meta & 8`) loses the quarter block its head occupies; a retracted one,
/// or one with an invalid facing, is the full cube.
fn piston_base(meta: u8) -> Bounds {
    if meta & 8 == 0 || meta & 7 > 5 {
        return FULL;
    }
    match Side::from_front(meta & 7) {
        Side::Down => [0.0, 0.25, 0.0, 1.0, 1.0, 1.0],
        Side::Up => [0.0, 0.0, 0.0, 1.0, 0.75, 1.0],
        Side::North => [0.0, 0.0, 0.25, 1.0, 1.0, 1.0],
        Side::South => [0.0, 0.0, 0.0, 1.0, 1.0, 0.75],
        Side::West => [0.25, 0.0, 0.0, 1.0, 1.0, 1.0],
        Side::East => [0.0, 0.0, 0.0, 0.75, 1.0, 1.0],
    }
}

/// MCP `BlockPistonExtension.applyHeadBounds`: the head is the quarter-block
/// plate on the facing side of the block.
fn piston_head(meta: u8) -> Bounds {
    if meta & 7 > 5 {
        return FULL;
    }
    match Side::from_front(meta & 7) {
        Side::Down => [0.0, 0.0, 0.0, 1.0, 0.25, 1.0],
        Side::Up => [0.0, 0.75, 0.0, 1.0, 1.0, 1.0],
        Side::North => [0.0, 0.0, 0.0, 1.0, 1.0, 0.25],
        Side::South => [0.0, 0.0, 0.75, 1.0, 1.0, 1.0],
        Side::West => [0.0, 0.0, 0.0, 0.25, 1.0, 1.0],
        Side::East => [0.75, 0.0, 0.0, 1.0, 1.0, 1.0],
    }
}

/// MCP `BlockSkull.setBlockBoundsBasedOnState`: facing `getFront(meta & 7)`;
/// up and down stand on the floor.
fn skull(meta: u8) -> Bounds {
    match Side::from_front(meta & 7) {
        Side::North => [0.25, 0.25, 0.5, 0.75, 0.75, 1.0],
        Side::South => [0.25, 0.25, 0.0, 0.75, 0.75, 0.5],
        Side::West => [0.5, 0.25, 0.25, 1.0, 0.75, 0.75],
        Side::East => [0.0, 0.25, 0.25, 0.5, 0.75, 0.75],
        Side::Up | Side::Down => [0.25, 0.0, 0.25, 0.75, 0.5, 0.75],
    }
}

/// MCP `BlockBasePressurePlate.setBlockBoundsBasedOnState0`.
fn pressure_plate(meta: u8) -> Bounds {
    let height = if meta > 0 { 0.03125 } else { 0.0625 };
    [0.0625, 0.0, 0.0625, 0.9375, height, 0.9375]
}

/// MCP `BlockPortal.setBlockBoundsBasedOnState`: a 0.25 thick slab along the
/// axis; `(meta & 3) == 2` is the Z axis.
fn nether_portal(meta: u8) -> Bounds {
    let (half_x, half_z) = if meta & 3 == 2 {
        (0.125_f32, 0.5_f32)
    } else {
        (0.5, 0.125)
    };
    [
        0.5 - half_x,
        0.0,
        0.5 - half_z,
        0.5 + half_x,
        1.0,
        0.5 + half_z,
    ]
}

/// MCP `BlockCake.setBlockBoundsBasedOnState`: each bite (`meta`) removes
/// 2/16 from the west side.
fn cake(meta: u8) -> Bounds {
    let west = f32::from(1 + meta * 2) / 16.0;
    [west, 0.0, 0.0625, 1.0 - 0.0625, 0.5, 1.0 - 0.0625]
}

/// Whether a neighbour holds up a fence or a wall: an opaque full cube that
/// is not a gourd (MCP `BlockFence.canConnectTo` / `BlockWall.canConnectTo`,
/// `block.blockMaterial.isOpaque() && isFullCube() && material != gourd`;
/// barriers never connect). Approximated by the normal-cube table minus the
/// gourds (86, 103), the barrier (166) and the end portal frame (120).
fn connects_to_solid(state: BlockState) -> bool {
    is_normal_cube(state) && !matches!(state.id(), 86 | 103 | 120 | 166)
}

/// Whether the block is a fence gate (`instanceof BlockFenceGate`).
fn is_fence_gate(id: u16) -> bool {
    matches!(id, 107 | 183..=187)
}

/// The neighbours north, south, west and east of `pos`, in the order of the
/// `flag..flag3` variables of the reference.
fn horizontal_neighbours<S: BlockSource + ?Sized>(world: &S, pos: BlockPos) -> [BlockState; 4] {
    [
        world.block(pos.x, pos.y, pos.z - 1),
        world.block(pos.x, pos.y, pos.z + 1),
        world.block(pos.x - 1, pos.y, pos.z),
        world.block(pos.x + 1, pos.y, pos.z),
    ]
}

/// MCP `BlockFence.setBlockBoundsBasedOnState`: a 0.25 wide post that
/// reaches the block edge towards each connected neighbour, 1.0 tall (the
/// 1.5 tall box is collision only). A fence connects to a fence of the same
/// material (wood or nether brick), to any gate and to solid cubes.
fn fence<S: BlockSource + ?Sized>(state: BlockState, world: &S, pos: BlockPos) -> Bounds {
    // `BlockFence` ids: 113 is the nether brick fence (rock material), the
    // others are wooden.
    let is_nether = |id: u16| id == 113;
    let is_fence = |id: u16| matches!(id, 85 | 113 | 188..=192);
    let [n, s, w, e] = horizontal_neighbours(world, pos).map(|other| {
        let id = other.id();
        (is_fence(id) && is_nether(id) == is_nether(state.id()))
            || is_fence_gate(id)
            || connects_to_solid(other)
    });
    post_bounds(n, s, w, e, 0.375, 1.0)
}

/// MCP `BlockWall.setBlockBoundsBasedOnState`: a 0.5 wide post, 1.0 tall, or
/// 0.8125 tall (and narrower) for a straight run.
fn wall<S: BlockSource + ?Sized>(world: &S, pos: BlockPos) -> Bounds {
    let [n, s, w, e] = horizontal_neighbours(world, pos)
        .map(|other| other.id() == 139 || is_fence_gate(other.id()) || connects_to_solid(other));
    let mut b = post_bounds(n, s, w, e, 0.25, 1.0);
    if n && s && !w && !e {
        b[0] = 0.3125;
        b[3] = 0.6875;
        b[4] = 0.8125;
    } else if !n && !s && w && e {
        b[2] = 0.3125;
        b[5] = 0.6875;
        b[4] = 0.8125;
    }
    b
}

/// A post of `[lo, 1 - lo]` that reaches the edge towards each connected
/// neighbour; shared by fences and walls.
fn post_bounds(north: bool, south: bool, west: bool, east: bool, lo: f32, height: f32) -> Bounds {
    let hi = 1.0 - lo;
    [
        if west { 0.0 } else { lo },
        0.0,
        if north { 0.0 } else { lo },
        if east { 1.0 } else { hi },
        height,
        if south { 1.0 } else { hi },
    ]
}

/// MCP `BlockPane.canPaneConnectToBlock`: a full block, a pane or glass.
fn pane_connects(this_id: u16, other: BlockState) -> bool {
    let id = other.id();
    id == this_id
        || matches!(id, 20 | 95 | 101 | 102 | 160)
        || (is_normal_cube(other) && !matches!(id, 120 | 166))
}

/// MCP `BlockPane.setBlockBoundsBasedOnState` (glass panes and iron bars): a
/// 0.125 thin plate that runs through the block when connected on both
/// sides of an axis, reaches one edge when connected on one side, and stays a
/// post when alone.
fn pane<S: BlockSource + ?Sized>(state: BlockState, world: &S, pos: BlockPos) -> Bounds {
    let [n, s, w, e] = horizontal_neighbours(world, pos).map(|o| pane_connects(state.id(), o));
    let any = n || s || w || e;
    let (mut min_x, mut max_x) = (0.4375_f32, 0.5625_f32);
    let (mut min_z, mut max_z) = (0.4375_f32, 0.5625_f32);
    if (!w || !e) && any {
        if w {
            min_x = 0.0;
        } else if e {
            max_x = 1.0;
        }
    } else {
        min_x = 0.0;
        max_x = 1.0;
    }
    if (!n || !s) && any {
        if n {
            min_z = 0.0;
        } else if s {
            max_z = 1.0;
        }
    } else {
        min_z = 0.0;
        max_z = 1.0;
    }
    [min_x, 0.0, min_z, max_x, 1.0, max_z]
}

/// MCP `BlockFenceGate.setBlockBoundsBasedOnState`: 0.25 thick across the
/// facing axis, 1.0 tall (collision is 1.5 tall and empty when open); the
/// axis is Z for `getHorizontal(meta)` south or north.
fn fence_gate(meta: u8) -> Bounds {
    match Facing::from_horizontal(meta) {
        Facing::South | Facing::North => [0.0, 0.0, 0.375, 1.0, 1.0, 0.625],
        Facing::West | Facing::East => [0.375, 0.0, 0.0, 0.625, 1.0, 1.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A block source backed by a hash map, for tests.
    #[derive(Default)]
    struct Blocks(HashMap<(i32, i32, i32), BlockState>);

    impl Blocks {
        fn with(blocks: &[((i32, i32, i32), BlockState)]) -> Self {
            Self(blocks.iter().copied().collect())
        }
    }

    impl BlockSource for Blocks {
        fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
            self.0.get(&(x, y, z)).copied().unwrap_or(BlockState::AIR)
        }
    }

    const ORIGIN: BlockPos = BlockPos { x: 0, y: 0, z: 0 };

    /// The single selection box of `state` as `[min.., max..]`.
    fn bounds_in(world: &Blocks, state: BlockState) -> [f64; 6] {
        let shape = selection_shape(state, world, ORIGIN);
        assert_eq!(shape.boxes().len(), 1, "one selection box");
        let b = shape.boxes()[0];
        [b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z]
    }

    fn bounds(id: u16, meta: u8) -> [f64; 6] {
        bounds_in(&Blocks::default(), BlockState::new(id, meta))
    }

    fn assert_close(actual: [f64; 6], expected: [f64; 6]) {
        for (a, e) in actual.iter().zip(expected) {
            assert!((a - e).abs() < 1e-6, "{actual:?} != {expected:?}");
        }
    }

    #[test]
    fn air_and_liquids_are_not_selectable() {
        let world = Blocks::default();
        for id in [0, 8, 9, 10, 11] {
            assert!(selection_shape(BlockState::new(id, 0), &world, ORIGIN).is_empty());
        }
    }

    #[test]
    fn full_blocks_are_full_cubes() {
        assert_eq!(bounds(1, 0), [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        // Fire, soul sand, hoppers and cauldrons keep the full cube.
        for id in [51, 88, 154, 118, 30] {
            assert_eq!(bounds(id, 0), [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        }
    }

    #[test]
    fn a_lone_fence_is_one_block_tall() {
        assert_close(bounds(85, 0), [0.375, 0.0, 0.375, 0.625, 1.0, 0.625]);
    }

    #[test]
    fn a_fence_reaches_towards_a_neighbouring_fence_and_stone() {
        let world = Blocks::with(&[
            ((0, 0, -1), BlockState::new(85, 0)),
            ((1, 0, 0), BlockState::new(1, 0)),
        ]);
        assert_close(
            bounds_in(&world, BlockState::new(85, 0)),
            [0.375, 0.0, 0.0, 1.0, 1.0, 0.625],
        );
    }

    #[test]
    fn a_wooden_fence_does_not_join_a_nether_brick_fence() {
        let world = Blocks::with(&[((0, 0, -1), BlockState::new(113, 0))]);
        assert_close(
            bounds_in(&world, BlockState::new(85, 0)),
            [0.375, 0.0, 0.375, 0.625, 1.0, 0.625],
        );
    }

    #[test]
    fn walls_are_one_tall_and_lower_when_straight() {
        assert_close(bounds(139, 0), [0.25, 0.0, 0.25, 0.75, 1.0, 0.75]);
        let world = Blocks::with(&[
            ((0, 0, -1), BlockState::new(139, 0)),
            ((0, 0, 1), BlockState::new(139, 0)),
        ]);
        assert_close(
            bounds_in(&world, BlockState::new(139, 0)),
            [0.3125, 0.0, 0.0, 0.6875, 0.8125, 1.0],
        );
    }

    #[test]
    fn fence_gates_follow_their_axis() {
        assert_close(bounds(107, 0), [0.0, 0.0, 0.375, 1.0, 1.0, 0.625]);
        assert_close(bounds(107, 1), [0.375, 0.0, 0.0, 0.625, 1.0, 1.0]);
        // Open or closed gates select the same box.
        assert_close(bounds(107, 4), bounds(107, 0));
    }

    #[test]
    fn panes_connect_to_neighbours() {
        assert_close(bounds(102, 0), [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        let world = Blocks::with(&[((-1, 0, 0), BlockState::new(1, 0))]);
        assert_close(
            bounds_in(&world, BlockState::new(102, 0)),
            [0.0, 0.0, 0.4375, 0.5625, 1.0, 0.5625],
        );
    }

    #[test]
    fn torches_depend_on_the_facing() {
        assert_close(bounds(50, 5), [0.35, 0.0, 0.35, 0.65, 0.6, 0.65]);
        assert_close(bounds(50, 1), [0.0, 0.2, 0.35, 0.3, 0.8, 0.65]);
        assert_close(bounds(50, 2), [0.7, 0.2, 0.35, 1.0, 0.8, 0.65]);
        assert_close(bounds(50, 3), [0.35, 0.2, 0.0, 0.65, 0.8, 0.3]);
        assert_close(bounds(76, 4), [0.35, 0.2, 0.7, 0.65, 0.8, 1.0]);
    }

    #[test]
    fn plants_are_small_posts() {
        // Red flower: 0.3..0.7, 0.6 tall.
        assert_close(bounds(38, 0), [0.3, 0.0, 0.3, 0.7, 0.6, 0.7]);
        // Tall grass: 0.1..0.9, 0.8 tall.
        assert_close(bounds(31, 1), [0.1, 0.0, 0.1, 0.9, 0.8, 0.9]);
        // Mushroom: 0.3..0.7, 0.4 tall.
        assert_close(bounds(39, 0), [0.3, 0.0, 0.3, 0.7, 0.4, 0.7]);
        // Wheat: full footprint, 0.25 tall.
        assert_close(bounds(59, 7), [0.0, 0.0, 0.0, 1.0, 0.25, 1.0]);
        // Pumpkin stem age 3: 8/16 tall.
        assert_close(bounds(104, 3), [0.375, 0.0, 0.375, 0.625, 0.5, 0.625]);
    }

    #[test]
    fn doors_use_the_metadata_of_both_halves() {
        // Closed, facing east (meta 0): a slab against the west side.
        let closed = Blocks::with(&[
            ((0, 0, 0), BlockState::new(64, 0)),
            ((0, 1, 0), BlockState::new(64, 8)),
        ]);
        assert_close(
            bounds_in(&closed, BlockState::new(64, 0)),
            [0.0, 0.0, 0.0, 0.1875, 1.0, 1.0],
        );
        // The upper half reads the lower half's facing: same box, 1.0 tall.
        let upper = selection_shape(BlockState::new(64, 8), &closed, BlockPos::new(0, 1, 0));
        assert_eq!(upper.boxes()[0].max.y, 1.0);
        assert_eq!(upper.boxes()[0].max.x, 0.1875);

        // Open (bit 4), hinge on the right (upper meta 8): swings to north.
        let open = Blocks::with(&[
            ((0, 0, 0), BlockState::new(64, 4)),
            ((0, 1, 0), BlockState::new(64, 8)),
        ]);
        assert_close(
            bounds_in(&open, BlockState::new(64, 4)),
            [0.0, 0.0, 0.0, 1.0, 1.0, 0.1875],
        );
        // Hinge on the left (upper meta 9) swings the other way.
        let left = Blocks::with(&[
            ((0, 0, 0), BlockState::new(64, 4)),
            ((0, 1, 0), BlockState::new(64, 9)),
        ]);
        assert_close(
            bounds_in(&left, BlockState::new(64, 4)),
            [0.0, 0.0, 0.8125, 1.0, 1.0, 1.0],
        );
    }

    #[test]
    fn trapdoors_open_and_closed() {
        assert_close(bounds(96, 0), [0.0, 0.0, 0.0, 1.0, 0.1875, 1.0]);
        assert_close(bounds(96, 8), [0.0, 0.8125, 0.0, 1.0, 1.0, 1.0]);
        // Open, facing north (meta 0 | 4): a wall on the south edge.
        assert_close(bounds(96, 4), [0.0, 0.0, 0.8125, 1.0, 1.0, 1.0]);
        assert_close(bounds(167, 7), [0.0, 0.0, 0.0, 0.1875, 1.0, 1.0]);
    }

    #[test]
    fn slabs_and_stairs() {
        assert_eq!(bounds(44, 0), [0.0, 0.0, 0.0, 1.0, 0.5, 1.0]);
        assert_eq!(bounds(44, 8), [0.0, 0.5, 0.0, 1.0, 1.0, 1.0]);
        assert_eq!(bounds(43, 0), [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        // Stairs select the full cube whatever their half or facing.
        assert_eq!(bounds(53, 0), [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        assert_eq!(bounds(53, 7), [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn chests_are_inset_and_join_neighbours() {
        assert_close(bounds(54, 2), [0.0625, 0.0, 0.0625, 0.9375, 0.875, 0.9375]);
        let world = Blocks::with(&[((0, 0, -1), BlockState::new(54, 2))]);
        assert_close(
            bounds_in(&world, BlockState::new(54, 2)),
            [0.0625, 0.0, 0.0, 0.9375, 0.875, 0.9375],
        );
        assert_close(bounds(130, 2), [0.0625, 0.0, 0.0625, 0.9375, 0.875, 0.9375]);
    }

    #[test]
    fn thin_blocks_keep_their_heights() {
        assert_close(bounds(171, 3), [0.0, 0.0, 0.0, 1.0, 0.0625, 1.0]);
        assert_close(bounds(78, 2), [0.0, 0.0, 0.0, 1.0, 0.375, 1.0]);
        assert_close(bounds(70, 0), [0.0625, 0.0, 0.0625, 0.9375, 0.0625, 0.9375]);
        assert_close(
            bounds(70, 1),
            [0.0625, 0.0, 0.0625, 0.9375, 0.03125, 0.9375],
        );
        assert_close(bounds(55, 5), [0.0, 0.0, 0.0, 1.0, 0.0625, 1.0]);
        assert_close(bounds(66, 0), [0.0, 0.0, 0.0, 1.0, 0.125, 1.0]);
        assert_close(bounds(66, 2), [0.0, 0.0, 0.0, 1.0, 0.625, 1.0]);
        assert_close(bounds(27, 5), [0.0, 0.0, 0.0, 1.0, 0.625, 1.0]);
        assert_close(bounds(26, 0), [0.0, 0.0, 0.0, 1.0, 0.5625, 1.0]);
        assert_close(bounds(116, 0), [0.0, 0.0, 0.0, 1.0, 0.75, 1.0]);
        assert_close(bounds(151, 0), [0.0, 0.0, 0.0, 1.0, 0.375, 1.0]);
        assert_close(bounds(111, 0), [0.0, 0.0, 0.0, 1.0, 0.015625, 1.0]);
    }

    #[test]
    fn wall_mounted_blocks_hug_their_wall() {
        // Ladder facing north (meta 2) sits on the south edge of the block.
        assert_close(bounds(65, 2), [0.0, 0.0, 0.875, 1.0, 1.0, 1.0]);
        // Wall sign facing south (meta 3).
        assert_close(bounds(68, 3), [0.0, 0.28125, 0.0, 1.0, 0.78125, 0.125]);
        // Wall banner facing east (meta 5).
        assert_close(bounds(177, 5), [0.0, 0.0, 0.0, 0.125, 0.78125, 1.0]);
        // Standing sign.
        assert_close(bounds(63, 0), [0.25, 0.0, 0.25, 0.75, 1.0, 0.75]);
        // Lever on a wall (east, meta 1), on the floor (meta 5), button.
        assert_close(bounds(69, 1), [0.0, 0.2, 0.3125, 0.375, 0.8, 0.6875]);
        assert_close(bounds(69, 5), [0.25, 0.0, 0.25, 0.75, 0.6, 0.75]);
        assert_close(bounds(77, 1), [0.0, 0.375, 0.3125, 0.125, 0.625, 0.6875]);
        assert_close(bounds(77, 9), [0.0, 0.375, 0.3125, 0.0625, 0.625, 0.6875]);
    }

    #[test]
    fn vines_follow_their_sides_and_the_block_above() {
        // South side (meta 1): a slab on the south edge.
        assert_close(bounds(106, 1), [0.0, 0.0, 0.9375, 1.0, 1.0, 1.0]);
        // East side (meta 8): a slab on the east edge.
        assert_close(bounds(106, 8), [0.9375, 0.0, 0.0, 1.0, 1.0, 1.0]);
        // No side, stone above: a slab at the top.
        let world = Blocks::with(&[((0, 1, 0), BlockState::new(1, 0))]);
        assert_close(
            bounds_in(&world, BlockState::new(106, 0)),
            [0.0, 0.9375, 0.0, 1.0, 1.0, 1.0],
        );
    }

    #[test]
    fn pistons_anvils_and_the_rest() {
        assert_close(bounds(33, 0), [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        assert_close(bounds(33, 8 | 1), [0.0, 0.0, 0.0, 1.0, 0.75, 1.0]);
        assert_close(bounds(34, 1), [0.0, 0.75, 0.0, 1.0, 1.0, 1.0]);
        // Anvil: meta 0 faces south (z axis), meta 1 faces west (x axis).
        assert_close(bounds(145, 0), [0.125, 0.0, 0.0, 0.875, 1.0, 1.0]);
        assert_close(bounds(145, 1), [0.0, 0.0, 0.125, 1.0, 1.0, 0.875]);
        assert_close(bounds(81, 0), [0.0625, 0.0, 0.0625, 0.9375, 1.0, 0.9375]);
        assert_close(bounds(92, 1), [0.1875, 0.0, 0.0625, 0.9375, 0.5, 0.9375]);
        assert_close(bounds(120, 4), [0.0, 0.0, 0.0, 1.0, 0.8125, 1.0]);
        assert_close(bounds(117, 0), [0.0, 0.0, 0.0, 1.0, 0.125, 1.0]);
        assert_close(bounds(144, 1), [0.25, 0.0, 0.25, 0.75, 0.5, 0.75]);
        assert_close(bounds(144, 2), [0.25, 0.25, 0.5, 0.75, 0.75, 1.0]);
        assert_close(bounds(90, 1), [0.0, 0.0, 0.375, 1.0, 1.0, 0.625]);
        assert_close(bounds(90, 2), [0.375, 0.0, 0.0, 0.625, 1.0, 1.0]);
        assert_close(bounds(127, 0), [0.375, 0.4375, 0.6875, 0.625, 0.75, 0.9375]);
        assert_close(bounds(132, 0), [0.0, 0.0, 0.0, 1.0, 0.09375, 1.0]);
        assert_close(bounds(131, 0), [0.3125, 0.2, 0.0, 0.6875, 0.8, 0.375]);
    }
}
