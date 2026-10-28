// SPDX-License-Identifier: MIT OR Apache-2.0

//! Item meshes placed in the world: dropped items (P3).
//!
//! Port of MCP `RenderEntityItem`: an item hovers (`sin(age / 10 + hover) *
//! 0.1 + 0.1`), spins (`age / 20 + hover` radians) and is drawn once, or as
//! two to five copies for stacks above 1, 16, 32 and 48. Lit blocks get a
//! small random offset per extra copy, flat sprites are stacked along z.
//! Items are not shaded in the world, so the mesh carries no face shade.
//!
//! The `hover` phase is random per entity in the reference; here it comes
//! from the entity id so a frame is reproducible (docs/parity.md).

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityItem, EntityKind, EntityRenderSnapshot};
use umicraft_game::hand::HandPose;
use umicraft_game::items;
use umicraft_mesh::item::{ItemMesh, ItemStyle};
use umicraft_mesh::tint::WHITE;
use umicraft_mesh::VertexLight;
use umicraft_model::display::DisplayContext;
use umicraft_model::item::pose::{ground_pose, held_pose};
use umicraft_model::item::{ItemGeometry, ItemModel};
use umicraft_model::ModelQuad;

use crate::item_icons::ItemResources;

/// Hover height offset per unit of the ground scale (`RenderEntityItem`).
const HOVER_BASE: f32 = 0.25;
/// Offset between stacked copies of a flat sprite along z.
const SPRITE_STACK_STEP: f32 = 0.046_875;
/// Largest random offset of an extra copy of a lit block.
const BLOCK_SCATTER: f32 = 0.15;
/// The fixed seed `RenderEntityItem` sets before every item.
const SCATTER_SEED: u64 = 187;

/// The item geometry of one frame's dropped items.
#[derive(Debug, Default)]
pub struct WorldItems {
    mesh: ItemMesh,
}

impl WorldItems {
    /// Rebuilds the mesh from the entity snapshots and returns it.
    ///
    /// The buffers of the previous frame are reused. Entities that are not
    /// dropped items, or whose item has no sprite model, add nothing.
    pub fn build(
        &mut self,
        snapshots: &[EntityRenderSnapshot],
        items: &ItemResources,
    ) -> &ItemMesh {
        self.mesh.clear();
        for entity in snapshots {
            if let (EntityKind::Item, Some(item)) = (entity.kind, entity.item.as_ref()) {
                push_dropped_item(&mut self.mesh, entity, item, items);
            }
        }
        &self.mesh
    }
}

fn push_dropped_item(
    mesh: &mut ItemMesh,
    entity: &EntityRenderSnapshot,
    item: &EntityItem,
    resources: &ItemResources,
) {
    let Some((model, quads)) = sprite_model(resources, item.item_id, item.damage) else {
        return;
    };
    let tints = [resources
        .tint(item.item_id, item.damage, item.nbt.as_deref())
        .unwrap_or(WHITE)];
    let style = ItemStyle {
        atlas: resources.atlas(),
        // Items are not lit by face in the world.
        light: VertexLight::new(0, 15, 15, false),
        tints: &tints,
    };
    for pose in copy_poses(entity, item.count, model) {
        mesh.push_quads(quads, &pose, &style);
    }
}

/// Appends the first-person held item to `mesh`, in camera space.
///
/// `bob` is the view-bobbing transform of the frame, applied to the hand
/// as well (MCP `EntityRenderer.renderHand`). Items without a sprite model
/// (a chest, a banner) are not drawn yet.
pub fn push_held_item(
    mesh: &mut ItemMesh,
    (item_id, damage, nbt): (i16, i16, Option<&[u8]>),
    hand: &HandPose,
    bob: Mat4,
    resources: &ItemResources,
) {
    let Some((model, quads)) = sprite_model(resources, item_id, damage) else {
        return;
    };
    let tints = [resources.tint(item_id, damage, nbt).unwrap_or(WHITE)];
    let style = ItemStyle {
        atlas: resources.atlas(),
        // The hand is lit at full sky light; lit blocks keep their face shade.
        light: VertexLight::new(0, 15, 15, model.gui_3d),
        tints: &tints,
    };
    let pose = bob * hand.item_matrix() * held_pose(model, DisplayContext::FirstPerson);
    mesh.push_quads(quads, &pose, &style);
}

/// The model and quads of an item that is drawn from a sprite or blocks.
fn sprite_model(
    resources: &ItemResources,
    item_id: i16,
    damage: i16,
) -> Option<(&ItemModel, &[ModelQuad])> {
    let damageable = items::lookup(i32::from(item_id)).is_some_and(items::ItemDef::is_damageable);
    let model = resources
        .models()
        .model(item_id as u16, damage as u16, damageable)?;
    match &model.geometry {
        ItemGeometry::Quads(quads) => Some((model, quads)),
        ItemGeometry::Entity => None,
    }
}

