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
    decode_png_bytes, procedural_chicken_texture, procedural_cow_texture,
    procedural_creeper_texture, procedural_fallback_skin, procedural_pig_texture,
    procedural_sheep_fur_texture, procedural_sheep_texture, procedural_skeleton_texture,
    procedural_zombie_skin, SkinModel,
};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::{
    emit_block_item_cube, ChickenModel, CreeperModel, HumanoidModel, MeshLayer, ModelVertex,
    QuadrupedModel, SheepModel, TextureSlot,
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
            slot: TextureSlot::Item,
            jar_path: "assets/minecraft/textures/blocks/dirt.png",
            fallback: || (procedural_dirt_texture(), 16, 16),
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

/// Builds mesh layers for a single entity snapshot.
///
/// Each layer is tagged with a [`TextureSlot`] so the batching loop can route
/// vertices to the correct GPU texture. Adding a new mob = one new match arm.
fn build_entity_layers(ent: &EntityRenderSnapshot) -> Vec<MeshLayer> {
    let head_yaw = ent.head_yaw - ent.yaw;
    let head_pitch = ent.pitch;

    match ent.kind {
        EntityKind::Player => {
            let is_slim = ent
                .uuid
                .is_some_and(|u| SkinModel::default_for_uuid(u).is_slim());
            let mut model = HumanoidModel::new(is_slim);
            model.set_angles(
                ent.limb_swing,
                ent.limb_swing_amount,
                head_yaw,
                head_pitch,
                false,
            );
            if ent.flags.is_sneaking() {
                model.apply_sneaking();
            }
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
                true,
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
        EntityKind::Item => {
            let mut verts = Vec::new();
            emit_block_item_cube(2.0, Mat4::IDENTITY, &mut verts);
            vec![MeshLayer {
                slot: TextureSlot::Item,
                vertices: verts,
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
        let rot = Mat4::from_rotation_y(std::f32::consts::PI - yaw_rad);

        let sneaking = ent.kind == EntityKind::Player && ent.flags.is_sneaking();
        let pos = if sneaking {
            Vec3::new(world_pos.x, world_pos.y + 0.2, world_pos.z)
        } else {
            world_pos
        };

        let is_item = ent.kind == EntityKind::Item;

        for layer in build_entity_layers(ent) {
            let batch = batches.entry(layer.slot).or_default();
            if is_item {
                let item_rot = rot * Mat4::from_scale(Vec3::splat(1.0 / 16.0));
                for mv in layer.vertices {
                    let p = item_rot.transform_point3(Vec3::from(mv.position));
                    let n = item_rot.transform_vector3(Vec3::from(mv.normal));
                    batch.push(EntityVertex {
                        position: [p.x + pos.x, p.y + pos.y, p.z + pos.z],
                        normal: n.to_array(),
                        uv: mv.uv,
                        light: 0x00FF_00FF,
                    });
                }
            } else {
                for mv in layer.vertices {
                    batch.push(transform_vertex(mv, rot, pos));
                }
            }
        }
    }

    batches
}

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
                held_item: None,
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
                held_item: None,
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
                held_item: None,
                flags: EntityFlags::default(),
            },
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
                held_item: None,
                flags: EntityFlags::default(),
            },
        ];

        let batches = mesh_entities(&snapshots);
        assert_eq!(batches[&TextureSlot::Steve].len(), 432);
        assert_eq!(batches[&TextureSlot::Zombie].len(), 432);
        assert_eq!(batches[&TextureSlot::Cow].len(), 324);
        assert_eq!(batches[&TextureSlot::Item].len(), 36);
        assert!(batches.get(&TextureSlot::Alex).is_none());
    }
}
