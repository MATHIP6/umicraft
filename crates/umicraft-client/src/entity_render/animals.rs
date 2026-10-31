// SPDX-License-Identifier: MIT OR Apache-2.0

//! Animals and villagers: the ground mobs that have babies and coats.

use umicraft_game::entity::meta::{HorseMeta, Variant};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::child::scale_mesh;
use umicraft_model::entity::{
    ChickenModel, EndermanModel, HorseLook, HorseModel, IronGolemModel, OcelotModel, OcelotPose,
    QuadrupedModel, RabbitModel, SheepModel, SpiderModel, TextureSlot, VillagerModel, WolfModel,
};

use super::glow::{glowing, Glow};
use super::mobs::{one, EntityLayer, Pose};
use super::mushrooms::mushroom_vertices;
use super::variants;

/// Scale of a villager (MCP `RenderVillager.preRenderCallback`); a child
/// is half of it.
const VILLAGER_SCALE: f32 = 0.9375;
/// Scale of a witch (MCP `RenderWitch.preRenderCallback`).
pub(crate) const WITCH_SCALE: f32 = 0.9375;
/// Scale of a donkey and of a mule (MCP `RenderHorse.preRenderCallback`).
const DONKEY_SCALE: f32 = 0.87;
const MULE_SCALE: f32 = 0.92;
/// Size of a foal as born (MCP `EntityHorse.getHorseSize`); the horse model
/// shrinks its parts by it.
const FOAL_SIZE: f32 = 0.5;

