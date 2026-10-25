// SPDX-License-Identifier: MIT OR Apache-2.0

//! Active entity textures and live mesh generator for the render thread.

use glam::{Mat4, Vec3};
use umicraft_assets::client_jar::ClientJar;
use umicraft_assets::skin::{
    decode_png_bytes, procedural_cow_texture, procedural_fallback_skin, procedural_zombie_skin,
    SkinModel,
};
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_model::entity::{emit_block_item_cube, HumanoidModel, ModelVertex, QuadrupedModel};
use umicraft_render::{EntityTexture, EntityVertex, Renderer};

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

/// Holds GPU textures for all common entity kinds.
pub struct EntityTextures {
    /// Steve player skin texture.
    pub steve: EntityTexture,
    /// Alex slim player skin texture.
    pub alex: EntityTexture,
    /// Zombie mob texture.
    pub zombie: EntityTexture,
    /// Cow quadruped texture.
    pub cow: EntityTexture,
    /// Dropped item block texture.
    pub item: EntityTexture,
}

impl EntityTextures {
    /// Loads the textures using the verified jar if present, falling back to clean-room synthetics.
    #[must_use]
    pub fn load(renderer: &Renderer) -> Self {
        let verified_jar = crate::official::find_verified_jar().ok();

        let (steve_rgba, sw, sh) = load_or_fallback(
            verified_jar.as_ref(),
            "assets/minecraft/textures/entity/steve.png",
            || (procedural_fallback_skin(SkinModel::Classic), 64, 64),
        );
        let steve = renderer.create_entity_texture(&steve_rgba, sw, sh);

        let (alex_rgba, aw, ah) = load_or_fallback(
            verified_jar.as_ref(),
            "assets/minecraft/textures/entity/alex.png",
            || (procedural_fallback_skin(SkinModel::Slim), 64, 64),
        );
        let alex = renderer.create_entity_texture(&alex_rgba, aw, ah);

        let (zombie_rgba, zw, zh) = load_or_fallback(
            verified_jar.as_ref(),
            "assets/minecraft/textures/entity/zombie/zombie.png",
            || (procedural_zombie_skin(), 64, 64),
        );
        let zombie = renderer.create_entity_texture(&zombie_rgba, zw, zh);

        let (cow_rgba, cw, ch) = load_or_fallback(
            verified_jar.as_ref(),
            "assets/minecraft/textures/entity/cow/cow.png",
            || (procedural_cow_texture(), 64, 32),
        );
        let cow = renderer.create_entity_texture(&cow_rgba, cw, ch);

        let (item_rgba, iw, ih) = load_or_fallback(
            verified_jar.as_ref(),
            "assets/minecraft/textures/blocks/dirt.png",
            || (procedural_dirt_texture(), 16, 16),
        );
        let item = renderer.create_entity_texture(&item_rgba, iw, ih);

        Self {
            steve,
            alex,
            zombie,
            cow,
            item,
        }
    }

    /// Meshes all visible entities in `snapshots` and uploads them to the GPU renderer.
    pub fn mesh_and_upload(&self, snapshots: &[EntityRenderSnapshot], renderer: &mut Renderer) {
        let batches = mesh_entities(snapshots);
        renderer.upload_entities(&[
            (&batches.steve, self.steve.bind_group()),
            (&batches.alex, self.alex.bind_group()),
            (&batches.zombie, self.zombie.bind_group()),
            (&batches.cow, self.cow.bind_group()),
            (&batches.item, self.item.bind_group()),
        ]);
    }
}

/// Vertex batches for each entity texture bind group.
#[derive(Debug, Default, Clone)]
pub struct EntityBatches {
    /// Classic Steve vertices.
    pub steve: Vec<EntityVertex>,
    /// Slim Alex vertices.
    pub alex: Vec<EntityVertex>,
    /// Zombie/biped vertices.
    pub zombie: Vec<EntityVertex>,
    /// Cow/quadruped vertices.
    pub cow: Vec<EntityVertex>,
    /// Dropped item vertices.
    pub item: Vec<EntityVertex>,
}

/// Meshes entity snapshots into vertex batches grouped by texture type.
#[must_use]
pub fn mesh_entities(snapshots: &[EntityRenderSnapshot]) -> EntityBatches {
    let mut batches = EntityBatches::default();

    for ent in snapshots {
        let world_pos = ent.position;
        let yaw_rad = ent.yaw.to_radians();
        let rot = Mat4::from_rotation_y(std::f32::consts::PI - yaw_rad);

        match ent.kind {
            EntityKind::Player => {
                let is_slim = ent
                    .uuid
                    .is_some_and(|u| SkinModel::default_for_uuid(u).is_slim());
                let mut model = HumanoidModel::new(is_slim);
                let head_yaw = (ent.head_yaw - ent.yaw).to_radians();
                let head_pitch = ent.pitch.to_radians();
                model.set_angles(
                    ent.limb_swing,
                    ent.limb_swing_amount,
                    head_yaw,
                    head_pitch,
                    ent.flags.is_sneaking(),
                );
                let mesh = model.emit_mesh();
                let target = if is_slim {
                    &mut batches.alex
                } else {
                    &mut batches.steve
                };
                for mv in mesh {
                    target.push(transform_vertex(mv, rot, world_pos));
                }
            }
            EntityKind::Zombie | EntityKind::Skeleton => {
                let mut model = HumanoidModel::biped(true);
                let head_yaw = (ent.head_yaw - ent.yaw).to_radians();
                let head_pitch = ent.pitch.to_radians();
                model.set_angles(
                    ent.limb_swing,
                    ent.limb_swing_amount,
                    head_yaw,
                    head_pitch,
                    true,
                );
                let mesh = model.emit_mesh();
                for mv in mesh {
                    batches.zombie.push(transform_vertex(mv, rot, world_pos));
                }
            }
            EntityKind::Cow | EntityKind::Pig | EntityKind::Sheep => {
                let mut model = QuadrupedModel::cow();
                let head_yaw = (ent.head_yaw - ent.yaw).to_radians();
                let head_pitch = ent.pitch.to_radians();
                model.set_angles(ent.limb_swing, ent.limb_swing_amount, head_yaw, head_pitch);
                let mesh = model.emit_mesh();
                for mv in mesh {
                    batches.cow.push(transform_vertex(mv, rot, world_pos));
                }
            }
            EntityKind::Item => {
                let item_rot = rot * Mat4::from_scale(Vec3::splat(1.0 / 16.0));
                let mut mesh = Vec::new();
                emit_block_item_cube(2.0, item_rot, &mut mesh);
                for mv in mesh {
                    batches.item.push(EntityVertex {
                        position: [
                            mv.position[0] + world_pos.x,
                            mv.position[1] + world_pos.y,
                            mv.position[2] + world_pos.z,
                        ],
                        normal: mv.normal,
                        uv: mv.uv,
                        light: 0x00FF_00FF,
                    });
                }
            }
            _ => {}
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
        assert!(batches.steve.is_empty());
        assert!(batches.alex.is_empty());
        assert!(batches.zombie.is_empty());
        assert!(batches.cow.is_empty());
        assert!(batches.item.is_empty());
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
                uuid: Some([0; 16]), // Steve default
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
        assert_eq!(batches.steve.len(), 432);
        assert_eq!(batches.zombie.len(), 432);
        assert_eq!(batches.cow.len(), 324);
        assert_eq!(batches.item.len(), 36);
        assert!(batches.alex.is_empty());
    }
}
