// SPDX-License-Identifier: MIT OR Apache-2.0

//! Connected textures (CTM): rules that swap the texture of a block face for
//! one of a set of tiles, chosen from the blocks around it.
//!
//! Two halves, both free of the mesher and the GPU:
//! - [`parse`] turns `ctm/**/*.properties` files into a [`CtmRules`] list.
//!   A bad file gives a typed [`CtmError`] and costs only its own rule.
//! - [`select`] holds the tile choice of each [`Method`]. It asks the mesher
//!   about the world through the small [`CtmNeighbours`] trait and allocates
//!   nothing, so tests can drive it with a fake world.
//!
//! Tiles are named by atlas key (`blocks/ctm/glass/0`, `blocks/glass`): the
//! client puts the pack's tile PNGs into the block atlas under those keys.
//!
//! The behaviour follows OptiFine's `ConnectedProperties` and
//! `ConnectedTextures` (ADR-0088, `docs/sources.md`, "OptiFine features"),
//! reimplemented from observed behaviour.

mod parse;
mod select;

use crate::{BiomeFilter, RangeList};
use thiserror::Error;

pub use parse::parse_rules;
pub use select::{pillar_axis_and_metadata, CtmNeighbours};

/// Why a connected-texture file was dropped.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CtmError {
    /// The `.properties` syntax is broken.
    #[error(transparent)]
    Properties(#[from] crate::PropertiesError),
    /// The `method` value is none of the known names.
    #[error("unknown method {0:?}")]
    UnknownMethod(String),
    /// There is no `tiles` key.
    #[error("no tiles specified")]
    NoTiles,
    /// A tile name cannot be turned into a block-atlas texture.
    #[error("invalid tile {0:?}")]
    InvalidTile(String),
    /// The tile count does not fit the method.
    #[error("method {method} needs {expected} tiles, found {found}")]
    TileCount {
        /// The method name.
        method: &'static str,
        /// What the method needs, as text (`47 or more`, `4`).
        expected: &'static str,
        /// What the file lists.
        found: usize,
    },
    /// `repeat` needs `width` and `height` between 1 and 16.
    #[error("invalid repeat size {width}x{height}")]
    InvalidRepeatSize {
        /// The `width` value, 0 when absent.
        width: i64,
        /// The `height` value, 0 when absent.
        height: i64,
    },
    /// A `matchBlocks` entry names no block, or uses block-state properties
    /// (`color=red`), which Umicraft does not resolve.
    #[error("unsupported block match {0:?}")]
    BadBlockMatch(String),
    /// `connect` is none of `block`, `tile`, `material`.
    #[error("unknown connect {0:?}")]
    UnknownConnect(String),
    /// A `faces` word is unknown.
    #[error("unknown face {0:?}")]
    UnknownFace(String),
    /// `renderPass` above 0 is not supported.
    #[error("render pass {0} is not supported")]
    RenderPass(i64),
    /// A list value (`metadata`, `heights`) is malformed.
    #[error("invalid value for {key}: {text:?}")]
    InvalidList {
        /// The property key.
        key: &'static str,
        /// The offending text.
        text: String,
    },
}

/// A face of a block, numbered as OptiFine numbers them (`getSide`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// -Y.
    Down = 0,
    /// +Y.
    Up = 1,
    /// -Z.
    North = 2,
    /// +Z.
    South = 3,
    /// -X.
    West = 4,
    /// +X.
    East = 5,
}

impl Side {
    /// The OptiFine index, 0 to 5.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// The axis of a log or quartz pillar. Face names in a rule refer to the
/// pillar standing upright, so the face is turned back before the test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Upright (and every block that is no pillar).
    Y,
    /// Lying along Z.
    Z,
    /// Lying along X.
    X,
}

/// The set of faces a rule applies to, as a bit per [`Side`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaceMask(u8);

impl FaceMask {
    /// Every face.
    pub const ALL: Self = Self(0b11_1111);
    const SIDES: u8 = 0b11_1100;

    /// Whether `side` of a block lying along `axis` is in the set. The side
    /// is mapped back to the upright pillar first (`fixSideByAxis`).
    #[must_use]
    pub fn allows(self, side: Side, axis: Axis) -> bool {
        let upright = match (axis, side) {
            (Axis::Z, Side::Down) => Side::North,
            (Axis::Z, Side::Up) => Side::South,
            (Axis::Z, Side::North) => Side::Up,
            (Axis::Z, Side::South) => Side::Down,
            (Axis::X, Side::Down) => Side::West,
            (Axis::X, Side::Up) => Side::East,
            (Axis::X, Side::West) => Side::Up,
            (Axis::X, Side::East) => Side::Down,
            (_, other) => other,
        };
        self.0 & (1 << upright.index()) != 0
    }
}

