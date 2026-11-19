// SPDX-License-Identifier: MIT OR Apache-2.0

//! The quads of a filled map on its 128 unit grid, and their placement.
//!
//! Port of MCP `MapItemRenderer.Instance.render`: the pixel quad covers
//! `0..128` in x (right) and y (down) just in front of the background, and
//! each marker is a 2x2 quad of the `map_icons` sheet (4 by 4 icons), turned
//! by sixteenths of a turn about its centre.

use glam::{Mat4, Vec3};
use umicraft_protocol::packets::play::clientbound::maps::MapIcon;
use umicraft_render::EntityVertex;

/// Side of the map in grid units.
pub(super) const GRID: f32 = 128.0;
/// How far the pixel quad sits in front of the background (the reference
/// uses `-0.01`).
const PIXEL_DEPTH: f32 = -0.01;
/// Depth of every marker in front of the grid, before its own offset.
const MARKER_DEPTH: f32 = -0.02;
/// Extra depth per marker so later markers draw over earlier ones.
const MARKER_STEP: f32 = -0.001;
/// Icons per row and column of the `map_icons` sheet.
const SHEET: u8 = 4;
/// The marker type of the player that owns the map (green arrow).
const OWN_MARKER: u8 = 1;
/// The normal of every vertex: it points up so the entity shader applies
/// its brightest face shade, because the reference draws maps unshaded.
const UNSHADED_NORMAL: [f32; 3] = [0.0, 1.0, 0.0];

/// A vertex on the map grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct GridVertex {
    pub(super) position: [f32; 3],
    pub(super) uv: [f32; 2],
}

/// The two triangles of a quad given by its four corners in the order of
/// the reference (`GL_QUADS`).
fn quad(corners: [[f32; 3]; 4], uvs: [[f32; 2]; 4]) -> [GridVertex; 6] {
    let vertex = |i: usize| GridVertex {
        position: corners[i],
        uv: uvs[i],
    };
    [0, 1, 2, 0, 2, 3].map(vertex)
}

/// The paper behind the map: `-7..135`, a seven unit border.
pub(super) fn background() -> [GridVertex; 6] {
    spanned(-7.0, 135.0, 0.0)
}

/// The map pixels: the whole grid, in front of the background.
pub(super) fn pixels() -> [GridVertex; 6] {
    spanned(0.0, GRID, PIXEL_DEPTH)
}

fn spanned(low: f32, high: f32, depth: f32) -> [GridVertex; 6] {
    quad(
        [
            [low, high, depth],
            [high, high, depth],
            [high, low, depth],
            [low, low, depth],
        ],
        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    )
}

