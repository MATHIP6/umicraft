// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entity texture registry and mesh generator.
//!
//! Adding a new mob requires three steps:
//! 1. Add a [`TextureSlot`] variant in `umicraft-model`.
//! 2. Add an entry to [`TEXTURE_SPECS`] with the jar path and procedural fallback.
//! 3. Add a match arm in [`build_entity_layers`] that creates the model and returns layers.

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::skin::{
    decode_png_bytes, procedural_bat_texture, procedural_blaze_texture,
    procedural_cave_spider_texture, procedural_chicken_texture, procedural_cow_texture,
    procedural_creeper_texture, procedural_dragon_texture, procedural_enderman_texture,
    procedural_endermite_texture, procedural_fallback_skin, procedural_ghast_texture,
    procedural_guardian_texture, procedural_horse_texture, procedural_iron_golem_texture,
    procedural_magma_cube_texture, procedural_mooshroom_texture, procedural_ocelot_texture,
    procedural_pig_texture, procedural_rabbit_texture, procedural_sheep_fur_texture,
    procedural_sheep_texture, procedural_silverfish_texture, procedural_skeleton_texture,
    procedural_slime_texture, procedural_snowman_texture, procedural_spider_texture,
    procedural_squid_texture, procedural_villager_texture, procedural_witch_texture,
    procedural_wither_texture, procedural_wolf_texture, procedural_zombie_pigman_texture,
    procedural_zombie_skin, procedural_zombie_villager_texture, SkinModel,
};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_game::hand::HandPose;
use umicraft_model::entity::{
    BatModel, BlazeModel, ChickenModel, CreeperModel, DragonModel, EndermanModel, EndermiteModel,
    GhastModel, GuardianModel, HorseModel, HumanoidModel, IronGolemModel, MeshLayer, ModelVertex,
    OcelotModel, QuadrupedModel, RabbitModel, SheepModel, SilverfishModel, SlimeModel,
    SnowmanModel, SpiderModel, SquidModel, TextureSlot, VillagerModel, WitherModel, WolfModel,
};
use umicraft_render::{EntityTexture, EntityVertex, Renderer};

// ---------------------------------------------------------------------------
// Texture loading
// ---------------------------------------------------------------------------

/// Specification for one entity texture: jar path + procedural fallback.
struct TextureSpec {
    slot: TextureSlot,
    jar_path: &'static str,
    fallback: fn() -> (Vec<u8>, u32, u32),
}

