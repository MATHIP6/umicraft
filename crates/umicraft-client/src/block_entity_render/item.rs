// SPDX-License-Identifier: MIT OR Apache-2.0

//! Skull and banner items (MCP `TileEntityItemStackRenderer.renderByItem`).
//!
//! An item whose model is `builtin/entity` has no quads: vanilla draws the
//! same tile-entity model as for a placed block, at the origin, after the
//! display pose of the context (GUI, hand, ground, frame). This module builds
//! that mesh once per stack, in the cube space of the baked item models, so
//! the poses of `umicraft_model::item::pose` apply to it unchanged.
//!
//! - A skull is drawn as a floor skull with `Rot` 0, scaled by 2 about the
//!   bottom centre, wearing the skin of `SkullOwner` when it is known.
//! - A banner is drawn standing, with its pole, unturned and with the sway
//!   of a banner at the origin at time 0 (no world, so no phase).

use glam::{Mat4, Vec3};
use umicraft_game::item_nbt::ItemDisplay;
use umicraft_model::entity::{banner_sway, BannerModel, ModelVertex, TextureSlot};
use umicraft_render::EntityVertex;
use umicraft_world::LightLevel;

use super::banner_texture::BannerDesign;
use super::skull::skull_vertices;
use super::{packed_light, BlockEntityMeshes};
use crate::entity_render::{owner_skin_slot, skull_texture, SkinLookup};

/// `Items.skull`.
const SKULL: i16 = 397;
/// `Items.banner`.
const BANNER: i16 = 425;
/// `renderByItem` scales a skull by 2 about the origin of its renderer.
const SKULL_SCALE: f32 = 2.0;

/// The tile-entity mesh of an item and the texture it samples.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EntityItemMesh {
    /// The texture of `vertices`; for a banner, the slot of `banner`.
    pub slot: TextureSlot,
    /// The model in the cube space of the item models (`0.0..=1.0`).
    pub vertices: Vec<ModelVertex>,
    /// The design the texture of a banner is composed from.
    pub banner: Option<BannerDesign>,
}

impl EntityItemMesh {
    /// The vertices moved by `pose` (cube space to the frame the caller
    /// draws in) and lit by `light`.
    pub(crate) fn placed(&self, pose: &Mat4, light: LightLevel) -> Vec<EntityVertex> {
        let light = packed_light(light);
        self.vertices
            .iter()
            .map(|vertex| EntityVertex {
                position: pose
                    .transform_point3(Vec3::from(vertex.position))
                    .to_array(),
                normal: pose
                    .transform_vector3(Vec3::from(vertex.normal))
                    .normalize_or_zero()
                    .to_array(),
                uv: vertex.uv,
                light,
            })
            .collect()
    }

    /// Records the banner design so that the texture cache composes it.
    pub(crate) fn record_design(&self, meshes: &mut BlockEntityMeshes) {
        if let Some(design) = &self.banner {
            meshes
                .banners
                .entry(self.slot)
                .or_insert_with(|| design.clone());
        }
    }

    /// Appends the mesh to `meshes` as [`Self::placed`] and records its design.
    pub(crate) fn push_into(&self, meshes: &mut BlockEntityMeshes, pose: &Mat4, light: LightLevel) {
        self.record_design(meshes);
        meshes
            .solid
            .entry(self.slot)
            .or_default()
            .extend(self.placed(pose, light));
    }
}

/// The mesh of a skull or banner stack, `None` for any other item.
///
/// `nbt` is the stack's raw tag, `skins` resolves the owner's skin of a
/// player skull.
#[must_use]
pub(crate) fn entity_item_mesh(
    item_id: i16,
    damage: i16,
    nbt: Option<&[u8]>,
    skins: &dyn SkinLookup,
) -> Option<EntityItemMesh> {
    match item_id {
        SKULL => Some(skull_item(damage, nbt, skins)),
        BANNER => Some(banner_item(damage, nbt)),
        _ => None,
    }
}

fn skull_item(damage: i16, nbt: Option<&[u8]>, skins: &dyn SkinLookup) -> EntityItemMesh {
    let (slot, humanoid) = skull_texture(damage);
    let owner = nbt
        .map(ItemDisplay::from_nbt)
        .and_then(|d| d.skull_textures);
    // The renderer is called at the origin with `translate(-0.5, 0, -0.5)` and
    // `scale(2)` around it.
    let place = Mat4::from_translation(Vec3::new(-0.5, 0.0, -0.5))
        * Mat4::from_scale(Vec3::splat(SKULL_SCALE));
    EntityItemMesh {
        slot: match slot {
            TextureSlot::Steve => owner
                .as_deref()
                .and_then(|value| owner_skin_slot(value, skins))
                .unwrap_or(slot),
            other => other,
        },
        vertices: into_item_space(skull_vertices(1, 0.0, humanoid), place),
        banner: None,
    }
}

fn banner_item(damage: i16, nbt: Option<&[u8]>) -> EntityItemMesh {
    let design = BannerDesign::from_item(nbt, damage);
    EntityItemMesh {
        slot: design.slot(),
        vertices: into_item_space(
            BannerModel::emit(true, 0.0, banner_sway(0.0)),
            Mat4::IDENTITY,
        ),
        banner: Some(design),
    }
}

