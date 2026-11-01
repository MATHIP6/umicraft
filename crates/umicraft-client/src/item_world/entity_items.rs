// SPDX-License-Identifier: MIT OR Apache-2.0

//! Skulls and banners lying in the world as items (MCP `RenderEntityItem`,
//! `RenderItemFrame`).
//!
//! These items are `builtin/entity`: they have no quads in the item layers.
//! Their tile-entity model goes to the entity pass instead, placed by the same
//! matrices as a sprite item, so a dropped skull hovers and spins and a framed
//! banner turns with the frame. The same goes for the item in a hand, seen from
//! the first person or from outside.

use glam::Mat4;
use umicraft_game::entity::{EntityItem, EntityKind, EntityRenderSnapshot};
use umicraft_game::hand::HandPose;
use umicraft_model::display::DisplayContext;
use umicraft_model::entity::TextureSlot;
use umicraft_model::item::pose::held_pose;
use umicraft_render::EntityVertex;
use umicraft_world::LightLevel;

use super::held::hand_frame;
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
        if let Some(held) = entity.held.as_ref().filter(|_| !entity.fishing) {
            push_held(meshes, entity, held, resources, skins);
        }
    }
}

/// The first-person held skull or banner in camera space, with its texture.
///
/// `(bob, light)` are the view-bobbing transform and the light at the eye, as
/// for [`super::push_held_item`]. The banner design is recorded in `meshes`,
/// so the texture exists when the hand is uploaded after the entities.
pub(crate) fn hand_item(
    meshes: &mut BlockEntityMeshes,
    (item_id, damage, nbt): (i16, i16, Option<&[u8]>),
    hand: &HandPose,
    (bob, light): (Mat4, LightLevel),
    resources: &ItemResources,
    skins: &dyn SkinLookup,
) -> Option<(TextureSlot, Vec<EntityVertex>)> {
    let model = entity_model(resources, item_id, damage)?;
    let mesh = entity_item_mesh(item_id, damage, nbt, skins)?;
    mesh.record_design(meshes);
    let pose = bob * hand.item_matrix() * held_pose(model, DisplayContext::FirstPerson);
    Some((mesh.slot, mesh.placed(&pose, light)))
}

/// The skull or banner in the hand of a player or mob (`LayerHeldItem`).
fn push_held(
    meshes: &mut BlockEntityMeshes,
    entity: &EntityRenderSnapshot,
    held: &EntityItem,
    resources: &ItemResources,
    skins: &dyn SkinLookup,
) {
    let Some(model) = entity_model(resources, held.item_id, held.damage) else {
        return;
    };
    let Some(mesh) = entity_item_mesh(held.item_id, held.damage, held.nbt.as_deref(), skins) else {
        return;
    };
    let Some(frame) = hand_frame(entity, held) else {
        return;
    };
    let pose = frame * held_pose(model, DisplayContext::ThirdPerson);
    mesh.push_into(meshes, &pose, entity.light);
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
