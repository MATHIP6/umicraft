// SPDX-License-Identifier: MIT OR Apache-2.0

//! Filled maps in the world (P3): the map in the first-person hands and in
//! item frames.
//!
//! The GUI icon of a filled map is its `filled_map` item sprite, as in the
//! reference (`RenderItem` never paints the map contents into a slot). Here
//! live the GPU textures (one 128 by 128 texture per map id, uploaded again
//! when its revision changes, like `MapItemRenderer.Instance`) and the
//! placement of the quads of [`geometry`].
//!
//! Sources: MCP `ItemRenderer.renderItemMap`, `RenderItemFrame.renderItem`
//! and `MapItemRenderer`.

mod geometry;

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use umicraft_assets::client_jar::ClientJar;
use umicraft_game::entity::{EntityKind, EntityRenderSnapshot};
use umicraft_game::hand::HandPose;
use umicraft_game::server::maps::{MapData, MapStore};
use umicraft_model::entity::{HumanoidModel, ModelVertex};
use umicraft_render::{EntityTexture, EntityVertex, Renderer};
use umicraft_world::LightLevel;

use crate::entity_render::load_or_fallback;
use crate::item_world::frame_base;
use geometry::{background, markers, pixels, place, GRID};

/// The item id of a filled map; its damage value is the map id.
pub const FILLED_MAP: i16 = 358;

const BACKGROUND_PATH: &str = "assets/minecraft/textures/map/map_background.png";
const ICONS_PATH: &str = "assets/minecraft/textures/map/map_icons.png";
/// Full block and sky light, for the unlit maps of item frames.
const FULL_LIGHT: u32 = 0x0000_00FF;
/// The light of the arms, as for the single arm (full sky light).
const ARM_LIGHT: u32 = 0x0000_00F0;
/// Frame items are pushed this far out of the frame centre.
const FRAME_ITEM_OFFSET: f32 = 0.4375;
/// The reference's frame map scale: one grid unit is `1/128` of a block.
const FRAME_UNIT: f32 = 1.0 / GRID;
/// Rotation steps of a framed map are two eighths of a turn each.
const FRAME_TURN: f32 = std::f32::consts::FRAC_PI_2;

/// The map id of a stack, `None` when it is not a filled map.
#[must_use]
pub fn map_id_of(item_id: i16, damage: i16) -> Option<i32> {
    (item_id == FILLED_MAP).then_some(i32::from(damage))
}

/// Vertices that share one texture.
pub struct MapBatch {
    /// The vertices, in the space the caller asked for.
    pub vertices: Vec<EntityVertex>,
    /// The texture they sample.
    pub bind_group: wgpu::BindGroup,
}

/// The GPU textures of the maps and the sheets they are drawn with.
pub struct MapTextures {
    background: EntityTexture,
    icons: EntityTexture,
    /// The texture of each map and the revision it shows.
    pixels: HashMap<i32, (u64, EntityTexture)>,
}

impl MapTextures {
    /// Loads the paper and the marker sheet from the official jar, with a
    /// plain paper colour and no markers when the jar is missing.
    #[must_use]
    pub fn load(renderer: &Renderer) -> Self {
        let jar = crate::official::find_verified_jar().ok();
        Self::from_jar(jar.as_ref(), renderer)
    }

    fn from_jar(jar: Option<&ClientJar>, renderer: &Renderer) -> Self {
        let paper = || (vec![214, 190, 140, 255], 1, 1);
        let nothing = || (vec![0, 0, 0, 0], 1, 1);
        let (rgba, w, h) = load_or_fallback(jar, BACKGROUND_PATH, paper);
        let background = renderer.create_entity_texture(&rgba, w, h);
        let (rgba, w, h) = load_or_fallback(jar, ICONS_PATH, nothing);
        let icons = renderer.create_entity_texture(&rgba, w, h);
        Self {
            background,
            icons,
            pixels: HashMap::new(),
        }
    }

