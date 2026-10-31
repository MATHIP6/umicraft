// SPDX-License-Identifier: MIT OR Apache-2.0

//! The mob that spins in a mob spawner (MCP `TileEntityMobSpawnerRenderer`,
//! `MobSpawnerBaseLogic`).
//!
//! The cage is a block model. The server names the mob in the NBT of the
//! Update Block Entity (type 1) as `EntityId`, an old-style entity name such
//! as `Zombie` or `LavaSlime`. The renderer draws that mob at the middle of
//! the block, spun `mobRotation * 10` degrees about the vertical axis, tilted
//! back 30 degrees about the middle of its body, at `0.4375` scale.
//!
//! The mob is meshed by the ordinary entity renderer from a still snapshot at
//! the origin and then moved by the spawner's matrix, so only its main layers
//! are drawn (not glowing eyes or glints), and no shadow, as in the reference.
//!
//! Deviations: `mobRotation` advances `1000 / (spawnDelay + 200)` degrees a
//! tick while a player is within 16 blocks. The client counts the delay down
//! to 0 within seconds, so the rate here is always the settled `5` degrees a
//! tick, derived from the clock rather than integrated, and the mob also
//! spins when no player is near.

use glam::{Mat4, Vec3};
use umicraft_game::entity::{EntityId, EntityKind, EntityRenderSnapshot};
use umicraft_game::nbt::Nbt;
use umicraft_mesh::BlockEntityRef;

use super::{Batches, BlockEntityScene};
use crate::entity_render::mesh_entities;

/// The `Update Block Entity` action of a mob spawner.
const SPAWNER_ACTION: u8 = 1;
/// `mobRotation` degrees per tick once the spawn delay has run out
/// (`1000 / (0 + 200)`).
const DEGREES_PER_TICK: f64 = 5.0;
/// The renderer multiplies the rotation by 10.
const SPIN_FACTOR: f64 = 10.0;
/// Scale of the mob (`f = 0.4375`).
const MOB_SCALE: f32 = 0.437_5;
/// Game ticks per second, for the clock of the spin.
const TICKS_PER_SECOND: f64 = 20.0;

/// The mob of the 1.8 entity name `name`, `None` for a name that is not a
/// mob the client draws (`EntityList.stringToClassMapping`).
#[must_use]
fn mob_of_name(name: &str) -> Option<EntityKind> {
    Some(match name {
        "Creeper" => EntityKind::Creeper,
        "Skeleton" => EntityKind::Skeleton,
        "Spider" => EntityKind::Spider,
        "Giant" => EntityKind::Giant,
        "Zombie" => EntityKind::Zombie,
        "Slime" => EntityKind::Slime,
        "Ghast" => EntityKind::Ghast,
        "PigZombie" => EntityKind::ZombiePigman,
        "Enderman" => EntityKind::Enderman,
        "CaveSpider" => EntityKind::CaveSpider,
        "Silverfish" => EntityKind::Silverfish,
        "Blaze" => EntityKind::Blaze,
        "LavaSlime" => EntityKind::MagmaCube,
        "EnderDragon" => EntityKind::Dragon,
        "WitherBoss" => EntityKind::Wither,
        "Bat" => EntityKind::Bat,
        "Witch" => EntityKind::Witch,
        "Endermite" => EntityKind::Endermite,
        "Guardian" => EntityKind::Guardian,
        "Pig" => EntityKind::Pig,
        "Sheep" => EntityKind::Sheep,
        "Cow" => EntityKind::Cow,
        "Chicken" => EntityKind::Chicken,
        "Squid" => EntityKind::Squid,
        "Wolf" => EntityKind::Wolf,
        "MushroomCow" => EntityKind::Mooshroom,
        "SnowMan" => EntityKind::Snowman,
        "Ozelot" => EntityKind::Ocelot,
        "VillagerGolem" => EntityKind::IronGolem,
        "EntityHorse" => EntityKind::Horse,
        "Rabbit" => EntityKind::Rabbit,
        "Villager" => EntityKind::Villager,
        _ => return None,
    })
}

/// The mob named in the NBT of a spawner update; `None` when the NBT is
/// malformed, names nothing or names something that is not a mob.
fn spawned_mob(nbt: &[u8]) -> Option<EntityKind> {
    let root = Nbt::parse(nbt).ok()?;
    mob_of_name(root.get("EntityId")?.as_str()?)
}

/// The spin of the mob in degrees at `ticks` game ticks of the clock.
fn spin_degrees(ticks: f64) -> f32 {
    // Reduced as the reference does (`% 360` on the rotation) before the
    // factor, so the angle keeps its precision however long the client runs.
    let rotation = (ticks * DEGREES_PER_TICK) % 360.0;
    (rotation * SPIN_FACTOR) as f32
}

/// The matrix from the mob's model space (at the origin, upright) to the
/// world: the renderer's `translate(.5, 0, .5)`, `translate(0, .4, 0)`,
/// `rotate(spin, y)`, `rotate(-30, x)`, `translate(0, -.4, 0)`, `scale(f)`.
fn mob_matrix(origin: Vec3, spin_degrees: f32) -> Mat4 {
    Mat4::from_translation(origin + Vec3::new(0.5, 0.4, 0.5))
        * Mat4::from_rotation_y(spin_degrees.to_radians())
        * Mat4::from_rotation_x((-30.0_f32).to_radians())
        * Mat4::from_translation(Vec3::new(0.0, -0.4, 0.0))
        * Mat4::from_scale(Vec3::splat(MOB_SCALE))
}

