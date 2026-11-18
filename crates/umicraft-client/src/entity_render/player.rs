// SPDX-License-Identifier: MIT OR Apache-2.0

//! The posed model of a remote player.

use umicraft_assets::skin::SkinModel;
use umicraft_game::entity::EntityRenderSnapshot;
use umicraft_model::entity::{BipedStyle, HumanoidModel, HumanoidPose};

use super::pose::humanoid_pose;

/// The posed humanoid model of a player snapshot in `style`, and whether it
/// has slim arms. Shared by the body mesh and by the held item, which hangs
/// on the posed right arm.
pub(crate) fn posed_player(ent: &EntityRenderSnapshot, style: BipedStyle) -> (HumanoidModel, bool) {
    let is_slim = has_slim_arms(ent);
    let mut model = HumanoidModel::new(is_slim);
    if ent.spectator {
        model.keep_head_only();
    } else {
        model.show_skin_layers(ent.flags.skin_parts);
    }
    model.set_pose(&HumanoidPose {
        style,
        ..humanoid_pose(ent, false)
    });
    (model, is_slim)
}

/// The posed head of a player, which is the same in every style.
pub(crate) fn player_head(ent: &EntityRenderSnapshot) -> umicraft_model::entity::ModelPart {
    posed_player(ent, BipedStyle::V1_8).0.head
}

/// Whether the player has the slim arms: from its profile, else from the
/// default skin of its UUID.
pub(crate) fn has_slim_arms(ent: &EntityRenderSnapshot) -> bool {
    ent.profile
        .as_ref()
        .and_then(|profile| profile.slim)
        .or_else(|| {
            ent.uuid
                .map(|uuid| SkinModel::default_for_uuid(uuid).is_slim())
        })
        .unwrap_or(false)
}
