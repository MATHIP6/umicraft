// SPDX-License-Identifier: MIT OR Apache-2.0

//! The cracks of the blocks being dug.
//!
//! The game thread sends one [`Crack`] per breaking block (the local
//! player's and the other players' `Block Break Animation`); the render side
//! turns them into quads that carry the destroy-stage texture, which the
//! renderer blends over the terrain (MCP
//! `RenderGlobal.drawBlockDamageTexture`).

use umicraft_mesh::{Vertex, VertexLight};
use umicraft_render::OutlineBox;

/// The number of destroy-stage textures (`destroy_stage_0` to `_9`).
pub const STAGES: usize = 10;

/// How far the quads stand off the block faces, in blocks, so that rounding
/// does not hide them inside the faces. The renderer also pulls them
/// towards the camera with a depth bias.
const STAND_OFF: f32 = 0.001;

/// A block being dug: its selection boxes in world space and the crack stage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crack {
    /// One box of the block shape (a slab has one, stairs have two).
    pub bounds: OutlineBox,
    /// The crack stage, `0..=9`.
    pub stage: u8,
}

/// The vertices and indices of the quads of `cracks`.
///
/// `layers` holds the texture-array layer of each destroy stage. A crack
/// whose stage is out of range is skipped.
#[must_use]
pub fn crack_mesh(cracks: &[Crack], layers: &[u32; STAGES]) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::with_capacity(cracks.len() * 24);
    let mut indices = Vec::with_capacity(cracks.len() * 36);
    for crack in cracks {
        let Some(&layer) = layers.get(usize::from(crack.stage)) else {
            continue;
        };
        for face in FACES {
            let first = vertices.len() as u32;
            vertices.extend(face_vertices(&crack.bounds, &face, layer));
            indices.extend([0, 1, 2, 0, 2, 3].map(|corner| first + corner));
        }
    }
    (vertices, indices)
}

/// A face of a box: its normal axis, whether it is the high side, and the
/// two axes that span the texture (u, v).
struct Face {
    axis: usize,
    high: bool,
    u: usize,
    v: usize,
}

const FACES: [Face; 6] = [
    Face {
        axis: 0,
        high: false,
        u: 2,
        v: 1,
    },
    Face {
        axis: 0,
        high: true,
        u: 2,
        v: 1,
    },
    Face {
        axis: 1,
        high: false,
        u: 0,
        v: 2,
    },
    Face {
        axis: 1,
        high: true,
        u: 0,
        v: 2,
    },
    Face {
        axis: 2,
        high: false,
        u: 0,
        v: 1,
    },
    Face {
        axis: 2,
        high: true,
        u: 0,
        v: 1,
    },
];

/// The four corners of a face, counter-clockwise seen from outside.
fn face_vertices(bounds: &OutlineBox, face: &Face, layer: u32) -> [Vertex; 4] {
    let plane = if face.high {
        bounds.max[face.axis] + STAND_OFF
    } else {
        bounds.min[face.axis] - STAND_OFF
    };
    let mut normal = [0.0; 3];
    normal[face.axis] = if face.high { 1.0 } else { -1.0 };
    // Counter-clockwise order depends on the sign of the normal and on the
    // handedness of the (axis, u, v) triple.
    let flip = face.high != is_right_handed(face);
    let corners = [(0, 0), (1, 0), (1, 1), (0, 1)];
    let order: [usize; 4] = if flip { [0, 3, 2, 1] } else { [0, 1, 2, 3] };
    order.map(|index| {
        let (u_high, v_high) = corners[index];
        let mut position = [0.0; 3];
        position[face.axis] = plane;
        position[face.u] = pick(bounds, face.u, u_high == 1);
        position[face.v] = pick(bounds, face.v, v_high == 1);
        Vertex {
            position,
            normal,
            // The texture covers the block, so a partial box shows the part
            // of the cracks that lies over it; v = 0 is the top. The
            // coordinates are taken from the block corner: wrapping them
            // would send the far side of a block (x = 1.0) back to 0.
            uv: [
                position[face.u] - bounds.min[face.u].floor(),
                1.0 - (position[face.v] - bounds.min[face.v].floor()),
            ],
            layer,
            light: VertexLight::DAY.pack(),
            tint: 0x00FF_FFFF,
        }
    })
}

/// The coordinate of the low or high side of a box on an axis.
fn pick(bounds: &OutlineBox, axis: usize, high: bool) -> f32 {
    if high {
        bounds.max[axis]
    } else {
        bounds.min[axis]
    }
}

