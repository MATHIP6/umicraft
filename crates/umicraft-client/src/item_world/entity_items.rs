// SPDX-License-Identifier: MIT OR Apache-2.0

//! Skulls and banners lying in the world as items (MCP `RenderEntityItem`,
//! `RenderItemFrame`).
//!
//! These items are `builtin/entity`: they have no quads in the item layers.
//! Their tile-entity model goes to the entity pass instead, placed by the same
//! matrices as a sprite item, so a dropped skull hovers and spins and a framed
//! banner turns with the frame.

use umicraft_game::entity::{EntityItem, EntityKind, EntityRenderSnapshot};

use super::{copy_poses, entity_model, framed_pose, FILLED_MAP};
use crate::block_entity_render::{entity_item_mesh, BlockEntityMeshes};
use crate::entity_render::SkinLookup;
use crate::item_icons::ItemResources;

/// Adds the skulls and banners that dropped items and item frames show.
pub(crate) fn push(
    meshes: &mut BlockEntityMeshes,
    snapshots: &[EntityRenderSnapshot],
    resources: &ItemResources,
    skins: &dyn SkinLookup,
) {
    for entity in snapshots {
        match (entity.kind, entity.item.as_ref()) {
            (EntityKind::Item, Some(item)) => {
                push_dropped(meshes, entity, item, resources, skins);
            }
            (EntityKind::ItemFrame, Some(item)) if item.item_id != FILLED_MAP => {
                push_framed(meshes, entity, item, resources, skins);
            }
            _ => {}
        }
    }
}

fn push_dropped(
    meshes: &mut BlockEntityMeshes,
    entity: &EntityRenderSnapshot,
    item: &EntityItem,
    resources: &ItemResources,
    skins: &dyn SkinLookup,
) {
    let Some(model) = entity_model(resources, item.item_id, item.damage) else {
        return;
    };
    let Some(mesh) = entity_item_mesh(item.item_id, item.damage, item.nbt.as_deref(), skins) else {
        return;
    };
    for pose in copy_poses(entity, item.count, model) {
        mesh.push_into(meshes, &pose, entity.light);
    }
}

fn push_framed(
    meshes: &mut BlockEntityMeshes,
    entity: &EntityRenderSnapshot,
    item: &EntityItem,
    resources: &ItemResources,
    skins: &dyn SkinLookup,
) {
    let Some(model) = entity_model(resources, item.item_id, item.damage) else {
        return;
    };
    let Some(mesh) = entity_item_mesh(item.item_id, item.damage, item.nbt.as_deref(), skins) else {
        return;
    };
    mesh.push_into(meshes, &framed_pose(entity, model), entity.light);
}