/// How a neighbour is tested (`connect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connect {
    /// Only an identical block state connects.
    Block,
    /// A neighbour whose face shows the same sprite connects.
    Tile,
    /// A neighbour of the same material connects.
    Material,
}

/// The tile-choice method of a rule (`method`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// The 47-tile table (`ctm`, `glass`).
    Ctm,
    /// Four tiles along the horizontal (`horizontal`, `bookshelf`).
    Horizontal,
    /// Four tiles along the vertical.
    Vertical,
    /// Seven tiles: horizontal first, vertical as fallback.
    HorizontalVertical,
    /// Seven tiles: vertical first, horizontal as fallback.
    VerticalHorizontal,
    /// One tile when the block above connects.
    Top,
    /// A tile picked by position hash, optionally weighted.
    Random,
    /// A `width` by `height` pattern repeated over the faces.
    Repeat {
        /// Pattern width, 1 to 16.
        width: u8,
        /// Pattern height, 1 to 16.
        height: u8,
    },
    /// Always the one tile.
    Fixed,
}

/// Which faces of a symmetric block share one random pick (`symmetry`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symmetry {
    /// Every face picks on its own.
    None,
    /// Opposite faces share a pick.
    Opposite,
    /// All faces share a pick.
    All,
}

impl Symmetry {
    /// The divisor `side / n * n` of `getConnectedTextureRandom`.
    const fn divisor(self) -> usize {
        match self {
            Self::None => 1,
            Self::Opposite => 2,
            Self::All => 6,
        }
    }
}

/// One `matchBlocks` entry: a block id and an optional metadata list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockMatch {
    /// The block id.
    pub id: u16,
    /// The metadata values accepted, every value when `None`.
    pub metadata: Option<RangeList>,
}

/// Cumulative `weights` of a random rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weights {
    /// Running sums, one per tile.
    pub cumulative: Vec<u32>,
    /// The sum of all weights, at least 1.
    pub total: u32,
}

/// One connected-texture rule.
#[derive(Debug, Clone, PartialEq)]
pub struct CtmRule {
    /// The file name without folder and extension.
    pub name: String,
    /// Blocks the rule applies to; `None` matches every block that shows a
    /// `match_tiles` sprite.
    pub match_blocks: Option<Vec<BlockMatch>>,
    /// Metadata values accepted, every value when `None`.
    pub metadata: Option<RangeList>,
    /// Sprites the rule replaces, as atlas keys. `None` with `match_blocks`
    /// `None` too means "the sprite named like the file" (see
    /// [`CtmRule::name`]); the client resolves that against its atlas.
    pub match_tiles: Option<Vec<String>>,
    /// The tile choice method.
    pub method: Method,
    /// The tiles as atlas keys, in file order.
    pub tiles: Vec<String>,
    /// How neighbours are tested.
    pub connect: Connect,
    /// The faces the rule applies to.
    pub faces: FaceMask,
    /// The biomes it applies to; every biome when `None`.
    pub biomes: Option<BiomeFilter>,
    /// The heights it applies to; every height when `None`.
    pub heights: Option<RangeList>,
    /// Whether a random pick starts from the bottom of a stack of the same
    /// block (`linked`).
    pub linked: bool,
    /// Random tile weights, when given.
    pub weights: Option<Weights>,
    /// Random symmetry.
    pub symmetry: Symmetry,
    /// The largest metadata the rule names (-1 when none), which decides how
    /// pillar metadata is read.
    pub metadata_max: i32,
}

impl CtmRule {
    /// Whether the block `id` is one the rule names (`matchBlocks`), any
    /// block when the rule names none.
    #[must_use]
    pub fn matches_block_id(&self, id: u16) -> bool {
        self.match_blocks
            .as_ref()
            .is_none_or(|blocks| blocks.iter().any(|entry| entry.id == id))
    }

    /// Whether the block `id` with `metadata` passes the block and metadata
    /// filters of the rule.
    #[must_use]
    pub fn matches_block(&self, id: u16, metadata: u8) -> bool {
        let block = self.match_blocks.as_ref().is_none_or(|blocks| {
            blocks.iter().any(|entry| {
                entry.id == id
                    && entry
                        .metadata
                        .as_ref()
                        .is_none_or(|list| list.contains(i64::from(metadata)))
            })
        });
        block
            && self
                .metadata
                .as_ref()
                .is_none_or(|list| list.contains(i64::from(metadata)))
    }

    /// Whether the rule accepts the height `y`.
    #[must_use]
    pub fn matches_height(&self, y: i32) -> bool {
        self.heights
            .as_ref()
            .is_none_or(|list| list.contains(i64::from(y)))
    }
}

/// The parsed rules of every pack, in priority order: the first rule of a
/// block or sprite that yields a tile wins.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CtmRules {
    /// The rules, in file order (sorted by path).
    pub rules: Vec<CtmRule>,
}

#[cfg(test)]
mod tests;
