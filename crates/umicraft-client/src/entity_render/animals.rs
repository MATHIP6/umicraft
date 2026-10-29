// SPDX-License-Identifier: MIT OR Apache-2.0

//! Animals and villagers: the ground mobs that have babies and coats.

use umicraft_game::entity::meta::Variant;
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::child::scale_mesh;
use umicraft_model::entity::{
    ChickenModel, EndermanModel, HorseModel, IronGolemModel, OcelotModel, OcelotPose,
    QuadrupedModel, RabbitModel, SheepModel, SpiderModel, TextureSlot, VillagerModel, WolfModel,
};

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
/// Scale of a foal; the vanilla model shrinks its body part by part.
const FOAL_SCALE: f32 = 0.5;

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
        EntityKind::Spider => walker!(SpiderModel::new(), TextureSlot::Spider, 1.0, 1.0),
        EntityKind::CaveSpider => walker!(
            SpiderModel::with_scale(0.7),
            TextureSlot::CaveSpider,
            1.0,
            1.0
        ),
        EntityKind::Enderman => {
            let mut model = EndermanModel::new();
            if let Variant::Enderman(enderman) = &ent.meta.variant {
                model.is_carrying = enderman.carried & 0x0FFF != 0;
                model.is_attacking = enderman.screaming;
            }
            model.set_angles(swing, amount, head_yaw, head_pitch);
            one(TextureSlot::Enderman, model.emit_mesh())
        }
        EntityKind::IronGolem => walker!(IronGolemModel::new(), TextureSlot::IronGolem, 1.0, 1.0),
        _ => return None,
    })
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
    let long_ears =
        matches!(&ent.meta.variant, Variant::Horse(horse) if matches!(horse.kind, 1 | 2));
    let mut model = if long_ears {
        HorseModel::mule()
    } else {
        HorseModel::new()
    };
    model.set_angles(pose.swing, pose.amount, pose.head_yaw, pose.head_pitch);
    let mut mesh = model.emit_mesh();
    if let (Variant::Horse(horse), false) = (&ent.meta.variant, ent.meta.baby) {
        mesh.extend(model.emit_gear(horse.saddled(), horse.chested()));
    }
    let (slot, scale) = match ent.meta.variant {
        Variant::Horse(horse) => (
            variants::horse(horse),
            match horse.kind {
                1 => DONKEY_SCALE,
                2 => MULE_SCALE,
                _ => 1.0,
            },
        ),
        _ => (TextureSlot::Horse, 1.0),
    };
    let young = if ent.meta.baby { FOAL_SCALE } else { 1.0 };
    scale_mesh(&mut mesh, scale * young);
    one(slot, mesh)
}
