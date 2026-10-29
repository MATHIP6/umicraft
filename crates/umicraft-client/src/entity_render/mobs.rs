// SPDX-License-Identifier: MIT OR Apache-2.0

//! Mesh builders for every mob, grouped by model family.
//!
//! Each family function returns `None` for kinds it does not own, so
//! [`build_entity_layers`] chains them. Each layer is tagged with a
//! [`TextureSlot`] so the batching loop routes vertices to the right texture.

use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::{
    BatModel, BlazeModel, ChickenModel, CreeperModel, DragonModel, EndermanModel, EndermiteModel,
    GhastModel, GuardianModel, HorseModel, HumanoidModel, IronGolemModel, MeshLayer, OcelotModel,
    QuadrupedModel, RabbitModel, SheepModel, SilverfishModel, SlimeModel, SnowmanModel,
    SpiderModel, SquidModel, TextureSlot, VillagerModel, WitherModel, WolfModel,
};

use super::player::posed_player;

/// The animation inputs every model's `set_angles` takes.
#[derive(Clone, Copy)]
struct Pose {
    swing: f32,
    amount: f32,
    /// Head yaw relative to the body, in degrees.
    head_yaw: f32,
    head_pitch: f32,
}

impl Pose {
    fn of(ent: &EntityRenderSnapshot) -> Self {
        Self {
            swing: ent.limb_swing,
            amount: ent.limb_swing_amount,
            head_yaw: ent.head_yaw - ent.body_yaw,
            head_pitch: ent.pitch,
        }
    }
}

/// A single-layer mesh on `slot`.
fn one(slot: TextureSlot, vertices: Vec<umicraft_model::entity::ModelVertex>) -> Vec<MeshLayer> {
    vec![MeshLayer { slot, vertices }]
}

/// Builds mesh layers for a single entity snapshot (empty for kinds with no
/// entity model, such as dropped items).
pub(super) fn build_entity_layers(ent: &EntityRenderSnapshot) -> Vec<MeshLayer> {
    let pose = Pose::of(ent);
    humanoid_layers(ent, pose)
        .or_else(|| animal_layers(ent.kind, pose))
        .or_else(|| creature_layers(ent.kind, pose))
        .unwrap_or_default()
}

/// Two-legged, two-armed models.
fn humanoid_layers(ent: &EntityRenderSnapshot, pose: Pose) -> Option<Vec<MeshLayer>> {
    if ent.kind == EntityKind::Player {
        let (model, is_slim) = posed_player(ent);
        let slot = if is_slim {
            TextureSlot::Alex
        } else {
            TextureSlot::Steve
        };
        return Some(one(slot, model.emit_mesh()));
    }
    let (mut model, slot, arms_raised, scale) = match ent.kind {
        EntityKind::Zombie => (HumanoidModel::zombie(), TextureSlot::Zombie, true, 1.0),
        EntityKind::Skeleton => (HumanoidModel::skeleton(), TextureSlot::Skeleton, false, 1.0),
        EntityKind::ZombiePigman => (
            HumanoidModel::zombie(),
            TextureSlot::ZombiePigman,
            true,
            1.0,
        ),
        EntityKind::Giant => (HumanoidModel::zombie(), TextureSlot::Zombie, true, 6.0),
        EntityKind::ZombieVillager => (
            HumanoidModel::zombie_villager(),
            TextureSlot::ZombieVillager,
            true,
            1.0,
        ),
        _ => return None,
    };
    model.set_angles(
        pose.swing,
        pose.amount,
        pose.head_yaw,
        pose.head_pitch,
        arms_raised,
    );
    let mut vertices = model.emit_mesh();
    if scale != 1.0 {
        for vertex in &mut vertices {
            vertex.position = vertex.position.map(|c| c * scale);
        }
    }
    Some(one(slot, vertices))
}

/// Animals, villagers and the other limb-swinging ground mobs.
fn animal_layers(kind: EntityKind, pose: Pose) -> Option<Vec<MeshLayer>> {
    let Pose {
        swing,
        amount,
        head_yaw,
        head_pitch,
    } = pose;
    macro_rules! walker {
        ($model:expr, $slot:expr) => {{
            let mut model = $model;
            model.set_angles(swing, amount, head_yaw, head_pitch);
            one($slot, model.emit_mesh())
        }};
    }
    Some(match kind {
        EntityKind::Creeper => walker!(CreeperModel::new(), TextureSlot::Creeper),
        EntityKind::Cow => walker!(QuadrupedModel::cow(), TextureSlot::Cow),
        EntityKind::Mooshroom => walker!(QuadrupedModel::cow(), TextureSlot::Mooshroom),
        EntityKind::Pig => walker!(QuadrupedModel::pig(), TextureSlot::Pig),
        EntityKind::Chicken => walker!(ChickenModel::new(), TextureSlot::Chicken),
        EntityKind::Spider => walker!(SpiderModel::new(), TextureSlot::Spider),
        EntityKind::CaveSpider => walker!(SpiderModel::with_scale(0.7), TextureSlot::CaveSpider),
        EntityKind::Enderman => walker!(EndermanModel::new(), TextureSlot::Enderman),
        EntityKind::Villager => walker!(VillagerModel::villager(), TextureSlot::Villager),
        EntityKind::Witch => walker!(VillagerModel::witch(), TextureSlot::Witch),
        EntityKind::IronGolem => walker!(IronGolemModel::new(), TextureSlot::IronGolem),
        EntityKind::Wolf => walker!(WolfModel::new(), TextureSlot::Wolf),
        EntityKind::Horse => walker!(HorseModel::new(), TextureSlot::Horse),
        EntityKind::Ocelot => walker!(OcelotModel::new(), TextureSlot::Ocelot),
        EntityKind::Rabbit => walker!(RabbitModel::new(), TextureSlot::Rabbit),
        EntityKind::Sheep => {
            let mut model = SheepModel::new();
            model.set_angles(swing, amount, head_yaw, head_pitch);
            vec![
                MeshLayer {
                    slot: TextureSlot::Sheep,
                    vertices: model.emit_body_mesh(),
                },
                MeshLayer {
                    slot: TextureSlot::SheepWool,
                    vertices: model.emit_wool_mesh(),
                },
            ]
        }
        _ => return None,
    })
}

/// Slimes, flyers, swimmers and the other models driven by fewer inputs.
fn creature_layers(kind: EntityKind, pose: Pose) -> Option<Vec<MeshLayer>> {
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
            one(TextureSlot::Slime, vertices)
        }
        EntityKind::MagmaCube => one(
            TextureSlot::MagmaCube,
            SlimeModel::magma_cube(1.0).emit_inner_mesh(),
        ),
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
            model.set_angles(swing);
            one(TextureSlot::Ghast, model.emit_mesh())
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
