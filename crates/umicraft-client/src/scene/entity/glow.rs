// SPDX-License-Identifier: MIT OR Apache-2.0

//! Glowing layers: the eyes of spiders, endermen and the dragon, the swirl of
//! a charged creeper and of a wounded wither, and the enchantment glint.
//!
//! In the reference they are drawn with `blendFunc(1, 1)` (or
//! `GL_SRC_COLOR, GL_ONE` for the glint) and no light, so the renderer adds
//! their colour to the frame. Their texture coordinates move with the age of
//! the entity (the texture matrix), which is done here on the vertices.

use std::f32::consts::PI;

use umicraft_model::entity::{ModelVertex, TextureSlot};

use super::mobs::EntityLayer;

/// How the texture coordinates of a glowing layer move: `uv' = scale *
/// rotate(angle) * (uv + shift)`, the order of the reference texture matrix
/// (`scale`, `rotate`, then `translate`, applied right to left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct UvMap {
    pub(super) shift: [f32; 2],
    pub(super) angle: f32,
    pub(super) scale: f32,
}

impl UvMap {
    /// The coordinates stay where the model puts them.
    pub(super) const STILL: Self = Self {
        shift: [0.0, 0.0],
        angle: 0.0,
        scale: 1.0,
    };

    /// A plain scroll.
    pub(super) const fn scroll(u: f32, v: f32) -> Self {
        Self {
            shift: [u, v],
            angle: 0.0,
            scale: 1.0,
        }
    }

    /// Maps a texture coordinate.
    pub(super) fn apply(self, uv: [f32; 2]) -> [f32; 2] {
        let (u, v) = (uv[0] + self.shift[0], uv[1] + self.shift[1]);
        let (sin, cos) = self.angle.sin_cos();
        [
            (u * cos - v * sin) * self.scale,
            (u * sin + v * cos) * self.scale,
        ]
    }
}

/// The glow of a layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Glow {
    /// Colour that multiplies the texture, `0..=1` per channel.
    pub(super) tint: [f32; 3],
    /// Whether the colour is squared (`blendFunc(GL_SRC_COLOR, GL_ONE)`).
    pub(super) squared: bool,
    /// The movement of the texture coordinates.
    pub(super) uv: UvMap,
    /// Drawn only where the body under it wrote the same depth (the glint of
    /// armour, `depthFunc(GL_EQUAL)`), so it does not fill the transparent
    /// parts of the shell.
    pub(super) equal_depth: bool,
}

/// Bit of the vertex light word that squares the colour (read by
/// `entity.wgsl`, `vs_additive`).
const SQUARED_BIT: u32 = 1 << 24;

impl Glow {
    /// Full colour, still: the eyes.
    pub(super) const EYES: Self = Self {
        tint: [1.0; 3],
        squared: false,
        uv: UvMap::STILL,
        equal_depth: false,
    };

    /// The light word of the vertices: the tint as `0xBBGGRR` plus the square
    /// bit.
    pub(super) fn light_word(self) -> u32 {
        let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
        channel(self.tint[0])
            | (channel(self.tint[1]) << 8)
            | (channel(self.tint[2]) << 16)
            | if self.squared { SQUARED_BIT } else { 0 }
    }
}

/// A glowing copy of `vertices` on `slot`.
pub(super) fn glowing(slot: TextureSlot, vertices: Vec<ModelVertex>, glow: Glow) -> EntityLayer {
    let mut layer = EntityLayer::new(slot, vertices);
    layer.glow = Some(glow);
    layer
}

/// The two passes of the enchantment glint of an armour piece (MCP
/// `LayerArmorBase.func_177183_a`): the texture is scaled by a third, turned
/// by `30 - 60 i` degrees and scrolled upwards at a speed that depends on the
/// pass, `age` being the ticks lived plus the partial tick.
pub(super) fn glint_layers(vertices: &[ModelVertex], age: f32) -> [EntityLayer; 2] {
    std::array::from_fn(|pass| {
        let pass = pass as f32;
        let glow = Glow {
            tint: [0.5 * 0.76, 0.25 * 0.76, 0.8 * 0.76],
            squared: true,
            uv: UvMap {
                shift: [0.0, age * (0.001 + pass * 0.003) * 20.0],
                angle: (30.0 - pass * 60.0) * PI / 180.0,
                scale: 1.0 / 3.0,
            },
            equal_depth: true,
        };
        glowing(TextureSlot::EnchantGlint, vertices.to_vec(), glow)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_still_map_keeps_the_coordinates() {
        assert_eq!(UvMap::STILL.apply([0.25, 0.75]), [0.25, 0.75]);
    }

    #[test]
    fn a_scroll_moves_the_coordinates() {
        assert_eq!(UvMap::scroll(0.5, 0.0).apply([0.25, 0.75]), [0.75, 0.75]);
    }

    #[test]
    fn the_glint_scales_turns_and_scrolls_like_the_texture_matrix() {
        // First pass: 30 degrees, no time yet.
        let layers = glint_layers(&[], 0.0);
        let glow = layers[0].glow.expect("glow");
        let [u, v] = glow.uv.apply([1.0, 0.0]);
        let (sin, cos) = (30.0_f32).to_radians().sin_cos();
        assert!((u - cos / 3.0).abs() < 1e-5 && (v - sin / 3.0).abs() < 1e-5);
        // The second pass turns the other way and scrolls three times faster.
        let second = layers[1].glow.expect("glow").uv;
        assert!(second.angle < 0.0);
        let first = glint_layers(&[], 100.0)[0].glow.expect("glow").uv.shift[1];
        let later = glint_layers(&[], 100.0)[1].glow.expect("glow").uv.shift[1];
        assert!((later / first - 4.0).abs() < 1e-3, "{later} {first}");
    }

    #[test]
    fn the_light_word_packs_the_tint_and_the_square_bit() {
        let word = Glow {
            tint: [1.0, 0.5, 0.0],
            squared: true,
            uv: UvMap::STILL,
            equal_depth: false,
        }
        .light_word();
        assert_eq!(word & 0xFF, 255);
        assert_eq!((word >> 8) & 0xFF, 128);
        assert_eq!((word >> 16) & 0xFF, 0);
        assert_ne!(word & SQUARED_BIT, 0);
        assert_eq!(Glow::EYES.light_word() & SQUARED_BIT, 0);
    }
}
