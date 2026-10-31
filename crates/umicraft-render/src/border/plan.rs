// SPDX-License-Identifier: MIT OR Apache-2.0

//! The CPU side of `RenderGlobal.renderWorldBorder`: which walls are close
//! enough to draw, their opacity and their geometry.
//!
//! The reference emits one quad per block of wall; texture coordinates grow
//! linearly (0.5 per block) and the texture repeats, so one stretched quad per
//! wall is identical and keeps the frame at 24 vertices.

use crate::sky::{quad, SkyVertex};

use super::view::BorderView;

/// Wall height: the quads run from `y = 0` to `y = 256`.
const WALL_TOP: f64 = 256.0;
/// Texture rows spanned by the wall (`f6 = 128.0F`).
const V_SPAN: f32 = 128.0;
/// Texture columns per block of wall length.
const U_PER_BLOCK: f64 = 0.5;
/// Period of the texture scroll in milliseconds (`getSystemTime() % 3000L`).
const SCROLL_PERIOD_MS: f64 = 3000.0;
/// Vertices of the four walls at most.
pub(super) const MAX_VERTICES: usize = 24;

/// What a frame draws: the tint and the wall vertices.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Walls {
    /// Tint with the fade in alpha.
    pub colour: [f32; 4],
    /// Vertices relative to the eye; only `count` are valid.
    pub vertices: [SkyVertex; MAX_VERTICES],
    /// Number of valid vertices (a multiple of 6).
    pub count: u32,
}

/// The texture scroll for a clock in seconds, in `0.0..1.0`.
pub(super) fn scroll(seconds: f64) -> f32 {
    (seconds * 1000.0).rem_euclid(SCROLL_PERIOD_MS) as f32 / SCROLL_PERIOD_MS as f32
}

/// The opacity at `closest` blocks from the nearest wall with a reach of
/// `distance` blocks: `(1 - closest / distance)^4`, clamped to 1 as GL clamps
/// the colour.
pub(super) fn fade(closest: f64, distance: f64) -> f32 {
    (1.0 - closest / distance).powi(4).min(1.0) as f32
}

/// Plans the frame for an eye at `eye` and a reach of `distance` blocks
/// (`renderDistanceChunks * 16`); `None` when no wall is in reach.
pub(super) fn plan(view: &BorderView, eye: [f64; 3], distance: f64, scroll: f32) -> Option<Walls> {
    let [x, _, z] = eye;
    let closest = view.closest_distance(x, z);
    if distance <= 0.0 || closest >= distance {
        return None;
    }
    let alpha = fade(closest, distance);
    let [r, g, b] = view.status.colour();
    let mut walls = Walls {
        colour: [r, g, b, alpha],
        vertices: [SkyVertex::plain([0.0; 3]); MAX_VERTICES],
        count: 0,
    };
    // Extents along the wall, clamped to the reach and to the border.
    let z_span = (
        (z - distance).floor().max(view.min_z()),
        (z + distance).ceil().min(view.max_z()),
    );
    let x_span = (
        (x - distance).floor().max(view.min_x()),
        (x + distance).ceil().min(view.max_x()),
    );
    let mut push = |along_x: bool, fixed: f64, span: (f64, f64)| {
        if span.1 > span.0 {
            let corners = wall_corners(along_x, fixed, span, eye, scroll);
            let at = walls.count as usize;
            walls.vertices[at..at + 6].copy_from_slice(&quad(corners));
            walls.count += 6;
        }
    };
    if x > view.max_x() - distance {
        push(false, view.max_x(), z_span);
    }
    if x < view.min_x() + distance {
        push(false, view.min_x(), z_span);
    }
    if z > view.max_z() - distance {
        push(true, view.max_z(), x_span);
    }
    if z < view.min_z() + distance {
        push(true, view.min_z(), x_span);
    }
    (walls.count > 0).then_some(walls)
}

