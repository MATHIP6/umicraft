// SPDX-License-Identifier: MIT OR Apache-2.0

//! The block a right click places, predicted on the client.
//!
//! The vanilla client places the block itself, in the same tick as the
//! click: `PlayerControllerMP.onPlayerRightClick` sends `Block Placement`,
//! then runs `ItemStack.onItemUse`, which for a block item is
//! `ItemBlock.onItemUse` (or `ItemSlab.onItemUse`) on the client world. The
//! block appears at once whatever the ping, and the server's `Block Change`
//! for the clicked block and its neighbour (sent after every placement by
//! `NetHandlerPlayServer.processPlayerBlockPlacement`) later confirms or
//! undoes it.
//!
//! This module ports, for the block items it knows:
//!
//! * the target cell (`ItemBlock.onItemUse`: the clicked block when it is
//!   replaceable, the cell next to the clicked face otherwise);
//! * `World.canBlockBePlaced`: no entity in the collision box of the block's
//!   default state, a replaceable material in the cell, and the block's own
//!   `canPlaceBlockAt` / `canPlaceBlockOnSide`;
//! * the state `Block.onBlockPlaced` / `onBlockPlacedBy` choose (stairs,
//!   slabs, logs, furnaces, pistons, torches, ladders, chests...).
//!
//! [`can_place_on_side`] is the check `onPlayerRightClick` makes before
//! sending anything (`ItemBlock.canPlaceBlockOnSide`); [`predict`] is the
//! placement itself. A block item this module does not model
//! ([`Rule::Unmodelled`]: flowers, rails, buttons, redstone...) is checked
//! with the default rules and never predicted: its block comes with the
//! server's `Block Change`, as before.

use glam::DVec3;
use umicraft_protocol::packets::play::facing::Facing;
use umicraft_protocol::packets::play::item_stack::ItemStack;
use umicraft_world::{BlockPos, BlockState};

use crate::interaction::{face_offset, wire_face};
use crate::raycast::RayHit;
use crate::shapes::{is_normal_cube, is_stairs, shape_of, Aabb, BlockSource};

/// The player who places the block (the `placer` of `Block.onBlockPlaced`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placer<'a> {
    /// The feet of the player (`posX`, `posY`, `posZ`).
    pub position: DVec3,
    /// The bounding box of the player, which a block may not be placed in.
    pub bounds: Aabb,
    /// The yaw in degrees (`rotationYaw`).
    pub yaw: f32,
    /// Whether the player sneaks, which lowers the eyes.
    pub sneaking: bool,
    /// The boxes of the other entities that keep a block out
    /// (`Entity.preventEntitySpawning`: living entities, boats, minecarts,
    /// primed TNT, falling blocks, ender crystals).
    pub others: &'a [Aabb],
}

impl Placer<'_> {
    /// A player alone in the world, facing south, whose box is `bounds`.
    #[must_use]
    pub fn alone(bounds: Aabb) -> Self {
        Placer {
            position: DVec3::new(
                (bounds.min.x + bounds.max.x) / 2.0,
                bounds.min.y,
                (bounds.min.z + bounds.max.z) / 2.0,
            ),
            bounds,
            yaw: 0.0,
            sneaking: false,
            others: &[],
        }
    }

    /// `Entity.getHorizontalFacing`:
    /// `getHorizontal(floor(rotationYaw * 4.0F / 360.0F + 0.5D) & 3)`.
    fn horizontal_facing(&self) -> Facing {
        let quarter = f64::from(self.yaw * 4.0 / 360.0) + 0.5;
        horizontal((quarter.floor() as i64 & 3) as u8)
    }

    /// Whether `area` holds this player or another entity
    /// (`World.checkNoEntityCollision` with no excluded entity).
    fn obstructs(&self, area: &Aabb) -> bool {
        self.bounds.intersects(area) || self.others.iter().any(|other| other.intersects(area))
    }
}

