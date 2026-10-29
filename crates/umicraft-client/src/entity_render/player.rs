// SPDX-License-Identifier: MIT OR Apache-2.0

//! The posed model of a remote player.

use umicraft_assets::skin::SkinModel;
use umicraft_game::entity::EntityRenderSnapshot;
use umicraft_model::entity::HumanoidModel;

use super::pose::humanoid_pose;

/// The posed humanoid model of a player snapshot, and whether it has slim
/// arms. Shared by the body mesh and by the held item, which hangs on the
/// posed right arm.
pub(crate) fn posed_player(ent: &EntityRenderSnapshot) -> (HumanoidModel, bool) {
    let is_slim = ent
        .uuid
        .is_some_and(|u| SkinModel::default_for_uuid(u).is_slim());
    let mut model = HumanoidModel::new(is_slim);
    model.set_pose(&humanoid_pose(ent, false));
    (model, is_slim)
}
