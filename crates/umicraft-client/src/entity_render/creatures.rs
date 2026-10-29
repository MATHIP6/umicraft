// SPDX-License-Identifier: MIT OR Apache-2.0

//! Slimes, creepers and the models driven by few inputs (flyers, swimmers,
//! golems, bosses).

use umicraft_game::entity::meta::Variant;
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::child::scale_mesh;
use umicraft_model::entity::{
    BatModel, BlazeModel, CreeperModel, DragonModel, EndermiteModel, GhastModel, GuardianModel,
    SilverfishModel, SlimeModel, SnowmanModel, SquidModel, TextureSlot, WitherModel, GHAST_SCALE,
};

use super::mobs::{one, EntityLayer, Pose};

/// Slimes, creepers, flyers, swimmers and the other models driven by fewer
/// inputs.
pub(super) fn creature_layers(ent: &EntityRenderSnapshot, pose: Pose) -> Option<Vec<EntityLayer>> {
    let kind = ent.kind;
    let Pose {
        swing,
        head_yaw,
        head_pitch,
        ..
    } = pose;
    Some(match kind {
        EntityKind::Slime => {
            let model = SlimeModel::slime(1.0);
            let mut vertices = model.emit_inner_mesh();
            vertices.extend(model.emit_outer_mesh());
            scale_mesh(&mut vertices, slime_size(ent));
            one(TextureSlot::Slime, vertices)
        }
        EntityKind::MagmaCube => {
            let mut vertices = SlimeModel::magma_cube(1.0).emit_inner_mesh();
            scale_mesh(&mut vertices, slime_size(ent));
            one(TextureSlot::MagmaCube, vertices)
        }
        EntityKind::Snowman => {
            let mut model = SnowmanModel::new();
            model.set_angles(head_yaw, head_pitch);
            one(TextureSlot::Snowman, model.emit_mesh())
        }
        EntityKind::Squid => {
            let mut model = SquidModel::new();
            model.set_angles(swing);
            one(TextureSlot::Squid, model.emit_mesh())
        }
        EntityKind::Bat => {
            let mut model = BatModel::new();
            model.set_angles(swing);
            one(TextureSlot::Bat, model.emit_mesh())
        }
        EntityKind::Blaze => {
            let mut model = BlazeModel::new();
            model.set_angles(swing, head_yaw, head_pitch);
            one(TextureSlot::Blaze, model.emit_mesh())
        }
        EntityKind::Dragon => {
            let mut model = DragonModel::new();
            model.set_angles(swing);
            one(TextureSlot::Dragon, model.emit_mesh())
        }
        EntityKind::Endermite => {
            let mut model = EndermiteModel::new();
            model.set_angles(swing);
            one(TextureSlot::Endermite, model.emit_mesh())
        }
        EntityKind::Ghast => {
            let mut model = GhastModel::new();
            model.set_angles(ent.age);
            let mut mesh = model.emit_mesh();
            scale_mesh(&mut mesh, GHAST_SCALE);
            let attacking = matches!(ent.meta.variant, Variant::Ghast { attacking: true });
            one(
                if attacking {
                    TextureSlot::GhastShooting
                } else {
                    TextureSlot::Ghast
                },
                mesh,
            )
        }
        EntityKind::Guardian => {
            let mut model = GuardianModel::new();
            model.set_angles(swing);
            one(TextureSlot::Guardian, model.emit_mesh())
        }
        EntityKind::Silverfish => {
            let mut model = SilverfishModel::new();
            model.set_angles(swing);
            one(TextureSlot::Silverfish, model.emit_mesh())
        }
        EntityKind::Wither => {
            let mut model = WitherModel::new();
            model.set_angles(swing, head_yaw, head_pitch);
            one(TextureSlot::Wither, model.emit_mesh())
        }
        _ => return None,
    })
}

/// The size of a slime or magma cube; vanilla draws the one model scaled by it
/// (MCP `RenderSlime.preRenderCallback`, without the squish).
fn slime_size(ent: &EntityRenderSnapshot) -> f32 {
    match ent.meta.variant {
        Variant::Slime(size) => f32::from(size),
        _ => 1.0,
    }
}

/// The creeper, swollen by its fuse (MCP `RenderCreeper.preRenderCallback`).
pub(super) fn creeper_layers(ent: &EntityRenderSnapshot, pose: Pose) -> Vec<EntityLayer> {
    let mut model = CreeperModel::new();
    model.set_angles(pose.swing, pose.amount, pose.head_yaw, pose.head_pitch);
    let mut vertices = model.emit_mesh();
    let (wide, tall) = creeper_swell(ent.fuse_flash);
    for vertex in &mut vertices {
        vertex.position[0] *= wide;
        vertex.position[1] *= tall;
        vertex.position[2] *= wide;
    }
    one(TextureSlot::Creeper, vertices)
}

/// The horizontal and vertical scale of a creeper at fuse flash `flash`.
fn creeper_swell(flash: f32) -> (f32, f32) {
    let wobble = 1.0 + (flash * 100.0).sin() * flash * 0.01;
    let grown = flash.clamp(0.0, 1.0).powi(4);
    ((1.0 + grown * 0.4) * wobble, (1.0 + grown * 0.1) / wobble)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_calm_creeper_keeps_its_size() {
        assert_eq!(creeper_swell(0.0), (1.0, 1.0));
    }

    #[test]
    fn a_creeper_at_the_end_of_its_fuse_is_wider_and_a_little_taller() {
        let (wide, tall) = creeper_swell(1.0);
        assert!((wide - 1.4).abs() < 0.02, "{wide}");
        assert!((tall - 1.1).abs() < 0.02, "{tall}");
    }
}