/// The four corners of a wall at coordinate `fixed`, running over `span` on
/// the other horizontal axis, in the reference's winding.
fn wall_corners(
    along_x: bool,
    fixed: f64,
    span: (f64, f64),
    eye: [f64; 3],
    scroll: f32,
) -> [SkyVertex; 4] {
    let position = |along: f64, y: f64| {
        let (x, z) = if along_x {
            (along, fixed)
        } else {
            (fixed, along)
        };
        [
            (x - eye[0]) as f32,
            (y - eye[1]) as f32,
            (z - eye[2]) as f32,
        ]
    };
    let u0 = scroll;
    let u1 = scroll + ((span.1 - span.0) * U_PER_BLOCK) as f32;
    let (v0, v1) = (scroll, scroll + V_SPAN);
    [
        SkyVertex::textured(position(span.0, WALL_TOP), [u0, v0]),
        SkyVertex::textured(position(span.1, WALL_TOP), [u1, v0]),
        SkyVertex::textured(position(span.1, 0.0), [u1, v1]),
        SkyVertex::textured(position(span.0, 0.0), [u0, v1]),
    ]
}

#[cfg(test)]
mod tests {
    use super::super::view::BorderStatus;
    use super::*;

    fn border() -> BorderView {
        BorderView {
            center: (0.0, 0.0),
            diameter: 1000.0,
            portal_boundary: 29_999_984,
            status: BorderStatus::Growing,
        }
    }

    #[test]
    fn nothing_is_drawn_far_from_every_wall() {
        assert!(plan(&border(), [0.0, 64.0, 0.0], 128.0, 0.0).is_none());
        // Exactly at the reach the fade is zero, so the reference's `>=`
        // check leaves it out too.
        assert!(plan(&border(), [372.0, 64.0, 0.0], 128.0, 0.0).is_none());
    }

    #[test]
    fn one_wall_near_an_edge_and_two_in_a_corner() {
        let one = plan(&border(), [450.0, 64.0, 0.0], 128.0, 0.0).expect("east wall");
        assert_eq!(one.count, 6);
        let two = plan(&border(), [450.0, 64.0, 450.0], 128.0, 0.0).expect("corner");
        assert_eq!(two.count, 12);
    }

    #[test]
    fn the_wall_is_clamped_to_the_reach_around_the_eye() {
        let walls = plan(&border(), [450.0, 64.0, 0.0], 128.0, 0.0).expect("east wall");
        let zs: Vec<f32> = walls.vertices[..6].iter().map(|v| v.position[2]).collect();
        assert_eq!(zs.iter().copied().fold(f32::MAX, f32::min), -128.0);
        assert_eq!(zs.iter().copied().fold(f32::MIN, f32::max), 128.0);
        let x = walls.vertices[0].position[0];
        assert_eq!(x, 50.0, "relative to the eye");
    }

    #[test]
    fn the_wall_is_clamped_to_the_other_walls() {
        // In the corner the east wall stops at the south wall (z = 500).
        let walls = plan(&border(), [450.0, 64.0, 450.0], 128.0, 0.0).expect("corner");
        let max_z = walls.vertices[..6]
            .iter()
            .map(|v| v.position[2])
            .fold(f32::MIN, f32::max);
        assert_eq!(max_z, 50.0);
    }

    #[test]
    fn the_fade_is_the_fourth_power_of_the_closeness() {
        assert_eq!(fade(0.0, 128.0), 1.0);
        assert!((fade(64.0, 128.0) - 0.0625).abs() < 1e-6);
        assert_eq!(fade(-10.0, 128.0), 1.0, "outside the border stays opaque");
        let walls = plan(&border(), [436.0, 64.0, 0.0], 128.0, 0.0).expect("wall");
        assert!((walls.colour[3] - 0.0625).abs() < 1e-6);
    }

    #[test]
    fn the_tint_follows_the_status() {
        let mut view = border();
        view.status = BorderStatus::Shrinking;
        let walls = plan(&view, [499.0, 64.0, 0.0], 128.0, 0.0).expect("wall");
        assert_eq!(&walls.colour[..3], &BorderStatus::Shrinking.colour());
    }

    #[test]
    fn the_scroll_wraps_every_three_seconds() {
        assert_eq!(scroll(0.0), 0.0);
        assert!((scroll(1.5) - 0.5).abs() < 1e-6);
        assert!(scroll(3.0).abs() < 1e-6);
        assert!((scroll(4.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn texture_coordinates_grow_half_a_column_per_block() {
        let walls = plan(&border(), [450.0, 64.0, 0.0], 128.0, 0.25).expect("wall");
        let [first, second] = [walls.vertices[0].uv, walls.vertices[1].uv];
        assert_eq!(first, [0.25, 0.25]);
        assert_eq!(second[0] - first[0], 128.0, "256 blocks long");
        assert_eq!(walls.vertices[2].uv[1], 0.25 + 128.0);
    }
}
