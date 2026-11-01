// SPDX-License-Identifier: MIT OR Apache-2.0

//! The quads of the particles: camera-facing billboards and flat footprints.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

/// One vertex of the particle pass.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(super) struct ParticleVertex {
    /// World position.
    pub position: [f32; 3],
    /// Texture coordinate inside the sheet or the atlas layer.
    pub uv: [f32; 2],
    /// Layer of the block atlas, or `-1.0` for the `particles.png` sheet.
    pub layer: f32,
    /// Tint and opacity (`color(r, g, b, a)` of the reference).
    pub color: [f32; 4],
    /// Block and sky light, each `0.0..=15.0`.
    pub light: [f32; 2],
}

impl ParticleVertex {
    /// Vertex attributes (locations 0..5).
    pub(super) const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Float32x2, 2 => Float32, 3 => Float32x4, 4 => Float32x2
    ];
}

/// The layer value that selects the `particles.png` sheet.
pub(super) const SHEET_LAYER: f32 = -1.0;

/// A particle ready to draw, resolved by the client from the simulation.
///
/// The client owns the sprite lookup, the light and the interpolation; the
/// renderer only turns this into a quad, so the render thread never reads the
/// world or the game crate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticleSprite {
    /// The centre in world coordinates, already interpolated.
    pub center: Vec3,
    /// Half the side of the quad (`0.1 * scale` in the reference).
    pub half_size: f32,
    /// Top-left corner of the picture, in `0..=1` of the sheet or layer.
    pub uv_min: [f32; 2],
    /// Bottom-right corner of the picture.
    pub uv_max: [f32; 2],
    /// Layer of the block atlas, or `None` for the `particles.png` sheet.
    pub atlas_layer: Option<u32>,
    /// Red, green, blue and alpha factors.
    pub color: [f32; 4],
    /// Block and sky light, each `0.0..=15.0`.
    pub light: [f32; 2],
    /// Lies flat on the ground (the footprint) instead of facing the camera.
    pub flat: bool,
}

/// The right and up vectors of the screen for a camera looking along
/// `forward`, the reference's `ActiveRenderInfo` rotation terms.
///
/// Falls back to the world axes when the camera looks straight up or down.
pub(super) fn screen_axes(forward: Vec3) -> (Vec3, Vec3) {
    let right = forward.cross(Vec3::Y).try_normalize().unwrap_or(Vec3::X);
    (right, right.cross(forward).normalize_or(Vec3::Y))
}

/// The six vertices of a sprite: two triangles over the corners
/// left-bottom, left-top, right-top, right-bottom (MCP
/// `EntityFX.renderParticle`, line 168).
pub(super) fn quad(sprite: &ParticleSprite, right: Vec3, up: Vec3) -> [ParticleVertex; 6] {
    let (right, up) = if sprite.flat {
        (Vec3::X, Vec3::Z)
    } else {
        (right, up)
    };
    let layer = sprite.atlas_layer.map_or(SHEET_LAYER, |layer| layer as f32);
    let [u0, v0] = sprite.uv_min;
    let [u1, v1] = sprite.uv_max;
    let half = sprite.half_size;
    let corner = |side: f32, lift: f32, u: f32, v: f32| ParticleVertex {
        position: (sprite.center + (right * side + up * lift) * half).to_array(),
        uv: [u, v],
        layer,
        color: sprite.color,
        light: sprite.light,
    };
    // A flat quad's "up" is +Z, which runs toward the viewer on the ground;
    // the picture keeps its top toward -Z, so the lift is flipped.
    let flip = if sprite.flat { -1.0 } else { 1.0 };
    let left_bottom = corner(-1.0, -flip, u0, v1);
    let left_top = corner(-1.0, flip, u0, v0);
    let right_top = corner(1.0, flip, u1, v0);
    let right_bottom = corner(1.0, -flip, u1, v1);
    [
        left_bottom,
        left_top,
        right_top,
        left_bottom,
        right_top,
        right_bottom,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprite() -> ParticleSprite {
        ParticleSprite {
            center: Vec3::new(1.0, 2.0, 3.0),
            half_size: 0.5,
            uv_min: [0.25, 0.5],
            uv_max: [0.5, 0.75],
            atlas_layer: None,
            color: [1.0; 4],
            light: [15.0, 0.0],
            flat: false,
        }
    }

    #[test]
    fn axes_are_orthonormal_and_up_leans_with_the_pitch() {
        let (right, up) = screen_axes(Vec3::NEG_Z);
        assert!((right - Vec3::X).length() < 1e-6);
        assert!((up - Vec3::Y).length() < 1e-6);
        let (right, up) = screen_axes(Vec3::new(0.0, 0.5, -1.0).normalize());
        assert!(right.dot(up).abs() < 1e-6);
        assert!(up.y > 0.0);
    }

    #[test]
    fn looking_straight_down_still_gives_axes() {
        let (right, up) = screen_axes(Vec3::NEG_Y);
        assert!(right.is_finite() && up.is_finite());
    }

    #[test]
    fn a_billboard_is_centred_and_sized_by_the_half_side() {
        let vertices = quad(&sprite(), Vec3::X, Vec3::Y);
        assert_eq!(vertices[0].position, [0.5, 1.5, 3.0]);
        assert_eq!(vertices[2].position, [1.5, 2.5, 3.0]);
        assert_eq!(vertices[0].uv, [0.25, 0.75]);
        assert_eq!(vertices[2].uv, [0.5, 0.5]);
        assert_eq!(vertices[0].layer, SHEET_LAYER);
    }

    #[test]
    fn an_atlas_sprite_carries_its_layer() {
        let mut block = sprite();
        block.atlas_layer = Some(7);
        assert_eq!(quad(&block, Vec3::X, Vec3::Y)[0].layer, 7.0);
    }

    #[test]
    fn a_flat_quad_lies_in_the_horizontal_plane() {
        let mut footprint = sprite();
        footprint.flat = true;
        for vertex in quad(&footprint, Vec3::X, Vec3::Y) {
            assert_eq!(vertex.position[1], 2.0);
        }
    }
}
