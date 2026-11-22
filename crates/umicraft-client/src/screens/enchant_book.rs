// SPDX-License-Identifier: MIT OR Apache-2.0

//! The book of the enchanting table screen, after
//! `GuiEnchantment.drawGuiContainerBackgroundLayer`.
//!
//! The reference draws `ModelBook` with its own perspective projection: a
//! 320 x 240 viewport centred on the screen, `gluPerspective(90, 4 / 3)` and a
//! clip-space shift of (-0.34, 0.23), which puts the book left of the option
//! rows. Here the same matrix stack poses the book in eye space and the
//! projection is applied per vertex, so the book lands in the portrait block
//! of the icon atlas at the reference's place and size. The texture is
//! interpolated without perspective correction inside a triangle; the book is
//! far enough (16 units) for that to stay invisible.

use std::f32::consts::PI;
use std::time::Instant;

use glam::{Mat3, Mat4, Vec3};
use umicraft_game::inventory::enchant_book::{BookTick, ScreenBook, ScreenBookPose};
use umicraft_model::entity::{BookModel, BookPose, ModelVertex};
use umicraft_render::EntityVertex;

use crate::scene::block_entity::facing;
use crate::screens::portrait::Placement;

/// The centre of the reference's viewport in the window's GUI pixels: the
/// window (176 x 166) and the viewport share the screen centre, then the clip
/// shift moves it by `-0.34 * 160` and `0.23 * 120`.
const CENTRE: (f32, f32) = (88.0 - 0.34 * 160.0, 83.0 - 0.23 * 120.0);
/// GUI pixels per unit of `x / -z` and `y / -z`: `gluPerspective(90, 4 / 3)`
/// over a 320 x 240 viewport gives 120 on both axes.
const PIXELS_PER_UNIT: f32 = 120.0;
/// Where the book sits in eye space (`translate(0, 3.3, -16)`), and how far
/// above the centre that is on screen.
const EYE_OFFSET: Vec3 = Vec3::new(0.0, 3.3, -16.0);
const BOOK_RISE: f32 = 3.3 / 16.0 * PIXELS_PER_UNIT;
/// Side of the portrait block in GUI pixels, and the icon box it maps to.
const BLOCK: f32 = 64.0;
const BOX_PER_PIXEL: f32 = 16.0 / BLOCK;
/// Box depth units per eye unit, inside the icon camera's range.
const DEPTH_SCALE: f32 = 4.0;
/// The light word of full daylight (sky 15, block 0).
const FULL_LIGHT: u32 = 15 << 4;
/// `GuiScreen.updateScreen` runs once per game tick.
const TICKS_PER_SECOND: f64 = 20.0;
/// Most ticks stepped in one frame, so a stalled window does not replay
/// minutes of animation.
const MAX_CATCH_UP: u64 = 40;
/// The seed of the page targets. The reference uses an unseeded
/// `new Random()`; any seed gives the same kind of motion.
const PAGE_SEED: u64 = 0x5EED_B00C;

/// The screen book and the wall clock that steps it at 20 ticks a second,
/// from the moment the enchanting screen opened.
#[derive(Debug, Clone)]
pub(crate) struct BookClock {
    book: ScreenBook,
    opened: Instant,
    ticks: u64,
}

impl BookClock {
    /// A shut book on a screen that opens at `now`.
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            book: ScreenBook::new(PAGE_SEED),
            opened: now,
            ticks: 0,
        }
    }

    /// Steps the book to `now` with what the screen shows, and returns the
    /// pose between the last two ticks.
    pub(crate) fn pose_at(&mut self, now: Instant, tick: BookTick<'_>) -> ScreenBookPose {
        let clock = now.saturating_duration_since(self.opened).as_secs_f64() * TICKS_PER_SECOND;
        let target = clock as u64;
        self.ticks = self.ticks.max(target.saturating_sub(MAX_CATCH_UP));
        while self.ticks < target {
            self.book.tick(tick);
            self.ticks += 1;
        }
        self.book.pose((clock - target as f64) as f32)
    }
}

/// The portrait block centred on the book. Only [`Placement::rect`] reads
/// the placement here: the block's left edge is `x - 32`, its top `y - 58`.
pub(crate) const BOOK: Placement = Placement {
    x: CENTRE.0,
    y: CENTRE.1 - BOOK_RISE - BLOCK / 2.0 + 58.0,
    scale: 1.0,
    head: 0.0,
};