/// A right click on a block with a block item.
#[derive(Debug, Clone, Copy)]
pub struct PlaceClick<'a, W: BlockSource + ?Sized> {
    /// The client world.
    pub world: &'a W,
    /// The clicked block, face and point.
    pub hit: &'a RayHit,
    /// The held stack.
    pub item: &'a ItemStack,
    /// The player, `None` when unknown (then no entity is in the way and the
    /// player faces south).
    pub placer: Option<&'a Placer<'a>>,
}

/// The blocks a predicted placement sets: the placed one, and the chest
/// beside it that turns to pair with it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    changes: [Option<(BlockPos, BlockState)>; 2],
}

impl Placement {
    fn one(pos: BlockPos, state: BlockState) -> Self {
        Self {
            changes: [Some((pos, state)), None],
        }
    }

    /// The `(position, state)` pairs to write, in order.
    pub fn changes(&self) -> impl Iterator<Item = (BlockPos, BlockState)> + '_ {
        self.changes.iter().flatten().copied()
    }

    /// The cell of the placed block.
    #[must_use]
    pub fn position(&self) -> Option<BlockPos> {
        self.changes.iter().flatten().last().map(|(pos, _)| *pos)
    }
}

/// How a block item places its block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// `getStateFromMeta(0)`: `ItemBlock.getMetadata` is 0.
    Plain,
    /// The damage is the metadata (`ItemMultiTexture`, `ItemCloth`,
    /// `ItemColored`), up to the given highest variant; a larger damage
    /// falls back to variant 0 (`EnumType.byMetadata`).
    Subtype(u8),
    /// Leaves: `ItemLeaves.getMetadata` is `damage | 4` (not decaying); the
    /// value is the number of wood variants.
    Leaves(u8),
    /// A log, its axis from the clicked face; the value is the number of
    /// wood variants.
    Log(u8),
    /// A hay bale, its axis from the clicked face.
    Hay,
    /// A quartz block, a pillar along the clicked face's axis.
    Quartz,
    /// Stairs, turned with the player and upside down on a high click.
    Stairs,
    /// A slab (`ItemSlab`): the double slab id and the number of variants.
    Slab(u16, u8),
    /// A pumpkin: faces the player, needs a solid top below.
    Pumpkin,
    /// Furnace, ender chest: faces the player (`getIndex`).
    FacesPlayer,
    /// End portal frame: faces the player (`getHorizontalIndex`), no eye.
    PortalFrame,
    /// Dispenser, dropper, pistons: `BlockPistonBase.getFacingFromEntity`.
    FacesEntity,
    /// A hopper: towards the clicked face, never up.
    Hopper,
    /// An anvil: across the player's view, damage from the item.
    Anvil,
    /// A huge mushroom block: its default state (`ALL_OUTSIDE`).
    HugeMushroom,
    /// A fence gate: along the player's view, needs a solid block below.
    FenceGate,
    /// A trapdoor: on the side of a support block.
    Trapdoor,
    /// A carpet: needs a block below.
    Carpet,
    /// A torch: on the clicked side when it holds, else on a wall.
    Torch,
    /// A ladder: on the clicked wall when it holds, else on another one.
    Ladder,
    /// A chest: faces the player and pairs with a chest beside it.
    Chest,
    /// A block item whose rules are not modelled: checked with the default
    /// rules, never predicted.
    Unmodelled,
}

