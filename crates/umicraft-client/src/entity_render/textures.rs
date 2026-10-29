// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entity texture registry: the jar paths, the procedural fallbacks and the
//! GPU textures keyed by [`TextureSlot`].
//!
//! Adding a new mob texture = one new entry in [`texture_specs`].

use std::collections::HashMap;

use glam::Mat4;

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
use umicraft_game::entity::EntityRenderSnapshot;
use umicraft_game::hand::HandPose;
use umicraft_model::entity::TextureSlot;
use umicraft_render::{EntityTexture, Renderer};

use super::hand::hand_vertices;
use super::mesh_entities;

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
