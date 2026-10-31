// SPDX-License-Identifier: MIT OR Apache-2.0

//! The humanoid pose of an entity snapshot.

use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_game::item_use::{use_action, UseAction};
use umicraft_model::entity::{ArmPose, HumanoidPose};
use umicraft_protocol::packets::play::ItemStack;

/// The pose of a humanoid mob or player (MCP `ModelBiped` inputs as
/// `RendererLivingEntity.doRender` and `RenderPlayer.setModelVisibilities`
/// set them). `arms_forward` is the zombie and skeleton stance.
#[must_use]
pub(super) fn humanoid_pose(entity: &EntityRenderSnapshot, arms_forward: bool) -> HumanoidPose {
    HumanoidPose {
        limb_swing: entity.limb_swing,
        limb_swing_amount: entity.limb_swing_amount,
        age: entity.age,
        head_yaw: entity.head_yaw - entity.body_yaw,
        head_pitch: entity.pitch,
        swing_progress: entity.swing_progress,
        sneaking: entity.kind == EntityKind::Player && entity.flags.is_sneaking(),
        arms: arm_pose(entity),
        arms_forward,
        riding: entity.riding,
    }
}

/// What the right hand of a player does: nothing, holds an item, or uses it
/// (the item-use flag of the metadata marks a sword raised or a bow drawn).
fn arm_pose(entity: &EntityRenderSnapshot) -> ArmPose {
    let Some(held) = entity
        .held
        .as_ref()
        .filter(|_| entity.kind == EntityKind::Player)
    else {
        return ArmPose::Empty;
    };
    if !entity.flags.is_using_item() {
        return ArmPose::Holding;
    }
    let stack = ItemStack {
        item_id: held.item_id,
        stack_size: 1,
        damage: held.damage,
        nbt: None,
    };
    match use_action(&stack) {
        Some(UseAction::Block) => ArmPose::Blocking,
        Some(UseAction::Bow) => ArmPose::AimingBow,
        _ => ArmPose::Holding,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::{EntityFlags, EntityId, EntityItem};

    const IRON_SWORD: i16 = 267;
    const BOW: i16 = 261;
    const APPLE: i16 = 260;

    fn player(held: Option<i16>, flags: u8) -> EntityRenderSnapshot {
        let mut entity =
            EntityRenderSnapshot::new(EntityId(1), EntityKind::Player, glam::Vec3::ZERO);
        entity.held = held.map(|item_id| EntityItem {
            item_id,
            damage: 0,
            count: 1,
            nbt: None,
        });
        entity.flags = EntityFlags::new(flags, 0);
        entity
    }

    #[test]
    fn an_empty_hand_swings_freely() {
        assert_eq!(arm_pose(&player(None, 0)), ArmPose::Empty);
    }

    #[test]
    fn a_held_item_is_carried_forward() {
        assert_eq!(arm_pose(&player(Some(IRON_SWORD), 0)), ArmPose::Holding);
    }

    #[test]
    fn a_sword_in_use_blocks_and_a_bow_in_use_aims() {
        assert_eq!(arm_pose(&player(Some(IRON_SWORD), 0x10)), ArmPose::Blocking);
        assert_eq!(arm_pose(&player(Some(BOW), 0x10)), ArmPose::AimingBow);
    }

    #[test]
    fn eating_keeps_the_item_in_the_hand() {
        assert_eq!(arm_pose(&player(Some(APPLE), 0x10)), ArmPose::Holding);
    }

    #[test]
    fn a_crouching_player_gets_the_crouch_pose_but_a_mob_does_not() {
        let mut entity = player(None, 0x02);
        assert!(humanoid_pose(&entity, false).sneaking);
        entity.kind = EntityKind::Zombie;
        assert!(!humanoid_pose(&entity, true).sneaking);
    }

    #[test]
    fn the_head_turns_relative_to_the_body() {
        let mut entity = player(None, 0);
        entity.body_yaw = 30.0;
        entity.head_yaw = 50.0;
        assert_eq!(humanoid_pose(&entity, false).head_yaw, 20.0);
    }
}