/// The placement rule of a block item id, `None` for an id that is not a
/// block item.
///
/// The ids are the `Item.registerItemBlock` registrations of MCP
/// `Item.registerItems` and the block classes behind them.
#[must_use]
pub fn rule_of(item_id: i16) -> Option<Rule> {
    let id = u16::try_from(item_id)
        .ok()
        .filter(|id| (1..256).contains(id))?;
    let rule = match id {
        1 | 97 => Rule::Subtype(if id == 1 { 6 } else { 5 }),
        3 | 24 | 168 | 179 => Rule::Subtype(2),
        5 => Rule::Subtype(5),
        12 | 19 | 139 => Rule::Subtype(1),
        98 => Rule::Subtype(3),
        35 | 95 | 159 | 160 => Rule::Subtype(15),
        18 => Rule::Leaves(4),
        161 => Rule::Leaves(2),
        17 => Rule::Log(4),
        162 => Rule::Log(2),
        170 => Rule::Hay,
        155 => Rule::Quartz,
        id if is_stairs(id) => Rule::Stairs,
        44 => Rule::Slab(43, 8),
        126 => Rule::Slab(125, 6),
        182 => Rule::Slab(181, 1),
        86 | 91 => Rule::Pumpkin,
        61 | 130 => Rule::FacesPlayer,
        120 => Rule::PortalFrame,
        23 | 29 | 33 | 158 => Rule::FacesEntity,
        154 => Rule::Hopper,
        145 => Rule::Anvil,
        99 | 100 => Rule::HugeMushroom,
        107 | 183..=187 => Rule::FenceGate,
        96 | 167 => Rule::Trapdoor,
        171 => Rule::Carpet,
        50 | 76 => Rule::Torch,
        65 => Rule::Ladder,
        54 | 146 => Rule::Chest,
        // Plain blocks whose `onBlockPlaced`, `onBlockPlacedBy` and
        // `canPlaceBlockAt` are the defaults (or only touch a tile entity).
        2
        | 4
        | 7
        | 13..=16
        | 20..=22
        | 25
        | 30
        | 41
        | 42
        | 45..=49
        | 52
        | 56..=58
        | 73
        | 79
        | 80
        | 82
        | 84
        | 85
        | 87..=89
        | 101
        | 102
        | 103
        | 110
        | 112
        | 113
        | 116
        | 121
        | 122
        | 123
        | 129
        | 133
        | 137
        | 138
        | 151
        | 152
        | 153
        | 165
        | 166
        | 169
        | 172..=174
        | 188..=192 => Rule::Plain,
        // Saplings, flowers, mushrooms, cactus, snow layers, lily pads, vines,
        // rails, plates, buttons, levers, tripwire hooks, double plants...
        _ => Rule::Unmodelled,
    };
    Some(rule)
}

/// `ItemBlock.canPlaceBlockOnSide` (and the `ItemSlab` override): whether
/// `onPlayerRightClick` may send the click at all. A `false` makes the
/// client skip the `Block Placement` of the block and use the item instead.
#[must_use]
pub fn can_place_on_side<W: BlockSource + ?Sized>(click: &PlaceClick<'_, W>) -> bool {
    let Some(rule) = rule_of(click.item.item_id) else {
        return true;
    };
    let face = wire_face(click.hit.face);
    let clicked = click.hit.block;
    if let Rule::Slab(_, variants) = rule {
        let variant = slab_variant(click.item.damage, variants);
        if merges_onto(block_at(click.world, clicked), click.item, variant, face) {
            return true;
        }
        let next = block_at(click.world, offset(clicked, face));
        if next.id() == item_block(click.item) && next.metadata() & 7 == variant {
            return true;
        }
    }
    let here = block_at(click.world, clicked);
    let (pos, side) = if here.id() == SNOW_LAYER {
        (clicked, Facing::Up)
    } else if is_replaceable(click.world, clicked) {
        (clicked, face)
    } else {
        (offset(clicked, face), face)
    };
    can_block_be_placed(click, rule, pos, side)
}

