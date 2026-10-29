// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mesh builders for every mob, grouped by model family.
//!
//! The families live in `animals` and `creatures`, the humanoids here. Each
//! family function returns `None` for kinds it does not own, so
//! [`build_entity_layers`] chains them. Each layer is tagged with a
//! [`TextureSlot`] so the batching loop routes vertices to the right texture,
//! and with the dye that tints it, if any.

use umicraft_game::entity::meta::Variant;
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::child::scale_mesh;
use umicraft_model::entity::{HumanoidModel, ModelVertex, TextureSlot};

use super::animals::animal_layers;
use super::creatures::{creature_layers, creeper_layers};
use super::player::posed_player;
use super::pose::humanoid_pose;

/// The animation inputs every model's `set_angles` takes.
#[derive(Clone, Copy)]
pub(super) struct Pose {
    pub(super) swing: f32,
    pub(super) amount: f32,
    /// Head yaw relative to the body, in degrees.
    pub(super) head_yaw: f32,
    pub(super) head_pitch: f32,
}

impl Pose {
    fn of(ent: &EntityRenderSnapshot) -> Self {
        Self {
            swing: ent.limb_swing,
            amount: ent.limb_swing_amount,
            head_yaw: ent.head_yaw - ent.body_yaw,
            head_pitch: ent.pitch,
        }
    }
}

/// A mesh with its texture and an optional dye tint.
pub(super) struct EntityLayer {
    /// The texture the vertices use.
    pub(super) slot: TextureSlot,
    /// Model-space vertices.
    pub(super) vertices: Vec<ModelVertex>,
    /// Dye (`0..=15`, MCP `EnumDyeColor` metadata) that multiplies the
    /// texture: the sheep wool and the wolf collar.
    pub(super) dye: Option<u8>,
}

impl EntityLayer {
    /// An untinted layer.
    pub(super) fn new(slot: TextureSlot, vertices: Vec<ModelVertex>) -> Self {
        Self {
            slot,
            vertices,
            dye: None,
        }
    }

    /// The layer tinted with the dye `color`.
    pub(super) fn dyed(mut self, color: u8) -> Self {
        self.dye = Some(color & 15);
        self
    }
}

/// A single-layer mesh on `slot`.
pub(super) fn one(slot: TextureSlot, vertices: Vec<ModelVertex>) -> Vec<EntityLayer> {
    vec![EntityLayer::new(slot, vertices)]
}

/// Builds mesh layers for a single entity snapshot (empty for kinds with no
/// entity model, such as dropped items).
pub(super) fn build_entity_layers(ent: &EntityRenderSnapshot) -> Vec<EntityLayer> {
    let pose = Pose::of(ent);
    if ent.kind == EntityKind::Creeper {
        return creeper_layers(ent, pose);
    }
    humanoid_layers(ent)
        .or_else(|| animal_layers(ent, pose))
        .or_else(|| creature_layers(ent, pose))
        .unwrap_or_default()
}

/// Two-legged, two-armed models.
fn humanoid_layers(ent: &EntityRenderSnapshot) -> Option<Vec<EntityLayer>> {
    if ent.kind == EntityKind::Player {
        let (model, is_slim) = posed_player(ent);
        let slot = if is_slim {
            TextureSlot::Alex
        } else {
            TextureSlot::Steve
        };
        return Some(one(slot, model.emit_mesh()));
    }
    let wither = matches!(ent.meta.variant, Variant::Skeleton { wither: true });
    let (mut model, slot, scale) = match ent.kind {
        EntityKind::Zombie if ent.meta.is_zombie_villager() => (
            HumanoidModel::zombie_villager(),
            TextureSlot::ZombieVillager,
            1.0,
        ),
        EntityKind::Zombie => (HumanoidModel::zombie(), TextureSlot::Zombie, 1.0),
        EntityKind::Skeleton if wither => (
            HumanoidModel::skeleton(),
            TextureSlot::SkeletonWither,
            WITHER_SKELETON_SCALE,
        ),
        EntityKind::Skeleton => (HumanoidModel::skeleton(), TextureSlot::Skeleton, 1.0),
        EntityKind::ZombiePigman => (HumanoidModel::zombie(), TextureSlot::ZombiePigman, 1.0),
        EntityKind::Giant => (HumanoidModel::zombie(), TextureSlot::Zombie, GIANT_SCALE),
        EntityKind::ZombieVillager => (
            HumanoidModel::zombie_villager(),
            TextureSlot::ZombieVillager,
            1.0,
        ),
        _ => return None,
    };
    // `ModelSkeleton` extends `ModelZombie`: all of them hold the arms forward.
    model.set_pose(&humanoid_pose(ent, true));
    let mut vertices = if ent.meta.baby {
        model.emit_child_mesh()
    } else {
        model.emit_mesh()
    };
    scale_mesh(&mut vertices, scale);
    Some(one(slot, vertices))
}

/// Scale of a wither skeleton (MCP `RenderSkeleton.preRenderCallback`).
const WITHER_SKELETON_SCALE: f32 = 1.2;
/// Scale of a giant (MCP `RenderGiantZombie`).
const GIANT_SCALE: f32 = 6.0;