    /// Drops the textures of maps the server no longer has, and uploads the
    /// pixels of `map_id` when they are new or changed.
    ///
    /// Returns the map, or `None` when the server never sent it.
    fn sync<'a>(
        &mut self,
        store: &'a MapStore,
        map_id: i32,
        renderer: &Renderer,
    ) -> Option<&'a MapData> {
        self.pixels.retain(|id, _| store.get(*id).is_some());
        let map = store.get(map_id)?;
        match self.pixels.get_mut(&map_id) {
            Some((revision, _)) if *revision == map.revision() => {}
            Some((revision, texture)) => {
                texture.write(renderer.queue(), &map.rgba());
                *revision = map.revision();
            }
            None => {
                let texture = renderer.create_entity_texture(&map.rgba(), 128, 128);
                self.pixels.insert(map_id, (map.revision(), texture));
            }
        }
        Some(map)
    }

    /// The batches of a map held in the hands: the paper, the pixels and the
    /// markers, in camera space (MCP `ItemRenderer.renderItemMap`).
    ///
    /// `bob` is the view-bobbing transform of the frame; `light` the light
    /// at the eye.
    pub fn hand_batches(
        &mut self,
        store: &MapStore,
        map_id: i32,
        (pose, bob, light): (&HandPose, Mat4, LightLevel),
        renderer: &Renderer,
    ) -> Vec<MapBatch> {
        let to_camera = bob * pose.map_matrix();
        let light = pack_light(light);
        let mut batches = vec![MapBatch {
            vertices: place(&background(), &to_camera, light),
            bind_group: self.background.bind_group().clone(),
        }];
        if let Some(map) = self.sync(store, map_id, renderer) {
            batches.extend(self.map_batches(map_id, map, &to_camera, light, false));
        }
        batches
    }

    /// The batches of the maps in the item frames of `snapshots`, in world
    /// space (MCP `RenderItemFrame.renderItem`).
    pub fn frame_batches(
        &mut self,
        snapshots: &[EntityRenderSnapshot],
        store: &MapStore,
        renderer: &Renderer,
    ) -> Vec<MapBatch> {
        let mut batches = Vec::new();
        for entity in snapshots.iter().filter(|e| e.kind == EntityKind::ItemFrame) {
            let Some(map_id) = entity
                .item
                .as_ref()
                .and_then(|item| map_id_of(item.item_id, item.damage))
            else {
                continue;
            };
            let Some(map) = self.sync(store, map_id, renderer) else {
                continue;
            };
            let to_world = frame_matrix(entity);
            batches.extend(self.map_batches(map_id, map, &to_world, FULL_LIGHT, true));
        }
        batches
    }

    /// The pixel and marker batches of a map that is already uploaded.
    fn map_batches(
        &self,
        map_id: i32,
        map: &MapData,
        matrix: &Mat4,
        light: u32,
        own_only: bool,
    ) -> Vec<MapBatch> {
        let mut batches = Vec::new();
        if let Some((_, texture)) = self.pixels.get(&map_id) {
            batches.push(MapBatch {
                vertices: place(&pixels(), matrix, light),
                bind_group: texture.bind_group().clone(),
            });
        }
        let decorations = markers(&map.icons, own_only);
        if !decorations.is_empty() {
            batches.push(MapBatch {
                vertices: place(&decorations, matrix, light),
                bind_group: self.icons.bind_group().clone(),
            });
        }
        batches
    }
}

/// The packed vertex light of a [`LightLevel`] (block in bits 0..=3, sky in
/// bits 4..=7).
fn pack_light(light: LightLevel) -> u32 {
    u32::from(light.block.min(15)) | (u32::from(light.sky.min(15)) << 4)
}

