// SPDX-License-Identifier: MIT OR Apache-2.0

//! The lines of the debug chords: the entity hitboxes (F3 + B) and the chunk
//! borders (F3 + G).
//!
//! Both are plain functions from world data to [`DebugLine`]s, so they are
//! tested without a window; the renderer only draws what they return.
//!
//! Hitboxes follow MCP `RenderManager.renderDebugBoundingBox`: a white box
//! around every entity. The chunk borders follow the F3 + G display of later
//! versions (1.8 has none), drawn in the same style: yellow lines at the
//! corners and at each 16-block section of the chunk the player stands in,
//! blue lines in a one-block grid on its four walls.

use umicraft_game::combat::hitbox::size_of;
use umicraft_game::entity::EntityRenderSnapshot;
use umicraft_render::{edge_vertices, DebugLine, OutlineBox};

/// The colour of a hitbox (`RenderGlobal.drawOutlinedBoundingBox` with
/// 255, 255, 255).
const HITBOX_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// The colour of the chunk and section borders.
const BORDER_COLOR: [f32; 4] = [1.0, 0.9, 0.0, 1.0];

/// The colour of the block grid on the walls of the current chunk.
const GRID_COLOR: [f32; 4] = [0.25, 0.35, 1.0, 1.0];

/// Blocks per chunk side and per chunk section.
const CHUNK: i32 = 16;

/// The height of the world in blocks (1.8 has no taller worlds).
const WORLD_HEIGHT: i32 = 256;

/// How far above and below the eye the grid on the walls reaches, in blocks.
const GRID_REACH: i32 = 16;

/// How many chunks around the player get yellow corner lines.
const CORNER_RADIUS: i32 = 2;

/// The hitbox lines of `entities`, in world coordinates.
///
/// The local player's own body is left out: vanilla does not render the view
/// entity in the first person, so it has no box either. A kind without a size
/// falls back to the width and height of [`umicraft_game::entity::EntityKind`].
#[must_use]
pub fn hitbox_lines(entities: &[EntityRenderSnapshot]) -> Vec<DebugLine> {
    let mut lines = Vec::with_capacity(entities.len() * 12);
    for entity in entities.iter().filter(|entity| !entity.local) {
        let (width, height) = size_of(entity.kind, &entity.meta).unwrap_or_else(|| {
            (
                f64::from(entity.kind.width()),
                f64::from(entity.kind.height()),
            )
        });
        let feet = entity.origin + entity.position.as_dvec3();
        let half = width / 2.0;
        let outline = OutlineBox {
            min: [
                (feet.x - half) as f32,
                feet.y as f32,
                (feet.z - half) as f32,
            ],
            max: [
                (feet.x + half) as f32,
                (feet.y + height) as f32,
                (feet.z + half) as f32,
            ],
        };
        push_box(&mut lines, &outline, HITBOX_COLOR);
    }
    lines
}

fn push_box(lines: &mut Vec<DebugLine>, outline: &OutlineBox, color: [f32; 4]) {
    lines.extend(
        edge_vertices(outline)
            .chunks_exact(2)
            .map(|edge| DebugLine {
                from: edge[0],
                to: edge[1],
                color,
            }),
    );
}

/// The chunk-border lines around `eye`, in world coordinates.
#[must_use]
pub fn chunk_border_lines(eye: [f64; 3]) -> Vec<DebugLine> {
    let chunk_x = (eye[0].floor() as i32).div_euclid(CHUNK) * CHUNK;
    let chunk_z = (eye[2].floor() as i32).div_euclid(CHUNK) * CHUNK;
    let mut lines = Vec::new();
    push_corners(&mut lines, chunk_x, chunk_z);
    push_sections(&mut lines, chunk_x, chunk_z);
    push_wall_grid(&mut lines, chunk_x, chunk_z, eye[1].floor() as i32);
    lines
}

/// A full-height line at every chunk corner near the chunk at (`x`, `z`).
fn push_corners(lines: &mut Vec<DebugLine>, x: i32, z: i32) {
    let span = -CORNER_RADIUS..=CORNER_RADIUS + 1;
    for dx in span.clone() {
        for dz in span.clone() {
            let (cx, cz) = ((x + dx * CHUNK) as f32, (z + dz * CHUNK) as f32);
            lines.push(DebugLine {
                from: [cx, 0.0, cz],
                to: [cx, WORLD_HEIGHT as f32, cz],
                color: BORDER_COLOR,
            });
        }
    }
}