/// Applies `place`, then the half turn `renderItem` gives a built-in
/// renderer: `rotate(180, y)` about the model centre. (The baked models have
/// no such turn; the built-in ones face the other way.)
fn into_item_space(vertices: Vec<ModelVertex>, place: Mat4) -> Vec<ModelVertex> {
    let turn = Mat4::from_translation(Vec3::new(0.5, 0.0, 0.5))
        * Mat4::from_rotation_y(std::f32::consts::PI)
        * Mat4::from_translation(Vec3::new(-0.5, 0.0, -0.5));
    let matrix = turn * place;
    vertices
        .into_iter()
        .map(|vertex| ModelVertex {
            position: matrix
                .transform_point3(Vec3::from(vertex.position))
                .to_array(),
            normal: matrix
                .transform_vector3(Vec3::from(vertex.normal))
                .normalize_or_zero()
                .to_array(),
            uv: vertex.uv,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_render::NoSkins;

    fn extent(mesh: &EntityItemMesh, axis: usize) -> (f32, f32) {
        mesh.vertices
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), v| {
                (low.min(v.position[axis]), high.max(v.position[axis]))
            })
    }

    fn mesh(item_id: i16, damage: i16, nbt: Option<&[u8]>) -> EntityItemMesh {
        entity_item_mesh(item_id, damage, nbt, &NoSkins).expect("an entity item")
    }

    #[test]
    fn pushing_places_the_vertices_and_records_the_banner_design() {
        let mut meshes = BlockEntityMeshes::default();
        let banner = mesh(BANNER, 3, None);
        let pose = Mat4::from_translation(Vec3::new(10.0, 20.0, 30.0));
        banner.push_into(&mut meshes, &pose, LightLevel { block: 4, sky: 9 });
        let drawn = &meshes.solid[&banner.slot];
        assert_eq!(drawn.len(), banner.vertices.len());
        assert_eq!(drawn[0].light, 4 | (9 << 4));
        let first = banner.vertices[0].position;
        assert!((drawn[0].position[0] - (first[0] + 10.0)).abs() < 1e-5);
        assert_eq!(meshes.banners[&banner.slot], BannerDesign::plain(3));
        // A skull records no design.
        let mut meshes = BlockEntityMeshes::default();
        mesh(SKULL, 0, None).push_into(&mut meshes, &pose, LightLevel::default());
        assert!(meshes.banners.is_empty());
    }

    #[test]
    fn other_items_have_no_entity_mesh() {
        assert_eq!(entity_item_mesh(1, 0, None, &NoSkins), None);
        // The chests are `builtin/entity` too but are not handled here.
        assert_eq!(entity_item_mesh(54, 0, None, &NoSkins), None);
    }

    #[test]
    fn a_skull_fills_the_item_cube_from_the_bottom() {
        // `scale(2)` of the 8 pixel head: a half block head becomes one block.
        let skull = mesh(SKULL, 0, None);
        let (low, high) = extent(&skull, 1);
        assert!(
            low.abs() < 1e-5 && (high - 1.0).abs() < 1e-5,
            "{low} {high}"
        );
        for axis in [0, 2] {
            let (low, high) = extent(&skull, axis);
            assert!(low.abs() < 1e-5 && (high - 1.0).abs() < 1e-5, "{axis}");
        }
    }

    #[test]
    fn the_damage_picks_the_skull_texture() {
        let slots: Vec<_> = (0..5).map(|kind| mesh(SKULL, kind, None).slot).collect();
        assert_eq!(
            slots,
            [
                TextureSlot::Skeleton,
                TextureSlot::SkeletonWither,
                TextureSlot::Zombie,
                TextureSlot::Steve,
                TextureSlot::Creeper
            ]
        );
    }

    #[test]
    fn a_skull_turns_half_a_circle_against_the_placed_floor_skull() {
        // The front of a floor skull with `Rot` 0 faces south; the item, which
        // the built-in renderer turns by 180 degrees, faces north.
        let placed = skull_vertices(1, 0.0, false);
        let item = mesh(SKULL, 0, None).vertices;
        assert_eq!(placed.len(), item.len());
        for (placed, item) in placed.iter().zip(&item) {
            assert!((placed.normal[2] + item.normal[2]).abs() < 1e-5);
            assert!((placed.normal[0] + item.normal[0]).abs() < 1e-5);
        }
    }

    #[test]
    fn a_banner_item_has_a_pole_and_a_design_from_its_tag_or_damage() {
        let plain = mesh(BANNER, 5, None);
        assert_eq!(plain.vertices.len(), 3 * 36);
        assert_eq!(plain.banner, Some(BannerDesign::plain(5)));
        assert_eq!(plain.slot, BannerDesign::plain(5).slot());
        // `{BlockEntityTag: {Base: 1}}` overrides the damage.
        let tagged = [
            10, 0, 0, 10, 0, 14, b'B', b'l', b'o', b'c', b'k', b'E', b'n', b't', b'i', b't', b'y',
            b'T', b'a', b'g', 3, 0, 4, b'B', b'a', b's', b'e', 0, 0, 0, 1, 0, 0,
        ];
        assert_eq!(
            mesh(BANNER, 5, Some(&tagged)).banner,
            Some(BannerDesign::plain(1))
        );
        // Unreadable NBT falls back to the damage.
        assert_eq!(
            mesh(BANNER, 5, Some(&[0xff])).banner,
            Some(BannerDesign::plain(5))
        );
    }
}
