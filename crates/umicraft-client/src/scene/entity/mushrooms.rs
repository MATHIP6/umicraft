// SPDX-License-Identifier: MIT OR Apache-2.0

//! The red mushrooms on a mooshroom (MCP `LayerMooshroomMushroom`).
//!
//! Two stand on the back and one on the neck. Each is the cross model of the
//! block (two crossed upright quads of `mushroom_red.png`) placed with the
//! transforms of the layer, including the quarter turn that
//! `BlockModelRenderer.renderModelBrightness` applies.

use glam::{Mat4, Vec3};
use umicraft_model::entity::{ModelPart, ModelVertex};

/// Height of the model origin above the feet, in blocks (MCP `translate(0, -1.5, 0)`).
const ORIGIN_HEIGHT: f32 = 1.5;

/// The mushrooms of an adult mooshroom, in model space. `head` is the posed
/// head of the cow model, which carries the third mushroom.
pub(super) fn mushroom_vertices(head: &ModelPart) -> Vec<ModelVertex> {
    // The vanilla model space has y pointing down; ours is that space with y
    // flipped, so a vanilla `scale(1, -1, 1)` is the identity on our y.
    let flip = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0));
    let to_model = Mat4::from_translation(Vec3::new(0.0, ORIGIN_HEIGHT, 0.0)) * flip;
    let turn = |degrees: f32| Mat4::from_rotation_y(degrees.to_radians());
    let shift = |x: f32, y: f32, z: f32| Mat4::from_translation(Vec3::new(x, y, z));
    // `renderModelBrightness`: a quarter turn, after the block is centred.
    let block = shift(-0.5, -0.5, 0.5) * turn(90.0);

    let back = flip * shift(0.2, 0.35, 0.5) * turn(42.0);
    let first = to_model * back * block;
    let second = to_model * back * shift(0.1, 0.0, -0.6) * turn(42.0) * block;

    let pivot = Vec3::from(head.pivot) / 16.0;
    let head_frame = Mat4::from_translation(pivot)
        * Mat4::from_rotation_z(head.rotation[2])
        * Mat4::from_rotation_y(head.rotation[1])
        * Mat4::from_rotation_x(head.rotation[0]);
    // The layer's own `scale(1, -1, 1)` and the flip into our y-up head frame
    // cancel, as for the back mushrooms.
    let third = head_frame * shift(0.0, 0.7, -0.2) * turn(12.0) * block;

    let mut out = Vec::new();
    for matrix in [first, second, third] {
        cross(&matrix, &mut out);
    }
    out
}

/// The two crossed quads of a plant block in `0..1` block space, transformed
/// by `matrix` (the `cross` model: planes along both diagonals, 0.05 from
/// the edges after the 45 degree rescaled rotation).
fn cross(matrix: &Mat4, out: &mut Vec<ModelVertex>) {
    const NEAR: f32 = 0.05;
    const FAR: f32 = 0.95;
    let planes = [[(NEAR, FAR), (FAR, NEAR)], [(NEAR, NEAR), (FAR, FAR)]];
    for [from, to] in planes {
        let corner = |(x, z): (f32, f32), y: f32, u: f32, v: f32| ModelVertex {
            position: matrix.transform_point3(Vec3::new(x, y, z)).to_array(),
            normal: [0.0, 1.0, 0.0],
            uv: [u, v],
        };
        let quad = [
            corner(from, 0.0, 0.0, 1.0),
            corner(to, 0.0, 1.0, 1.0),
            corner(to, 1.0, 1.0, 0.0),
            corner(from, 1.0, 0.0, 0.0),
        ];
        for index in [0, 1, 2, 0, 2, 3] {
            out.push(quad[index]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_model::entity::QuadrupedModel;

    #[test]
    fn three_crossed_pairs_of_quads_make_36_vertices() {
        let cow = QuadrupedModel::cow();
        assert_eq!(mushroom_vertices(&cow.head).len(), 3 * 2 * 6);
    }

    #[test]
    fn each_mushroom_is_a_block_tall_and_the_back_ones_stand_on_the_body() {
        let cow = QuadrupedModel::cow();
        let vertices = mushroom_vertices(&cow.head);
        for mushroom in vertices.chunks(12) {
            let ys: Vec<f32> = mushroom.iter().map(|v| v.position[1]).collect();
            let height = ys.iter().cloned().fold(f32::MIN, f32::max)
                - ys.iter().cloned().fold(f32::MAX, f32::min);
            assert!((height - 1.0).abs() < 1e-4, "{height}");
        }
        // The back mushrooms are centred on the cow, within its body length.
        for mushroom in &vertices.chunks(12).collect::<Vec<_>>()[..2] {
            let zs: Vec<f32> = mushroom.iter().map(|v| v.position[2]).collect();
            let centre = (zs.iter().cloned().fold(f32::MIN, f32::max)
                + zs.iter().cloned().fold(f32::MAX, f32::min))
                / 2.0;
            assert!(centre.abs() < 1.0, "{centre}");
        }
    }

    #[test]
    fn the_neck_mushroom_follows_the_head() {
        let mut cow = QuadrupedModel::cow();
        let before = mushroom_vertices(&cow.head);
        cow.set_angles(0.0, 0.0, 60.0, 0.0);
        let after = mushroom_vertices(&cow.head);
        assert_eq!(before[..24], after[..24], "the back does not move");
        assert_ne!(before[24..], after[24..]);
    }

    #[test]
    fn the_neck_mushroom_stands_above_the_head_pivot() {
        let mut cow = QuadrupedModel::cow();
        cow.set_angles(0.0, 0.0, 0.0, 0.0);
        let pivot_y = cow.head.pivot[1] / 16.0;
        let vertices = mushroom_vertices(&cow.head);
        let lowest = vertices[24..]
            .iter()
            .map(|v| v.position[1])
            .fold(f32::MAX, f32::min);
        assert!(lowest > pivot_y, "{lowest} is not above {pivot_y}");
    }
}