/// The blocks `ItemBlock.onItemUse` (or `ItemSlab.onItemUse`) sets on the
/// client, `None` when it sets nothing or the item is not modelled. A
/// `Some` also means the held stack lost one item (outside creative).
#[must_use]
pub fn predict<W: BlockSource + ?Sized>(click: &PlaceClick<'_, W>) -> Option<Placement> {
    let rule = rule_of(click.item.item_id)?;
    if rule == Rule::Unmodelled || click.item.stack_size <= 0 {
        return None;
    }
    let face = wire_face(click.hit.face);
    if let Rule::Slab(double, variants) = rule {
        if let Some(merged) = merge_slab(click, double, slab_variant(click.item.damage, variants)) {
            return merged;
        }
    }
    let pos = target(click.world, click.hit);
    // `World.setBlockState` refuses the cells outside the world, and
    // `onItemUse` a solid block in the top layer.
    if !(0..256).contains(&pos.y) || (pos.y == 255 && is_solid_rule(rule)) {
        return None;
    }
    if !can_block_be_placed(click, rule, pos, face) {
        return None;
    }
    let cursor = hit_cursor(click.hit);
    let placer = click.placer;
    let facing = placer.map_or(Facing::South, Placer::horizontal_facing);
    let block = item_block(click.item);
    let damage = click.item.damage;
    let metadata = match rule {
        Rule::Plain | Rule::Unmodelled => 0,
        Rule::Subtype(highest) => subtype(damage, highest),
        Rule::Carpet => subtype(damage, 15),
        Rule::Leaves(variants) => {
            let variant = (damage & 3) as u8;
            if variant >= variants {
                return None;
            }
            variant | 4 | (damage & 8) as u8
        }
        Rule::Log(variants) => {
            let variant = (damage & 3) as u8;
            if variant >= variants {
                return None;
            }
            variant | axis_bits(face)
        }
        Rule::Hay => axis_bits(face),
        Rule::Quartz => match subtype(damage, 4) {
            2 => match face {
                Facing::West | Facing::East => 3,
                Facing::North | Facing::South => 4,
                Facing::Down | Facing::Up => 2,
            },
            1 => 1,
            _ => 0,
        },
        Rule::Stairs => stair_facing(facing) | if bottom_half(face, cursor[1]) { 0 } else { 4 },
        Rule::Slab(_, variants) => {
            slab_variant(damage, variants) | if bottom_half(face, cursor[1]) { 0 } else { 8 }
        }
        Rule::Pumpkin => horizontal_index(opposite(facing)),
        Rule::FacesPlayer => index(opposite(facing)),
        Rule::PortalFrame => horizontal_index(opposite(facing)),
        Rule::FacesEntity => index(facing_from_entity(placer, facing, pos)),
        Rule::Hopper => match opposite(face) {
            Facing::Up => index(Facing::Down),
            other => index(other),
        },
        Rule::Anvil => {
            let damage = u8::try_from(damage).ok().filter(|damage| *damage <= 2)?;
            horizontal_index(rotate_y(facing)) | damage << 2
        }
        Rule::HugeMushroom => 14,
        Rule::FenceGate => horizontal_index(facing),
        Rule::Trapdoor => trapdoor_facing(face) | if cursor[1] > 0.5 { 8 } else { 0 },
        Rule::Torch => torch_metadata(torch_facing(click.world, pos, face)),
        Rule::Ladder => index(ladder_facing(click.world, pos, face)),
        Rule::Chest => return Some(place_chest(click.world, block, pos, facing)),
    };
    Some(Placement::one(pos, BlockState::new(block, metadata)))
}

/// The cell `ItemBlock.onItemUse` places into: the clicked block when it is
/// replaceable (tall grass, a single snow layer...), else the cell next to
/// the clicked face.
#[must_use]
pub fn target<W: BlockSource + ?Sized>(world: &W, hit: &RayHit) -> BlockPos {
    if is_replaceable(world, hit.block) {
        hit.block
    } else {
        offset(hit.block, wire_face(hit.face))
    }
}

/// The snow layer block (`Blocks.snow_layer`), which
/// `ItemBlock.canPlaceBlockOnSide` places into from any side.
const SNOW_LAYER: u16 = 78;

/// The block a block item places.
fn item_block(item: &ItemStack) -> u16 {
    u16::try_from(item.item_id).unwrap_or(0)
}

fn block_at<W: BlockSource + ?Sized>(world: &W, pos: BlockPos) -> BlockState {
    world.block(pos.x, pos.y, pos.z)
}

fn offset(pos: BlockPos, face: Facing) -> BlockPos {
    let delta = face_offset(face);
    BlockPos::new(pos.x + delta.x, pos.y + delta.y, pos.z + delta.z)
}