/// Central texture registry. Adding a new mob texture = one new entry here.
fn texture_specs() -> Vec<TextureSpec> {
    vec![
        TextureSpec {
            slot: TextureSlot::Steve,
            jar_path: "assets/minecraft/textures/entity/steve.png",
            fallback: || (procedural_fallback_skin(SkinModel::Classic), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::Alex,
            jar_path: "assets/minecraft/textures/entity/alex.png",
            fallback: || (procedural_fallback_skin(SkinModel::Slim), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::Zombie,
            jar_path: "assets/minecraft/textures/entity/zombie/zombie.png",
            fallback: || (procedural_zombie_skin(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::Skeleton,
            jar_path: "assets/minecraft/textures/entity/skeleton/skeleton.png",
            fallback: || (procedural_skeleton_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Creeper,
            jar_path: "assets/minecraft/textures/entity/creeper/creeper.png",
            fallback: || (procedural_creeper_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Cow,
            jar_path: "assets/minecraft/textures/entity/cow/cow.png",
            fallback: || (procedural_cow_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Pig,
            jar_path: "assets/minecraft/textures/entity/pig/pig.png",
            fallback: || (procedural_pig_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Sheep,
            jar_path: "assets/minecraft/textures/entity/sheep/sheep.png",
            fallback: || (procedural_sheep_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::SheepWool,
            jar_path: "assets/minecraft/textures/entity/sheep/sheep_fur.png",
            fallback: || (procedural_sheep_fur_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Chicken,
            jar_path: "assets/minecraft/textures/entity/chicken.png",
            fallback: || (procedural_chicken_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Spider,
            jar_path: "assets/minecraft/textures/entity/spider/spider.png",
            fallback: || (procedural_spider_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::CaveSpider,
            jar_path: "assets/minecraft/textures/entity/spider/cave_spider.png",
            fallback: || (procedural_cave_spider_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Enderman,
            jar_path: "assets/minecraft/textures/entity/enderman/enderman.png",
            fallback: || (procedural_enderman_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Slime,
            jar_path: "assets/minecraft/textures/entity/slime/slime.png",
            fallback: || (procedural_slime_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::MagmaCube,
            jar_path: "assets/minecraft/textures/entity/slime/magmacube.png",
            fallback: || (procedural_magma_cube_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::ZombiePigman,
            jar_path: "assets/minecraft/textures/entity/zombie_pigman.png",
            fallback: || (procedural_zombie_pigman_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Villager,
            jar_path: "assets/minecraft/textures/entity/villager/farmer.png",
            fallback: || (procedural_villager_texture(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::ZombieVillager,
            jar_path: "assets/minecraft/textures/entity/zombie/zombie_villager.png",
            fallback: || (procedural_zombie_villager_texture(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::Witch,
            jar_path: "assets/minecraft/textures/entity/witch.png",
            fallback: || (procedural_witch_texture(), 64, 128),
        },
        TextureSpec {
            slot: TextureSlot::IronGolem,
            jar_path: "assets/minecraft/textures/entity/iron_golem.png",
            fallback: || (procedural_iron_golem_texture(), 128, 128),
        },
        TextureSpec {
            slot: TextureSlot::Snowman,
            jar_path: "assets/minecraft/textures/entity/snowman.png",
            fallback: || (procedural_snowman_texture(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::Wolf,
            jar_path: "assets/minecraft/textures/entity/wolf/wolf.png",
            fallback: || (procedural_wolf_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Mooshroom,
            jar_path: "assets/minecraft/textures/entity/cow/mooshroom.png",
            fallback: || (procedural_mooshroom_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Squid,
            jar_path: "assets/minecraft/textures/entity/squid.png",
            fallback: || (procedural_squid_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Bat,
            jar_path: "assets/minecraft/textures/entity/bat.png",
            fallback: || (procedural_bat_texture(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::Blaze,
            jar_path: "assets/minecraft/textures/entity/blaze.png",
            fallback: || (procedural_blaze_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Dragon,
            jar_path: "assets/minecraft/textures/entity/enderdragon/dragon.png",
            fallback: || (procedural_dragon_texture(), 256, 256),
        },
        TextureSpec {
            slot: TextureSlot::Endermite,
            jar_path: "assets/minecraft/textures/entity/endermite.png",
            fallback: || (procedural_endermite_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Ghast,
            jar_path: "assets/minecraft/textures/entity/ghast/ghast.png",
            fallback: || (procedural_ghast_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Guardian,
            jar_path: "assets/minecraft/textures/entity/guardian.png",
            fallback: || (procedural_guardian_texture(), 64, 64),
        },
        TextureSpec {
            slot: TextureSlot::Horse,
            jar_path: "assets/minecraft/textures/entity/horse/horse_white.png",
            fallback: || (procedural_horse_texture(), 128, 128),
        },
        TextureSpec {
            slot: TextureSlot::Ocelot,
            jar_path: "assets/minecraft/textures/entity/cat/ocelot.png",
            fallback: || (procedural_ocelot_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Rabbit,
            jar_path: "assets/minecraft/textures/entity/rabbit/brown.png",
            fallback: || (procedural_rabbit_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Silverfish,
            jar_path: "assets/minecraft/textures/entity/silverfish.png",
            fallback: || (procedural_silverfish_texture(), 64, 32),
        },
        TextureSpec {
            slot: TextureSlot::Wither,
            jar_path: "assets/minecraft/textures/entity/wither/wither.png",
            fallback: || (procedural_wither_texture(), 64, 64),
        },
    ]
}

/// Loads official textures from the jar if available, or falls back to procedural assets.
pub fn load_or_fallback<F>(
    jar: Option<&ClientJar>,
    jar_path: &str,
    fallback_fn: F,
) -> (Vec<u8>, u32, u32)
where
    F: FnOnce() -> (Vec<u8>, u32, u32),
{
    if let Some(jar) = jar {
        if let Ok(bytes) = jar.read_entry(jar_path) {
            if let Ok(decoded) = decode_png_bytes(&bytes) {
                return decoded;
            }
        }
    }
    fallback_fn()
}

/// Procedural dirt texture (16×16) used as a clean-room fallback for dropped blocks.
#[must_use]
pub fn procedural_dirt_texture() -> Vec<u8> {
    let mut rgba = vec![0u8; 16 * 16 * 4];
    for y in 0..16 {
        for x in 0..16 {
            let idx = (y * 16 + x) * 4;
            let hash = ((x * 37 + y * 17) ^ ((x << 3) + y)) % 256;
            let shade = (hash % 30) as u8 + 110;
            rgba[idx] = (f32::from(shade) * 0.65) as u8;
            rgba[idx + 1] = (f32::from(shade) * 0.45) as u8;
            rgba[idx + 2] = (f32::from(shade) * 0.28) as u8;
            rgba[idx + 3] = 255;
        }
    }
    rgba
}

// ---------------------------------------------------------------------------
// Texture registry
// ---------------------------------------------------------------------------

/// GPU texture registry keyed by [`TextureSlot`].
///
/// Adding a new mob = add one entry in [`texture_specs`].
pub struct EntityTextures {
    textures: HashMap<TextureSlot, EntityTexture>,
}

impl EntityTextures {
    /// Loads all entity textures from the verified jar or procedural fallbacks.
    #[must_use]
    pub fn load(renderer: &Renderer) -> Self {
        let verified_jar = crate::official::find_verified_jar().ok();
        let mut textures = HashMap::new();
        for spec in texture_specs() {
            let (rgba, w, h) =
                load_or_fallback(verified_jar.as_ref(), spec.jar_path, spec.fallback);
            textures.insert(spec.slot, renderer.create_entity_texture(&rgba, w, h));
        }
        Self { textures }
    }

    /// Uploads the first-person hand, or hides it when `pose` shows no arm.
    ///
    /// The arm wears the default skin until the local profile is known (the
    /// same one the third-person body wears).
    pub fn upload_hand(&self, pose: &HandPose, bob: Mat4, renderer: &mut Renderer) {
        let vertices = hand_vertices(pose, bob, false);
        match (vertices.is_empty(), self.textures.get(&TextureSlot::Steve)) {
            (false, Some(skin)) => renderer.set_hand(&vertices, skin.bind_group()),
            _ => renderer.clear_hand(),
        }
    }

    /// Meshes all visible entities and uploads them to the GPU renderer.
    pub fn mesh_and_upload(&self, snapshots: &[EntityRenderSnapshot], renderer: &mut Renderer) {
        let batches = mesh_entities(snapshots);
        let pairs: Vec<_> = batches
            .iter()
            .filter_map(|(slot, verts)| {
                if verts.is_empty() {
                    return None;
                }
                self.textures
                    .get(slot)
                    .map(|tex| (verts.as_slice(), tex.bind_group()))
            })
            .collect();
        renderer.upload_entities(&pairs);
    }
}

// ---------------------------------------------------------------------------
// Per-entity mesh building
// ---------------------------------------------------------------------------

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

/// Builds mesh layers for a single entity snapshot.
///
/// Each layer is tagged with a [`TextureSlot`] so the batching loop can route
/// vertices to the correct GPU texture. Adding a new mob = one new match arm.
fn build_entity_layers(ent: &EntityRenderSnapshot) -> Vec<MeshLayer> {
    let head_yaw = ent.head_yaw - ent.yaw;
    let head_pitch = ent.pitch;

    match ent.kind {
        EntityKind::Player => {
            let (model, is_slim) = posed_player(ent);
            let slot = if is_slim {
                TextureSlot::Alex
            } else {
                TextureSlot::Steve
            };
            vec![MeshLayer {
                slot,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Zombie => {
            let mut model = HumanoidModel::zombie();
            model.set_angles(
                ent.limb_swing,
                ent.limb_swing_amount,
                head_yaw,
                head_pitch,
                true,
            );
            vec![MeshLayer {
                slot: TextureSlot::Zombie,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Skeleton => {
            let mut model = HumanoidModel::skeleton();
            model.set_angles(
                ent.limb_swing,
                ent.limb_swing_amount,
                head_yaw,
                head_pitch,
                false,
            );
            vec![MeshLayer {
                slot: TextureSlot::Skeleton,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Creeper => {
            let mut model = CreeperModel::new();
            model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::Creeper,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Cow => {
            let mut model = QuadrupedModel::cow();
            model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::Cow,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Sheep => {
            let mut model = SheepModel::new();
            model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
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
        EntityKind::Pig => {
            let mut model = QuadrupedModel::pig();
            model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::Pig,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Chicken => {
            let mut model = ChickenModel::new();
            model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::Chicken,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Spider => {
            let mut model = SpiderModel::new();
            model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::Spider,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::CaveSpider => {
            let mut model = SpiderModel::with_scale(0.7);
            model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::CaveSpider,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Enderman => {
            let mut model = EndermanModel::new();
            model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::Enderman,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Slime => {
            let model = SlimeModel::slime(1.0);
            let mut verts = model.emit_inner_mesh();
            verts.extend(model.emit_outer_mesh());
            vec![MeshLayer {
                slot: TextureSlot::Slime,
                vertices: verts,
            }]
        }
        EntityKind::MagmaCube => {
            let model = SlimeModel::magma_cube(1.0);
            vec![MeshLayer {
                slot: TextureSlot::MagmaCube,
                vertices: model.emit_inner_mesh(),
            }]
        }
        EntityKind::ZombiePigman => {
            let mut model = HumanoidModel::zombie();
            model.set_angles(
                ent.limb_swing,
                ent.limb_swing_amount,
                head_yaw,
                head_pitch,
                true,
            );
            vec![MeshLayer {
                slot: TextureSlot::ZombiePigman,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Giant => {
            let mut model = HumanoidModel::zombie();
            model.set_angles(
                ent.limb_swing,
                ent.limb_swing_amount,
                head_yaw,
                head_pitch,
                true,
            );
            let mut verts = model.emit_mesh();
            for v in &mut verts {
                v.position[0] *= 6.0;
                v.position[1] *= 6.0;
                v.position[2] *= 6.0;
            }
            vec![MeshLayer {
                slot: TextureSlot::Zombie,
                vertices: verts,
            }]
        }
        EntityKind::Villager => {
            let mut model = VillagerModel::villager();
            model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::Villager,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::ZombieVillager => {
            let mut model = HumanoidModel::zombie_villager();
            model.set_angles(
                ent.limb_swing,
                ent.limb_swing_amount,
                head_yaw,
                head_pitch,
                true,
            );
            vec![MeshLayer {
                slot: TextureSlot::ZombieVillager,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Witch => {
            let mut model = VillagerModel::witch();
            model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::Witch,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::IronGolem => {
            let mut model = IronGolemModel::new();
            model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::IronGolem,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Snowman => {
            let mut model = SnowmanModel::new();
            model.set_angles(head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::Snowman,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Wolf => {
            let mut model = WolfModel::new();
            model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::Wolf,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Mooshroom => {
            let mut model = QuadrupedModel::cow();
            model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::Mooshroom,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Squid => {
            let mut model = SquidModel::new();
            model.set_angles(ent.limb_swing);
            vec![MeshLayer {
                slot: TextureSlot::Squid,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Bat => {
            let mut model = BatModel::new();
            model.set_angles(ent.limb_swing);
            vec![MeshLayer {
                slot: TextureSlot::Bat,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Blaze => {
            let mut model = BlazeModel::new();
            model.set_angles(ent.limb_swing, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::Blaze,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Dragon => {
            let mut model = DragonModel::new();
            model.set_angles(ent.limb_swing);
            vec![MeshLayer {
                slot: TextureSlot::Dragon,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Endermite => {
            let mut model = EndermiteModel::new();
            model.set_angles(ent.limb_swing);
            vec![MeshLayer {
                slot: TextureSlot::Endermite,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Ghast => {
            let mut model = GhastModel::new();
            model.set_angles(ent.limb_swing);
            vec![MeshLayer {
                slot: TextureSlot::Ghast,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Guardian => {
            let mut model = GuardianModel::new();
            model.set_angles(ent.limb_swing);
            vec![MeshLayer {
                slot: TextureSlot::Guardian,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Horse => {
            let mut model = HorseModel::new();
            model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::Horse,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Ocelot => {
            let mut model = OcelotModel::new();
            model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::Ocelot,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Rabbit => {
            let mut model = RabbitModel::new();
            model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::Rabbit,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Silverfish => {
            let mut model = SilverfishModel::new();
            model.set_angles(ent.limb_swing);
            vec![MeshLayer {
                slot: TextureSlot::Silverfish,
                vertices: model.emit_mesh(),
            }]
        }
        EntityKind::Wither => {
            let mut model = WitherModel::new();
            model.set_angles(ent.limb_swing, head_yaw, head_pitch);
            vec![MeshLayer {
                slot: TextureSlot::Wither,
                vertices: model.emit_mesh(),
            }]
        }
        _ => vec![],
    }
}

/// Meshes entity snapshots into batched vertex lists grouped by [`TextureSlot`].
#[must_use]
pub fn mesh_entities(
    snapshots: &[EntityRenderSnapshot],
) -> HashMap<TextureSlot, Vec<EntityVertex>> {
    let mut batches: HashMap<TextureSlot, Vec<EntityVertex>> = HashMap::new();

    for ent in snapshots {
        let world_pos = ent.position;
        let yaw_rad = ent.yaw.to_radians();
        // `RendererLivingEntity.rotateCorpse`: rotate(180 - yaw). The model faces -Z,
        // and a Minecraft yaw of zero looks towards +Z.
        let rot = Mat4::from_rotation_y(std::f32::consts::PI - yaw_rad);

        let sneaking = ent.kind == EntityKind::Player && ent.flags.is_sneaking();
        let pos = if sneaking {
            Vec3::new(world_pos.x, world_pos.y + 0.2, world_pos.z)
        } else {
            world_pos
        };

        for layer in build_entity_layers(ent) {
            let batch = batches.entry(layer.slot).or_default();
            for mv in layer.vertices {
                batch.push(transform_vertex(mv, rot, pos));
            }
        }
    }

    batches
}

/// The vertices of the first-person arm in camera space.
///
/// Empty when the pose shows no arm. `bob` is the view-bobbing transform of
/// the frame (MCP applies it to the hand as well).
#[must_use]
pub fn hand_vertices(pose: &HandPose, bob: Mat4, is_slim: bool) -> Vec<EntityVertex> {
    if !pose.shows_arm {
        return Vec::new();
    }
    let to_camera = bob * pose.arm_matrix();
    HumanoidModel::first_person_arm(is_slim)
        .into_iter()
        .map(|vertex| EntityVertex {
            position: to_camera
                .transform_point3(Vec3::from(vertex.position))
                .to_array(),
            normal: to_camera
                .transform_vector3(Vec3::from(vertex.normal))
                .normalize_or_zero()
                .to_array(),
            uv: vertex.uv,
            // Full sky light, no block light: the hand darkens at night.
            light: HAND_LIGHT,
        })
        .collect()
}

/// The packed light of the hand (bits 4..=7 are the sky light).
const HAND_LIGHT: u32 = 0x0000_00F0;

fn transform_vertex(mv: ModelVertex, rot: Mat4, world_pos: Vec3) -> EntityVertex {
    let p = rot.transform_point3(Vec3::from(mv.position));
    let n = rot.transform_vector3(Vec3::from(mv.normal));
    EntityVertex {
        position: [p.x + world_pos.x, p.y + world_pos.y, p.z + world_pos.z],
        normal: n.to_array(),
        uv: mv.uv,
        light: 0x00FF_00FF,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umicraft_game::entity::{EntityFlags, EntityId};

    #[test]
    fn a_mob_with_yaw_zero_reaches_out_towards_south() {
        // Minecraft looks towards +Z at yaw 0, and a zombie holds its arms forward.
        let zombie = |yaw| EntityRenderSnapshot {
            id: EntityId(1),
            kind: EntityKind::Zombie,
            position: Vec3::ZERO,
            yaw,
            pitch: 0.0,
            head_yaw: yaw,
            limb_swing: 0.0,
            limb_swing_amount: 0.0,
            uuid: None,
            held: None,
            item: None,
            age: 0.0,
            frame_direction: 0,
            frame_rotation: 0,
            flags: EntityFlags::default(),
        };
        let reach = |yaw: f32| {
            let batches = mesh_entities(&[zombie(yaw)]);
            let z = batches.values().flatten().map(|v| v.position[2]);
            let (min, max) = z.fold((f32::MAX, f32::MIN), |(lo, hi), z| (lo.min(z), hi.max(z)));
            max + min
        };
        assert!(reach(0.0) > 0.1, "faces +Z at yaw 0");
        assert!(reach(180.0) < -0.1, "faces -Z at yaw 180");
    }

    #[test]
    fn empty_snapshots_produce_empty_batches() {
        let batches = mesh_entities(&[]);
        assert!(batches.is_empty());
    }

    #[test]
    fn live_entities_mesh_into_their_respective_batches() {
        let snapshots = [
            EntityRenderSnapshot {
                id: EntityId(1),
                kind: EntityKind::Player,
                position: Vec3::new(10.0, 64.0, 10.0),
                yaw: 0.0,
                pitch: 0.0,
                head_yaw: 0.0,
                limb_swing: 0.5,
                limb_swing_amount: 0.3,
                uuid: Some([0; 16]),
                held: None,
                item: None,
                age: 0.0,
                frame_direction: 0,
                frame_rotation: 0,
                flags: EntityFlags::default(),
            },
            EntityRenderSnapshot {
                id: EntityId(2),
                kind: EntityKind::Zombie,
                position: Vec3::new(12.0, 64.0, 10.0),
                yaw: 90.0,
                pitch: 0.0,
                head_yaw: 90.0,
                limb_swing: 0.0,
                limb_swing_amount: 0.0,
                uuid: None,
                held: None,
                item: None,
                age: 0.0,
                frame_direction: 0,
                frame_rotation: 0,
                flags: EntityFlags::default(),
            },
            EntityRenderSnapshot {
                id: EntityId(3),
                kind: EntityKind::Cow,
                position: Vec3::new(14.0, 64.0, 10.0),
                yaw: 180.0,
                pitch: 0.0,
                head_yaw: 180.0,
                limb_swing: 1.0,
                limb_swing_amount: 0.5,
                uuid: None,
                held: None,
                item: None,
                age: 0.0,
                frame_direction: 0,
                frame_rotation: 0,
                flags: EntityFlags::default(),
            },
            // A dropped item has no entity mesh: P3 draws it from its item model.
            EntityRenderSnapshot {
                id: EntityId(4),
                kind: EntityKind::Item,
                position: Vec3::new(16.0, 64.25, 10.0),
                yaw: 45.0,
                pitch: 0.0,
                head_yaw: 45.0,
                limb_swing: 0.0,
                limb_swing_amount: 0.0,
                uuid: None,
                held: None,
                item: None,
                age: 0.0,
                frame_direction: 0,
                frame_rotation: 0,
                flags: EntityFlags::default(),
            },
        ];

        let batches = mesh_entities(&snapshots);
        assert_eq!(batches[&TextureSlot::Steve].len(), 432);
        assert_eq!(batches[&TextureSlot::Zombie].len(), 432);
        assert_eq!(batches[&TextureSlot::Cow].len(), 324);
        assert_eq!(batches.len(), 3, "the dropped item adds no entity batch");
        assert!(!batches.contains_key(&TextureSlot::Alex));
    }

    fn shown() -> HandPose {
        HandPose {
            shows_arm: true,
            ..HandPose::default()
        }
    }

    #[test]
    fn a_hand_that_shows_no_arm_has_no_vertices() {
        assert!(hand_vertices(&HandPose::default(), Mat4::IDENTITY, false).is_empty());
    }

    #[test]
    fn the_resting_arm_is_in_the_lower_right_in_front_of_the_camera() {
        let vertices = hand_vertices(&shown(), Mat4::IDENTITY, false);
        assert_eq!(vertices.len(), 72);
        let count = vertices.len() as f32;
        let centre = vertices
            .iter()
            .fold(Vec3::ZERO, |sum, v| sum + Vec3::from(v.position))
            / count;
        assert!(centre.x > 0.1, "to the right: {centre:?}");
        assert!(centre.y < 0.0, "below the eyes: {centre:?}");
        assert!(centre.z < -0.3, "in front of the camera: {centre:?}");
        for vertex in &vertices {
            assert!(vertex.position[2] < 0.0, "behind the camera: {vertex:?}");
            assert_eq!(vertex.light, HAND_LIGHT);
        }
    }

    #[test]
    fn the_view_bobbing_moves_the_hand_with_the_camera() {
        let still = hand_vertices(&shown(), Mat4::IDENTITY, false);
        let bobbed = hand_vertices(
            &shown(),
            Mat4::from_translation(Vec3::new(0.0, -0.05, 0.0)),
            false,
        );
        assert!((bobbed[0].position[1] - still[0].position[1] + 0.05).abs() < 1e-5);
    }

    /// The real arm, rendered with a red skin on a software adapter, sits in
    /// the lower right of the frame and leaves the upper left alone.
    #[test]
    #[ignore = "requires a GPU adapter"]
    fn the_arm_is_drawn_in_the_lower_right_of_the_frame() {
        use umicraft_render::{Camera, Overlay};

        let (width, height) = (320, 180);
        let mut renderer = Renderer::new_offscreen(width, height).expect("offscreen renderer");
        let skin = renderer.create_entity_texture(&[255, 0, 0, 255], 1, 1);
        let vertices = hand_vertices(&shown(), Mat4::IDENTITY, false);
        renderer.set_hand(&vertices, skin.bind_group());

        let mut camera = Camera::default();
        camera.set_aspect(width, height);
        renderer
            .render(&camera, &Overlay::new(width, height))
            .expect("render the arm");
        let image = renderer.read_pixels().expect("pixels");

        let red = |x: u32, y: u32| {
            let at = ((y * width + x) * 4) as usize;
            image.rgba[at] > 60 && image.rgba[at + 1] < image.rgba[at] / 3
        };
        let count = |x0: u32, y0: u32, x1: u32, y1: u32| {
            (y0..y1)
                .flat_map(|y| (x0..x1).map(move |x| (x, y)))
                .filter(|&(x, y)| red(x, y))
                .count()
        };
        let lower_right = count(width / 2, height / 2, width, height);
        let upper_left = count(0, 0, width / 2, height / 2);
        assert!(lower_right > 400, "the arm must show: {lower_right} pixels");
        assert_eq!(upper_left, 0, "and stay out of the upper left");
    }
}
