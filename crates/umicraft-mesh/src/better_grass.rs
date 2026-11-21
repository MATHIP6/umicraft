// SPDX-License-Identifier: MIT OR Apache-2.0

//! OptiFine "Better Grass": the side faces of grass, mycelium and podzol show
//! the top texture (or the snow texture under snow) so a lawn reads as one
//! surface (`optifine.BetterGrass.getFaceQuads`, ADR-0088/0089).
//!
//! The side quads of a block are replaced, not blended: the vanilla grass
//! block draws a `grass_side` quad plus a tinted `grass_side_overlay` quad,
//! and OptiFine swaps both for a single top-textured quad, so the overlay is
//! dropped here.

use umicraft_model::Face;
use umicraft_world::BlockState;

use crate::{BetterGrass, BlockSource};

/// Block id of grass.
const GRASS: u16 = 2;
/// Block id of dirt (podzol is metadata 2).
const DIRT: u16 = 3;
/// Block id of mycelium.
const MYCELIUM: u16 = 110;
/// Block id of a snow layer.
pub(crate) const SNOW_LAYER: u16 = 78;
/// Block id of a snow block.
const SNOW_BLOCK: u16 = 80;
/// Metadata of podzol on dirt.
const PODZOL: u8 = 2;

/// The grass-side overlay quad, which a replaced face drops.
const GRASS_OVERLAY: &str = "blocks/grass_side_overlay";
/// The texture shown on a side that sits under snow.
const SNOW_TEXTURE: &str = "blocks/snow";

/// What replaces the side quads of one face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SideTexture {
    /// The atlas key drawn instead of the side texture.
    pub(crate) key: &'static str,
    /// Whether the replacement takes the block's biome tint.
    pub(crate) tinted: bool,
}

impl SideTexture {
    /// The texture for a quad that currently shows `texture`, or `None` when
    /// the quad is dropped (the overlay).
    pub(crate) fn replace(self, texture: &str) -> Option<Self> {
        (texture != GRASS_OVERLAY).then_some(self)
    }
}

/// The replacement for the side `face` of `state` at `position`, or `None`
/// when vanilla textures stay: Better Grass is off, the face is not a side,
/// the block is not grass-like, or Fancy finds no matching neighbour.
pub(crate) fn side_texture<S: BlockSource + ?Sized>(
    source: &S,
    position: [i32; 3],
    state: BlockState,
    face: Face,
    mode: BetterGrass,
) -> Option<SideTexture> {
    if mode == BetterGrass::Off || matches!(face, Face::Up | Face::Down) {
        return None;
    }
    let top = top_texture(state)?;
    let [x, y, z] = position;
    let [dx, _, dz] = face.offset();
    let at = |dy: i32, side: bool| {
        let (sx, sz) = if side { (x + dx, z + dz) } else { (x, z) };
        source.block(sx, y + dy, sz)
    };
    let under_snow = matches!(at(1, false).id(), SNOW_LAYER | SNOW_BLOCK);
    let fancy = mode == BetterGrass::Fancy;
    if under_snow {
        let continues = at(0, true).id() == SNOW_LAYER;
        (!fancy || continues).then_some(SideTexture {
            key: SNOW_TEXTURE,
            tinted: false,
        })
    } else {
        let continues = same_family(at(-1, true), state);
        (!fancy || continues).then_some(top)
    }
}

/// The top texture of a grass-like block, `None` for any other block.
fn top_texture(state: BlockState) -> Option<SideTexture> {
    let (key, tinted) = match (state.id(), state.metadata()) {
        (GRASS, _) => ("blocks/grass_top", true),
        (MYCELIUM, _) => ("blocks/mycelium_top", false),
        (DIRT, PODZOL) => ("blocks/dirt_podzol_top", false),
        _ => return None,
    };
    Some(SideTexture { key, tinted })
}

