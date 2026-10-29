// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where and how a model stands in the world: its orientation, the death
//! fall, the bed, the player scale and the crouch.

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};

/// Scale of the player model (MCP `RenderPlayer.preRenderCallback`).
const PLAYER_SCALE: f32 = 0.9375;
/// How far a crouching player sinks, in blocks
/// (MCP `ModelBiped.render`: `translate(0, 0.2, 0)` in the flipped frame).
const SNEAK_SINK: f32 = 0.2;
/// Death fall in degrees at the end of the fall (MCP `getDeathMaxRotation`).
const DEATH_FALL_DEGREES: f32 = 90.0;
/// Ticks of the death fall that the angle is spread over, and the speed-up
/// of its square root (MCP `RendererLivingEntity.rotateCorpse`).
const DEATH_FALL_TICKS: f32 = 20.0;
const DEATH_FALL_SPEED: f32 = 1.6;

/// The matrix that carries the vertices of a model of `entity` to the world:
/// [`body_matrix`] and the mirror every vanilla model gets.
#[must_use]
pub(crate) fn model_matrix(entity: &EntityRenderSnapshot) -> Mat4 {
    body_matrix(entity) * x_mirror()
}

/// The vanilla model space is drawn through `scale(-1, -1, 1)`; the models of
/// this crate undo the y half (y up) but keep the box layout and the UVs of
/// the vanilla model space, so they are mirrored along x to be seen as in
/// vanilla: the right arm on the entity's right, skins not mirrored, and
/// vanilla turns (head yaw, torso turn) in the vanilla direction.
#[must_use]
pub(crate) fn x_mirror() -> Mat4 {
    Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0))
}

/// Position, turn, scale and crouch of `entity`, without the model mirror.
/// Attachments given in the vanilla frame (the held item) hang on it.
#[must_use]
pub(crate) fn body_matrix(entity: &EntityRenderSnapshot) -> Mat4 {
    let is_player = entity.kind == EntityKind::Player;
    let mut matrix = Mat4::from_translation(entity.position) * orientation(entity);
    if is_player {
        matrix *= Mat4::from_scale(Vec3::splat(PLAYER_SCALE));
        if entity.flags.is_sneaking() {
            matrix *= Mat4::from_translation(Vec3::new(0.0, -SNEAK_SINK, 0.0));
        }
    }
    matrix
}

/// The turn of the body: towards its yaw and tipped over when dead, or flat
/// in a bed.
///
/// Models face -Z, and a Minecraft yaw of zero looks towards +Z, hence the
/// half turn (MCP `rotateCorpse`: `rotate(180 - yaw)`).
fn orientation(entity: &EntityRenderSnapshot) -> Mat4 {
    if let Some(bed) = entity.bed {
        // MCP `RenderPlayer.rotateCorpse` and `renderLivingAt`.
        return Mat4::from_translation(bed.render_offset)
            * Mat4::from_rotation_y(bed.orientation.to_radians())
            * Mat4::from_rotation_z(90.0_f32.to_radians())
            * Mat4::from_rotation_y(270.0_f32.to_radians());
    }
    Mat4::from_rotation_y((180.0 - entity.body_yaw).to_radians()) * death_fall(entity.death_time)
}

/// The roll of a dying entity onto its side: fast at first, then slowing
/// (`sqrt` of the elapsed fraction), at most [`DEATH_FALL_DEGREES`].
fn death_fall(death_time: f32) -> Mat4 {
    if death_time <= 0.0 {
        return Mat4::IDENTITY;
    }
    let fraction = ((death_time - 1.0) / DEATH_FALL_TICKS * DEATH_FALL_SPEED)
        .max(0.0)
        .sqrt()
        .min(1.0);
    Mat4::from_rotation_z((fraction * DEATH_FALL_DEGREES).to_radians())
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::{EntityFlags, EntityId};

    fn snapshot(kind: EntityKind) -> EntityRenderSnapshot {
        EntityRenderSnapshot::new(EntityId(1), kind, Vec3::new(10.0, 64.0, 20.0))
    }

    fn front(entity: &EntityRenderSnapshot) -> Vec3 {
        model_matrix(entity).transform_vector3(Vec3::NEG_Z)
    }

    #[test]
    fn a_model_looks_where_the_yaw_says() {
        // Yaw 0 looks towards +Z, 90 towards -X (MCP `getLook`).
        let mut entity = snapshot(EntityKind::Zombie);
        assert!((front(&entity) - Vec3::Z).length() < 1e-5);
        entity.body_yaw = 90.0;
        assert!((front(&entity) - Vec3::NEG_X).length() < 1e-5);
        entity.body_yaw = 180.0;
        assert!((front(&entity) - Vec3::NEG_Z).length() < 1e-5);
    }

    #[test]
    fn the_right_arm_is_on_the_entitys_right() {
        // Facing south (yaw 0) the entity's right is west (-X); the model's
        // right arm sits at x = -5 / 16.
        let entity = snapshot(EntityKind::Zombie);
        let arm = model_matrix(&entity).transform_point3(Vec3::new(-5.0 / 16.0, 1.0, 0.0));
        assert!(arm.x < entity.position.x, "{arm}");
    }

    #[test]
    fn a_player_is_a_sixteenth_smaller() {
        let player = model_matrix(&snapshot(EntityKind::Player));
        let top = player.transform_point3(Vec3::new(0.0, 1.0, 0.0));
        assert!((top.y - 64.9375).abs() < 1e-5);
    }

    #[test]
    fn a_crouching_player_sinks_by_a_fifth_of_a_block() {
        let mut entity = snapshot(EntityKind::Player);
        entity.flags = EntityFlags::new(0x02, 0);
        let feet = model_matrix(&entity).transform_point3(Vec3::ZERO);
        assert!((feet.y - (64.0 - 0.2 * 0.9375)).abs() < 1e-5);
    }

    #[test]
    fn only_players_crouch() {
        let mut entity = snapshot(EntityKind::Zombie);
        entity.flags = EntityFlags::new(0x02, 0);
        let feet = model_matrix(&entity).transform_point3(Vec3::ZERO);
        assert_eq!(feet.y, 64.0);
    }

    #[test]
    fn a_dying_entity_tips_onto_its_side() {
        let mut entity = snapshot(EntityKind::Zombie);
        entity.death_time = 1.0;
        let head = model_matrix(&entity).transform_point3(Vec3::Y) - entity.position;
        assert!(
            (head - Vec3::Y).length() < 1e-5,
            "upright when the fall starts"
        );
        entity.death_time = 21.0;
        let head = model_matrix(&entity).transform_point3(Vec3::Y) - entity.position;
        assert!(head.y.abs() < 1e-5, "flat at the end: {head}");
        assert!((head.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_death_fall_never_exceeds_a_quarter_turn() {
        let mut entity = snapshot(EntityKind::Zombie);
        entity.death_time = 200.0;
        let head = model_matrix(&entity).transform_point3(Vec3::Y) - entity.position;
        assert!(head.y.abs() < 1e-5);
    }

    #[test]
    fn a_sleeper_lies_flat_along_the_bed() {
        let mut entity = snapshot(EntityKind::Player);
        entity.bed = Some(umicraft_game::entity::BedPose::from_facing(0));
        let head = model_matrix(&entity).transform_point3(Vec3::Y) - entity.position;
        assert!(head.y.abs() < 0.1, "head is level with the feet: {head}");
    }
}
