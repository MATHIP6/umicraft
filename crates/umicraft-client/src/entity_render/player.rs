// SPDX-License-Identifier: MIT OR Apache-2.0

//! The posed model of a remote player.

use umicraft_assets::skin::SkinModel;
use umicraft_game::entity::EntityRenderSnapshot;
use umicraft_model::entity::HumanoidModel;

/// The posed humanoid model of a player snapshot, and whether it has slim
/// arms. Shared by the body mesh and by the held item, which hangs on the
/// posed right arm.
pub(crate) fn posed_player(ent: &EntityRenderSnapshot) -> (HumanoidModel, bool) {
    let is_slim = ent
        .uuid
        .is_some_and(|u| SkinModel::default_for_uuid(u).is_slim());
    let mut model = HumanoidModel::new(is_slim);
    model.set_angles(
        ent.limb_swing,
        ent.limb_swing_amount,
        ent.head_yaw - ent.yaw,
        ent.pitch,
        false,
    );
    if ent.flags.is_sneaking() {
        model.apply_sneaking();
    }
    (model, is_slim)
}