/// The marker quads. With `own_only` only the owner's arrow is drawn, as in
/// an item frame (`renderMap(.., true)`).
pub(super) fn markers(icons: &[MapIcon], own_only: bool) -> Vec<GridVertex> {
    let mut out = Vec::new();
    for icon in icons
        .iter()
        .filter(|icon| !own_only || icon.icon_type == OWN_MARKER)
    {
        let depth = out.len() as f32 / 6.0 * MARKER_STEP;
        let placement = marker_placement(icon);
        let kind = icon.icon_type & 15;
        let (column, row) = (f32::from(kind % SHEET), f32::from(kind / SHEET));
        let sheet = f32::from(SHEET);
        let (u0, v0) = (column / sheet, row / sheet);
        let (u1, v1) = ((column + 1.0) / sheet, (row + 1.0) / sheet);
        let corner = |x: f32, y: f32| placement.transform_point3(Vec3::new(x, y, depth));
        out.extend(quad(
            [
                corner(-1.0, 1.0).to_array(),
                corner(1.0, 1.0).to_array(),
                corner(1.0, -1.0).to_array(),
                corner(-1.0, -1.0).to_array(),
            ],
            [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
        ));
    }
    out
}

/// The matrix of one marker: its position, turn and size.
fn marker_placement(icon: &MapIcon) -> Mat4 {
    let turn = f32::from(icon.direction & 15) * 360.0 / 16.0;
    Mat4::from_translation(Vec3::new(
        f32::from(icon.x) / 2.0 + GRID / 2.0,
        f32::from(icon.z) / 2.0 + GRID / 2.0,
        MARKER_DEPTH,
    )) * Mat4::from_rotation_z(turn.to_radians())
        * Mat4::from_scale(Vec3::new(4.0, 4.0, 3.0))
        * Mat4::from_translation(Vec3::new(-0.125, 0.125, 0.0))
}

/// The vertices of `grid` moved by `matrix`, lit by the packed `light`.
pub(super) fn place(grid: &[GridVertex], matrix: &Mat4, light: u32) -> Vec<EntityVertex> {
    grid.iter()
        .map(|vertex| EntityVertex {
            position: matrix
                .transform_point3(Vec3::from(vertex.position))
                .to_array(),
            normal: UNSHADED_NORMAL,
            uv: vertex.uv,
            light,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon(icon_type: u8, direction: u8, x: i8, z: i8) -> MapIcon {
        MapIcon {
            icon_type,
            direction,
            x,
            z,
        }
    }

    fn bounds(vertices: &[GridVertex]) -> ([f32; 2], [f32; 2]) {
        let xs = vertices.iter().map(|v| v.position[0]);
        let ys = vertices.iter().map(|v| v.position[1]);
        (
            [
                xs.clone().fold(f32::MAX, f32::min),
                xs.fold(f32::MIN, f32::max),
            ],
            [
                ys.clone().fold(f32::MAX, f32::min),
                ys.fold(f32::MIN, f32::max),
            ],
        )
    }

    #[test]
    fn the_pixels_cover_the_grid_and_the_paper_has_a_border() {
        assert_eq!(bounds(&pixels()), ([0.0, 128.0], [0.0, 128.0]));
        assert_eq!(bounds(&background()), ([-7.0, 135.0], [-7.0, 135.0]));
        assert!(pixels()[0].position[2] < background()[0].position[2]);
    }

    #[test]
    fn the_top_of_the_image_is_the_top_of_the_grid() {
        let top = pixels().iter().filter(|v| v.position[1] == 0.0).count();
        assert!(pixels()
            .iter()
            .filter(|v| v.position[1] == 0.0)
            .all(|v| v.uv[1] == 0.0));
        assert_eq!(top, 2 + 1, "two corners, one shared by both triangles");
    }

    #[test]
    fn a_marker_sits_at_half_its_offset_from_the_centre() {
        let quads = markers(&[icon(0, 0, 20, -40)], false);
        assert_eq!(quads.len(), 6);
        let ([x0, x1], [y0, y1]) = bounds(&quads);
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        // 64 + 20/2, nudged by the (-0.125, 0.125) * 4 centring.
        assert!((cx - (74.0 - 0.5)).abs() < 1e-4, "{cx}");
        assert!((cy - (44.0 + 0.5)).abs() < 1e-4, "{cy}");
        assert!((x1 - x0 - 8.0).abs() < 1e-4, "eight units wide");
    }

    #[test]
    fn marker_types_pick_their_cell_of_the_sheet() {
        let quads = markers(&[icon(6, 0, 0, 0)], false);
        let us: Vec<f32> = quads.iter().map(|v| v.uv[0]).collect();
        let vs: Vec<f32> = quads.iter().map(|v| v.uv[1]).collect();
        assert!(us.iter().all(|&u| u == 0.5 || u == 0.75));
        assert!(vs.iter().all(|&v| v == 0.25 || v == 0.5));
    }

    #[test]
    fn a_quarter_turn_swaps_the_corners_of_a_marker() {
        let upright = markers(&[icon(0, 0, 0, 0)], false);
        let turned = markers(&[icon(0, 4, 0, 0)], false);
        assert_ne!(upright[0].position, turned[0].position);
        let (xs, ys) = bounds(&turned);
        assert!((xs[1] - xs[0] - 8.0).abs() < 1e-3 && (ys[1] - ys[0] - 8.0).abs() < 1e-3);
    }

    #[test]
    fn frames_draw_only_the_owner_arrow() {
        let icons = [icon(0, 0, 0, 0), icon(1, 0, 0, 0), icon(5, 0, 0, 0)];
        assert_eq!(markers(&icons, true).len(), 6);
        assert_eq!(markers(&icons, false).len(), 18);
    }

    #[test]
    fn later_markers_draw_in_front_of_earlier_ones() {
        let quads = markers(&[icon(0, 0, 0, 0), icon(0, 0, 0, 0)], false);
        assert!(quads[6].position[2] < quads[0].position[2]);
    }

    #[test]
    fn markers_with_hostile_bytes_stay_in_the_sheet() {
        let quads = markers(&[icon(255, 255, -128, 127)], false);
        assert!(quads.iter().all(|v| (0.0..=1.0).contains(&v.uv[0])));
        assert!(quads.iter().all(|v| (0.0..=1.0).contains(&v.uv[1])));
    }

    #[test]
    fn placing_keeps_uvs_and_applies_light_and_matrix() {
        let placed = place(
            &pixels(),
            &Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0)),
            0xFF,
        );
        assert_eq!(placed.len(), 6);
        assert_eq!(placed[0].position, [1.0, 130.0, 2.99]);
        assert_eq!(placed[0].light, 0xFF);
        assert_eq!(placed[0].uv, [0.0, 1.0]);
    }
}