/// The book posed and projected into the icon box of the portrait block.
pub(crate) fn book_vertices(pose: ScreenBookPose) -> Vec<EntityVertex> {
    let stack = eye_stack(pose.spread);
    let model = BookPose {
        // `MODEL_BOOK.render(null, 0.0F, f3, f4, f2, ...)`: no breathing.
        age: 0.0,
        rotation: 0.0,
        flip_right: pose.flip_right,
        flip_left: pose.flip_left,
        spread: pose.spread,
    };
    let normals = Mat3::from_mat4(stack);
    let eye: Vec<ModelVertex> = BookModel::emit_in(model, stack);
    facing(eye, Vec3::ZERO)
        .into_iter()
        .map(|vertex| project(vertex, normals))
        .collect()
}

/// The matrix stack of the reference, from model units to eye space.
fn eye_stack(spread: f32) -> Mat4 {
    let shut = 1.0 - spread;
    Mat4::from_translation(EYE_OFFSET)
        * Mat4::from_scale(Vec3::splat(5.0))
        * Mat4::from_rotation_z(PI)
        * Mat4::from_rotation_x(20.0_f32.to_radians())
        * Mat4::from_translation(Vec3::new(shut * 0.2, shut * 0.1, shut * 0.25))
        * Mat4::from_rotation_y((-shut * 90.0 - 90.0).to_radians())
        * Mat4::from_rotation_x(PI)
        * BookModel::MODEL_SCALE
}

/// One eye-space vertex in the icon box (x right, y down, larger z nearer).
/// The normal was turned by the model part only, so `normals` finishes it.
fn project(vertex: ModelVertex, normals: Mat3) -> EntityVertex {
    let p = Vec3::from(vertex.position);
    let depth = -p.z;
    let gui_x = CENTRE.0 + p.x / depth * PIXELS_PER_UNIT;
    let gui_y = CENTRE.1 - p.y / depth * PIXELS_PER_UNIT;
    let (left, top) = (BOOK.x - BLOCK / 2.0, BOOK.y - 58.0);
    let normal = (normals * Vec3::from(vertex.normal)).normalize_or_zero();
    EntityVertex {
        position: [
            (gui_x - left) * BOX_PER_PIXEL,
            (gui_y - top) * BOX_PER_PIXEL,
            (p.z - EYE_OFFSET.z) * DEPTH_SCALE,
        ],
        normal: [normal.x, -normal.y, normal.z],
        uv: vertex.uv,
        light: FULL_LIGHT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPEN: ScreenBookPose = ScreenBookPose {
        spread: 1.0,
        flip_right: 0.3,
        flip_left: 0.6,
    };

    #[test]
    fn the_book_sits_left_of_the_rows_inside_its_block() {
        let vertices = book_vertices(OPEN);
        assert!(!vertices.is_empty());
        for vertex in &vertices {
            let [x, y, _] = vertex.position;
            assert!(
                (0.0..=16.0).contains(&x) && (0.0..=16.0).contains(&y),
                "{x} {y}"
            );
        }
        // The middle of the book, in window pixels: left of the rows (x 60).
        let n = vertices.len() as f32;
        let mean = vertices
            .iter()
            .fold(Vec3::ZERO, |acc, v| acc + Vec3::from(v.position))
            / n;
        let gui_x = BOOK.x - BLOCK / 2.0 + mean.x / BOX_PER_PIXEL;
        let gui_y = BOOK.y - 58.0 + mean.y / BOX_PER_PIXEL;
        assert!((20.0..50.0).contains(&gui_x), "{gui_x}");
        assert!((15.0..45.0).contains(&gui_y), "{gui_y}");
    }

    #[test]
    fn an_open_book_is_wider_than_a_shut_one() {
        let width = |spread| {
            let xs: Vec<f32> = book_vertices(ScreenBookPose { spread, ..OPEN })
                .iter()
                .map(|v| v.position[0])
                .collect();
            let (min, max) = xs
                .iter()
                .fold((f32::MAX, f32::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
            max - min
        };
        assert!(width(1.0) > width(0.0));
    }

    #[test]
    fn depths_stay_inside_the_icon_camera() {
        for vertex in book_vertices(OPEN) {
            assert!(vertex.position[2].abs() < 32.0);
        }
    }
}
