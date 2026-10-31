// SPDX-License-Identifier: MIT OR Apache-2.0

//! The book above an enchanting table (MCP `ModelBook`) as
//! `TileEntityEnchantmentTableRenderer` places it.
//!
//! The book is seven flat parts on a 64x32 sheet: two covers, the spine, two
//! stacks of pages and two pages that flip. The covers and flipping pages are
//! boxes of depth 0, i.e. single sheets with a front and a back.

use std::f32::consts::{FRAC_PI_2, PI};

use glam::{Mat4, Vec3};

use super::{ModelBox, ModelPart, ModelVertex};

/// The sheet is 64x32.
const TEXTURE: [f32; 2] = [64.0, 32.0];

/// The pose of the book for one frame, all values already interpolated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BookPose {
    /// Ticks since the tile entity started plus the partial tick (`f`): the
    /// book bobs and breathes with it.
    pub age: f32,
    /// Turn of the book about the vertical axis, in radians
    /// (`bookRotation`, interpolated along the short way).
    pub rotation: f32,
    /// How far the right page is flipped, `0..=1` (`f3`).
    pub flip_right: f32,
    /// How far the left page is flipped, `0..=1` (`f4`).
    pub flip_left: f32,
    /// How open the book is, `0` shut to `1` open (`bookSpread`).
    pub spread: f32,
}

/// The opening angle of the covers, `ModelBook.setRotationAngles`.
#[must_use]
pub fn cover_angle(age: f32, spread: f32) -> f32 {
    ((age * 0.02).sin() * 0.1 + 1.25) * spread
}

/// The enchanting table book.
#[derive(Debug, Clone, Copy, Default)]
pub struct BookModel;

impl BookModel {
    /// The book in the block space of its table.
    ///
    /// The renderer's stack is `translate(.5, .75, .5)`, a bob of
    /// `0.1 + sin(age * 0.1) * 0.01`, `rotate(-rotation, y)`, `rotate(80, z)`
    /// and the model in sixteenths. The reference's boxes are y-down and the
    /// crate's y-up: every box here is symmetric about `y = 0`, so only the
    /// `-1/16` on y of the final scale undoes the flip (as in the chest).
    #[must_use]
    pub fn emit(pose: BookPose) -> Vec<ModelVertex> {
        let angle = cover_angle(pose.age, pose.spread);
        let page_x = angle.sin();
        let part = |pivot: [f32; 3], origin: [f32; 3], size: [f32; 3], uv: [f32; 2]| {
            ModelPart::new(pivot).with_box(ModelBox::new(origin, size, uv, TEXTURE, 0.0))
        };
        let turned = |mut part: ModelPart, turn: f32| {
            part.rotation[1] = turn;
            part
        };
        let cover_right = turned(
            part(
                [0.0, 0.0, -1.0],
                [-6.0, -5.0, 0.0],
                [6.0, 10.0, 0.0],
                [0.0, 0.0],
            ),
            PI + angle,
        );
        let cover_left = turned(
            part(
                [0.0, 0.0, 1.0],
                [0.0, -5.0, 0.0],
                [6.0, 10.0, 0.0],
                [16.0, 0.0],
            ),
            -angle,
        );
        let spine = turned(
            part([0.0; 3], [-1.0, -5.0, 0.0], [2.0, 10.0, 0.0], [12.0, 0.0]),
            FRAC_PI_2,
        );
        let pages_right = turned(
            part(
                [page_x, 0.0, 0.0],
                [0.0, -4.0, -0.99],
                [5.0, 8.0, 1.0],
                [0.0, 10.0],
            ),
            angle,
        );
        let pages_left = turned(
            part(
                [page_x, 0.0, 0.0],
                [0.0, -4.0, -0.01],
                [5.0, 8.0, 1.0],
                [12.0, 10.0],
            ),
            -angle,
        );
        let flipping = |flip: f32| {
            turned(
                part(
                    [page_x, 0.0, 0.0],
                    [0.0, -4.0, 0.0],
                    [5.0, 8.0, 0.0],
                    [24.0, 10.0],
                ),
                angle - angle * 2.0 * flip,
            )
        };
        let root = Self::root(pose);
        let mut out = Vec::new();
        for part in [
            &cover_right,
            &cover_left,
            &spine,
            &pages_right,
            &pages_left,
            &flipping(pose.flip_right),
            &flipping(pose.flip_left),
        ] {
            part.emit(root, &mut out);
        }
        out
    }

    /// The renderer's matrix stack down to the model scale.
    fn root(pose: BookPose) -> Mat4 {
        let bob = 0.1 + (pose.age * 0.1).sin() * 0.01;
        Mat4::from_translation(Vec3::new(0.5, 0.75 + bob, 0.5))
            * Mat4::from_rotation_y(-pose.rotation)
            * Mat4::from_rotation_z(80.0_f32.to_radians())
            * Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0) / 16.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POSE: BookPose = BookPose {
        age: 10.0,
        rotation: 0.0,
        flip_right: 0.5,
        flip_left: 0.5,
        spread: 1.0,
    };

    #[test]
    fn the_book_is_seven_boxes() {
        assert_eq!(BookModel::emit(POSE).len(), 7 * 36);
    }

    #[test]
    fn a_shut_book_has_shut_covers_and_an_open_one_about_seventy_degrees() {
        assert_eq!(cover_angle(0.0, 0.0), 0.0);
        let open = cover_angle(0.0, 1.0);
        assert!((open - 1.25).abs() < 1e-6, "{open}");
        // The breathing stays within 0.1 rad of 1.25.
        assert!((cover_angle(78.5, 1.0) - 1.35).abs() < 1e-3);
    }

    #[test]
    fn the_book_floats_above_the_table_within_its_block_column() {
        let vertices = BookModel::emit(POSE);
        for vertex in &vertices {
            let [x, y, z] = vertex.position;
            assert!((-0.1..=1.1).contains(&x), "{x}");
            assert!((0.7..=1.6).contains(&y), "{y}");
            assert!((-0.1..=1.1).contains(&z), "{z}");
        }
    }

    #[test]
    fn the_turn_moves_the_book_about_the_block_centre() {
        let sum = |pose| {
            let vertices = BookModel::emit(pose);
            let n = vertices.len() as f32;
            let total = vertices
                .iter()
                .fold(Vec3::ZERO, |acc, v| acc + Vec3::from(v.position));
            total / n
        };
        let a = sum(POSE);
        let b = sum(BookPose {
            rotation: PI,
            ..POSE
        });
        // Turning by half a circle mirrors the horizontal offsets about 0.5.
        assert!((a.x + b.x - 1.0).abs() < 1e-3);
        assert!((a.z + b.z - 1.0).abs() < 1e-3);
        assert!((a.y - b.y).abs() < 1e-4);
    }
}