/// `(float)(hitVec - hitPos)` of `onPlayerRightClick`.
fn hit_cursor(hit: &RayHit) -> [f32; 3] {
    [
        (hit.position.x - f64::from(hit.block.x)) as f32,
        (hit.position.y - f64::from(hit.block.y)) as f32,
        (hit.position.z - f64::from(hit.block.z)) as f32,
    ]
}

/// A subtype damage within the variants, 0 beyond them.
fn subtype(damage: i16, highest: u8) -> u8 {
    u8::try_from(damage)
        .ok()
        .filter(|meta| *meta <= highest)
        .unwrap_or(0)
}

/// The slab variant of a stack (`BlockSlab.getVariant`: `damage & 7`, the
/// first variant when out of range).
fn slab_variant(damage: i16, variants: u8) -> u8 {
    let variant = (damage & 7) as u8;
    if variant < variants {
        variant
    } else {
        0
    }
}

/// Whether a click on `face` of the slab `state` turns it into a double slab
/// (`ItemSlab`: the top of a bottom slab or the bottom of a top slab, of the
/// held variant).
fn merges_onto(state: BlockState, item: &ItemStack, variant: u8, face: Facing) -> bool {
    let top = state.metadata() & 8 != 0;
    state.id() == item_block(item)
        && ((face == Facing::Up && !top) || (face == Facing::Down && top))
        && state.metadata() & 7 == variant
}

/// `ItemSlab.onItemUse` before it falls back to `ItemBlock`: the clicked slab
/// or the slab next to the face becomes a double slab. `Some(None)` is a
/// merge the entities refuse (the click is spent, nothing changes).
fn merge_slab<W: BlockSource + ?Sized>(
    click: &PlaceClick<'_, W>,
    double: u16,
    variant: u8,
) -> Option<Option<Placement>> {
    let face = wire_face(click.hit.face);
    let clicked = click.hit.block;
    let target = if merges_onto(block_at(click.world, clicked), click.item, variant, face) {
        clicked
    } else {
        let next = offset(clicked, face);
        let state = block_at(click.world, next);
        if state.id() != item_block(click.item) || state.metadata() & 7 != variant {
            return None;
        }
        next
    };
    let cube = world_box(Aabb::new(DVec3::ZERO, DVec3::ONE), target);
    if click.placer.is_some_and(|placer| placer.obstructs(&cube)) {
        return Some(None);
    }
    Some(Some(Placement::one(
        target,
        BlockState::new(double, variant),
    )))
}

/// `World.canBlockBePlaced` for the block of `rule` at `pos`, clicked on
/// `side`.
fn can_block_be_placed<W: BlockSource + ?Sized>(
    click: &PlaceClick<'_, W>,
    rule: Rule,
    pos: BlockPos,
    side: Facing,
) -> bool {
    let block = item_block(click.item);
    if let (Some(placer), Some(area)) = (click.placer, default_box(click.world, block, pos)) {
        if placer.obstructs(&area) {
            return false;
        }
    }
    has_replaceable_material(block_at(click.world, pos))
        && can_place_at(click.world, rule, block, pos, side)
}

/// The collision box of the block's default state at `pos`
/// (`getCollisionBoundingBox(world, pos, getDefaultState())`), `None` when
/// it has none. A block that collides with several boxes stands in with
/// the box around them all.
fn default_box<W: BlockSource + ?Sized>(world: &W, block: u16, pos: BlockPos) -> Option<Aabb> {
    let shape = shape_of(BlockState::new(block, 0), world, pos);
    let union = shape
        .boxes()
        .iter()
        .copied()
        .reduce(|a, b| Aabb::new(a.min.min(b.min), a.max.max(b.max)))?;
    Some(world_box(union, pos))
}