/// Animals, villagers and the other limb-swinging ground mobs.
pub(super) fn animal_layers(ent: &EntityRenderSnapshot, pose: Pose) -> Option<Vec<EntityLayer>> {
    let Pose {
        swing,
        amount,
        head_yaw,
        head_pitch,
    } = pose;
    let baby = ent.meta.baby;
    // A model with proper baby proportions.
    macro_rules! ageable {
        ($model:expr, $slot:expr) => {{
            let mut model = $model;
            model.set_angles(swing, amount, head_yaw, head_pitch);
            let mesh = if baby {
                model.emit_child_mesh()
            } else {
                model.emit_mesh()
            };
            one($slot, mesh)
        }};
    }
    // A model that is only scaled down when young.
    macro_rules! walker {
        ($model:expr, $slot:expr, $adult:expr, $young:expr) => {{
            let mut model = $model;
            model.set_angles(swing, amount, head_yaw, head_pitch);
            let mut mesh = model.emit_mesh();
            scale_mesh(&mut mesh, if baby { $young } else { $adult });
            one($slot, mesh)
        }};
    }
    Some(match ent.kind {
        EntityKind::Cow => ageable!(QuadrupedModel::cow(), TextureSlot::Cow),
        EntityKind::Mooshroom => {
            let mut model = QuadrupedModel::cow();
            model.set_angles(swing, amount, head_yaw, head_pitch);
            let mesh = if baby {
                model.emit_child_mesh()
            } else {
                model.emit_mesh()
            };
            let mut layers = one(TextureSlot::Mooshroom, mesh);
            // An adult carries three red mushrooms (`LayerMooshroomMushroom`).
            if !baby {
                layers.push(EntityLayer::new(
                    TextureSlot::MushroomRed,
                    mushroom_vertices(&model.head),
                ));
            }
            layers
        }
        EntityKind::Pig => {
            let mut layers = ageable!(QuadrupedModel::pig(), TextureSlot::Pig);
            if matches!(ent.meta.variant, Variant::Pig { saddled: true }) {
                layers.extend(ageable!(
                    QuadrupedModel::pig_saddle(),
                    TextureSlot::PigSaddle
                ));
            }
            layers
        }
        EntityKind::Chicken => ageable!(ChickenModel::new(), TextureSlot::Chicken),
        EntityKind::Sheep => sheep_layers(ent, pose),
        EntityKind::Wolf => wolf_layers(ent, pose),
        EntityKind::Ocelot => {
            let (slot, sitting) = match ent.meta.variant {
                Variant::Ocelot(ocelot) => (variants::ocelot(ocelot), ocelot.sitting),
                _ => (TextureSlot::Ocelot, false),
            };
            // MCP `ModelOcelot.setLivingAnimations`: sneaking, then sprinting,
            // then sitting decide the pose.
            let mut model = OcelotModel::new();
            model.set_pose(if ent.flags.is_sneaking() {
                OcelotPose::Sneaking
            } else if ent.flags.is_sprinting() {
                OcelotPose::Sprinting
            } else if sitting {
                OcelotPose::Sitting
            } else {
                OcelotPose::Standing
            });
            ageable!(model, slot)
        }
        EntityKind::Rabbit => {
            let slot = match ent.meta.variant {
                Variant::Rabbit(rabbit) => {
                    variants::rabbit(rabbit, ent.meta.custom_name.as_deref())
                }
                _ => TextureSlot::Rabbit,
            };
            ageable!(RabbitModel::new(), slot)
        }
        EntityKind::Villager => {
            let slot = match ent.meta.variant {
                Variant::Villager { profession } => variants::villager(profession),
                _ => TextureSlot::Villager,
            };
            walker!(
                VillagerModel::villager(),
                slot,
                VILLAGER_SCALE,
                VILLAGER_SCALE * 0.5
            )
        }
        EntityKind::Witch => walker!(
            VillagerModel::witch(),
            TextureSlot::Witch,
            WITCH_SCALE,
            WITCH_SCALE
        ),
        EntityKind::Horse => horse_layers(ent, pose),
        EntityKind::Spider => {
            let mut model = SpiderModel::new();
            model.set_angles(swing, amount, head_yaw, head_pitch);
            with_eyes(
                TextureSlot::Spider,
                TextureSlot::SpiderEyes,
                model.emit_mesh(),
            )
        }
        EntityKind::CaveSpider => {
            let mut model = SpiderModel::with_scale(0.7);
            model.set_angles(swing, amount, head_yaw, head_pitch);
            with_eyes(
                TextureSlot::CaveSpider,
                TextureSlot::SpiderEyes,
                model.emit_mesh(),
            )
        }
        EntityKind::Enderman => {
            let mut model = EndermanModel::new();
            if let Variant::Enderman(enderman) = &ent.meta.variant {
                model.is_carrying = enderman.carried & 0x0FFF != 0;
                model.is_attacking = enderman.screaming;
            }
            model.set_angles(swing, amount, head_yaw, head_pitch);
            with_eyes(
                TextureSlot::Enderman,
                TextureSlot::EndermanEyes,
                model.emit_mesh(),
            )
        }
        EntityKind::IronGolem => walker!(IronGolemModel::new(), TextureSlot::IronGolem, 1.0, 1.0),
        _ => return None,
    })
}

/// A body with the glowing eyes of the same shape drawn over it (MCP
/// `LayerSpiderEyes`, `LayerEndermanEyes`).
pub(super) fn with_eyes(
    body: TextureSlot,
    eyes: TextureSlot,
    mesh: Vec<umicraft_model::entity::ModelVertex>,
) -> Vec<EntityLayer> {
    vec![
        EntityLayer::new(body, mesh.clone()),
        glowing(eyes, mesh, Glow::EYES),
    ]
}

/// The sheep: its body, and its wool in the dye colour unless shorn.
fn sheep_layers(ent: &EntityRenderSnapshot, pose: Pose) -> Vec<EntityLayer> {
    let mut model = SheepModel::new();
    model.set_angles(pose.swing, pose.amount, pose.head_yaw, pose.head_pitch);
    let (body, wool) = if ent.meta.baby {
        (model.emit_child_body_mesh(), model.emit_child_wool_mesh())
    } else {
        (model.emit_body_mesh(), model.emit_wool_mesh())
    };
    let mut layers = one(TextureSlot::Sheep, body);
    if let Variant::Sheep(sheep) = ent.meta.variant {
        if sheep.sheared {
            return layers;
        }
        layers.push(EntityLayer::new(TextureSlot::SheepWool, wool).dyed(sheep.color));
    } else {
        layers.push(EntityLayer::new(TextureSlot::SheepWool, wool).dyed(0));
    }
    layers
}