/// Whether `(axis, u, v)` is a right-handed triple (a cyclic permutation).
fn is_right_handed(face: &Face) -> bool {
    (face.axis + 1) % 3 == face.u && (face.u + 1) % 3 == face.v
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYERS: [u32; STAGES] = [10, 11, 12, 13, 14, 15, 16, 17, 18, 19];

    fn block(x: f32) -> Crack {
        Crack {
            bounds: OutlineBox {
                min: [x, 64.0, 0.0],
                max: [x + 1.0, 65.0, 1.0],
            },
            stage: 3,
        }
    }

    #[test]
    fn a_block_has_six_quads_with_the_layer_of_its_stage() {
        let (vertices, indices) = crack_mesh(&[block(0.0)], &LAYERS);
        assert_eq!(vertices.len(), 24);
        assert_eq!(indices.len(), 36);
        assert!(vertices.iter().all(|vertex| vertex.layer == 13));
        assert!(indices
            .iter()
            .all(|&index| (index as usize) < vertices.len()));
    }

    #[test]
    fn every_quad_faces_outwards() {
        let (vertices, indices) = crack_mesh(&[block(2.0)], &LAYERS);
        let centre = [2.5, 64.5, 0.5];
        for triangle in indices.chunks(3) {
            let [a, b, c] =
                [0, 1, 2].map(|i| glam::Vec3::from(vertices[triangle[i] as usize].position));
            let normal = (b - a).cross(c - a);
            let outward = a - glam::Vec3::from(centre);
            assert!(normal.dot(outward) > 0.0, "a quad faces inwards: {a:?}");
            let declared = glam::Vec3::from(vertices[triangle[0] as usize].normal);
            assert!(normal.normalize().dot(declared) > 0.99);
        }
    }

    #[test]
    fn the_quads_stand_just_off_the_faces() {
        let (vertices, _) = crack_mesh(&[block(0.0)], &LAYERS);
        let max_x = vertices
            .iter()
            .map(|v| v.position[0])
            .fold(f32::MIN, f32::max);
        let min_x = vertices
            .iter()
            .map(|v| v.position[0])
            .fold(f32::MAX, f32::min);
        assert!((max_x - 1.001).abs() < 1e-6);
        assert!((min_x + 0.001).abs() < 1e-6);
    }

    #[test]
    fn the_texture_covers_the_block_whatever_its_position() {
        let (vertices, _) = crack_mesh(&[block(7.0)], &LAYERS);
        for vertex in &vertices {
            assert!((0.0..=1.0).contains(&vertex.uv[0]));
            assert!((0.0..=1.0).contains(&vertex.uv[1]));
        }
    }

    /// Regression: the far side of a block (x = 1.0) wrapped back to 0, so
    /// the texture collapsed onto a line and no crack showed.
    #[test]
    fn a_whole_block_shows_the_whole_texture_on_each_face() {
        let (vertices, _) = crack_mesh(&[block(7.0)], &LAYERS);
        for quad in vertices.chunks(4) {
            let us: Vec<f32> = quad.iter().map(|v| v.uv[0]).collect();
            let vs: Vec<f32> = quad.iter().map(|v| v.uv[1]).collect();
            let spans = |values: &[f32]| {
                values.iter().copied().fold(f32::MIN, f32::max)
                    - values.iter().copied().fold(f32::MAX, f32::min)
            };
            assert!((spans(&us) - 1.0).abs() < 1e-5, "u spans {us:?}");
            assert!((spans(&vs) - 1.0).abs() < 1e-5, "v spans {vs:?}");
        }
    }

    #[test]
    fn a_half_block_shows_half_of_the_texture() {
        let slab = Crack {
            bounds: OutlineBox {
                min: [3.0, 64.0, 5.0],
                max: [4.0, 64.5, 6.0],
            },
            stage: 0,
        };
        let (vertices, _) = crack_mesh(&[slab], &LAYERS);
        // The +X face spans z (u) fully and y (v) by half.
        let side = &vertices[4..8];
        let vs: Vec<f32> = side.iter().map(|v| v.uv[1]).collect();
        let span = vs.iter().copied().fold(f32::MIN, f32::max)
            - vs.iter().copied().fold(f32::MAX, f32::min);
        assert!((span - 0.5).abs() < 1e-5, "{vs:?}");
    }

    #[test]
    fn an_unknown_stage_is_skipped_and_nothing_gives_an_empty_mesh() {
        let mut crack = block(0.0);
        crack.stage = 10;
        assert!(crack_mesh(&[crack], &LAYERS).0.is_empty());
        assert!(crack_mesh(&[], &LAYERS).1.is_empty());
    }
}
