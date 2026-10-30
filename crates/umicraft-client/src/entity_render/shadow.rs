// SPDX-License-Identifier: MIT OR Apache-2.0

//! The blob shadow under an entity (MCP `Render.renderShadow`,
//! `func_180549_a`).
//!
//! The game thread finds the blocks the shadow falls on; here each of them
//! gets a square on its top, textured with the shadow so that the blob is
//! centred on the entity, and an opacity that falls with the height of the
//! entity above the block, with the light of the cell and with the distance
//! to the camera.

use std::collections::HashMap;

use glam::Vec3;
use umicraft_game::entity::shadow::{shadow_opacity, shadow_size};
use umicraft_game::entity::EntityRenderSnapshot;
use umicraft_model::entity::TextureSlot;
use umicraft_render::EntityVertex;

/// How far above the top of a block the shadow lies, to stay clear of it
/// (`0.015625` in the reference).
const LIFT: f32 = 0.015_625;

/// The opacity of the shadow at the camera distance `distance_squared`
/// (`Render.doRenderShadowAndFire`: `(1 - d / 256) * shadowOpaque`, `d` being
/// the squared distance), or `None` when it has faded out.
fn base_opacity(ent: &EntityRenderSnapshot, camera: Vec3) -> Option<f32> {
    let distance_squared = (ent.position - camera).length_squared();
    let opacity = (1.0 - distance_squared / 256.0) * shadow_opacity(ent.kind);
    (opacity > 0.0).then_some(opacity)
}

/// Adds the shadow of `ent` to `batches` when it has one: it has a size, is not
/// invisible and the camera is near enough to see it.
pub(super) fn add_shadow(
    batches: &mut HashMap<TextureSlot, Vec<EntityVertex>>,
    ent: &EntityRenderSnapshot,
    camera: Vec3,
) {
    if ent.shadow.is_empty() || ent.flags.is_invisible() {
        return;
    }
    let size = shadow_size(ent.kind, ent.meta.baby, &ent.meta.variant);
    let Some(base) = base_opacity(ent, camera) else {
        return;
    };
    if size <= 0.0 {
        return;
    }
    let batch = batches.entry(TextureSlot::Shadow).or_default();
    for block in ent.shadow.iter() {
        // `(alpha - (y - top) / 2) * 0.5 * brightness`, kept to `0..=1`.
        let opacity =
            ((base - (ent.position.y - block.top) / 2.0) * 0.5 * block.brightness).min(1.0);
        if opacity < 0.0 {
            continue;
        }
        let height = block.top + LIFT;
        // Texture coordinates centre the blob on the entity.
        let u = |x: f32| (ent.position.x - x) / 2.0 / size + 0.5;
        let v = |z: f32| (ent.position.z - z) / 2.0 / size + 0.5;
        let light = (opacity * 255.0).round() as u32;
        let corner = |x: f32, z: f32| EntityVertex {
            position: [x, height, z],
            normal: [0.0, 1.0, 0.0],
            uv: [u(x), v(z)],
            light,
        };
        let quad = [
            corner(block.x[0], block.z[0]),
            corner(block.x[0], block.z[1]),
            corner(block.x[1], block.z[1]),
            corner(block.x[1], block.z[0]),
        ];
        for index in [0, 1, 2, 0, 2, 3] {
            batch.push(quad[index]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use umicraft_game::entity::shadow::ShadowBlock;
    use umicraft_game::entity::{EntityFlags, EntityId, EntityKind};

    fn pig_over(blocks: &[ShadowBlock]) -> EntityRenderSnapshot {
        let mut pig =
            EntityRenderSnapshot::new(EntityId(1), EntityKind::Pig, Vec3::new(0.5, 64.0, 0.5));
        pig.shadow = Arc::from(blocks);
        pig
    }

    fn block(x: f32, z: f32, top: f32) -> ShadowBlock {
        ShadowBlock {
            x: [x, x + 1.0],
            z: [z, z + 1.0],
            top,
            brightness: 1.0,
        }
    }

    fn shadow_of(ent: &EntityRenderSnapshot, camera: Vec3) -> Vec<EntityVertex> {
        let mut batches = HashMap::new();
        add_shadow(&mut batches, ent, camera);
        batches.remove(&TextureSlot::Shadow).unwrap_or_default()
    }

    #[test]
    fn each_block_gets_a_square_lifted_above_its_top() {
        let pig = pig_over(&[block(0.0, 0.0, 64.0), block(1.0, 0.0, 64.0)]);
        let vertices = shadow_of(&pig, Vec3::new(0.5, 65.0, 4.0));
        assert_eq!(vertices.len(), 2 * 6);
        assert!(vertices
            .iter()
            .all(|v| (v.position[1] - 64.015_625).abs() < 1e-6));
    }

    #[test]
    fn the_blob_is_centred_on_the_entity() {
        // A block centred under the pig: the texture spans 0..1 over a block
        // when the shadow size is 0.7 and the block 1.4 wide would; here the
        // block corners are 0.5 away, so the centre of the block is `0.5`.
        let pig = pig_over(&[block(0.0, 0.0, 64.0)]);
        let vertices = shadow_of(&pig, Vec3::new(0.5, 65.0, 4.0));
        let (u_min, u_max) = vertices
            .iter()
            .map(|v| v.uv[0])
            .fold((f32::MAX, f32::MIN), |(a, b), u| (a.min(u), b.max(u)));
        assert!((u_min + u_max - 1.0).abs() < 1e-5, "{u_min} {u_max}");
    }

    #[test]
    fn the_shadow_fades_with_height_and_with_the_distance_to_the_camera() {
        let low = pig_over(&[block(0.0, 0.0, 63.5)]);
        let near = Vec3::new(0.5, 65.0, 3.0);
        let strong = shadow_of(&low, near)[0].light;
        // Higher above the ground: lighter, and gone when it is too high.
        let mut high = pig_over(&[block(0.0, 0.0, 63.0)]);
        high.position.y = 64.0;
        assert!(shadow_of(&high, near)[0].light < strong);
        let mut floating = pig_over(&[block(0.0, 0.0, 61.0)]);
        floating.position.y = 64.0;
        assert!(shadow_of(&floating, near).is_empty());
        // Further from the camera: lighter, and gone at 16 blocks.
        let far = Vec3::new(0.5, 65.0, 12.0);
        assert!(shadow_of(&low, far)[0].light < strong);
        assert!(shadow_of(&low, Vec3::new(0.5, 65.0, 17.0)).is_empty());
    }

    #[test]
    fn invisible_entities_and_those_without_blocks_cast_nothing() {
        let mut pig = pig_over(&[block(0.0, 0.0, 64.0)]);
        pig.flags = EntityFlags::new(0x20, 0x7F);
        assert!(shadow_of(&pig, Vec3::new(0.5, 65.0, 3.0)).is_empty());
        let bare = pig_over(&[]);
        assert!(shadow_of(&bare, Vec3::new(0.5, 65.0, 3.0)).is_empty());
    }
}
