// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the tile entities drawn with the additive entity pipeline share (the
//! beacon beam, the end portal).
//!
//! The additive pass (`entity.wgsl`, `fs_additive`) adds `texture * tint` to the
//! frame, with no light and no alpha test. The tint is the vertex `light` word
//! (`0xBBGGRR`), the same encoding the glowing mob layers use.

use umicraft_render::EntityVertex;

/// The `light` word that tints an additive vertex; channels are clamped to
/// `0..=1`.
pub(super) fn tint_word([red, green, blue]: [f32; 3]) -> u32 {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    channel(red) | (channel(green) << 8) | (channel(blue) << 16)
}

/// An additive vertex; the normal is unused by the pass.
pub(super) fn vertex(position: [f32; 3], uv: [f32; 2], tint: u32) -> EntityVertex {
    EntityVertex {
        position,
        normal: [0.0, 1.0, 0.0],
        uv,
        light: tint,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tint_packs_red_green_blue_and_clamps() {
        let word = tint_word([1.0, 0.5, -1.0]);
        assert_eq!(word & 0xFF, 255);
        assert_eq!((word >> 8) & 0xFF, 128);
        assert_eq!(word >> 16, 0);
    }
}
