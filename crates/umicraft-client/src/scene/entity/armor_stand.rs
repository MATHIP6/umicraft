// SPDX-License-Identifier: MIT OR Apache-2.0

//! The armour stand (MCP `ArmorStandRenderer`): the stand model on
//! `armorstand/wood.png` and its armour, posed by the metadata angles.
//!
//! The held item and the block or skull on the head hang on the biped of the
//! stand through [`posed_stand_biped`], like those of the other bipeds
//! (`LayerHeldItem`, `LayerCustomHead`). The stand has no shadow
//! (`shadowSize` 0) and its body always faces its yaw (`func_110146_f`).

use umicraft_game::entity::meta::{ArmorStandMeta, Variant};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::{
    ArmorStandModel, ArmorStandPose, HumanoidModel, TextureSlot, ARMOR_LEG_PIVOT_Y,
};

use super::armor::posed_armor_layers;
use super::mobs::{one, EntityLayer};

/// The layers of an armour stand, or `None` for any other kind: the stand,
/// then its armour.
pub(super) fn armor_stand_layers(ent: &EntityRenderSnapshot) -> Option<Vec<EntityLayer>> {
    let pose = stand_pose(ent)?;
    let mut model = ArmorStandModel::new();
    model.set_pose(&pose);
    let mesh = if ent.meta.baby {
        model.emit_child_mesh()
    } else {
        model.emit_mesh()
    };
    let mut layers = one(TextureSlot::ArmorStand, mesh);
    layers.extend(posed_armor_layers(ent, 1.0, |shell| {
        pose.pose_biped(shell, ARMOR_LEG_PIVOT_Y);
    }));
    Some(layers)
}

/// The biped of a posed armour stand, for the layers that hang on its arm or
/// its head; `None` for any other kind.
pub(crate) fn posed_stand_biped(ent: &EntityRenderSnapshot) -> Option<HumanoidModel> {
    let mut model = ArmorStandModel::new();
    model.set_pose(&stand_pose(ent)?);
    Some(model.biped)
}

/// The pose of an armour stand from its metadata, or the pose of a new stand
/// before any metadata arrived.
fn stand_pose(ent: &EntityRenderSnapshot) -> Option<ArmorStandPose> {
    if ent.kind != EntityKind::ArmorStand {
        return None;
    }
    let meta = match ent.meta.variant {
        Variant::ArmorStand(stand) => stand,
        _ => ArmorStandMeta::default(),
    };
    Some(ArmorStandPose {
        head: meta.head,
        body: meta.body,
        left_arm: meta.left_arm,
        right_arm: meta.right_arm,
        left_leg: meta.left_leg,
        right_leg: meta.right_leg,
        yaw: ent.yaw,
        arms: meta.has_arms(),
        base_plate: !meta.no_base_plate(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::{EntityId, EntityItem};

    fn stand(flags: u8) -> EntityRenderSnapshot {
        let mut ent =
            EntityRenderSnapshot::new(EntityId(7), EntityKind::ArmorStand, glam::Vec3::ZERO);
        ent.meta.variant = Variant::ArmorStand(ArmorStandMeta {
            flags,
            ..ArmorStandMeta::default()
        });
        ent
    }

    #[test]
    fn other_kinds_are_not_stands() {
        let zombie = EntityRenderSnapshot::new(EntityId(1), EntityKind::Zombie, glam::Vec3::ZERO);
        assert!(armor_stand_layers(&zombie).is_none());
        assert!(posed_stand_biped(&zombie).is_none());
    }

    #[test]
    fn a_bare_stand_is_one_wooden_layer() {
        let layers = armor_stand_layers(&stand(0)).expect("a stand");
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].slot, TextureSlot::ArmorStand);
    }

    #[test]
    fn the_arms_flag_adds_the_arm_sticks() {
        let count = |flags| {
            armor_stand_layers(&stand(flags)).expect("a stand")[0]
                .vertices
                .len()
        };
        assert_eq!(count(4), count(0) + 2 * 36);
    }

    #[test]
    fn worn_armour_follows_the_stand() {
        let mut ent = stand(0);
        ent.armor[3] = Some(EntityItem {
            item_id: 310,
            damage: 0,
            count: 1,
            nbt: None,
        });
        let layers = armor_stand_layers(&ent).expect("a stand");
        assert_eq!(layers.len(), 2);
        assert!(layers[1].slot.is_armor());
    }

    #[test]
    fn a_stand_without_metadata_takes_the_default_arm_pose() {
        let mut ent = stand(0);
        ent.meta.variant = Variant::None;
        let biped = posed_stand_biped(&ent).expect("a stand");
        let expected = [15f32.to_radians(), 0.0, -10f32.to_radians()];
        for (got, want) in biped.right_arm.rotation.iter().zip(expected) {
            assert!((got - want).abs() < 1e-6, "{got} {want}");
        }
    }
}
