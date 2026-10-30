// SPDX-License-Identifier: MIT OR Apache-2.0

//! Lightning (MCP `RenderLightningBolt`).
//!
//! A bolt is a zigzag of eight 16 block segments seeded by `boltVertex`, drawn
//! four times with growing width and with two thinner forks, as pale blue
//! triangle strips added to the frame. The reference re-seeds it at each flash;
//! the seed here is the one of the entity plus the index of the flash.

use std::collections::HashMap;

use umicraft_game::entity::store::LIGHTNING_FLASH_TICKS;
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::TextureSlot;
use umicraft_render::EntityVertex;

use crate::java_random::JavaRandom;

/// The tint of the strips as an additive light word. The reference adds
/// `(0.45, 0.45, 0.5) * 0.3` in display space; the target blends in linear
/// space, where that amount is the tint raised to 2.2 (see `fs_additive`).
const TINT: u32 = 103 | (103 << 8) | (108 << 16);

/// `random.nextInt(bound) - offset` of the reference.
fn step(random: &mut JavaRandom, bound: u32, offset: i32) -> f64 {
    f64::from(random.next_int(bound) as i32 - offset)
}

/// Adds the lightning bolt `ent` to `additive` when it is one.
pub(super) fn add_lightning(
    additive: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    ent: &EntityRenderSnapshot,
) {
    if ent.kind != EntityKind::Lightning {
        return;
    }
    let seed = u64::from(ent.object_data as u32)
        .wrapping_add((ent.age / LIGHTNING_FLASH_TICKS as f32) as u64);
    let (mut sum_x, mut sum_z) = (0.0_f64, 0.0_f64);
    let (mut spine_x, mut spine_z) = ([0.0_f64; 8], [0.0_f64; 8]);
    let mut random = JavaRandom::new(seed);
    for i in (0..8).rev() {
        spine_x[i] = sum_x;
        spine_z[i] = sum_z;
        sum_x += step(&mut random, 11, 5);
        sum_z += step(&mut random, 11, 5);
    }
    let out = additive.entry(TextureSlot::Lightning).or_default();
    let origin = ent.position.as_dvec3();
    for k1 in 0..4 {
        let mut random = JavaRandom::new(seed);
        for j in 0..3_i32 {
            let top: i32 = if j > 0 { 7 - j } else { 7 };
            let bottom: i32 = if j > 0 { top - 2 } else { 0 };
            let mut d2 = spine_x[top as usize] - sum_x;
            let mut d3 = spine_z[top as usize] - sum_z;
            for i1 in (bottom..=top).rev() {
                let (d4, d5) = (d2, d3);
                if j == 0 {
                    d2 += step(&mut random, 11, 5);
                    d3 += step(&mut random, 11, 5);
                } else {
                    d2 += step(&mut random, 31, 15);
                    d3 += step(&mut random, 31, 15);
                }
                let mut upper = 0.1 + f64::from(k1) * 0.2;
                let mut lower = upper;
                if j == 0 {
                    upper *= f64::from(i1) * 0.1 + 1.0;
                    lower *= f64::from(i1 - 1) * 0.1 + 1.0;
                }
                let mut strip = Vec::with_capacity(10);
                for j1 in 0..5 {
                    let corner = |half: f64| {
                        let mut x = origin.x + 0.5 - half;
                        let mut z = origin.z + 0.5 - half;
                        if j1 == 1 || j1 == 2 {
                            x += half * 2.0;
                        }
                        if j1 == 2 || j1 == 3 {
                            z += half * 2.0;
                        }
                        (x, z)
                    };
                    let (x10, z11) = corner(lower);
                    let (x8, z9) = corner(upper);
                    strip.push([
                        (x10 + d2) as f32,
                        (origin.y + f64::from(i1 * 16)) as f32,
                        (z11 + d3) as f32,
                    ]);
                    strip.push([
                        (x8 + d4) as f32,
                        (origin.y + f64::from((i1 + 1) * 16)) as f32,
                        (z9 + d5) as f32,
                    ]);
                }
                let vertex = |position: [f32; 3]| EntityVertex {
                    position,
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.5, 0.5],
                    light: TINT,
                };
                // A triangle strip as a list of triangles.
                for t in 0..strip.len() - 2 {
                    out.extend([vertex(strip[t]), vertex(strip[t + 1]), vertex(strip[t + 2])]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use umicraft_game::entity::EntityId;

    fn bolt(object_data: i32, age: f32) -> Vec<EntityVertex> {
        let mut ent = EntityRenderSnapshot::new(
            EntityId(1),
            EntityKind::Lightning,
            Vec3::new(0.0, 64.0, 0.0),
        );
        ent.object_data = object_data;
        ent.age = age;
        let mut batches = HashMap::new();
        add_lightning(&mut batches, &ent);
        batches.remove(&TextureSlot::Lightning).unwrap_or_default()
    }

    #[test]
    fn a_bolt_is_fifty_six_strips_of_eight_triangles() {
        // Per width: eight segments of the bolt and two forks of three.
        assert_eq!(bolt(7, 0.0).len(), 4 * (8 + 3 + 3) * 8 * 3);
    }

    #[test]
    fn a_bolt_reaches_128_blocks_up_from_its_position() {
        let bolt = bolt(7, 0.0);
        let low = bolt.iter().map(|v| v.position[1]).fold(f32::MAX, f32::min);
        let high = bolt.iter().map(|v| v.position[1]).fold(f32::MIN, f32::max);
        assert_eq!((low, high), (64.0, 64.0 + 128.0));
    }

    #[test]
    fn the_shape_follows_the_seed_and_the_flash() {
        let key = |vertices: Vec<EntityVertex>| -> Vec<[f32; 3]> {
            vertices.iter().map(|v| v.position).collect()
        };
        assert_eq!(key(bolt(7, 0.0)), key(bolt(7, 2.0)), "the same flash");
        assert_ne!(key(bolt(7, 0.0)), key(bolt(8, 0.0)), "another seed");
        assert_ne!(key(bolt(7, 0.0)), key(bolt(7, 3.0)), "the next flash");
    }

    #[test]
    fn only_bolts_are_drawn_as_bolts() {
        let ent = EntityRenderSnapshot::new(EntityId(1), EntityKind::Pig, Vec3::ZERO);
        let mut batches = HashMap::new();
        add_lightning(&mut batches, &ent);
        assert!(batches.is_empty());
    }
}
