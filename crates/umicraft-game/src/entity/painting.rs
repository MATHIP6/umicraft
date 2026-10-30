// SPDX-License-Identifier: MIT OR Apache-2.0

//! Paintings (MCP `EntityPainting`, `EntityHanging`).
//!
//! `Spawn Painting` names the artwork and the block the painting hangs on;
//! where its centre is depends on the size of the artwork.

use glam::DVec3;

/// One artwork of `EntityPainting.EnumArt`: its title, its size in pixels and
/// where it is on the 256 x 256 sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Art {
    /// The name the server sends.
    pub title: &'static str,
    /// Width in pixels (16 per block).
    pub width: u16,
    /// Height in pixels (16 per block).
    pub height: u16,
    /// Left edge on the sheet, in texels.
    pub u: u16,
    /// Top edge on the sheet, in texels.
    pub v: u16,
}

const fn art(title: &'static str, width: u16, height: u16, u: u16, v: u16) -> Art {
    Art {
        title,
        width,
        height,
        u,
        v,
    }
}

/// `EnumArt.values()`, in the order of the reference.
pub const ARTS: [Art; 26] = [
    art("Kebab", 16, 16, 0, 0),
    art("Aztec", 16, 16, 16, 0),
    art("Alban", 16, 16, 32, 0),
    art("Aztec2", 16, 16, 48, 0),
    art("Bomb", 16, 16, 64, 0),
    art("Plant", 16, 16, 80, 0),
    art("Wasteland", 16, 16, 96, 0),
    art("Pool", 32, 16, 0, 32),
    art("Courbet", 32, 16, 32, 32),
    art("Sea", 32, 16, 64, 32),
    art("Sunset", 32, 16, 96, 32),
    art("Creebet", 32, 16, 128, 32),
    art("Wanderer", 16, 32, 0, 64),
    art("Graham", 16, 32, 16, 64),
    art("Match", 32, 32, 0, 128),
    art("Bust", 32, 32, 32, 128),
    art("Stage", 32, 32, 64, 128),
    art("Void", 32, 32, 96, 128),
    art("SkullAndRoses", 32, 32, 128, 128),
    art("Wither", 32, 32, 160, 128),
    art("Fighters", 64, 32, 0, 96),
    art("Pointer", 64, 64, 0, 192),
    art("Pigscene", 64, 64, 64, 192),
    art("BurningSkull", 64, 64, 128, 192),
    art("Skeleton", 64, 48, 192, 64),
    art("DonkeyKong", 64, 48, 192, 112),
];

/// The index in [`ARTS`] of the artwork called `title`.
#[must_use]
pub fn art_index(title: &str) -> Option<usize> {
    ARTS.iter().position(|art| art.title == title)
}

/// Where the centre of a painting is (`EntityHanging.updateBoundingBox`): on
/// the block `block`, 15/32 of a block in front of the middle towards the
/// opposite of its facing, shifted so that an even number of blocks is
/// centred on the edge between two.
///
/// `facing` is the horizontal index (0 south, 1 west, 2 north, 3 east).
#[must_use]
pub fn centre(block: [i32; 3], facing: u8, art: &Art) -> DVec3 {
    const FRONT: [(f64, f64); 4] = [(0.0, 1.0), (-1.0, 0.0), (0.0, -1.0), (1.0, 0.0)];
    let half = |size: u16| if size.is_multiple_of(32) { 0.5 } else { 0.0 };
    let (front_x, front_z) = FRONT[usize::from(facing & 3)];
    // `rotateYCCW`: south to east, east to north, north to west, west to south.
    let (side_x, side_z) = FRONT[usize::from((facing + 3) & 3)];
    let across = half(art.width);
    DVec3::new(
        f64::from(block[0]) + 0.5 - front_x * 0.46875 + across * side_x,
        f64::from(block[1]) + 0.5 + half(art.height),
        f64::from(block[2]) + 0.5 - front_z * 0.46875 + across * side_z,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_title_of_the_reference_is_known() {
        assert_eq!(ARTS.len(), 26);
        assert_eq!(art_index("Kebab"), Some(0));
        assert_eq!(art_index("SkullAndRoses"), Some(18));
        assert_eq!(art_index("DonkeyKong"), Some(25));
        assert_eq!(art_index("Mona Lisa"), None);
    }

    #[test]
    fn a_one_block_painting_sits_against_its_wall() {
        // Facing south, the painting is 15/32 towards the north of the middle
        // of its block.
        let at = centre([10, 64, 20], 0, &ARTS[0]);
        assert_eq!(at, DVec3::new(10.5, 64.5, 20.5 - 0.46875));
        let at = centre([10, 64, 20], 3, &ARTS[0]);
        assert_eq!(at, DVec3::new(10.5 - 0.46875, 64.5, 20.5));
    }

    #[test]
    fn a_wide_painting_is_centred_on_the_edge_between_two_blocks() {
        // Pool is 32 x 16: half a block sideways, no lift.
        let pool = ARTS[art_index("Pool").unwrap()];
        assert_eq!(
            centre([0, 0, 0], 0, &pool),
            DVec3::new(1.0, 0.5, 0.5 - 0.46875)
        );
        // Wanderer is 16 x 32: half a block up, no shift.
        let wanderer = ARTS[art_index("Wanderer").unwrap()];
        assert_eq!(
            centre([0, 0, 0], 0, &wanderer),
            DVec3::new(0.5, 1.0, 0.5 - 0.46875)
        );
        // Skeleton is 64 x 48, facing north: centred sideways, on a block row.
        let skeleton = ARTS[art_index("Skeleton").unwrap()];
        assert_eq!(
            centre([0, 0, 0], 2, &skeleton),
            DVec3::new(0.0, 0.5, 0.5 + 0.46875)
        );
    }
}
