// SPDX-License-Identifier: MIT OR Apache-2.0

//! The two static cloud meshes, `renderClouds` (fast) and `renderCloudsFancy`.
//!
//! Both are texture driven in the reference: it draws full squares and lets
//! the alpha of `clouds.png` decide which texels exist. The meshes here are
//! the same squares and strips in a fixed layout around the eye; the frame
//! only shifts them ([`super::drift`]), so nothing is rebuilt per frame.

use std::ops::Range;

use crate::sky::{quad, SkyVertex};

use super::drift::UV_PER_TEXEL;

/// Half the side of the fast layer in blocks (`-256..256`).
const FAST_HALF: i32 = 256;
/// Side of one fast square in blocks.
const FAST_STEP: i32 = 32;
/// Texture coordinate of one block in the fast version.
const FAST_UV_PER_BLOCK: f32 = 1.0 / 2048.0;
/// Tiles per axis of the fancy grid (`j1, k1 = -3..=4`).
const TILES: std::ops::RangeInclusive<i32> = -3..=4;
/// Cells per tile side (`8`).
const TILE: i32 = 8;
/// Thickness of a fancy cloud in unscaled units (`4.0`).
const THICKNESS: f32 = 4.0;
/// Inset of the top and the far side faces, against z-fighting (`9.765625E-4`).
const INSET: f32 = 0.000_976_562_5;
/// Face shades of the fancy version: bottom, top, X sides, Z sides.
const SHADE_BOTTOM: f32 = 0.7;
const SHADE_TOP: f32 = 1.0;
const SHADE_X: f32 = 0.9;
const SHADE_Z: f32 = 0.8;

/// The vertex ranges of the fancy mesh: the faces the eye height decides on
/// and the sides, which are always drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FancyRanges {
    pub bottom: Range<u32>,
    pub top: Range<u32>,
    pub sides: Range<u32>,
}

/// Both meshes in one vertex list.
pub(super) struct Meshes {
    pub vertices: Vec<SkyVertex>,
    pub fast: Range<u32>,
    pub fancy: FancyRanges,
}

/// Builds the fast and the fancy mesh.
pub(super) fn build() -> Meshes {
    let mut vertices = fast();
    let fast = 0..vertices.len() as u32;
    let (fancy_vertices, local) = fancy();
    let base = vertices.len() as u32;
    vertices.extend(fancy_vertices);
    let shift = |range: Range<u32>| range.start + base..range.end + base;
    Meshes {
        vertices,
        fast,
        fancy: FancyRanges {
            bottom: shift(local.bottom),
            top: shift(local.top),
            sides: shift(local.sides),
        },
    }
}

/// A vertex at `(x, y, z)` whose texture coordinate is `(u, v)` texels.
fn vertex(x: f32, y: f32, z: f32, u: f32, v: f32, shade: f32) -> SkyVertex {
    SkyVertex::coloured(
        [x, y, z],
        [u * UV_PER_TEXEL, v * UV_PER_TEXEL],
        [shade, shade, shade, 1.0],
    )
}

/// The fast layer: 32-block squares from -256 to 256, with the texture
/// coordinate proportional to the position (one texel per 8 blocks).
fn fast() -> Vec<SkyVertex> {
    let steps = (FAST_HALF * 2 / FAST_STEP) as usize;
    let mut vertices = Vec::with_capacity(steps * steps * 6);
    let corner = |x: i32, z: i32| {
        SkyVertex::textured(
            [x as f32, 0.0, z as f32],
            [x as f32 * FAST_UV_PER_BLOCK, z as f32 * FAST_UV_PER_BLOCK],
        )
    };
    for x in (-FAST_HALF..FAST_HALF).step_by(FAST_STEP as usize) {
        for z in (-FAST_HALF..FAST_HALF).step_by(FAST_STEP as usize) {
            vertices.extend(quad([
                corner(x, z + FAST_STEP),
                corner(x + FAST_STEP, z + FAST_STEP),
                corner(x + FAST_STEP, z),
                corner(x, z),
            ]));
        }
    }
    vertices
}

/// The fancy layer in unscaled units (one unit is one texel, 12 blocks).
fn fancy() -> (Vec<SkyVertex>, FancyRanges) {
    let mut bottom = Vec::new();
    let mut top = Vec::new();
    let mut sides = Vec::new();
    for j in TILES {
        for k in TILES {
            let (x, z) = ((j * TILE) as f32, (k * TILE) as f32);
            bottom.extend(flat_face(x, z, 0.0, SHADE_BOTTOM));
            top.extend(flat_face(x, z, THICKNESS - INSET, SHADE_TOP));
            // The outer tiles skip the sides that face away from the eye
            // (`j1 > -1`, `j1 <= 1`, `k1 > -1`, `k1 <= 1`).
            if j > -1 {
                sides.extend(x_sides(x, z, 0.0));
            }
            if j <= 1 {
                sides.extend(x_sides(x, z, 1.0 - INSET));
            }
            if k > -1 {
                sides.extend(z_sides(x, z, 0.0));
            }
            if k <= 1 {
                sides.extend(z_sides(x, z, 1.0 - INSET));
            }
        }
    }
    let ranges = FancyRanges {
        bottom: 0..bottom.len() as u32,
        top: bottom.len() as u32..(bottom.len() + top.len()) as u32,
        sides: (bottom.len() + top.len()) as u32..(bottom.len() + top.len() + sides.len()) as u32,
    };
    let vertices = [bottom, top, sides].concat();
    (vertices, ranges)
}