fn world_box(local: Aabb, pos: BlockPos) -> Aabb {
    let origin = DVec3::new(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
    local.offset(origin)
}

/// `Block.isReplaceable`: the replaceable materials (air, liquids, fire,
/// vines and tall grass, snow), narrowed by the overrides of `BlockSnow`
/// (one layer only) and `BlockDoublePlant` (grass and fern only).
fn is_replaceable<W: BlockSource + ?Sized>(world: &W, pos: BlockPos) -> bool {
    let state = block_at(world, pos);
    match state.id() {
        SNOW_LAYER => state.metadata() & 7 == 0,
        DOUBLE_PLANT => {
            let lower = if state.metadata() & 8 != 0 {
                block_at(world, BlockPos::new(pos.x, pos.y - 1, pos.z))
            } else {
                state
            };
            lower.id() != DOUBLE_PLANT || matches!(lower.metadata() & 7, 2 | 3)
        }
        _ => has_replaceable_material(state),
    }
}

/// The double plant block (`Blocks.double_plant`).
const DOUBLE_PLANT: u16 = 175;

/// `Material.isReplaceable` (MCP `Material.java`: air, water, lava, fire,
/// vine, snow): air, liquids, fire, tall grass, dead bushes, vines, double
/// plants and snow layers.
fn has_replaceable_material(state: BlockState) -> bool {
    matches!(state.id(), 0 | 8..=11 | 31 | 32 | 51 | 78 | 106 | 175)
}

/// Whether the rule's block is solid (`Material.isSolid`): all but the
/// torches, ladders and carpets of the modelled ones.
fn is_solid_rule(rule: Rule) -> bool {
    !matches!(rule, Rule::Torch | Rule::Ladder | Rule::Carpet)
}

/// `Material.isSolid` of a block: false for air, liquids, fire, portals and
/// the `MaterialLogic` blocks (plants, vines, circuits, carpet, snow layer).
fn is_solid(state: BlockState) -> bool {
    !matches!(
        state.id(),
        0 | 6
            | 8..=11
            | 27
            | 28
            | 31
            | 32
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

/// The block's `canReplace` (`canPlaceBlockOnSide`, by default
/// `canPlaceBlockAt`) on top of the replaceable material.
fn can_place_at<W: BlockSource + ?Sized>(
    world: &W,
    rule: Rule,
    block: u16,
    pos: BlockPos,
    side: Facing,
) -> bool {
    let below = BlockPos::new(pos.x, pos.y - 1, pos.z);
    match rule {
        // `BlockPumpkin.canPlaceBlockAt`.
        Rule::Pumpkin => has_solid_top(block_at(world, below)),
        // `BlockFenceGate.canPlaceBlockAt`.
        Rule::FenceGate => is_solid(block_at(world, below)),
        // `BlockTrapDoor.canPlaceBlockOnSide`.
        Rule::Trapdoor => {
            is_horizontal(side) && holds_trapdoor(block_at(world, offset(pos, opposite(side))))
        }
        // `BlockCarpet.canBlockStay`.
        Rule::Carpet => !block_at(world, below).is_air(),
        // `BlockTorch.canPlaceBlockAt`: any of up, north, south, west, east.
        Rule::Torch => [
            Facing::Up,
            Facing::North,
            Facing::South,
            Facing::West,
            Facing::East,
        ]
        .into_iter()
        .any(|facing| torch_holds(world, pos, facing)),
        // `BlockLadder.canPlaceBlockAt`: a normal cube west, east, north or south.
        Rule::Ladder => [Facing::West, Facing::East, Facing::North, Facing::South]
            .into_iter()
            .any(|facing| is_normal_cube(block_at(world, offset(pos, facing)))),
        Rule::Chest => chest_fits(world, block, pos),
        _ => true,
    }
}

/// `World.doesBlockHaveSolidTopSurface`: an opaque full cube, a top stair or
/// slab, a hopper, or a snow layer of seven layers.
fn has_solid_top(state: BlockState) -> bool {
    let meta = state.metadata();
    match state.id() {
        id if is_stairs(id) => meta & 4 != 0,
        44 | 126 | 182 => meta & 8 != 0,
        154 => true,
        SNOW_LAYER => meta & 7 == 6,
        // An opaque full cube: the normal cubes and the redstone block.
        152 => true,
        _ => is_normal_cube(state),
    }
}

/// `BlockTrapDoor.isValidSupportBlock`: an opaque full cube, glowstone, a
/// slab or a stair.
fn holds_trapdoor(state: BlockState) -> bool {
    let id = state.id();
    is_normal_cube(state)
        || matches!(id, 89 | 152 | 43 | 44 | 125 | 126 | 181 | 182)
        || is_stairs(id)
}

/// `BlockTorch.canPlaceAt`: a torch facing `facing` hangs on a normal cube
/// behind it, or stands on a block it can stand on.
fn torch_holds<W: BlockSource + ?Sized>(world: &W, pos: BlockPos, facing: Facing) -> bool {
    let support = block_at(world, offset(pos, opposite(facing)));
    (is_horizontal(facing) && is_normal_cube(support))
        || (facing == Facing::Up && torch_stands_on(support))
}

/// `BlockTorch.canPlaceOn`: a solid top, a fence, glass, a wall or stained glass.
fn torch_stands_on(state: BlockState) -> bool {
    has_solid_top(state) || matches!(state.id(), 20 | 85 | 95 | 113 | 139 | 188..=192)
}

/// `BlockTorch.onBlockPlaced`: the clicked side when it holds, else the
/// first wall (north, east, south, west), else upright.
fn torch_facing<W: BlockSource + ?Sized>(world: &W, pos: BlockPos, side: Facing) -> Facing {
    if torch_holds(world, pos, side) {
        return side;
    }
    HORIZONTALS
        .into_iter()
        .find(|facing| is_normal_cube(block_at(world, offset(pos, opposite(*facing)))))
        .unwrap_or(Facing::Up)
}

/// `BlockTorch.getMetaFromState`.
fn torch_metadata(facing: Facing) -> u8 {
    match facing {
        Facing::East => 1,
        Facing::West => 2,
        Facing::South => 3,
        Facing::North => 4,
        Facing::Up | Facing::Down => 5,
    }
}

/// `BlockLadder.onBlockPlaced`: the clicked wall when it holds, else the
/// first one that does (north, east, south, west), else north.
fn ladder_facing<W: BlockSource + ?Sized>(world: &W, pos: BlockPos, side: Facing) -> Facing {
    let holds = |facing: Facing| is_normal_cube(block_at(world, offset(pos, opposite(facing))));
    if is_horizontal(side) && holds(side) {
        return side;
    }
    HORIZONTALS
        .into_iter()
        .find(|facing| holds(*facing))
        .unwrap_or(Facing::North)
}

/// `BlockChest.canPlaceBlockAt`: at most one chest beside the cell, and that
/// chest not already double.
fn chest_fits<W: BlockSource + ?Sized>(world: &W, block: u16, pos: BlockPos) -> bool {
    let mut beside = 0;
    for facing in [Facing::West, Facing::East, Facing::North, Facing::South] {
        let next = offset(pos, facing);
        if block_at(world, next).id() == block {
            let double = HORIZONTALS
                .into_iter()
                .any(|side| block_at(world, offset(next, side)).id() == block);
            if double {
                return false;
            }
            beside += 1;
        }
    }
    beside <= 1
}

/// `BlockChest.onBlockPlaced` then `onBlockPlacedBy`: the chest faces the
/// player, and a chest beside it along the right axis turns to match.
fn place_chest<W: BlockSource + ?Sized>(
    world: &W,
    block: u16,
    pos: BlockPos,
    facing: Facing,
) -> Placement {
    let turned = opposite(facing);
    let chest = |side: Facing| {
        let next = offset(pos, side);
        (block_at(world, next).id() == block).then_some(next)
    };
    let [north, south, west, east] =
        [Facing::North, Facing::South, Facing::West, Facing::East].map(chest);
    let along_x = matches!(turned, Facing::West | Facing::East);
    let partner = if north.is_none() && south.is_none() && west.is_none() && east.is_none() {
        Some(None)
    } else if !along_x || (north.is_none() && south.is_none()) {
        (!along_x && (west.is_some() || east.is_some())).then(|| west.or(east))
    } else {
        Some(north.or(south))
    };
    let state = |facing: Facing| BlockState::new(block, index(facing));
    match partner {
        Some(neighbour) => Placement {
            changes: [
                neighbour.map(|next| (next, state(turned))),
                Some((pos, state(turned))),
            ],
        },
        // The chest keeps the facing `onBlockPlaced` gave it.
        None => Placement::one(pos, state(facing)),
    }
}

/// `BlockPistonBase.getFacingFromEntity`: up or down when the player's eyes
/// are well above or below a block close by, else towards the player.
fn facing_from_entity(placer: Option<&Placer<'_>>, facing: Facing, pos: BlockPos) -> Facing {
    if let Some(placer) = placer {
        let near = |feet: f64, block: i32| (feet as f32 - block as f32).abs() < 2.0;
        if near(placer.position.x, pos.x) && near(placer.position.z, pos.z) {
            let eyes = placer.position.y + crate::physics::eye_height(placer.sneaking);
            if eyes - f64::from(pos.y) > 2.0 {
                return Facing::Up;
            }
            if f64::from(pos.y) - eyes > 0.0 {
                return Facing::Down;
            }
        }
    }
    opposite(facing)
}

/// Whether the half chosen by a click on `face` at `hit_y` is the bottom
/// one (`BlockStairs` and `BlockSlab.onBlockPlaced`).
fn bottom_half(face: Facing, hit_y: f32) -> bool {
    face != Facing::Down && (face == Facing::Up || f64::from(hit_y) <= 0.5)
}

/// The axis bits of a log or hay bale (`BlockLog.EnumAxis`): 0 for Y, 4 for
/// X, 8 for Z.
fn axis_bits(face: Facing) -> u8 {
    match face {
        Facing::Down | Facing::Up => 0,
        Facing::West | Facing::East => 4,
        Facing::North | Facing::South => 8,
    }
}

/// `BlockStairs.getMetaFromState`: `5 - facing.getIndex()`.
fn stair_facing(facing: Facing) -> u8 {
    5 - index(facing)
}

/// `BlockTrapDoor.getMetaForFacing`.
fn trapdoor_facing(facing: Facing) -> u8 {
    match facing {
        Facing::South => 1,
        Facing::West => 2,
        Facing::East => 3,
        _ => 0,
    }
}

/// `EnumFacing.Plane.HORIZONTAL` order.
const HORIZONTALS: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];

/// `EnumFacing.getHorizontal`.
fn horizontal(index: u8) -> Facing {
    match index & 3 {
        0 => Facing::South,
        1 => Facing::West,
        2 => Facing::North,
        _ => Facing::East,
    }
}

/// `EnumFacing.getIndex`.
fn index(facing: Facing) -> u8 {
    facing.code()
}

/// `EnumFacing.getHorizontalIndex` (vertical faces have none; 0 stands in).
fn horizontal_index(facing: Facing) -> u8 {
    match facing {
        Facing::West => 1,
        Facing::North => 2,
        Facing::East => 3,
        _ => 0,
    }
}

fn opposite(facing: Facing) -> Facing {
    match facing {
        Facing::Down => Facing::Up,
        Facing::Up => Facing::Down,
        Facing::North => Facing::South,
        Facing::South => Facing::North,
        Facing::West => Facing::East,
        Facing::East => Facing::West,
    }
}

/// `EnumFacing.rotateY`: a quarter turn clockwise seen from above.
fn rotate_y(facing: Facing) -> Facing {
    match facing {
        Facing::North => Facing::East,
        Facing::East => Facing::South,
        Facing::South => Facing::West,
        Facing::West => Facing::North,
        vertical => vertical,
    }
}

fn is_horizontal(facing: Facing) -> bool {
    !matches!(facing, Facing::Down | Facing::Up)
}

#[cfg(test)]
mod tests;
