// SPDX-License-Identifier: MIT OR Apache-2.0

//! The armour of a biped: the two `ModelBiped` shells and the materials.
//!
//! MCP `LayerBipedArmor` builds `ModelBiped(0.5)` for the leggings and
//! `ModelBiped(1.0)` for the other three pieces, on a 64×32 texture, and
//! shows only the parts the slot covers.

use super::{HumanoidModel, ModelBox, ModelPart};

/// The material of a piece of armour (MCP `ItemArmor.ArmorMaterial`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArmorMaterial {
    /// Leather, which is dyed and has an overlay texture.
    Leather,
    /// Chainmail.
    Chain,
    /// Iron.
    Iron,
    /// Gold.
    Gold,
    /// Diamond.
    Diamond,
}

impl ArmorMaterial {
    /// The material of the armour item `item_id` (MCP `Item.registerItem`
    /// 298 to 317: four pieces per material, in the order leather, chain,
    /// iron, diamond, gold), or `None` for any other item.
    #[must_use]
    pub fn from_item_id(item_id: i16) -> Option<Self> {
        Some(match (item_id - 298).div_euclid(4) {
            0 if item_id >= 298 => Self::Leather,
            1 => Self::Chain,
            2 => Self::Iron,
            3 => Self::Diamond,
            4 if item_id <= 317 => Self::Gold,
            _ => return None,
        })
    }

    /// The name in the texture path (`ArmorMaterial.getName`).
    #[must_use]
    pub const fn texture_name(self) -> &'static str {
        match self {
            Self::Leather => "leather",
            Self::Chain => "chainmail",
            Self::Iron => "iron",
            Self::Gold => "gold",
            Self::Diamond => "diamond",
        }
    }
}

impl HumanoidModel {
    /// The armour shell for equipment slot `slot` (1 boots, 2 leggings,
    /// 3 chestplate, 4 helmet), inflated by `0.5` for the leggings and `1.0`
    /// otherwise, with only the parts that slot covers
    /// (MCP `LayerBipedArmor.func_177179_a`).
    #[must_use]
    pub fn armor(slot: u8) -> Self {
        let size = if slot == 2 { 0.5 } else { 1.0 };
        let tex = [64.0, 32.0];
        let part = |pivot: [f32; 3], origin: [f32; 3], dims: [f32; 3], uv: [f32; 2], mirror| {
            let shell = ModelBox::new(origin, dims, uv, tex, size);
            ModelPart::new(pivot).with_box(if mirror { shell.mirrored() } else { shell })
        };
        let covers = |parts: &[u8]| parts.contains(&slot);
        // Which slots show which part: boots 1, leggings 2, chest 3, helmet 4.
        let mut head = part(
            [0.0, 24.0, 0.0],
            [-4.0, 0.0, -4.0],
            [8.0; 3],
            [0.0, 0.0],
            false,
        );
        head = head.with_box(ModelBox::new(
            [-4.0, 0.0, -4.0],
            [8.0; 3],
            [32.0, 0.0],
            tex,
            size + 0.5,
        ));
        let mut body = part(
            [0.0, 24.0, 0.0],
            [-4.0, -12.0, -2.0],
            [8.0, 12.0, 4.0],
            [16.0, 16.0],
            false,
        );
        let mut right_arm = part(
            [-5.0, 22.0, 0.0],
            [-3.0, -10.0, -2.0],
            [4.0, 12.0, 4.0],
            [40.0, 16.0],
            false,
        );
        let mut left_arm = part(
            [5.0, 22.0, 0.0],
            [-1.0, -10.0, -2.0],
            [4.0, 12.0, 4.0],
            [40.0, 16.0],
            true,
        );
        let mut right_leg = part(
            [-1.9, 12.0, 0.0],
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 16.0],
            false,
        );
        let mut left_leg = part(
            [1.9, 12.0, 0.0],
            [-2.0, -12.0, -2.0],
            [4.0, 12.0, 4.0],
            [0.0, 16.0],
            true,
        );
        for (model_part, shown) in [
            (&mut head, covers(&[4])),
            (&mut body, covers(&[2, 3])),
            (&mut right_arm, covers(&[3])),
            (&mut left_arm, covers(&[3])),
            (&mut right_leg, covers(&[1, 2])),
            (&mut left_leg, covers(&[1, 2])),
        ] {
            if !shown {
                model_part.boxes.clear();
            }
        }
        Self {
            head,
            body,
            right_arm,
            left_arm,
            right_leg,
            left_leg,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_item_ids_map_to_their_material() {
        assert_eq!(
            ArmorMaterial::from_item_id(298),
            Some(ArmorMaterial::Leather)
        );
        assert_eq!(
            ArmorMaterial::from_item_id(301),
            Some(ArmorMaterial::Leather)
        );
        assert_eq!(ArmorMaterial::from_item_id(302), Some(ArmorMaterial::Chain));
        assert_eq!(ArmorMaterial::from_item_id(306), Some(ArmorMaterial::Iron));
        assert_eq!(
            ArmorMaterial::from_item_id(310),
            Some(ArmorMaterial::Diamond)
        );
        assert_eq!(ArmorMaterial::from_item_id(317), Some(ArmorMaterial::Gold));
        assert_eq!(ArmorMaterial::from_item_id(297), None);
        assert_eq!(ArmorMaterial::from_item_id(318), None);
        assert_eq!(ArmorMaterial::from_item_id(-1), None);
    }

    #[test]
    fn each_slot_covers_the_parts_of_layer_biped_armor() {
        let boxes = |slot| {
            let m = HumanoidModel::armor(slot);
            [
                &m.head,
                &m.body,
                &m.right_arm,
                &m.left_arm,
                &m.right_leg,
                &m.left_leg,
            ]
            .map(|part| part.boxes.len())
        };
        assert_eq!(boxes(1), [0, 0, 0, 0, 1, 1], "boots: legs");
        assert_eq!(boxes(2), [0, 1, 0, 0, 1, 1], "leggings: body and legs");
        assert_eq!(boxes(3), [0, 1, 1, 1, 0, 0], "chestplate: body and arms");
        assert_eq!(boxes(4), [2, 0, 0, 0, 0, 0], "helmet: head and headwear");
    }

    #[test]
    fn the_leggings_shell_is_thinner_than_the_others() {
        let width = |slot| {
            let mesh = HumanoidModel::armor(slot).emit_mesh();
            let xs = mesh.iter().map(|v| v.position[0]);
            xs.clone().fold(f32::MIN, f32::max) - xs.fold(f32::MAX, f32::min)
        };
        assert!(width(2) < width(3));
    }
}