/// One horizontal square of a tile at height `y`.
fn flat_face(x: f32, z: f32, y: f32, shade: f32) -> [SkyVertex; 6] {
    let size = TILE as f32;
    quad([
        vertex(x, y, z + size, x, z + size, shade),
        vertex(x + size, y, z + size, x + size, z + size, shade),
        vertex(x + size, y, z, x + size, z, shade),
        vertex(x, y, z, x, z, shade),
    ])
}

/// The eight strips facing along X at `x + l + offset`, each sampling the
/// texel column `l` at its centre. `offset` 0 is the west face.
fn x_sides(x: f32, z: f32, offset: f32) -> Vec<SkyVertex> {
    let size = TILE as f32;
    (0..TILE)
        .flat_map(|l| {
            let px = x + l as f32 + offset;
            let u = x + l as f32 + 0.5;
            quad([
                vertex(px, 0.0, z + size, u, z + size, SHADE_X),
                vertex(px, THICKNESS, z + size, u, z + size, SHADE_X),
                vertex(px, THICKNESS, z, u, z, SHADE_X),
                vertex(px, 0.0, z, u, z, SHADE_X),
            ])
        })
        .collect()
}

/// The eight strips facing along Z at `z + l + offset`.
fn z_sides(x: f32, z: f32, offset: f32) -> Vec<SkyVertex> {
    let size = TILE as f32;
    (0..TILE)
        .flat_map(|l| {
            let pz = z + l as f32 + offset;
            let v = z + l as f32 + 0.5;
            quad([
                vertex(x, THICKNESS, pz, x, v, SHADE_Z),
                vertex(x + size, THICKNESS, pz, x + size, v, SHADE_Z),
                vertex(x + size, 0.0, pz, x + size, v, SHADE_Z),
                vertex(x, 0.0, pz, x, v, SHADE_Z),
            ])
        })
        .collect()
}

/// The ranges the eye height needs, given the base of the layer `base`
/// blocks above the eye: the bottom is skipped from far above
/// (`f3 > -5`), the top from far below (`f3 <= 5`).
pub(super) fn visible_ranges(ranges: &FancyRanges, base: f32) -> [Range<u32>; 3] {
    let empty = |range: &Range<u32>| range.start..range.start;
    [
        if base > -5.0 {
            ranges.bottom.clone()
        } else {
            empty(&ranges.bottom)
        },
        if base <= 5.0 {
            ranges.top.clone()
        } else {
            empty(&ranges.top)
        },
        ranges.sides.clone(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fast_layer_is_sixteen_squares_per_side() {
        let meshes = build();
        assert_eq!(meshes.fast.len(), 16 * 16 * 6);
    }

    #[test]
    fn the_fancy_layer_has_a_top_and_bottom_per_tile() {
        let meshes = build();
        assert_eq!(meshes.fancy.bottom.len(), 64 * 6);
        assert_eq!(meshes.fancy.top.len(), 64 * 6);
    }

    #[test]
    fn outer_tiles_skip_the_sides_facing_away() {
        // Each of the four directions is kept by 5 of the 8 tile columns.
        let sides_per_direction = 5 * 8 * 8;
        assert_eq!(build().fancy.sides.len(), 4 * sides_per_direction * 6);
    }

    #[test]
    fn the_ranges_tile_the_vertex_list_without_gaps() {
        let meshes = build();
        assert_eq!(meshes.fast.end, meshes.fancy.bottom.start);
        assert_eq!(meshes.fancy.bottom.end, meshes.fancy.top.start);
        assert_eq!(meshes.fancy.top.end, meshes.fancy.sides.start);
        assert_eq!(meshes.fancy.sides.end as usize, meshes.vertices.len());
    }

    #[test]
    fn the_fast_texture_scale_is_one_texel_per_eight_blocks() {
        let meshes = build();
        let fast = &meshes.vertices[..meshes.fast.end as usize];
        let far = fast.iter().map(|v| v.position[0]).fold(f32::MIN, f32::max);
        assert_eq!(far, 256.0);
        let corner = fast.iter().find(|v| v.position[0] == 256.0).unwrap();
        assert!((corner.uv[0] - 0.125).abs() < 1e-7);
    }

    #[test]
    fn the_faces_carry_the_reference_shades() {
        let meshes = build();
        let shade = |range: &Range<u32>| meshes.vertices[range.start as usize].color[0];
        assert_eq!(shade(&meshes.fancy.bottom), 0.7);
        assert_eq!(shade(&meshes.fancy.top), 1.0);
        let sides = &meshes.vertices[meshes.fancy.sides.start as usize..];
        let shades: Vec<f32> = sides.iter().map(|v| v.color[0]).collect();
        assert!(shades.contains(&0.9) && shades.contains(&0.8));
    }

    #[test]
    fn a_side_strip_samples_one_texel_column_at_its_centre() {
        let strips = x_sides(0.0, 0.0, 0.0);
        assert_eq!(strips.len(), 8 * 6);
        // The third strip (l = 2) reads texel 2.5.
        assert!((strips[2 * 6].uv[0] - 2.5 * UV_PER_TEXEL).abs() < 1e-9);
    }

    #[test]
    fn the_layer_faces_follow_the_eye_height() {
        let ranges = build().fancy;
        let lens = |base: f32| visible_ranges(&ranges, base).map(|r| r.len());
        // Far below the layer: no top face. Far above: no bottom face.
        assert_eq!(lens(60.0)[..2], [384, 0]);
        assert_eq!(lens(-60.0)[..2], [0, 384]);
        assert_eq!(lens(0.0)[..2], [384, 384]);
    }
}