/// Whether `other` is the same grass-like block as `state` (`Blocks.grass`,
/// `Blocks.mycelium`, or podzol dirt).
fn same_family(other: BlockState, state: BlockState) -> bool {
    other.id() == state.id() && (state.id() != DIRT || other.metadata() == PODZOL)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sparse world of explicit blocks.
    struct Blocks(Vec<([i32; 3], BlockState)>);

    impl BlockSource for Blocks {
        fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
            self.0
                .iter()
                .find(|(at, _)| *at == [x, y, z])
                .map_or(BlockState::AIR, |(_, state)| *state)
        }
    }

    fn grass() -> BlockState {
        BlockState::new(GRASS, 0)
    }

    fn pick(
        world: &Blocks,
        state: BlockState,
        face: Face,
        mode: BetterGrass,
    ) -> Option<&'static str> {
        side_texture(world, [0, 0, 0], state, face, mode).map(|side| side.key)
    }

    #[test]
    fn off_and_vertical_faces_keep_vanilla_textures() {
        let world = Blocks(vec![([0, 0, 0], grass())]);
        assert_eq!(pick(&world, grass(), Face::East, BetterGrass::Off), None);
        assert_eq!(pick(&world, grass(), Face::Up, BetterGrass::Fast), None);
        assert_eq!(pick(&world, grass(), Face::Down, BetterGrass::Fast), None);
    }

    #[test]
    fn fast_puts_the_top_texture_on_every_side_with_the_tint() {
        let world = Blocks(vec![([0, 0, 0], grass())]);
        let side = side_texture(&world, [0, 0, 0], grass(), Face::North, BetterGrass::Fast)
            .expect("a replacement");
        assert_eq!((side.key, side.tinted), ("blocks/grass_top", true));
    }

    #[test]
    fn fancy_needs_the_same_block_diagonally_below() {
        let mut world = Blocks(vec![([0, 0, 0], grass())]);
        assert_eq!(pick(&world, grass(), Face::East, BetterGrass::Fancy), None);
        world.0.push(([1, -1, 0], grass()));
        assert_eq!(
            pick(&world, grass(), Face::East, BetterGrass::Fancy),
            Some("blocks/grass_top")
        );
        assert_eq!(pick(&world, grass(), Face::West, BetterGrass::Fancy), None);
    }

    #[test]
    fn mycelium_and_podzol_use_their_own_tops() {
        let world = Blocks(vec![]);
        let fast = |state| pick(&world, state, Face::South, BetterGrass::Fast);
        assert_eq!(
            fast(BlockState::new(MYCELIUM, 0)),
            Some("blocks/mycelium_top")
        );
        assert_eq!(
            fast(BlockState::new(DIRT, PODZOL)),
            Some("blocks/dirt_podzol_top")
        );
        assert_eq!(fast(BlockState::new(DIRT, 0)), None);
        assert_eq!(fast(BlockState::new(1, 0)), None);
    }

    #[test]
    fn snow_on_top_shows_the_snow_texture_untinted() {
        let mut world = Blocks(vec![([0, 1, 0], BlockState::new(SNOW_LAYER, 0))]);
        let side = side_texture(&world, [0, 0, 0], grass(), Face::East, BetterGrass::Fast)
            .expect("snow side");
        assert_eq!((side.key, side.tinted), (SNOW_TEXTURE, false));
        // Fancy wants the neighbouring snow layer at the same height.
        assert_eq!(pick(&world, grass(), Face::East, BetterGrass::Fancy), None);
        world.0.push(([1, 0, 0], BlockState::new(SNOW_LAYER, 0)));
        assert_eq!(
            pick(&world, grass(), Face::East, BetterGrass::Fancy),
            Some(SNOW_TEXTURE)
        );
    }

    #[test]
    fn the_overlay_quad_is_dropped() {
        let side = top_texture(grass()).expect("grass");
        assert_eq!(side.replace(GRASS_OVERLAY), None);
        assert_eq!(side.replace("blocks/grass_side"), Some(side));
    }
}