/// The matrix from the grid of a framed map to the world.
///
/// `RenderItemFrame.renderItem`: out of the frame centre, rolled by whole
/// quarter turns, turned half a turn, scaled to blocks, centred and pushed
/// one unit behind the frame plane.
fn frame_matrix(entity: &EntityRenderSnapshot) -> Mat4 {
    let quarter_turns = f32::from(entity.frame_rotation % 4);
    frame_base(entity)
        * Mat4::from_translation(Vec3::new(0.0, 0.0, FRAME_ITEM_OFFSET))
        * Mat4::from_rotation_z(quarter_turns * FRAME_TURN)
        * Mat4::from_rotation_z(std::f32::consts::PI)
        * Mat4::from_scale(Vec3::splat(FRAME_UNIT))
        * Mat4::from_translation(Vec3::new(-GRID / 2.0, -GRID / 2.0, -1.0))
}

/// The first-person arms that hold a map, in camera space.
#[must_use]
pub fn map_arm_vertices(pose: &HandPose, bob: Mat4, is_slim: bool) -> Vec<EntityVertex> {
    let [right, left] = pose.map_arm_matrices();
    [
        (right, HumanoidModel::first_person_arm(is_slim)),
        (left, HumanoidModel::first_person_left_arm(is_slim)),
    ]
    .into_iter()
    .flat_map(|(matrix, model)| arm_vertices(&(bob * matrix), model))
    .collect()
}

/// Moves arm model vertices by `matrix`, lit like the hand.
fn arm_vertices(matrix: &Mat4, model: Vec<ModelVertex>) -> Vec<EntityVertex> {
    model
        .into_iter()
        .map(|vertex| EntityVertex {
            position: matrix
                .transform_point3(Vec3::from(vertex.position))
                .to_array(),
            normal: matrix
                .transform_vector3(Vec3::from(vertex.normal))
                .normalize_or_zero()
                .to_array(),
            uv: vertex.uv,
            light: ARM_LIGHT,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(rotation: u8) -> EntityRenderSnapshot {
        let mut snapshot = EntityRenderSnapshot::new(
            umicraft_game::entity::EntityId(1),
            EntityKind::ItemFrame,
            Vec3::ZERO,
        );
        snapshot.frame_rotation = rotation;
        snapshot
    }

    #[test]
    fn only_filled_maps_have_a_map_id() {
        assert_eq!(map_id_of(358, 7), Some(7));
        assert_eq!(map_id_of(395, 7), None);
    }

    #[test]
    fn light_packs_block_low_and_sky_high() {
        assert_eq!(pack_light(LightLevel { block: 3, sky: 12 }), 0xC3);
        assert_eq!(pack_light(LightLevel { block: 99, sky: 99 }), 0xFF);
    }

    #[test]
    fn a_framed_map_is_a_block_wide_pane_in_front_of_the_frame() {
        let matrix = frame_matrix(&frame(0));
        let a = matrix.transform_point3(Vec3::new(0.0, 64.0, 0.0));
        let b = matrix.transform_point3(Vec3::new(128.0, 64.0, 0.0));
        assert!((a.distance(b) - 1.0).abs() < 1e-4);
        let centre = matrix.transform_point3(Vec3::new(64.0, 64.0, 0.0));
        let from_block_centre = centre - Vec3::splat(0.5);
        assert!((from_block_centre.z.abs() - (FRAME_ITEM_OFFSET - FRAME_UNIT)).abs() < 1e-4);
    }

    #[test]
    fn the_four_rotations_repeat() {
        let corner = Vec3::new(0.0, 0.0, 0.0);
        let at = |r| frame_matrix(&frame(r)).transform_point3(corner);
        assert!(at(0).distance(at(4)) < 1e-5);
        assert!(at(0).distance(at(1)) > 0.5);
    }

    #[test]
    fn both_arms_make_two_models_of_vertices() {
        let arms = map_arm_vertices(&HandPose::default(), Mat4::IDENTITY, false);
        assert_eq!(arms.len(), 144);
    }
}