/// The outline of the chunk at (`x`, `z`) at every 16-block section height.
fn push_sections(lines: &mut Vec<DebugLine>, x: i32, z: i32) {
    let (x0, z0) = (x as f32, z as f32);
    let (x1, z1) = ((x + CHUNK) as f32, (z + CHUNK) as f32);
    for y in (0..=WORLD_HEIGHT).step_by(CHUNK as usize) {
        let y = y as f32;
        let corners = [[x0, z0], [x1, z0], [x1, z1], [x0, z1]];
        for (index, from) in corners.iter().enumerate() {
            let to = corners[(index + 1) % 4];
            lines.push(DebugLine {
                from: [from[0], y, from[1]],
                to: [to[0], y, to[1]],
                color: BORDER_COLOR,
            });
        }
    }
}

/// The one-block grid on the four walls of the chunk at (`x`, `z`), within
/// [`GRID_REACH`] blocks of `eye_y`.
fn push_wall_grid(lines: &mut Vec<DebugLine>, x: i32, z: i32, eye_y: i32) {
    let low = (eye_y - GRID_REACH).max(0);
    let high = (eye_y + GRID_REACH).min(WORLD_HEIGHT);
    let (x1, z1) = (x + CHUNK, z + CHUNK);
    // Each wall: its start corner and the unit step along it.
    let walls = [
        ((x, z), (1, 0)),
        ((x, z1), (1, 0)),
        ((x, z), (0, 1)),
        ((x1, z), (0, 1)),
    ];
    for ((wall_x, wall_z), (step_x, step_z)) in walls {
        for offset in 0..=CHUNK {
            let (px, pz) = (
                (wall_x + step_x * offset) as f32,
                (wall_z + step_z * offset) as f32,
            );
            lines.push(DebugLine {
                from: [px, low as f32, pz],
                to: [px, high as f32, pz],
                color: GRID_COLOR,
            });
        }
        for y in low..=high {
            lines.push(DebugLine {
                from: [wall_x as f32, y as f32, wall_z as f32],
                to: [
                    (wall_x + step_x * CHUNK) as f32,
                    y as f32,
                    (wall_z + step_z * CHUNK) as f32,
                ],
                color: GRID_COLOR,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_render::MAX_DEBUG_LINES;

    #[test]
    fn the_chunk_of_the_eye_is_found_below_zero_too() {
        let lines = chunk_border_lines([-0.5, 64.0, -17.0]);
        // The sections of the chunk (-16, -32) start at that corner.
        let at_corner = lines
            .iter()
            .any(|line| line.from == [-16.0, 64.0, -32.0] && line.color == BORDER_COLOR);
        assert!(at_corner, "the section outline starts at the chunk corner");
    }

    #[test]
    fn the_corner_lines_span_the_whole_world() {
        let lines = chunk_border_lines([3.0, 70.0, 3.0]);
        let tall = lines
            .iter()
            .filter(|line| line.to[1] - line.from[1] == WORLD_HEIGHT as f32)
            .count();
        assert_eq!(
            tall,
            ((2 * CORNER_RADIUS + 2) * (2 * CORNER_RADIUS + 2)) as usize
        );
    }

    #[test]
    fn the_wall_grid_stays_inside_the_world() {
        for eye_y in [-5.0, 2.0, 128.0, 255.0, 400.0] {
            for line in chunk_border_lines([0.0, eye_y, 0.0]) {
                assert!(line.from[1] >= 0.0 && line.to[1] <= WORLD_HEIGHT as f32);
            }
        }
    }

    #[test]
    fn the_borders_fit_the_line_buffer() {
        assert!(chunk_border_lines([0.0, 128.0, 0.0]).len() < MAX_DEBUG_LINES);
    }

    #[test]
    fn no_entities_have_no_hitboxes() {
        assert!(hitbox_lines(&[]).is_empty());
    }
}
