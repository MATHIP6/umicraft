// SPDX-License-Identifier: MIT OR Apache-2.0

//! The block an enderman carries (MCP `LayerHeldBlock`).
//!
//! The block is placed in the vanilla model frame with the transforms of the
//! layer and then turned a quarter by `BlockModelRenderer.renderModelBrightness`,
//! like the mushrooms of a mooshroom.

use glam::{Mat4, Vec3};
use umicraft_game::entity::meta::Variant;
use umicraft_game::entity::{EntityItem, EntityKind, EntityRenderSnapshot};
use umicraft_mesh::item::ItemStyle;

use super::{sprite_model, world_light, ItemLayers};
use crate::entity_render::{body_matrix, x_mirror};
use crate::item_icons::ItemResources;

/// Height of the model origin above the feet, in blocks.
const ORIGIN_HEIGHT: f32 = 1.5;

/// Appends the block carried by `entity`, if it is an enderman with one.
pub(super) fn push(
    layers: &mut ItemLayers,
    entity: &EntityRenderSnapshot,
    resources: &ItemResources,
) {
    let Some((item, frame)) = carried_block(entity) else {
        return;
    };
    let Some((_, quads)) = sprite_model(resources, item.item_id, item.damage) else {
        return;
    };
    let tints = resources.tints(item.item_id, item.damage, None);
    let style = ItemStyle {
        atlas: resources.atlas(),
        light: world_light(entity.light, true),
        tints: &tints,
    };
    layers.push(quads, &frame, &style, false);
}

/// The block (as an item) an enderman carries, with the frame that places the
/// block quads (in `0..1` block space) on it.
fn carried_block(entity: &EntityRenderSnapshot) -> Option<(EntityItem, Mat4)> {
    let Variant::Enderman(meta) = &entity.meta.variant else {
        return None;
    };
    if entity.kind != EntityKind::Enderman || meta.carried & 0x0FFF == 0 {
        return None;
    }
    let item = EntityItem {
        item_id: (meta.carried & 0x0FFF) as i16,
        damage: (meta.carried >> 12) as i16,
        count: 1,
        nbt: None,
    };
    // From the vanilla model space (y down, origin 1.5 above the feet) to this
    // crate's (y up).
    let to_model = Mat4::from_translation(Vec3::new(0.0, ORIGIN_HEIGHT, 0.0))
        * Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0));
    let layer = Mat4::from_translation(Vec3::new(0.0, 0.6875, -0.75))
        * Mat4::from_rotation_x(20.0_f32.to_radians())
        * Mat4::from_rotation_y(45.0_f32.to_radians())
        * Mat4::from_translation(Vec3::new(0.25, 0.1875, 0.25))
        * Mat4::from_scale(Vec3::new(-0.5, -0.5, 0.5))
        // `renderModelBrightness`.
        * Mat4::from_rotation_y(90.0_f32.to_radians());
    Some((item, body_matrix(entity) * x_mirror() * to_model * layer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::meta::EndermanMeta;
    use umicraft_game::entity::EntityId;

    fn enderman(carried: u16) -> EntityRenderSnapshot {
        let mut ent =
            EntityRenderSnapshot::new(EntityId(1), EntityKind::Enderman, Vec3::new(0.0, 64.0, 0.0));
        ent.meta.variant = Variant::Enderman(EndermanMeta {
            carried,
            screaming: false,
        });
        ent
    }

    #[test]
    fn an_enderman_carries_the_block_of_its_state() {
        assert!(carried_block(&enderman(0)).is_none());
        let (item, _) = carried_block(&enderman(2 | (1 << 12))).expect("grass");
        assert_eq!((item.item_id, item.damage), (2, 1));
    }

    #[test]
    fn only_an_enderman_carries_a_block() {
        let mut cow = enderman(2);
        cow.kind = EntityKind::Cow;
        assert!(carried_block(&cow).is_none());
    }

    #[test]
    fn the_block_is_held_in_front_at_about_waist_height() {
        let (_, frame) = carried_block(&enderman(2)).expect("dirt");
        let centre = frame.transform_point3(Vec3::splat(0.5));
        let height = centre.y - 64.0;
        // The vanilla layer puts it about 1.5 - 0.69 above the feet.
        assert!(height > 0.5 && height < 1.6, "{height}");
        assert!(centre.x.abs() < 1.0 && centre.z.abs() < 1.5);
    }
}