/// The wolf, and the collar of a tamed one in its dye colour.
fn wolf_layers(ent: &EntityRenderSnapshot, pose: Pose) -> Vec<EntityLayer> {
    let mut model = WolfModel::new();
    if let Variant::Wolf(wolf) = &ent.meta.variant {
        let health = wolf.tamed.then_some(f32::from(wolf.health));
        model.set_state(wolf.sitting, wolf.angry, wolf.begging, health);
    }
    model.set_angles(pose.swing, pose.amount, pose.head_yaw, pose.head_pitch);
    let mesh = |model: &WolfModel| {
        if ent.meta.baby {
            model.emit_child_mesh()
        } else {
            model.emit_mesh()
        }
    };
    let Variant::Wolf(wolf) = ent.meta.variant else {
        return one(TextureSlot::Wolf, mesh(&model));
    };
    let mut layers = one(variants::wolf(wolf), mesh(&model));
    if wolf.tamed {
        // The collar entry is the dye damage; the palette is by metadata.
        layers.push(EntityLayer::new(TextureSlot::WolfCollar, mesh(&model)).dyed(15 - wolf.collar));
    }
    layers
}

/// The horse, donkey, mule, zombie or skeleton horse.
fn horse_layers(ent: &EntityRenderSnapshot, pose: Pose) -> Vec<EntityLayer> {
    let horse = match ent.meta.variant {
        Variant::Horse(horse) => horse,
        _ => HorseMeta::default(),
    };
    let mut model = if matches!(horse.kind, 1 | 2) {
        HorseModel::mule()
    } else {
        HorseModel::new()
    };
    let state = ent.horse.unwrap_or_default();
    model.set_look(HorseLook {
        limb_swing: pose.swing,
        limb_amount: pose.amount,
        head_yaw: pose.head_yaw,
        head_pitch: pose.head_pitch,
        grazing: state.grazing,
        rearing: state.rearing,
        mouth: state.mouth,
        tail_swinging: state.tail_swinging,
        age: ent.age,
        adult: !ent.meta.baby,
        // The age of a foal is not tracked: it is drawn as born.
        size: if ent.meta.baby { FOAL_SIZE } else { 1.0 },
        saddled: horse.saddled(),
        ridden: ent.ridden,
        chested: horse.chested(),
    });
    let (slot, scale) = (
        variants::horse(horse),
        match horse.kind {
            1 => DONKEY_SCALE,
            2 => MULE_SCALE,
            _ => 1.0,
        },
    );
    let mut mesh = model.emit();
    scale_mesh(&mut mesh, scale);
    one(slot, mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use umicraft_game::entity::horse_state::HorsePose;
    use umicraft_game::entity::meta::HorseMeta;
    use umicraft_game::entity::EntityId;

    fn horse_at(id: i32, x: f32, flags: i32) -> EntityRenderSnapshot {
        let mut horse =
            EntityRenderSnapshot::new(EntityId(id), EntityKind::Horse, Vec3::new(x, 64.0, 0.0));
        horse.meta.variant = Variant::Horse(HorseMeta {
            flags,
            variant: 0x0100,
            ..HorseMeta::default()
        });
        horse
    }

    fn top(layers: &[EntityLayer]) -> f32 {
        layers[0]
            .vertices
            .iter()
            .map(|v| v.position[1])
            .fold(f32::MIN, f32::max)
    }

    #[test]
    fn a_horse_grazes_rears_and_a_foal_is_small() {
        let pose = |ent: &EntityRenderSnapshot| Pose {
            swing: ent.limb_swing,
            amount: ent.limb_swing_amount,
            head_yaw: 0.0,
            head_pitch: 0.0,
        };
        let layers = |ent: &EntityRenderSnapshot| horse_layers(ent, pose(ent));
        let calm = horse_at(1, 0.0, 0);
        let mut grazing = horse_at(2, 0.0, 0);
        grazing.horse = Some(HorsePose {
            grazing: 1.0,
            ..HorsePose::default()
        });
        let mut foal = horse_at(3, 0.0, 0);
        foal.meta.baby = true;
        assert!(top(&layers(&grazing)) < top(&layers(&calm)) - 0.2);
        assert!(top(&layers(&foal)) < top(&layers(&calm)) * 0.8);
        // A saddled adult shows 10 more boxes, the reins 2 more when ridden.
        let boxes = |ent: &EntityRenderSnapshot| layers(ent)[0].vertices.len() / 36;
        let mut saddled = horse_at(4, 0.0, 4);
        let plain = boxes(&saddled);
        saddled.ridden = true;
        assert_eq!(boxes(&saddled), plain + 2);
        assert_eq!(plain, boxes(&calm) + 10);
    }

    /// Renders horses at rest, walking, rearing under a rider, grazing, a foal
    /// and a donkey with chests. Set `UMICRAFT_HORSE_DUMP` to a PNG path.
    #[test]
    #[ignore = "requires the official 1.8.8 client jar and a GPU adapter"]
    fn horses_in_the_world() {
        use crate::entity_render::EntityTextures;
        use umicraft_render::{Camera, Overlay, Renderer};

        let mut renderer = Renderer::new_offscreen(900, 340).expect("renderer");
        let mut walking = horse_at(2, -3.0, 4);
        walking.limb_swing = 2.0;
        walking.limb_swing_amount = 1.0;
        let mut rearing = horse_at(3, 0.0, 4 | 64);
        rearing.ridden = true;
        rearing.horse = Some(HorsePose {
            rearing: 1.0,
            mouth: 1.0,
            ..HorsePose::default()
        });
        let mut grazing = horse_at(4, 3.0, 0);
        grazing.horse = Some(HorsePose {
            grazing: 1.0,
            tail_swinging: true,
            ..HorsePose::default()
        });
        grazing.age = 3.0;
        let mut foal = horse_at(5, 5.5, 0);
        foal.meta.baby = true;
        let mut donkey = horse_at(6, 7.5, 8);
        donkey.meta.variant = Variant::Horse(HorseMeta {
            kind: 1,
            flags: 8,
            ..HorseMeta::default()
        });
        let mut snapshots = [
            horse_at(1, -6.0, 0),
            walking,
            rearing,
            grazing,
            foal,
            donkey,
        ];
        // Seen from the side when `UMICRAFT_HORSE_SIDE` is set: lined up along z.
        let side = std::env::var("UMICRAFT_HORSE_SIDE").is_ok();
        if side {
            for ent in &mut snapshots {
                ent.position = Vec3::new(ent.position.x * 0.55, 64.0, 0.0);
                ent.yaw = 90.0;
                ent.body_yaw = 90.0;
                ent.head_yaw = 90.0;
            }
        }
        EntityTextures::load(&renderer).mesh_and_upload(&snapshots, &mut renderer);
        let mut camera = if side {
            Camera::new(Vec3::new(0.5, 65.0, -6.5), std::f32::consts::PI, -0.05)
        } else {
            Camera::new(Vec3::new(0.5, 65.6, -9.5), std::f32::consts::PI, -0.08)
        };
        camera.set_aspect(900, 340);
        renderer
            .render(&camera, &Overlay::new(900, 340))
            .expect("render");
        let image = renderer.read_pixels().expect("readback");
        if let Ok(path) = std::env::var("UMICRAFT_HORSE_DUMP") {
            let file = std::fs::File::create(path).expect("dump file");
            let mut encoder = png::Encoder::new(file, image.width, image.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .and_then(|mut writer| writer.write_image_data(&image.rgba))
                .expect("png");
        }
    }
}
