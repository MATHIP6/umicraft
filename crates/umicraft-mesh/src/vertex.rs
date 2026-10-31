// SPDX-License-Identifier: MIT OR Apache-2.0

//! GPU vertex layout and the packed per-vertex light payload.

use bytemuck::{Pod, Zeroable};

/// One chunk face vertex, laid out for direct GPU upload.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    /// World-space position.
    pub position: [f32; 3],
    /// Face normal (unit axis).
    pub normal: [f32; 3],
    /// Texture coordinates; `v = 0` is the top of the texture.
    pub uv: [f32; 2],
    /// Fallback texture-array layer.
    pub layer: u32,
    /// Packed [`VertexLight`] payload (block/sky light, AO and face shading).
    ///
    /// The shader unpacks bits 0..=3 (block), 4..=7 (sky), 8..=11 (AO) and
    /// bit 12 (shade).
    pub light: u32,
    /// Biome tint as `0x00RRGGBB`, white (`0x00FF_FFFF`) when untinted.
    ///
    /// Only the grass block, leaves, vines, tall grass and water set it
    /// (ADR-0009); the shader multiplies the sampled texel by
    /// `tint / 255`. The vertex is 44 bytes, still `Pod`/`repr(C)`.
    pub tint: u32,
}

/// Per-vertex lighting payload.
///
/// Light levels and ambient occlusion are nibbles; `shade` is bit 12. The
/// renderer uses the two light channels independently so sky light can follow
/// the time of day while block light stays constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VertexLight {
    /// Block-light level (`0..=15`).
    pub block: u8,
    /// Sky-light level (`0..=15`).
    pub sky: u8,
    /// Ambient-occlusion strength (`0..=15`, where 15 is fully open).
    pub ao: u8,
    /// Whether the renderer applies the 1.8 per-face shade factor.
    pub shade: bool,
}

impl VertexLight {
    /// Full daylight, no ambient occlusion, with face shading enabled.
    pub const DAY: Self = Self {
        block: 0,
        sky: 15,
        ao: 15,
        shade: true,
    };

    /// Fully open ambient occlusion.
    pub const AO_OPEN: u8 = 15;

    /// Builds a payload, clamping every nibble to `0..=15`.
    #[must_use]
    pub const fn new(block: u8, sky: u8, ao: u8, shade: bool) -> Self {
        Self {
            block: clamp_light(block),
            sky: clamp_light(sky),
            ao: clamp_light(ao),
            shade,
        }
    }

    /// Packs the payload for [`Vertex::light`].
    #[must_use]
    pub const fn pack(self) -> u32 {
        (self.block as u32)
            | ((self.sky as u32) << 4)
            | ((self.ao as u32) << 8)
            | ((self.shade as u32) << 12)
    }

    /// Unpacks a [`Vertex::light`] payload.
    #[must_use]
    pub const fn unpack(packed: u32) -> Self {
        Self {
            block: (packed & 0x0F) as u8,
            sky: ((packed >> 4) & 0x0F) as u8,
            ao: ((packed >> 8) & 0x0F) as u8,
            shade: (packed & (1 << 12)) != 0,
        }
    }
}

const fn clamp_light(level: u8) -> u8 {
    if level > 15 {
        15
    } else {
        level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vertex_layout_is_pod_and_four_byte_aligned() {
        assert_eq!(std::mem::size_of::<Vertex>(), 44);
        assert_eq!(std::mem::align_of::<Vertex>(), 4);
    }

    #[test]
    fn vertex_light_round_trips_and_clamps() {
        let light = VertexLight::new(20, 3, 9, false);
        assert_eq!(light.block, 15);
        assert_eq!(VertexLight::unpack(light.pack()), light);
        assert_eq!(
            VertexLight::unpack(VertexLight::DAY.pack()),
            VertexLight::DAY
        );
        assert_eq!(VertexLight::DAY.pack() & 0x0F, 0);
        assert_eq!((VertexLight::DAY.pack() >> 4) & 0x0F, 15);
        assert_eq!((VertexLight::DAY.pack() >> 8) & 0x0F, 15);
        assert_ne!(VertexLight::DAY.pack() & (1 << 12), 0);
    }
}
