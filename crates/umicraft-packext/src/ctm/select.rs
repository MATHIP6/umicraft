// SPDX-License-Identifier: MIT OR Apache-2.0

//! The tile choice of each [`Method`](super::Method).
//!
//! Every function answers with the index of a tile in
//! [`CtmRule::tiles`](super::CtmRule::tiles), or `None` when the rule leaves
//! the texture alone. Offsets are world offsets: east is +X, south is +Z, up
//! is +Y.

use super::{Axis, CtmRule, Method, Side};

/// What the mesher tells the selection about the world around one block.
pub trait CtmNeighbours {
    /// The position of the block whose face is being textured.
    fn position(&self) -> [i32; 3];

    /// Whether the block at `offset` from the block counts as connected under
    /// the rule's [`Connect`](super::Connect) mode.
    fn connects(&self, offset: [i32; 3]) -> bool;

    /// Whether the block at `offset` is the same block (same id, any
    /// metadata) as the one being textured. Used by `linked` random rules.
    fn same_block(&self, offset: [i32; 3]) -> bool;
}

const W: [i32; 3] = [-1, 0, 0];
const E: [i32; 3] = [1, 0, 0];
const D: [i32; 3] = [0, -1, 0];
const U: [i32; 3] = [0, 1, 0];
const N: [i32; 3] = [0, 0, -1];
const S: [i32; 3] = [0, 0, 1];

const fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// The four edge neighbours of the 47-tile method, per side: left, right, up,
/// down as seen on the face (`getConnectedTextureCtm`).
const EDGES: [[[i32; 3]; 4]; 6] = [
    [W, E, N, S],
    [W, E, S, N],
    [E, W, D, U],
    [W, E, D, U],
    [N, S, D, U],
    [S, N, D, U],
];

/// The four corner neighbours, per side, in the order the refinement reads
/// them.
const CORNERS: [[[i32; 3]; 4]; 6] = [
    [add(E, N), add(W, N), add(E, S), add(W, S)],
    [add(E, S), add(W, S), add(E, N), add(W, N)],
    [add(W, D), add(E, D), add(W, U), add(E, U)],
    [add(E, D), add(W, D), add(E, U), add(W, U)],
    [add(D, S), add(D, N), add(U, S), add(U, N)],
    [add(D, N), add(D, S), add(U, N), add(U, S)],
];

/// The tile of the 47-tile table for each combination of the four edges,
/// indexed by `left | right << 1 | up << 2 | down << 3`.
const BASE_TILE: [usize; 16] = [0, 3, 1, 2, 12, 15, 13, 14, 36, 39, 37, 38, 24, 27, 25, 26];

/// The pair of neighbours a horizontal or vertical rule reads, or `None` for
/// a face parallel to the pillar axis. `Pair.0` is the "first" neighbour.
type Pair = Option<([i32; 3], [i32; 3])>;

fn horizontal_pair(axis: Axis, side: Side) -> Pair {
    match (axis, side) {
        (Axis::Y, Side::North) => Some((E, W)),
        (Axis::Y, Side::South) => Some((W, E)),
        (Axis::Y, Side::West) => Some((N, S)),
        (Axis::Y, Side::East) => Some((S, N)),
        (Axis::Z, Side::Down | Side::Up) => Some((W, E)),
        (Axis::Z, Side::West) => Some((D, U)),
        (Axis::Z, Side::East) => Some((U, D)),
        (Axis::X, Side::Down | Side::Up) => Some((N, S)),
        (Axis::X, Side::North) => Some((D, U)),
        (Axis::X, Side::South) => Some((U, D)),
        _ => None,
    }
}

fn vertical_pair(axis: Axis, side: Side) -> Pair {
    match (axis, side) {
        (Axis::Y, Side::Down | Side::Up) => None,
        (Axis::Y, _) => Some((D, U)),
        (Axis::Z, Side::North | Side::South) => None,
        (Axis::Z, _) => Some((S, N)),
        (Axis::X, Side::West | Side::East) => None,
        (Axis::X, _) => Some((W, E)),
    }
}

/// The neighbour above a face of a pillar, for `top`.
fn top_offset(axis: Axis, side: Side) -> Option<[i32; 3]> {
    match (axis, side) {
        (Axis::Y, Side::Down | Side::Up)
        | (Axis::Z, Side::North | Side::South)
        | (Axis::X, Side::West | Side::East) => None,
        (Axis::Y, _) => Some(U),
        (Axis::Z, _) => Some(S),
        (Axis::X, _) => Some(E),
    }
}

