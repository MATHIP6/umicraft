// SPDX-License-Identifier: MIT OR Apache-2.0

//! Paintings (MCP `RenderPainting`).
//!
//! A painting is one slab of 16 x 16 pixels per block of its artwork, 1 block
//! thick: the artwork on the front, a strip of the sheet on the five other
//! sides. The reference draws it in the GL frame (y up) in pixels under
//! `scale(1/16)`; this ports the vertices as they are.

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use umicraft_game::entity::painting::ARTS;
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::TextureSlot;
use umicraft_render::EntityVertex;

/// Texels of the sheet, a side.
const SHEET: f32 = 256.0;
/// Where the back and the edges of a painting are on the sheet.
const BACK: (f32, f32) = (0.75, 0.8125);
const EDGE_V: f32 = 0.001_953_125;
const SIDE_U: f32 = 0.751_953_1;

/// Adds the painting `ent` to `batches` when it is one. The light is that of
/// the entity as a whole; the reference lights each block of the artwork by
/// the light of the cell in front of it.
pub(super) fn add_painting(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    ent: &EntityRenderSnapshot,
    light: u32,
) {
    if ent.kind != EntityKind::Painting {
        return;
    }
    let Some(art) = ARTS.get(ent.object_data.max(0) as usize) else {
        return;
    };
    let placed = Mat4::from_translation(ent.position)
        * Mat4::from_rotation_y((180.0 - ent.yaw).to_radians())
        * Mat4::from_scale(Vec3::splat(1.0 / 16.0));
    let out = batches.entry(TextureSlot::Painting).or_default();
    let (width, height) = (f32::from(art.width), f32::from(art.height));
    let (u, v) = (f32::from(art.u), f32::from(art.v));

    let rows = u32::from(art.height / 16);
    for i in 0..u32::from(art.width / 16) {
        for j in 0..rows {
            // The light in front of this block of the artwork; the whole
            // entity's light when the game thread has not sampled the cells.
            let light = ent
                .block_lights
                .get((i * rows + j) as usize)
                .map_or(light, |cell| {
                    (light & !0xFF)
                        | u32::from(cell.block.min(15))
                        | (u32::from(cell.sky.min(15)) << 4)
                });
            let mut quad = |corners: [([f32; 3], [f32; 2]); 4], normal: [f32; 3]| {
                let normal = placed
                    .transform_vector3(Vec3::from(normal))
                    .normalize_or_zero()
                    .to_array();
                let vertex = |(position, uv): ([f32; 3], [f32; 2])| EntityVertex {
                    position: placed.transform_point3(Vec3::from(position)).to_array(),
                    normal,
                    uv,
                    light,
                };
                let [a, b, c, d] = corners.map(vertex);
                out.extend([a, b, c, a, c, d]);
            };
            let (i, j) = (i as f32, j as f32);
            let x15 = -width / 2.0 + (i + 1.0) * 16.0;
            let x16 = -width / 2.0 + i * 16.0;
            let y17 = -height / 2.0 + (j + 1.0) * 16.0;
            let y18 = -height / 2.0 + j * 16.0;
            let u19 = (u + width - i * 16.0) / SHEET;
            let u20 = (u + width - (i + 1.0) * 16.0) / SHEET;
            let v21 = (v + height - j * 16.0) / SHEET;
            let v22 = (v + height - (j + 1.0) * 16.0) / SHEET;
            let (f3, f4) = BACK;
            let (f9, f14) = (EDGE_V, 0.0625);
            quad(
                [
                    ([x15, y18, -8.0], [u20, v21]),
                    ([x16, y18, -8.0], [u19, v21]),
                    ([x16, y17, -8.0], [u19, v22]),
                    ([x15, y17, -8.0], [u20, v22]),
                ],
                [0.0, 0.0, -1.0],
            );
            quad(
                [
                    ([x15, y17, 8.0], [f3, 0.0]),
                    ([x16, y17, 8.0], [f4, 0.0]),
                    ([x16, y18, 8.0], [f4, f14]),
                    ([x15, y18, 8.0], [f3, f14]),
                ],
                [0.0, 0.0, 1.0],
            );
            quad(
                [
                    ([x15, y17, -8.0], [f3, f9]),
                    ([x16, y17, -8.0], [f4, f9]),
                    ([x16, y17, 8.0], [f4, f9]),
                    ([x15, y17, 8.0], [f3, f9]),
                ],
                [0.0, 1.0, 0.0],
            );
            quad(
                [
                    ([x15, y18, 8.0], [f3, f9]),
                    ([x16, y18, 8.0], [f4, f9]),
                    ([x16, y18, -8.0], [f4, f9]),
                    ([x15, y18, -8.0], [f3, f9]),
                ],
                [0.0, -1.0, 0.0],
            );
            quad(
                [
                    ([x15, y17, 8.0], [SIDE_U, 0.0]),
                    ([x15, y18, 8.0], [SIDE_U, f14]),
                    ([x15, y18, -8.0], [SIDE_U, f14]),
                    ([x15, y17, -8.0], [SIDE_U, 0.0]),
                ],
                [-1.0, 0.0, 0.0],
            );
            quad(
                [
                    ([x16, y17, -8.0], [SIDE_U, 0.0]),
                    ([x16, y18, -8.0], [SIDE_U, f14]),
                    ([x16, y18, 8.0], [SIDE_U, f14]),
                    ([x16, y17, 8.0], [SIDE_U, 0.0]),
                ],
                [1.0, 0.0, 0.0],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::painting::art_index;
    use umicraft_game::entity::EntityId;

    fn painting(title: &str, yaw: f32) -> Vec<EntityVertex> {
        let mut ent = EntityRenderSnapshot::new(
            EntityId(1),
            EntityKind::Painting,
            Vec3::new(10.5, 64.5, 20.03125),
        );
        ent.object_data = art_index(title).unwrap() as i32;
        ent.yaw = yaw;
        let mut batches = HashMap::new();
        add_painting(&mut batches, &ent, 0xFF);
        batches.remove(&TextureSlot::Painting).unwrap_or_default()
    }

    fn range(vertices: &[&EntityVertex], pick: impl Fn(&EntityVertex) -> f32) -> (f32, f32) {
        let values: Vec<f32> = vertices.iter().map(|v| pick(v)).collect();
        (
            values.iter().cloned().fold(f32::MAX, f32::min),
            values.iter().cloned().fold(f32::MIN, f32::max),
        )
    }

    #[test]
    fn a_painting_is_six_faces_per_block_of_its_artwork() {
        assert_eq!(painting("Kebab", 0.0).len(), 36);
        assert_eq!(painting("Pool", 0.0).len(), 2 * 36);
        assert_eq!(painting("Pointer", 0.0).len(), 16 * 36);
        assert_eq!(painting("Skeleton", 0.0).len(), 12 * 36);
    }

    #[test]
    fn a_painting_is_a_block_thick_and_shows_its_artwork_to_its_facing() {
        // Facing south (yaw 0) the artwork is towards +z, half a block from
        // the centre, and the back towards the wall.
        let vertices = painting("Kebab", 0.0);
        let art: Vec<_> = vertices.iter().filter(|v| v.normal[2] > 0.5).collect();
        assert_eq!(art.len(), 6);
        assert!(art
            .iter()
            .all(|v| (v.position[2] - (20.03125 + 0.5)).abs() < 1e-4));
        let all: Vec<_> = vertices.iter().collect();
        let (z0, z1) = range(&all, |v| v.position[2]);
        assert!((z1 - z0 - 1.0).abs() < 1e-4);
        // Facing east (yaw 270) it faces +x instead.
        let east = painting("Kebab", 270.0);
        assert!(east
            .iter()
            .filter(|v| v.normal[0] > 0.5)
            .any(|v| (v.position[0] - (10.5 + 0.5)).abs() < 1e-4));
    }

    #[test]
    fn the_artwork_reads_its_cell_of_the_sheet() {
        // Skeleton: 64 x 48 at (192, 64): u in 192..256, v in 64..112.
        let vertices = painting("Skeleton", 0.0);
        let art: Vec<_> = vertices.iter().filter(|v| v.normal[2] > 0.5).collect();
        assert_eq!(range(&art, |v| v.uv[0]), (192.0 / 256.0, 1.0));
        assert_eq!(range(&art, |v| v.uv[1]), (64.0 / 256.0, 112.0 / 256.0));
        // The back is the strip of the sheet that is the same for every one.
        let back: Vec<_> = vertices.iter().filter(|v| v.normal[2] < -0.5).collect();
        assert_eq!(range(&back, |v| v.uv[0]), (0.75, 0.8125));
    }

    #[test]
    fn every_block_takes_the_light_of_its_own_cell() {
        let mut ent = EntityRenderSnapshot::new(EntityId(1), EntityKind::Painting, Vec3::ZERO);
        ent.object_data = art_index("Pool").unwrap() as i32;
        // Two blocks wide, one high: a torch on the left, daylight on the right.
        ent.block_lights = std::sync::Arc::from([
            umicraft_world::LightLevel { block: 14, sky: 0 },
            umicraft_world::LightLevel { block: 0, sky: 15 },
        ]);
        let mut batches = HashMap::new();
        add_painting(&mut batches, &ent, 0x100);
        let vertices = &batches[&TextureSlot::Painting];
        let words: std::collections::BTreeSet<u32> = vertices.iter().map(|v| v.light).collect();
        assert_eq!(words, [0x100 | 14, 0x100 | (15 << 4)].into());
    }
}