/// Draws the mob of the spawner at `entry`, if the server named one.
pub(super) fn add_spawner(
    batches: &mut Batches,
    entry: &BlockEntityRef,
    scene: &BlockEntityScene<'_>,
) {
    let [x, y, z] = entry.position;
    let Some(update) = scene
        .data
        .get(&(x, y, z))
        .filter(|u| u.action == SPAWNER_ACTION)
    else {
        return;
    };
    let Some(kind) = update.nbt.as_deref().and_then(spawned_mob) else {
        return;
    };
    let ticks = f64::from(scene.seconds.max(0.0)) * TICKS_PER_SECOND;
    let mut mob = EntityRenderSnapshot::new(EntityId(0), kind, Vec3::ZERO);
    mob.light = entry.light;
    mob.age = ticks as f32;
    let matrix = mob_matrix(Vec3::new(x as f32, y as f32, z as f32), spin_degrees(ticks));
    for (slot, vertices) in mesh_entities(&[mob]) {
        batches
            .entry(slot)
            .or_default()
            .extend(vertices.into_iter().map(|mut vertex| {
                vertex.position = matrix
                    .transform_point3(Vec3::from(vertex.position))
                    .to_array();
                vertex.normal = matrix
                    .transform_vector3(Vec3::from(vertex.normal))
                    .normalize_or_zero()
                    .to_array();
                vertex
            }));
    }
}

#[cfg(test)]
mod tests {
    use umicraft_game::server::block_entities::{BlockEntityData, EntityTable, SignTable};
    use umicraft_game::server::chest_lids::ChestLids;
    use umicraft_model::entity::TextureSlot;
    use umicraft_world::{BlockState, LightLevel};

    use super::*;

    fn nbt_with(id: &str) -> Vec<u8> {
        let mut out = vec![10, 0, 0, 8, 0, 8];
        out.extend(b"EntityId");
        out.extend((id.len() as u16).to_be_bytes());
        out.extend(id.as_bytes());
        out.push(0);
        out
    }

    fn entry() -> BlockEntityRef {
        BlockEntityRef {
            position: [4, 70, 9],
            state: BlockState::new(52, 0),
            same_neighbour: umicraft_mesh::SameNeighbours::default(),
            light: LightLevel { block: 0, sky: 15 },
        }
    }

    fn draw(action: u8, nbt: Option<Vec<u8>>, seconds: f32) -> Batches {
        let (lids, signs) = (ChestLids::default(), SignTable::new());
        let mut data = EntityTable::new();
        data.insert((4, 70, 9), BlockEntityData { action, nbt });
        let scene = BlockEntityScene {
            lids: &lids,
            signs: &signs,
            data: &data,
            seconds,
            font: None,
            skins: &crate::entity_render::NoSkins,
            partial_tick: 0.0,
            camera: Vec3::ZERO,
        };
        let mut batches = Batches::new();
        add_spawner(&mut batches, &entry(), &scene);
        batches
    }

    #[test]
    fn the_names_of_the_reference_map_to_mobs() {
        assert_eq!(mob_of_name("Zombie"), Some(EntityKind::Zombie));
        assert_eq!(mob_of_name("LavaSlime"), Some(EntityKind::MagmaCube));
        assert_eq!(mob_of_name("PigZombie"), Some(EntityKind::ZombiePigman));
        assert_eq!(mob_of_name("Item"), None);
        assert_eq!(mob_of_name(""), None);
    }

    #[test]
    fn the_mob_stands_in_the_middle_of_the_cage_and_is_scaled() {
        let batches = draw(1, Some(nbt_with("Creeper")), 0.0);
        let vertices = &batches[&TextureSlot::Creeper];
        assert!(!vertices.is_empty());
        // 1.8 blocks tall at 0.4375 scale, tilted: inside the 1x1 cage.
        for vertex in vertices {
            let [x, y, z] = vertex.position;
            assert!((3.9..=5.1).contains(&x), "{x}");
            assert!((69.5..=71.0).contains(&y), "{y}");
            assert!((8.9..=10.1).contains(&z), "{z}");
        }
    }

    #[test]
    fn the_mob_spins_with_the_clock() {
        let a = draw(1, Some(nbt_with("Creeper")), 0.0);
        let b = draw(1, Some(nbt_with("Creeper")), 0.3);
        assert_ne!(
            a[&TextureSlot::Creeper][0].position,
            b[&TextureSlot::Creeper][0].position
        );
        assert_eq!(spin_degrees(0.0), 0.0);
        // One tick turns the rotation by 5 degrees, the mob by 50.
        assert!((spin_degrees(1.0) - 50.0).abs() < 1e-3);
        assert!(spin_degrees(1e9) < 3600.0);
    }

    #[test]
    fn nothing_is_drawn_without_a_known_mob_or_for_another_block_entity() {
        assert!(draw(1, None, 0.0).is_empty());
        assert!(draw(1, Some(nbt_with("Boat")), 0.0).is_empty());
        assert!(draw(1, Some(vec![10, 0, 0, 8, 0, 8, b'E']), 0.0).is_empty());
        assert!(draw(1, Some(vec![0xFF; 7]), 0.0).is_empty());
        assert!(draw(5, Some(nbt_with("Creeper")), 0.0).is_empty());
    }
}