/// The world matrix of every copy of a stack.
fn copy_poses(entity: &EntityRenderSnapshot, count: u8, model: &ItemModel) -> Vec<Mat4> {
    let hover = hover_phase(entity.id.0);
    let ground = model.transform(DisplayContext::Ground);
    let bob = ((entity.age / 10.0 + hover).sin() * 0.1) + 0.1;
    let spin = (entity.age / 20.0 + hover).to_degrees();
    let copies = copy_count(count);
    let mut frame = Mat4::from_translation(
        entity.position + Vec3::new(0.0, bob + HOVER_BASE * ground.scale[1], 0.0),
    ) * Mat4::from_rotation_y(spin.to_radians());
    if !model.gui_3d {
        let back = -SPRITE_STACK_STEP * (copies - 1) as f32 * 0.5;
        frame *= Mat4::from_translation(Vec3::new(0.0, 0.0, back));
    }
    let base = ground_pose(model);
    let mut random = JavaRandom::new(SCATTER_SEED);
    let mut poses = Vec::with_capacity(copies);
    for copy in 0..copies {
        if model.gui_3d {
            let scatter = if copy > 0 {
                Vec3::new(
                    random.next_signed_float(),
                    random.next_signed_float(),
                    random.next_signed_float(),
                ) * BLOCK_SCATTER
            } else {
                Vec3::ZERO
            };
            poses.push(frame * Mat4::from_translation(scatter) * base);
        } else {
            poses.push(frame * base);
            frame *=
                Mat4::from_translation(Vec3::new(0.0, 0.0, SPRITE_STACK_STEP * ground.scale[2]));
        }
    }
    poses
}

/// How many copies a stack is drawn as (`RenderEntityItem.func_177078_a`).
fn copy_count(count: u8) -> usize {
    match count {
        49.. => 5,
        33..=48 => 4,
        17..=32 => 3,
        2..=16 => 2,
        _ => 1,
    }
}

/// A stable pseudo-random phase in `0..2π` for the hover and spin of an
/// entity (the reference draws `rand * π * 2` once per entity).
fn hover_phase(entity_id: i32) -> f32 {
    let hash = (entity_id as u32).wrapping_mul(0x9E37_79B9);
    (hash >> 8) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU
}

/// The 48-bit linear congruential generator of `java.util.Random`, enough
/// for the seeded scatter of stacked blocks.
struct JavaRandom(u64);

impl JavaRandom {
    const MULTIPLIER: u64 = 0x5_DEEC_E66D;
    const MASK: u64 = (1 << 48) - 1;

    fn new(seed: u64) -> Self {
        Self((seed ^ Self::MULTIPLIER) & Self::MASK)
    }

    /// `Random.nextFloat`: 24 random bits as a fraction.
    fn next_float(&mut self) -> f32 {
        self.0 = (self.0.wrapping_mul(Self::MULTIPLIER).wrapping_add(0xB)) & Self::MASK;
        (self.0 >> 24) as f32 / (1u32 << 24) as f32
    }

    /// `nextFloat() * 2 - 1`, the `-1..1` draw of the scatter.
    fn next_signed_float(&mut self) -> f32 {
        self.next_float() * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacks_grow_the_copy_count_at_the_reference_thresholds() {
        let counts: Vec<usize> = [1, 2, 16, 17, 32, 33, 48, 49, 64]
            .into_iter()
            .map(copy_count)
            .collect();
        assert_eq!(counts, [1, 2, 2, 3, 3, 4, 4, 5, 5]);
    }

    #[test]
    fn the_java_generator_follows_the_documented_recurrence() {
        // The values of `new Random(187)` from the recurrence of the
        // `java.util.Random` specification (seed ^ 0x5DEECE66D, then
        // seed * 0x5DEECE66D + 0xB, top 24 bits).
        let mut random = JavaRandom::new(187);
        let values = [
            random.next_float(),
            random.next_float(),
            random.next_float(),
        ];
        let expected = [0.740_373_8_f32, 0.583_058_1, 0.438_374_76];
        for (value, expected) in values.into_iter().zip(expected) {
            assert!((value - expected).abs() < 1e-6, "{value} vs {expected}");
        }
    }

    #[test]
    fn hover_phases_are_stable_and_inside_a_turn() {
        for id in [-5, 0, 1, 77, i32::MAX] {
            let phase = hover_phase(id);
            assert!((0.0..std::f32::consts::TAU).contains(&phase), "{phase}");
            assert_eq!(phase, hover_phase(id));
        }
        assert_ne!(hover_phase(1), hover_phase(2));
    }
}