/// The 4-tile choice of the horizontal and vertical methods: both neighbours
/// 1, only the first 2, only the second 0, none 3.
fn pair_tile(pair: Pair, ctx: &impl CtmNeighbours) -> Option<usize> {
    let (first, second) = pair?;
    Some(match (ctx.connects(first), ctx.connects(second)) {
        (true, true) => 1,
        (true, false) => 2,
        (false, true) => 0,
        (false, false) => 3,
    })
}

/// The seven-tile combination: the first method decides unless it shows its
/// "no neighbour" tile, then the second decides and its tiles 0 to 2 move to
/// 4 to 6.
fn combined(first: Option<usize>, second: impl FnOnce() -> Option<usize>) -> Option<usize> {
    match first {
        Some(tile) if tile != 3 => Some(tile),
        _ => second().map(|tile| if tile < 3 { tile + 4 } else { tile }),
    }
}

impl CtmRule {
    /// The tile this rule picks for `side` of the block `ctx` describes, or
    /// `None` when the rule leaves the texture alone. `axis` is the pillar
    /// axis of the block; `fancy` turns on the corner tests of the 47-tile
    /// method.
    ///
    /// The filters (block, face, height, biome) are not checked here: see
    /// [`CtmRule::matches_block`] and [`FaceMask::allows`](super::FaceMask::allows).
    #[must_use]
    pub fn select(
        &self,
        ctx: &impl CtmNeighbours,
        side: Side,
        axis: Axis,
        fancy: bool,
    ) -> Option<usize> {
        match self.method {
            Method::Ctm => Some(ctm_tile(ctx, side, fancy)),
            Method::Horizontal => pair_tile(horizontal_pair(axis, side), ctx),
            Method::Vertical => pair_tile(vertical_pair(axis, side), ctx),
            Method::HorizontalVertical => {
                combined(pair_tile(horizontal_pair(axis, side), ctx), || {
                    pair_tile(vertical_pair(axis, side), ctx)
                })
            }
            Method::VerticalHorizontal => {
                combined(pair_tile(vertical_pair(axis, side), ctx), || {
                    pair_tile(horizontal_pair(axis, side), ctx)
                })
            }
            Method::Top => top_offset(axis, side)
                .filter(|offset| ctx.connects(*offset))
                .map(|_| 0),
            Method::Random => Some(self.random_tile(ctx, side)),
            Method::Repeat { width, height } => Some(repeat_tile(
                ctx.position(),
                side,
                usize::from(width),
                usize::from(height),
                self.tiles.len(),
            )),
            Method::Fixed => Some(0),
        }
    }

    fn random_tile(&self, ctx: &impl CtmNeighbours, side: Side) -> usize {
        if self.tiles.len() == 1 {
            return 0;
        }
        let divisor = self.symmetry.divisor();
        let face = (side.index() / divisor * divisor) as i32;
        let mut position = ctx.position();
        if self.linked {
            let mut depth = 0;
            while position[1] - depth > 0 && ctx.same_block([0, -(depth + 1), 0]) {
                depth += 1;
            }
            position[1] -= depth;
        }
        let hash = position_hash(position, face) & i32::MAX;
        match &self.weights {
            None => hash as usize % self.tiles.len(),
            Some(weights) => {
                let pick = hash as u32 % weights.total;
                weights
                    .cumulative
                    .iter()
                    .position(|&sum| pick < sum)
                    .unwrap_or(0)
            }
        }
    }
}

/// The 47-tile choice for one face: edges first, then (Fancy) corners.
fn ctm_tile(ctx: &impl CtmNeighbours, side: Side, fancy: bool) -> usize {
    let at = |offset: [i32; 3]| ctx.connects(offset);
    let edges = EDGES[side.index()];
    let mask = edges.iter().enumerate().fold(0, |mask, (bit, offset)| {
        mask | (usize::from(at(*offset)) << bit)
    });
    let base = BASE_TILE[mask];
    if base == 0 || !fancy {
        return base;
    }
    let [c0, c1, c2, c3] = CORNERS[side.index()].map(|offset| !at(offset));
    refine(base, [c0, c1, c2, c3])
}

