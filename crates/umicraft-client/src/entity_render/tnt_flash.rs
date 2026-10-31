// SPDX-License-Identifier: MIT OR Apache-2.0

//! The white flash of a primed TNT (MCP `RenderTntPrimed.doRender`).
//!
//! Every second 5 ticks of its fuse the block is drawn a second time in white,
//! added over itself, a little more strongly as the fuse runs out: `0.15` at the
//! start to `0.8` at the end. The block itself is the item pass's; this is
//! only the white cube over it.

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::TextureSlot;
use umicraft_render::EntityVertex;

use crate::item_world::objects::tnt_matrix;

/// How much bigger than the block the cube is, so that it is not the same
/// surface (the reference uses a polygon offset).
const GROWTH: f32 = 1.001;
/// The fuse of a primed TNT when it is created, in ticks.
const FUSE: f32 = 80.0;

/// How strongly the flash is added at `age` ticks, or `None` between flashes
/// (`entity.fuse / 5 % 2 == 0`, `(1 - (fuse - partial + 1) / 100) * 0.8`).
fn strength(age: f32) -> Option<f32> {
    let fuse = (FUSE - age.floor()) as i32;
    if fuse < 0 || (fuse / 5) % 2 != 0 {
        return None;
    }
    let remaining = FUSE - age + 1.0;
    Some(((1.0 - remaining / 100.0) * 0.8).clamp(0.0, 1.0))
}

/// Adds the flash of the primed TNT `ent` to `additive` when it is on.
pub(super) fn add_flash(
    additive: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    ent: &EntityRenderSnapshot,
) {
    if ent.kind != EntityKind::PrimedTnt {
        return;
    }
    let Some(strength) = strength(ent.age) else {
        return;
    };
    // The light word of an additive vertex is the tint, which the shader raises
    // to 2.2: this adds `strength` in linear space.
    let channel = (strength.powf(1.0 / 2.2) * 255.0).round() as u32;
    let light = channel | (channel << 8) | (channel << 16);
    let cube = tnt_matrix(ent)
        * Mat4::from_translation(Vec3::splat(0.5))
        * Mat4::from_scale(Vec3::splat(GROWTH))
        * Mat4::from_translation(Vec3::splat(-0.5));
    let out = additive.entry(TextureSlot::Lightning).or_default();
    // The six faces of the unit cube as pairs of triangles.
    for (axis, side) in [(0, 0.0), (0, 1.0), (1, 0.0), (1, 1.0), (2, 0.0), (2, 1.0)] {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        let corner = |a: f32, b: f32| {
            let mut local = [0.0_f32; 3];
            local[axis] = side;
            local[u] = a;
            local[v] = b;
            EntityVertex {
                position: cube.transform_point3(Vec3::from(local)).to_array(),
                normal: [0.0, 1.0, 0.0],
                uv: [0.5, 0.5],
                light,
            }
        };
        let quad = [
            corner(0.0, 0.0),
            corner(1.0, 0.0),
            corner(1.0, 1.0),
            corner(0.0, 1.0),
        ];
        out.extend([quad[0], quad[1], quad[2], quad[0], quad[2], quad[3]]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::EntityId;

    fn tnt(age: f32) -> EntityRenderSnapshot {
        let mut ent = EntityRenderSnapshot::new(
            EntityId(1),
            EntityKind::PrimedTnt,
            Vec3::new(4.0, 70.0, 4.0),
        );
        ent.age = age;
        ent
    }

    fn flash(ent: &EntityRenderSnapshot) -> Vec<EntityVertex> {
        let mut batches = HashMap::new();
        add_flash(&mut batches, ent);
        batches.remove(&TextureSlot::Lightning).unwrap_or_default()
    }

    #[test]
    fn it_flashes_for_five_ticks_out_of_ten() {
        // `fuse / 5` is even: fuse 80, then 74 to 70, 64 to 60, ...
        let on: Vec<u32> = (0..=20u32)
            .filter(|age| strength(*age as f32).is_some())
            .collect();
        assert_eq!(on, [0, 6, 7, 8, 9, 10, 16, 17, 18, 19, 20]);
    }

    #[test]
    fn it_grows_stronger_as_the_fuse_runs_out() {
        let early = strength(0.0).unwrap();
        let late = strength(78.0).unwrap();
        assert!((early - 0.152).abs() < 1e-3, "{early}");
        assert!(late > 0.75 && late <= 0.8, "{late}");
        assert!(strength(81.0).is_none(), "the fuse is spent");
    }

    #[test]
    fn the_flash_is_a_cube_over_the_block() {
        let cube = flash(&tnt(8.0));
        assert_eq!(cube.len(), 36);
        let centre = cube
            .iter()
            .fold(Vec3::ZERO, |sum, v| sum + Vec3::from(v.position))
            / 36.0;
        // The block is centred half a block over the entity.
        assert!(
            centre.distance(Vec3::new(4.0, 70.5, 4.0)) < 1e-3,
            "{centre}"
        );
        assert!(flash(&tnt(2.0)).is_empty(), "between two flashes");
    }

    #[test]
    fn only_a_primed_tnt_flashes() {
        let mut other = tnt(8.0);
        other.kind = EntityKind::Pig;
        assert!(flash(&other).is_empty());
    }
}
