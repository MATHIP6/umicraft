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

use super::animals::with_eyes;
use super::glow::{glowing, Glow, UvMap};
use super::mobs::{one, EntityLayer, Pose};

/// Scale of a wither (MCP `RenderWither.preRenderCallback`, without the
/// shrink of the spawn animation).
const WITHER_SCALE: f32 = 2.0;
/// Health at or below which a wither is armoured (half of its 300).
const WITHER_ARMORED_HEALTH: f32 = 150.0;

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
            with_eyes(
                TextureSlot::Dragon,
                TextureSlot::DragonEyes,
                model.emit_mesh(),
            )
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
            let invulnerable = match ent.meta.variant {
                Variant::Wither { invulnerable } => invulnerable,
                _ => 0,
            };
            let scale = wither_scale(invulnerable);
            let mut mesh = model.emit_mesh();
            scale_mesh(&mut mesh, scale);
            let mut layers = one(wither_texture(invulnerable), mesh);
            // A wounded wither (half its health or less) shows its swirling
            // armour (MCP `LayerWitherAura`, `EntityWither.isArmored`).
            if ent.meta.living.health <= WITHER_ARMORED_HEALTH {
                let mut aura = WitherModel::aura();
                aura.set_angles(swing, head_yaw, head_pitch);
                let mut aura_mesh = aura.emit_mesh();
                scale_mesh(&mut aura_mesh, scale);
                layers.push(glowing(
                    TextureSlot::WitherAura,
                    aura_mesh,
                    Glow {
                        tint: [0.5; 3],
                        squared: false,
                        uv: UvMap::scroll((ent.age * 0.02).cos() * 3.0, ent.age * 0.01),
                        equal_depth: false,
                    },
                ));
            }
            layers
        }
        _ => return None,
    })
}

/// The scale of a wither with `invulnerable` ticks of spawn countdown left: it
/// starts small and grows to 2 (MCP `RenderWither.preRenderCallback`).
fn wither_scale(invulnerable: i32) -> f32 {
    if invulnerable > 0 {
        WITHER_SCALE - invulnerable as f32 / 220.0 * 0.5
    } else {
        WITHER_SCALE
    }
}

/// The texture of a wither: the white-flashing one while it is spawning, in
/// the first 80 ticks and then every other 5 ticks (MCP
/// `RenderWither.getEntityTexture`).
fn wither_texture(invulnerable: i32) -> TextureSlot {
    if invulnerable > 0 && (invulnerable > 80 || invulnerable / 5 % 2 != 1) {
        TextureSlot::WitherInvulnerable
    } else {
        TextureSlot::Wither
    }
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
    let mut layers = one(TextureSlot::Creeper, vertices);
    // A creeper struck by lightning is wrapped in a swirling shell inflated by
    // 2 pixels (MCP `LayerCreeperCharge`).
    if matches!(&ent.meta.variant, Variant::Creeper(creeper) if creeper.powered) {
        let mut shell = CreeperModel::charged();
        shell.set_angles(pose.swing, pose.amount, pose.head_yaw, pose.head_pitch);
        let mut mesh = shell.emit_mesh();
        for vertex in &mut mesh {
            vertex.position[0] *= wide;
            vertex.position[1] *= tall;
            vertex.position[2] *= wide;
        }
        layers.push(glowing(
            TextureSlot::CreeperCharge,
            mesh,
            Glow {
                tint: [0.5; 3],
                squared: false,
                uv: UvMap::scroll(ent.age * 0.01, ent.age * 0.01),
                equal_depth: false,
            },
        ));
    }
    layers
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

    #[test]
    fn a_spawning_wither_grows_and_flashes_like_the_reference() {
        assert_eq!(wither_scale(0), 2.0);
        assert!(
            (wither_scale(220) - 1.5).abs() < 1e-6,
            "the smallest, at the start"
        );
        assert!(wither_scale(100) > wither_scale(200));
        assert_eq!(wither_texture(0), TextureSlot::Wither);
        assert_eq!(wither_texture(200), TextureSlot::WitherInvulnerable);
        assert_eq!(wither_texture(81), TextureSlot::WitherInvulnerable);
        // Under 80 ticks it blinks: the odd groups of 5 ticks show the normal skin.
        assert_eq!(
            wither_texture(80),
            TextureSlot::WitherInvulnerable,
            "group 16"
        );
        assert_eq!(wither_texture(79), TextureSlot::Wither, "group 15");
        assert_eq!(wither_texture(75), TextureSlot::Wither, "group 15");
        assert_eq!(
            wither_texture(74),
            TextureSlot::WitherInvulnerable,
            "group 14"
        );
        assert_eq!(wither_texture(69), TextureSlot::Wither, "group 13");
        assert_eq!(
            wither_texture(1),
            TextureSlot::WitherInvulnerable,
            "group 0"
        );
    }
}