/// Replaces a base tile by the tile that also draws the missing corners.
/// `open[i]` is true when corner `i` is not connected. The chain follows the
/// order of `getConnectedTextureCtm`.
fn refine(base: usize, open: [bool; 4]) -> usize {
    let [o0, o1, o2, o3] = open;
    match base {
        13 if o0 => 4,
        15 if o1 => 5,
        37 if o2 => 16,
        39 if o3 => 17,
        14 if o0 && o1 => 7,
        25 if o0 && o2 => 6,
        27 if o3 && o1 => 19,
        38 if o3 && o2 => 18,
        14 if !o0 && o1 => 31,
        25 if o0 && !o2 => 30,
        27 if !o3 && o1 => 41,
        38 if o3 && !o2 => 40,
        14 if o0 && !o1 => 29,
        25 if !o0 && o2 => 28,
        27 if o3 && !o1 => 43,
        38 if !o3 && o2 => 42,
        26 => match open {
            [true, true, true, true] => 46,
            [false, true, true, true] => 9,
            [true, false, true, true] => 21,
            [true, true, false, true] => 8,
            [true, true, true, false] => 20,
            [true, true, false, false] => 11,
            [false, false, true, true] => 22,
            [false, true, false, true] => 23,
            [true, false, true, false] => 10,
            [true, false, false, true] => 34,
            [false, true, true, false] => 35,
            [true, false, false, false] => 32,
            [false, true, false, false] => 33,
            [false, false, true, false] => 44,
            [false, false, false, true] => 45,
            [false, false, false, false] => 26,
        },
        _ => base,
    }
}

/// The tile of a `repeat` pattern on `side` of the block at `position`.
fn repeat_tile(position: [i32; 3], side: Side, width: usize, height: usize, tiles: usize) -> usize {
    if tiles == 1 {
        return 0;
    }
    let [x, y, z] = position;
    let (u, v) = match side {
        Side::Down | Side::Up => (x, z),
        Side::North => (-x - 1, -y),
        Side::South => (x, -y),
        Side::West => (z, -y),
        Side::East => (-z - 1, -y),
    };
    let u = u.rem_euclid(width as i32) as usize;
    let v = v.rem_euclid(height as i32) as usize;
    v * width + u
}

/// OptiFine's `Config.intHash`, a small integer mixer.
fn int_hash(x: i32) -> i32 {
    let mut x = x ^ 61 ^ (x >> 16);
    x = x.wrapping_add(x.wrapping_shl(3));
    x ^= x >> 4;
    x = x.wrapping_mul(668_265_261);
    x ^ (x >> 15)
}

/// OptiFine's `Config.getRandom(pos, face)`.
fn position_hash([x, y, z]: [i32; 3], face: i32) -> i32 {
    let mut hash = int_hash(face.wrapping_add(37));
    hash = int_hash(hash.wrapping_add(x));
    hash = int_hash(hash.wrapping_add(z));
    int_hash(hash.wrapping_add(y))
}

/// The pillar axis and the metadata a rule should test for `id` with
/// `metadata`. Logs (17, 162) and hay (170) lie along the axis in metadata
/// bits 2 and 3; quartz pillars (155) in metadata 3 and 4. A rule that names
/// no metadata above 3 (logs) or 2 (quartz) sees the plain variant.
#[must_use]
pub fn pillar_axis_and_metadata(id: u16, metadata: u8, rule_metadata_max: i32) -> (Axis, u8) {
    match id {
        17 | 162 | 170 => {
            let axis = match (metadata & 12) >> 2 {
                1 => Axis::X,
                2 => Axis::Z,
                _ => Axis::Y,
            };
            let kept = if rule_metadata_max <= 3 {
                metadata & 3
            } else {
                metadata
            };
            (axis, kept)
        }
        155 => {
            let axis = match metadata {
                3 => Axis::X,
                4 => Axis::Z,
                _ => Axis::Y,
            };
            let kept = if rule_metadata_max <= 2 && metadata > 2 {
                2
            } else {
                metadata
            };
            (axis, kept)
        }
        _ => (Axis::Y, metadata),
    }
}

/// [`refine`] for the tests.
#[cfg(test)]
pub(super) fn refine_for_test(base: usize, open: [bool; 4]) -> usize {
    refine(base, open)
}
